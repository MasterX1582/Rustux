//! Ring-3 process support: per-task page tables (kernel entries
//! copied supervisor-only), PT_LOAD mapping from the ELF image, a user
//! stack, a per-task kernel stack for SYSCALL entries, and the
//! iretq/sysretq privilege-transition pair.
//!
//! Task model: one resident task at a time, run to completion. A Task
//! owns every frame it was allocated — page tables, data pages, stacks
//! — in a ledger that reap() returns to the buddy wholesale, so task
//! exit leaks nothing (the CVE-2022-50661 discipline: teardown freeing
//! is designed in, not patched on).
//!
//! The user page table shares the kernel's low PML4 slots (copied
//! verbatim — they were created supervisor-only, so user code cannot
//! touch kernel memory through them), while user pages land in PML4
//! slots 128..256 (elf::USER_VA_LO..HI). There is no scheduler yet:
//! run() is a synchronous drop — the image's exit syscall unwinds the
//! kernel frame saved by user_enter and control returns here.

use crate::{cpu, elf, kalloc, syscall};

const PAGE: u64 = 0x1000;
const PRESENT: u64 = 1;
const WRITE: u64 = 2;
const USER: u64 = 4;
const NX: u64 = 1 << 63;
/// Physical-address bits of a PTE: strip flags AND the NX bit — a
/// plain `& !0xfff` would keep NX set and fabricate a non-canonical
/// "address" out of any read-only mapping.
const PHYS_MASK: u64 = 0x000f_ffff_ffff_f000;

/// User stack: 16 pages high in the user window.
const USTACK_VA: u64 = 0x5556_0000_0000;
const USTACK_PAGES: u64 = 16;

/// Per-task kernel stack for SYSCALL entries — replaces the shared
/// static stack so a task's kernel state can't bleed into the next.
const KSTACK_SIZE: usize = 8192;

/// Maximum distinct user VA ranges recorded for syscall validation.
const MAX_RANGES: usize = 8;

/// Max resident tasks — a hard bound so spawn can't exhaust the
/// kernel heap by enqueueing forever.
const MAX_TASKS: usize = 16;

/// A resumable user-mode context: the 15 GPRs in sched_dispatch's
/// push order plus the iret frame. Every task transition — first
/// launch, timer preemption, exit — is the same operation: point
/// rsp at a UCtx, pop, iretq.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UCtx {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl UCtx {
    const ZERO: UCtx = UCtx {
        r15: 0, r14: 0, r13: 0, r12: 0, rbp: 0, rbx: 0,
        r11: 0, r10: 0, r9: 0, r8: 0,
        rdi: 0, rsi: 0, rdx: 0, rcx: 0, rax: 0,
        rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0,
    };
    /// The fabricated ctx of a never-run task: clean GPRs, user
    /// segments, IF set, entry + fresh stack — same shape a timer
    /// IRQ snapshot would leave.
    fn launch(rip: u64, rsp: u64) -> UCtx {
        UCtx {
            rip,
            cs: 0x33,
            rflags: 0x202,
            rsp,
            ss: 0x2b,
            ..UCtx::ZERO
        }
    }
}

/// The scheduler's state model: Runnable tasks can be picked; Exited
/// ones persist as zombies until their parent's wait4 collects them
/// (or they're orphaned); Blocked tasks sleep inside a syscall whose
/// resume ctx is already saved — a matching child exit wakes them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskState {
    Runnable,
    Exited,
    Blocked,
}

/// Max fds per task — a hard bound so a hostile process cannot
/// exhaust kernel space by opening forever or force the table to
/// grow by a user-chosen size (the CVE-2025-39756 class: dup2 must
/// never size an allocation from its target fd number).
const MAX_FDS: usize = 16;

/// The object an fd slot names — Linux's inode-level identity.
enum FdKind {
    /// fd 0: reads return EOF, writes fail EBADF.
    Stdin,
    /// fds 1/2: writes go to the serial ring, reads fail EBADF.
    Console,
    /// Initrd-backed read-only file. Borrows LOADER_DATA — immortal,
    /// so no refcount on the backing bytes is needed.
    File(&'static [u8]),
    /// The flat archive root. pos is the index of the next archive
    /// entry getdents64 should emit — Linux dir offsets are opaque
    /// cookies, ours just happens to be an index.
    Dir,
}

/// An open file description — the Linux `struct file` analog. fds
/// are only slots pointing here; dup'd slots share one OFD, which is
/// why the seek offset lives here and not in the slot.
struct OpenFile {
    kind: FdKind,
    pos: usize,
    /// fd slots pointing at this OFD; freed at zero.
    refs: usize,
}

/// A resident task: every resource it owns, in one owned bundle.
pub struct Task {
    /// Physical base of the process PML4.
    pml4: u64,
    entry: u64,
    stack_top: u64,
    /// kmalloc'd syscall stack; top is the rsp the entry stub loads.
    kstack_base: u64,
    kstack_top: u64,
    /// [lo, hi, writable) address ranges user pointers may legally
    /// reference. The syscall layer's user-pointer gate walks exactly
    /// this table; writable marks ranges the kernel may write INTO
    /// (read() target buffers must be user-writable, like Linux).
    ranges: [(u64, u64, bool); MAX_RANGES],
    nranges: usize,
    /// fd slots -> GLOBAL OFD indices; 0/1/2 pre-seeded std streams.
    fds: [Option<u8>; MAX_FDS],
    /// Saved user context for switches — fabricated at spawn, then
    /// snapshotted by the timer IRQ on every preemption.
    ctx: UCtx,
    state: TaskState,
    /// Process identity — Linux-visible pid and parent pid (0 = none).
    pid: u64,
    ppid: u64,
    /// The exit() argument — consumed by wait4 (status = code<<8).
    exit_code: u64,
    /// wait4 target while Blocked: -1 = any child, else a pid.
    wait_target: i64,
    /// wait4 status user pointer — validated writable before blocking.
    wait_status_va: u64,
}

/// Max open file descriptions system-wide — a hard bound so fd
/// pressure from one task can't exhaust kernel memory. OFDs are
/// GLOBAL (not per-task) because fork MUST share them: a child's
/// fds name the same descriptions — same offsets, same refs —
/// exactly like Linux's files_struct sharing of struct file.
static mut OFDS: [Option<OpenFile>; MAX_OFDS] = [const { None }; MAX_OFDS];
const MAX_OFDS: usize = 64;

/// Process-id allocator — monotonically increasing, never reused.
static mut NEXT_PID: u64 = 0;
fn next_pid() -> u64 {
    unsafe {
        let p = core::ptr::addr_of_mut!(NEXT_PID);
        *p += 1;
        *p
    }
}

impl Task {
    /// free_pgtables, Rustux style: walk the user half of the page
    /// table and return every frame — leaf and table alike — to the
    /// buddy. The page table IS the ledger (CVE-2022-50661 discipline:
    /// teardown freeing is designed in). Slots 0..128 are shared
    /// kernel mappings and must NOT be freed; slots 128..256 are the
    /// user window, entirely task-owned.
    /// Free every frame under user PML4 slots 128..256 — leaf, PT,
    /// PD, PDPT — and clear the slots so the address space can be
    /// reused (exec) or the table freed (reap). Shared kernel slots
    /// 0..127 are never touched.
    fn free_user_mappings(&mut self) {
        for i in 128..256u64 {
            let e = rd64(self.pml4 + 8 * i);
            if e & PRESENT == 0 {
                continue;
            }
            let pdpt = e & PHYS_MASK;
            for j in 0..512u64 {
                let de = rd64(pdpt + 8 * j);
                if de & PRESENT == 0 {
                    continue;
                }
                let pd = de & PHYS_MASK;
                for k in 0..512u64 {
                    let te = rd64(pd + 8 * k);
                    if te & PRESENT == 0 {
                        continue;
                    }
                    let pt = te & PHYS_MASK;
                    for l in 0..512u64 {
                        let leaf = rd64(pt + 8 * l);
                        if leaf & PRESENT != 0 {
                            kalloc::free_frame(leaf & PHYS_MASK);
                        }
                    }
                    kalloc::free_frame(pt);
                }
                kalloc::free_frame(pd);
            }
            kalloc::free_frame(pdpt);
            wr64(self.pml4 + 8 * i, 0);
        }
    }

    /// Drop every fd's reference on its (global) OFD — Linux's
    /// exit_files: shared descriptions outlive the task when
    /// another task still points at them.
    fn release_fds(&mut self) {
        let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
        for s in self.fds.iter_mut() {
            if let Some(i) = s.take() {
                if let Some(o) = ofds.get_mut(i as usize).and_then(|x| x.as_mut()) {
                    o.refs -= 1;
                    if o.refs == 0 {
                        ofds[i as usize] = None;
                    }
                }
            }
        }
    }

    fn reap(&mut self) {
        self.release_fds();
        self.free_user_mappings();
        kalloc::free_frame(self.pml4);
        if self.kstack_base != 0 {
            // SAFETY: kstack_base came from kmalloc, freed once here.
            unsafe { kalloc::kfree(self.kstack_base as *mut u8) };
            self.kstack_base = 0;
            self.kstack_top = 0;
        }
    }
}

/// The task table — the scheduler's world. Tasks are never moved
/// once slotted: ctx pointers handed to the asm must stay stable.
static mut TASKS: [Option<Task>; MAX_TASKS] = [const { None }; MAX_TASKS];

/// Index of the running task; usize::MAX when the scheduler isn't
/// running (before launch / after the last exit unwound to main).
static mut CUR_I: usize = usize::MAX;

/// The initrd archive as a flat read-only filesystem — set once at
/// boot before the first task runs. It names LOADER_DATA pages the
/// buddy never pools, so the slice is valid for the kernel's life.
static mut INITRD: Option<&'static [u8]> = None;

/// Point fd_open at the archive. Called once from main context.
pub fn set_initrd(img: &'static [u8]) {
    // SAFETY: sole writer, before any task exists.
    unsafe { *core::ptr::addr_of_mut!(INITRD) = Some(img) };
}

/// Resolve a flat name to archive bytes — the filesystem lookup used
/// by open and execve alike.
pub fn fs_lookup(name: &[u8]) -> Option<&'static [u8]> {
    let arc = unsafe { *core::ptr::addr_of!(INITRD) }?;
    crate::cpio::find(arc, name)
}

/// Borrow the running task mutably — syscall-context accessor.
fn cur_mut() -> Option<&'static mut Task> {
    // SAFETY: single CPU, IF-masked syscall path — sole accessor.
    let i = unsafe { *core::ptr::addr_of!(CUR_I) };
    unsafe { (*core::ptr::addr_of_mut!(TASKS)).get_mut(i)?.as_mut() }
}

fn rd64(p: u64) -> u64 {
    // SAFETY: callers only name page-table frames inside the identity
    // map; the kernel runs under an identity mapping.
    unsafe { (p as *const u64).read_volatile() }
}

fn wr64(p: u64, v: u64) {
    // SAFETY: see rd64.
    unsafe { (p as *mut u64).write_volatile(v) }
}

/// Map `va` -> a fresh zeroed frame under `t.pml4`, creating PDPT/PD/PT
/// tables on demand and tracking every allocation in the task ledger.
/// Intermediate entries get PRESENT|RW|USER (CPL3 traversal needs U/S
/// at every level); the leaf carries `flags`. Returns the backing
/// frame's physical address. A page already present keeps its frame
/// and ORs in the new permissions — union semantics for overlapping
/// segments.
fn map_page(t: &mut Task, va: u64, flags: u64) -> Option<u64> {
    let mut tab = t.pml4;
    for shift in [39u64, 30, 21] {
        let idx = ((va >> shift) & 0x1ff) as usize;
        let e = rd64(tab + 8 * idx as u64);
        if e & PRESENT != 0 {
            if e & (1 << 7) != 0 {
                return None; // collides with a huge page
            }
            tab = e & PHYS_MASK;
        } else {
            let f = kalloc::alloc_frame()?;
            wr64(tab + 8 * idx as u64, f | PRESENT | WRITE | USER);
            tab = f;
        }
    }
    let idx = ((va >> 12) & 0x1ff) as usize;
    let ent = tab + 8 * idx as u64;
    let e = rd64(ent);
    if e & PRESENT != 0 {
        wr64(ent, (e & PHYS_MASK) | flags | PRESENT);
        return Some(e & PHYS_MASK);
    }
    let f = kalloc::alloc_frame()?;
    wr64(ent, f | flags | PRESENT);
    Some(f)
}

/// Populate `t`'s address space from the ELF image. All header fields
/// are hostile — every extent is checked before use.
fn load_inner(t: &mut Task, e: &elf::Elf, img: &[u8]) -> bool {
    // Copy the kernel PML4 — its entries are supervisor-only, so user
    // code can't reach kernel memory through them even though the
    // mappings are shared.
    let kpml4 = cpu::read_cr3() & !0xfff;
    for i in 0..512u64 {
        let ent = rd64(kpml4 + 8 * i);
        if ent & PRESENT != 0 {
            wr64(t.pml4 + 8 * i, ent);
        }
    }
    for p in e.phdrs(img) {
        if p.ty != elf::PT_LOAD {
            continue;
        }
        // filesz<=memsz, file range inside the image, virtual range
        // inside user space — all checked, checked arithmetic only.
        if p.filesz > p.memsz {
            return false;
        }
        let Some(fend) = p.off.checked_add(p.filesz) else {
            return false;
        };
        if fend > img.len() as u64 {
            return false;
        }
        let Some(mend) = p.va.checked_add(p.memsz) else {
            return false;
        };
        if p.memsz == 0 {
            continue;
        }
        if p.va < elf::USER_VA_LO || mend > elf::USER_VA_HI {
            return false;
        }
        let mut fl = USER;
        if p.flags & elf::PF_W != 0 {
            fl |= WRITE;
        }
        if p.flags & elf::PF_X == 0 {
            fl |= NX;
        }
        // mend < 2^47, so +0xfff is safe after the range check above.
        let va0 = p.va & !0xfff;
        let va1 = (mend + 0xfff) & !0xfff;
        let mut v = va0;
        while v < va1 {
            let Some(frame) = map_page(t, v, fl) else {
                return false;
            };
            // Copy the overlap of this page with [va, va+filesz).
            let lo = core::cmp::max(v, p.va);
            let hi = core::cmp::min(v + PAGE, p.va + p.filesz);
            if lo < hi {
                let src = p.off + (lo - p.va);
                // SAFETY: frame is an owned page in the identity map;
                // n <= PAGE and src..src+n <= img.len() (fend check).
                unsafe {
                    ((frame + (lo - v)) as *mut u8)
                        .copy_from(img.as_ptr().add(src as usize), (hi - lo) as usize);
                }
            }
            v += PAGE;
        }
        if t.nranges < MAX_RANGES {
            t.ranges[t.nranges] = (p.va, mend, p.flags & elf::PF_W != 0);
            t.nranges += 1;
        }
    }
    for i in 0..USTACK_PAGES {
        if map_page(t, USTACK_VA + i * PAGE, USER | WRITE | NX).is_none() {
            return false;
        }
    }
    t.stack_top = USTACK_VA + USTACK_PAGES * PAGE;
    if t.nranges < MAX_RANGES {
        t.ranges[t.nranges] = (USTACK_VA, t.stack_top, true);
        t.nranges += 1;
    }
    true
}

/// Load `img` into a fresh Task. Any malformed input or allocation
/// failure reaps the partial task — nothing half-allocated escapes.
fn load(img: &[u8]) -> Option<Task> {
    let e = elf::parse(img)?;
    let mut t = Task {
        pml4: kalloc::alloc_frame()?,
        entry: e.entry,
        stack_top: 0,
        kstack_base: 0,
        kstack_top: 0,
        ranges: [(0, 0, false); MAX_RANGES],
        nranges: 0,
        fds: {
            const NONE: Option<u8> = None;
            [NONE; MAX_FDS]
        },
        ctx: UCtx::ZERO,
        state: TaskState::Runnable,
        pid: 0,
        ppid: 0,
        exit_code: 0,
        wait_target: 0,
        wait_status_va: 0,
    };
    // std streams: 0=stdin (EOF), 1/2=serial console — each a fresh
    // global OFD, like separate opens of /dev/console.
    {
        let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
        for i in 0..3usize {
            let Some(oi) = ofds.iter().position(Option::is_none) else {
                t.reap();
                return None;
            };
            ofds[oi] = Some(OpenFile {
                kind: if i == 0 { FdKind::Stdin } else { FdKind::Console },
                pos: 0,
                refs: 1,
            });
            t.fds[i] = Some(oi as u8);
        }
    }
    if !load_inner(&mut t, &e, img) {
        t.reap();
        return None;
    }
    let ks = kalloc::kmalloc(KSTACK_SIZE) as u64;
    if ks == 0 {
        t.reap();
        return None;
    }
    t.kstack_base = ks;
    t.kstack_top = ks + KSTACK_SIZE as u64;
    Some(t)
}

/// Is `[va, va+len)` wholly inside a mapped user range? This is THE
/// user-pointer gate for the syscall layer — checked arithmetic only.
/// `need_write` restricts to ranges the kernel may write into.
fn range_check(va: u64, len: u64, need_write: bool) -> bool {
    if len == 0 {
        return false;
    }
    let Some(end) = va.checked_add(len) else {
        return false;
    };
    // SAFETY: read-only walk of the task table.
    let i = unsafe { *core::ptr::addr_of!(CUR_I) };
    let cur = unsafe { (*core::ptr::addr_of!(TASKS)).get(i).and_then(|s| s.as_ref()) };
    cur.is_some_and(|p| {
        p.ranges[..p.nranges]
            .iter()
            .any(|&(lo, hi, w)| va >= lo && end <= hi && (!need_write || w))
    })
}

/// Read-side gate: [va, va+len) inside a mapped user range.
pub fn range_ok(va: u64, len: u64) -> bool {
    range_check(va, len, false)
}

/// Write-side gate: same range check, but the range must be
/// user-writable — the Linux rule for read()-target buffers.
pub fn range_ok_w(va: u64, len: u64) -> bool {
    range_check(va, len, true)
}

// --- fd layer -----------------------------------------------------
// errno values are returned to syscall.rs as negative u64s.
const EBADF: u64 = (-9i64) as u64;
const EFAULT: u64 = (-14i64) as u64;
const ENOENT: u64 = (-2i64) as u64;
const EINVAL: u64 = (-22i64) as u64;
const EMFILE: u64 = (-24i64) as u64;
const ESPIPE: u64 = (-29i64) as u64;
const ENOTDIR: u64 = (-20i64) as u64;
const EISDIR: u64 = (-21i64) as u64;

/// Resolve an fd slot to its OFD in the global table.
fn ofd(t: &mut Task, fd: u64) -> Option<&'static mut OpenFile> {
    let i = *t.fds.get(fd as usize)?.as_ref()?;
    unsafe { (*core::ptr::addr_of_mut!(OFDS)).get_mut(i as usize)?.as_mut() }
}

/// May `fd` be a write() target? Only the console streams.
pub fn fd_is_console(fd: u64) -> bool {
    let Some(t) = cur_mut() else { return false };
    matches!(ofd(t, fd).map(|o| &o.kind), Some(FdKind::Console))
}

/// read(fd, buf, len): File yields up to `len` bytes from its shared
/// position; Stdin yields EOF; Console and closed slots are EBADF.
/// The kernel validates that [buf, buf+n) is user-writable before
/// copying — checked arithmetic, no partial writes past the gate.
pub fn fd_read(fd: u64, buf: u64, len: u64) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let Some(o) = ofd(t, fd) else { return EBADF };
    match &o.kind {
        FdKind::Stdin => 0,
        FdKind::Console => EBADF,
        FdKind::Dir => EISDIR,
        FdKind::File(data) => {
            // pos may sit past EOF after lseek — saturating, not panicking.
            let avail = data.len().saturating_sub(o.pos);
            let n = core::cmp::min(len as usize, avail);
            if n == 0 {
                return 0;
            }
            if !range_ok_w(buf, n as u64) {
                return EFAULT;
            }
            // SAFETY: range_ok_w proved [buf, buf+n) lies in a writable
            // user range under the current (task) page table.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(o.pos),
                    buf as *mut u8,
                    n,
                )
            };
            o.pos += n;
            n as u64
        }
    }
}

/// open(path, O_RDONLY): "" and "." name the archive root (a Dir
/// OFD); anything else resolves through cpio::find. ENOENT when
/// absent, EMFILE when either table is full — deterministic errnos,
/// no allocation growth.
pub fn fd_open(path: &[u8]) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let kind = if path.is_empty() || path == b"." {
        FdKind::Dir
    } else {
        let Some(arc) = (unsafe { *core::ptr::addr_of!(INITRD) }) else {
            return ENOENT;
        };
        let Some(data) = crate::cpio::find(arc, path) else {
            return ENOENT;
        };
        FdKind::File(data)
    };
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    let Some(oi) = ofds.iter().position(Option::is_none) else {
        return EMFILE;
    };
    let Some(fi) = t.fds.iter().position(Option::is_none) else {
        return EMFILE;
    };
    ofds[oi] = Some(OpenFile {
        kind,
        pos: 0,
        refs: 1,
    });
    t.fds[fi] = Some(oi as u8);
    fi as u64
}

/// lseek(fd, off, whence): regular files only — Stdin/Console return
/// ESPIPE like Linux devices. SEEK_SET/CUR/END with signed arithmetic;
/// a negative result is EINVAL, seeking past EOF is legal and reads
/// simply hit EOF (Linux semantics).
pub fn fd_lseek(fd: u64, off: i64, whence: u64) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let Some(o) = ofd(t, fd) else { return EBADF };
    // A directory may only be rewound to the start — dir offsets are
    // opaque cookies, so a nonzero seek target is meaningless here.
    if matches!(o.kind, FdKind::Dir) {
        if whence == 0 && off == 0 {
            o.pos = 0;
            return 0;
        }
        return EINVAL;
    }
    let FdKind::File(data) = &o.kind else {
        return ESPIPE;
    };
    let base = match whence {
        0 => 0i64,                       // SEEK_SET
        1 => o.pos as i64,               // SEEK_CUR
        2 => data.len() as i64,          // SEEK_END
        _ => return EINVAL,
    };
    let Some(np) = base.checked_add(off) else {
        return EINVAL;
    };
    if np < 0 {
        return EINVAL;
    }
    o.pos = np as usize;
    np as u64
}

/// dup(fd): lowest free slot names the same OFD — shared offset,
/// exactly like Linux. EMFILE at the table bound.
pub fn fd_dup(fd: u64) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let Some(&oi) = t.fds.get(fd as usize).and_then(|s| s.as_ref()) else {
        return EBADF;
    };
    let Some(fi) = t.fds.iter().position(Option::is_none) else {
        return EMFILE;
    };
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    if let Some(o) = ofds.get_mut(oi as usize).and_then(|s| s.as_mut()) {
        o.refs += 1;
    }
    t.fds[fi] = Some(oi);
    fi as u64
}

/// dup2(old, new): `new` names old's OFD, silently closing whatever it
/// held first. A huge target can't grow the table — the CVE-2025-39756
/// class is structurally dead here (fixed bound, EBADF).
pub fn fd_dup2(old: u64, new: u64) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let Some(&oi) = t.fds.get(old as usize).and_then(|s| s.as_ref()) else {
        return EBADF;
    };
    if new as usize >= MAX_FDS {
        return EBADF;
    }
    if old == new {
        return new;
    }
    // Release whatever `new` held, then point it at old's OFD.
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    if let Some(prev) = t.fds[new as usize].take() {
        if let Some(o) = ofds.get_mut(prev as usize).and_then(|s| s.as_mut()) {
            o.refs -= 1;
            if o.refs == 0 {
                ofds[prev as usize] = None;
            }
        }
    }
    if let Some(o) = ofds.get_mut(oi as usize).and_then(|s| s.as_mut()) {
        o.refs += 1;
    }
    t.fds[new as usize] = Some(oi);
    new
}

/// close(fd): drops one reference; the OFD dies with its last slot.
/// Linux lets you close std streams, so we do too — the next use
/// gets EBADF, which is correct.
pub fn fd_close(fd: u64) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    let Some(slot) = t.fds.get_mut(fd as usize) else {
        return EBADF;
    };
    let Some(oi) = slot.take() else { return EBADF };
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    if let Some(o) = ofds.get_mut(oi as usize).and_then(|s| s.as_mut()) {
        o.refs -= 1;
        if o.refs == 0 {
            ofds[oi as usize] = None;
        }
    }
    0
}

/// getdents64(fd, dirp, count): emit linux_dirent64 records for the
/// archive entries from `pos` on, filling at most `count` bytes. The
/// whole user buffer is validated writable up front — a record is
/// only ever written inside it, and never partially (each record's
/// reclen is checked against the remaining space before the write).
/// EINVAL when count can't hold even the next record; 0 at the end.
pub fn fd_getdents(fd: u64, buf: u64, count: u64) -> u64 {
    const MAX: u64 = 4096;
    let Some(t) = cur_mut() else { return EBADF };
    let Some(o) = ofd(t, fd) else { return EBADF };
    if !matches!(o.kind, FdKind::Dir) {
        return ENOTDIR;
    }
    if count == 0 || count > MAX {
        return EINVAL;
    }
    if !range_ok_w(buf, count) {
        return EFAULT;
    }
    let Some(arc) = (unsafe { *core::ptr::addr_of!(INITRD) }) else {
        return 0;
    };
    let mut off = 0usize;
    let mut idx = o.pos;
    for (name, _data) in crate::cpio::entries(arc).skip(o.pos) {
        // linux_dirent64: ino@0 u64, off@8 u64, reclen@16 u16,
        // type@18 u8, name@19 NUL-terminated — padded to 8.
        let reclen = (19 + name.len() + 1 + 7) & !7;
        if off + reclen > count as usize {
            if off == 0 {
                return EINVAL; // buffer can't hold even one record
            }
            break;
        }
        let base = buf + off as u64;
        // SAFETY: [buf, buf+count) is validated writable user memory
        // and off+reclen <= count. write_unaligned for the packed
        // fields; the name copy stays inside reclen by construction.
        unsafe {
            ((base) as *mut u64).write_unaligned(idx as u64 + 1);
            ((base + 8) as *mut u64).write_unaligned(idx as u64 + 1);
            ((base + 16) as *mut u16).write_unaligned(reclen as u16);
            ((base + 18) as *mut u8).write(8); // DT_REG
            core::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (base + 19) as *mut u8,
                name.len(),
            );
            ((base + 19 + name.len() as u64) as *mut u8).write(0);
            // Zero the pad tail so kernel stack garbage can't leak.
            let pad_end = off + reclen;
            let mut z = off + 19 + name.len() + 1;
            while z < pad_end {
                ((buf + z as u64) as *mut u8).write(0);
                z += 1;
            }
        }
        off += reclen;
        idx += 1;
    }
    o.pos = idx;
    off as u64
}

/// execve: replace the resident task's image. Frees every user
/// mapping (the SAME pml4/kstack/fds persist — fds surviving exec is
/// the Linux semantic), reloads from `img`, and rewrites the syscall
/// ctx so the return leg lands on the new entry with a clean
/// register file — the same UCtx every switch resumes through. The
/// caller parses the ELF first, so the only post-reset failure is
/// allocator exhaustion — treated as fatal by the caller since the
/// old program's code pages are already gone.
pub fn exec(img: &[u8], ctx: &mut UCtx) -> bool {
    let Some(t) = cur_mut() else { return false };
    let Some(e) = elf::parse(img) else { return false };
    t.free_user_mappings();
    t.nranges = 0;
    t.stack_top = 0;
    if !load_inner(t, &e, img) {
        return false;
    }
    t.entry = e.entry;
    // The pml4 address is unchanged but its contents are entirely
    // new — the TLB still caches the OLD user translations and must
    // be flushed or user code could run on freed frames.
    // SAFETY: reloading the current cr3 only flushes the TLB.
    unsafe { core::arch::asm!("mov {0}, cr3", "mov cr3, {0}", out(reg) _) };
    *ctx = UCtx::launch(t.entry, t.stack_top);
    true
}

// --- scheduler ----------------------------------------------------
// Round-robin over TASKS, driven by the LAPIC timer's dedicated
// dispatch path. Every switch is a UCtx resume; syscall context is
// non-preemptible (IF is masked on entry, and a tick landing on
// kernel CS never switches).

/// Enqueue `img` as a Runnable task — fabricate its launch ctx and
/// take the first free table slot. False on bad image or full table.
pub fn spawn(img: &[u8]) -> bool {
    let Some(mut t) = load(img) else {
        return false;
    };
    t.ctx = UCtx::launch(t.entry, t.stack_top);
    t.pid = next_pid();
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        if s.is_none() {
            *s = Some(t);
            return true;
        }
    }
    t.reap(); // table full — drop the task cleanly
    false
}

/// Reap reapable zombies — Exited tasks whose parent can't collect
/// them (ppid==0 means orphaned, spawned, or already collected) —
/// except the running one (its kstack may be underfoot). Zombies
/// with a living parent persist until wait4 or the parent's exit.
fn pick_next() -> Option<usize> {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    // Snapshot living pids — a zombie is reapable only when no living
    // task can still wait4 it. Blocked counts as living.
    let mut live = [0u64; 16];
    for (i, s) in tasks.iter().enumerate() {
        if let Some(t) = s {
            if t.state != TaskState::Exited {
                live[i] = t.pid;
            }
        }
    }
    for (i, s) in tasks.iter_mut().enumerate() {
        let reapable = s.as_ref().is_some_and(|t| {
            t.state == TaskState::Exited && (t.ppid == 0 || !live.contains(&t.ppid))
        });
        if i != cur && reapable {
            s.as_mut().unwrap().reap();
            *s = None;
        }
    }
    for k in 1..=MAX_TASKS {
        let i = if cur == usize::MAX {
            k - 1
        } else {
            (cur + k) % MAX_TASKS
        };
        if tasks[i].as_ref().is_some_and(|t| t.state == TaskState::Runnable) {
            return Some(i);
        }
    }
    None
}

/// Enter the scheduler: pick the first task and resume its ctx.
/// Returns only when the run queue is empty — the last task's exit
/// unwinds to the kernel context sched_launch saved.
pub fn launch() {
    let Some(n) = pick_next() else {
        return;
    };
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    cpu::init_syscall(syscall::entry_addr());
    let (ctx, pml4, kst) = {
        let t = unsafe { (*core::ptr::addr_of!(TASKS))[n].as_ref().unwrap() };
        (&t.ctx as *const UCtx, t.pml4, t.kstack_top)
    };
    syscall::set_kstack(kst);
    unsafe { syscall::sched_launch(ctx, pml4) };
}

/// Resume the chosen task: set its kstack for syscalls and iret
/// into its saved ctx. Never returns to the caller's context.
fn switch_to(n: usize) -> ! {
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    let (ctx, pml4, kst) = {
        let t = unsafe { (*core::ptr::addr_of!(TASKS))[n].as_ref().unwrap() };
        (&t.ctx as *const UCtx, t.pml4, t.kstack_top)
    };
    syscall::set_kstack(kst);
    unsafe { syscall::sched_iret(ctx, pml4) }
}

/// The running task exited: mark it Exited, orphan its children
/// (ppid=0 -> their zombies reap immediately), and wake a parent
/// blocked in wait4 on it — the child's status goes into the
/// parent's user buffer via physical translation, the pid into its
/// rax. Then switch to the next Runnable task, or unwind to the
/// kernel context sched_launch saved when the queue is empty.
pub fn task_exit(code: u64) -> ! {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let (cpid, cppid) = {
        let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
        if let Some(t) = tasks.get_mut(cur).and_then(|s| s.as_mut()) {
            t.state = TaskState::Exited;
            t.exit_code = code;
            (t.pid, t.ppid)
        } else {
            (0, 0)
        }
    };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    // Wake a wait-blocked parent whose target just exited.
    let mut collected = false;
    for s in tasks.iter_mut() {
        let Some(p) = s else { continue };
        if p.state == TaskState::Blocked
            && p.pid == cppid
            && (p.wait_target == -1 || p.wait_target == cpid as i64)
        {
            // wait4 returns the child's pid; its status was already
            // validated writable when the parent blocked.
            p.ctx.rax = cpid;
            if p.wait_status_va != 0 {
                if let Some(pa) = translate(p.pml4, p.wait_status_va) {
                    // SAFETY: pa is a translated, present user frame.
                    unsafe {
                        ((pa) as *mut u32)
                            .write_unaligned(((code & 0xff) << 8) as u32)
                    };
                }
            }
            p.state = TaskState::Runnable;
            p.wait_target = 0;
            p.wait_status_va = 0;
            collected = true;
        }
    }
    // The parent collected the status — this zombie reaps free.
    if collected {
        if let Some(t) = tasks.get_mut(cur).and_then(|x| x.as_mut()) {
            t.ppid = 0;
        }
    }
    // Orphan cur's children — no one can wait on them now.
    for s in tasks.iter_mut() {
        if let Some(t) = s.as_mut() {
            if t.ppid == cpid && t.state != TaskState::Exited {
                t.ppid = 0;
            }
        }
    }
    if let Some(n) = pick_next() {
        switch_to(n)
    }
    syscall::terminate()
}

/// The running task blocked in wait4: its syscall ctx is already
/// stored in `ctx`. Switch to any Runnable task; if none exists,
/// idle on hlt until a tick re-arms the pick loop.
fn block_switch() -> ! {
    loop {
        if let Some(n) = pick_next() {
            switch_to(n)
        }
        // Nothing runnable — sleep one tick. A child exiting or any
        // wake makes the next pick find a Runnable task.
        // SAFETY: halts with interrupts enabled, masks again after.
        unsafe { core::arch::asm!("sti", "hlt", "cli", options(nostack)) };
    }
}

/// Timer IRQ — called from the sched dispatch stub with the full
/// context image it pushed. A kernel frame (cs&3==0 — mid-syscall,
/// where IF is masked anyway, or any kernel context) just ticks.
/// A CPL3 frame is a preemptible task: snapshot its ctx and round-
/// robin. Returns the ctx the stub should resume.
pub fn tick(img: &mut UCtx) -> *const UCtx {
    if img.cs & 3 != 3 {
        return img;
    }
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if cur == usize::MAX {
        return img;
    }
    // Snapshot the preempted task for a later resume.
    if let Some(t) = unsafe { (*core::ptr::addr_of_mut!(TASKS))[cur].as_mut() } {
        t.ctx = *img;
    }
    let Some(n) = pick_next() else {
        return img;
    };
    if n == cur {
        return img;
    }
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    let t = unsafe { (*core::ptr::addr_of_mut!(TASKS))[n].as_mut().unwrap() };
    syscall::set_kstack(t.kstack_top);
    // Switch address space before returning the new task's ctx —
    // shared kernel slots make the swap safe mid-function.
    // SAFETY: pml4 is a live task-owned PML4.
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) t.pml4) };
    &t.ctx
}

/// Translate a user VA to its identity-mapped physical address under
/// `pml4` — used to write a wait4 status into a BLOCKED task's
/// address space while a different task's tables are active.
fn translate(pml4: u64, va: u64) -> Option<u64> {
    let mut tab = pml4;
    for shift in [39u64, 30, 21] {
        let e = rd64(tab + 8 * ((va >> shift) & 0x1ff));
        if e & PRESENT == 0 || e & (1 << 7) != 0 {
            return None;
        }
        tab = e & PHYS_MASK;
    }
    let e = rd64(tab + 8 * ((va >> 12) & 0x1ff));
    if e & PRESENT == 0 {
        return None;
    }
    Some((e & PHYS_MASK) | (va & 0xfff))
}

/// Deep-copy `ppml4`'s user half (slots 128-255) into `c`'s address
/// space, frame for frame — eager fork semantics (no COW yet; the
/// honest copy is correct and the refcount machinery isn't worth it
/// at this scale). Kernel slots are shared by reference like every
/// load. Returns false on allocation failure — caller reaps `c`.
fn fork_address_space(c: &mut Task, ppml4: u64) -> bool {
    for i in 0..128u64 {
        let e = rd64(ppml4 + 8 * i);
        if e & PRESENT != 0 {
            wr64(c.pml4 + 8 * i, e);
        }
    }
    for i in 128..256u64 {
        let e = rd64(ppml4 + 8 * i);
        if e & PRESENT == 0 {
            continue;
        }
        let Some(cpdpt) = kalloc::alloc_frame() else { return false };
        wr64(c.pml4 + 8 * i, cpdpt | PRESENT | WRITE | USER);
        let pdpt = e & PHYS_MASK;
        for j in 0..512usize {
            let pe = rd64(pdpt + 8 * j as u64);
            if pe & PRESENT == 0 {
                continue;
            }
            if pe & (1 << 7) != 0 {
                return false; // 1G page — never created for user space
            }
            let Some(cpd) = kalloc::alloc_frame() else { return false };
            wr64(cpdpt + 8 * j as u64, cpd | PRESENT | WRITE | USER);
            let pd = pe & PHYS_MASK;
            for k in 0..512usize {
                let te = rd64(pd + 8 * k as u64);
                if te & PRESENT == 0 {
                    continue;
                }
                if te & (1 << 7) != 0 {
                    return false; // 2M page — never created
                }
                let Some(cpt) = kalloc::alloc_frame() else { return false };
                wr64(cpd + 8 * k as u64, cpt | PRESENT | WRITE | USER);
                let pt = te & PHYS_MASK;
                for l in 0..512usize {
                    let le = rd64(pt + 8 * l as u64);
                    if le & PRESENT == 0 {
                        continue;
                    }
                    let Some(nf) = kalloc::alloc_frame() else { return false };
                    // SAFETY: both frames are owned, identity-mapped,
                    // full 4 KiB each — a verbatim copy.
                    unsafe {
                        (nf as *mut u8)
                            .copy_from((le & PHYS_MASK) as *const u8, 4096)
                    };
                    // Leaf flags verbatim — USER/RW/NX preserved.
                    wr64(cpt + 8 * l as u64, nf | (le & !PHYS_MASK));
                }
            }
        }
    }
    true
}

/// fork(): a new task that is a copy of the running one — deep
/// address-space copy, copied fd table pointing at the SAME global
/// OFDs (shared offsets — the Linux semantic), and a ctx equal to
/// the parent's except rax=0. Parent's rax gets the child pid.
pub fn fork(ctx: &mut UCtx) -> u64 {
    const EAGAIN: u64 = (-11i64) as u64;
    const ENOMEM: u64 = (-12i64) as u64;
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    let Some(slot) = tasks.iter().position(Option::is_none) else {
        return EAGAIN;
    };
    let Some(p) = tasks.get_mut(cur).and_then(|s| s.as_mut()) else {
        return EAGAIN;
    };
    let mut c = Task {
        pml4: match kalloc::alloc_frame() {
            Some(f) => f,
            None => return ENOMEM,
        },
        entry: p.entry,
        stack_top: p.stack_top,
        kstack_base: 0,
        kstack_top: 0,
        ranges: p.ranges,
        nranges: p.nranges,
        fds: p.fds,
        ctx: {
            let mut x = *ctx;
            x.rax = 0; // child sees fork()==0
            x
        },
        state: TaskState::Runnable,
        pid: next_pid(),
        ppid: p.pid,
        exit_code: 0,
        wait_target: 0,
        wait_status_va: 0,
    };
    // Either failure frees the (possibly partial) user page-table
    // tree + pml4 — NOT reap(): child's fd slots name OFDs whose refs
    // were never bumped, so release_fds would corrupt the count.
    if !fork_address_space(&mut c, p.pml4) {
        c.free_user_mappings();
        kalloc::free_frame(c.pml4);
        return ENOMEM;
    }
    let ks = kalloc::kmalloc(KSTACK_SIZE) as u64;
    if ks == 0 {
        c.free_user_mappings();
        kalloc::free_frame(c.pml4);
        return ENOMEM;
    }
    c.kstack_base = ks;
    c.kstack_top = ks + KSTACK_SIZE as u64;
    // The child shares the parent's OFDs — bump each referenced one.
    {
        let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
        for s in c.fds.iter().flatten() {
            if let Some(o) = ofds.get_mut(*s as usize).and_then(|x| x.as_mut()) {
                o.refs += 1;
            }
        }
    }
    let cpid = c.pid;
    tasks[slot] = Some(c);
    cpid
}

/// wait4(pid, status_va, flags): collect an exited child's status.
/// pid=-1 waits for any child; status goes through the (validated)
/// user pointer as Linux wait status — exit code in bits 8..15.
/// Blocks when matching children are still running; ECHILD when
/// there are none to wait for.
pub fn wait4(ctx: &mut UCtx, pid: i64, status_va: u64, _flags: u64) -> u64 {
    const ECHILD: u64 = (-10i64) as u64;
    const EFAULT_: u64 = (-14i64) as u64;
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let cpid = unsafe { (*core::ptr::addr_of!(TASKS))[cur].as_ref().map(|t| t.pid).unwrap_or(0) };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    let relevant = |t: &Task| t.ppid == cpid && (pid == -1 || t.pid == pid as u64);
    // A collectable zombie first — write status and free it for reap.
    if let Some(i) = tasks.iter().position(|s| {
        s.as_ref().is_some_and(|t| relevant(t) && t.state == TaskState::Exited)
    }) {
        let t = tasks[i].as_mut().unwrap();
        if status_va != 0 {
            if !range_ok_w(status_va, 4) {
                return EFAULT_;
            }
            // SAFETY: validated writable user range in cur's space.
            unsafe {
                (status_va as *mut u32)
                    .write_unaligned(((t.exit_code & 0xff) << 8) as u32)
            };
        }
        let r = t.pid;
        t.ppid = 0; // collected — the next pick reaps the zombie
        return r;
    }
    if !tasks.iter().flatten().any(relevant) {
        return ECHILD;
    }
    // Live children — validate the status target BEFORE blocking,
    // then sleep: the exiting child's task_exit writes rax + status.
    if status_va != 0 && !range_ok_w(status_va, 4) {
        return EFAULT_;
    }
    {
        let t = tasks[cur].as_mut().unwrap();
        t.wait_target = pid;
        t.wait_status_va = status_va;
        t.ctx = *ctx;
        t.state = TaskState::Blocked;
    }
    block_switch()
}

/// getpid() -> current task's pid.
pub fn getpid() -> u64 {
    cur_mut().map(|t| t.pid).unwrap_or(0)
}

/// getppid() -> current task's parent pid (0 for spawned roots).
pub fn getppid() -> u64 {
    cur_mut().map(|t| t.ppid).unwrap_or(0)
}

/// End of the scheduling run: reap whatever tasks remain — the last
/// Exited task and anything left unrunnable by a failed spawn.
pub fn reap_all() {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        if let Some(t) = s.as_mut() {
            t.reap();
        }
        *s = None;
    }
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = usize::MAX };
}
