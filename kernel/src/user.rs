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
const MAX_TASKS: usize = 32;

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

/// What a Blocked task sleeps on — wait4's child, or a pipe's
/// read/write side. The waker writes the syscall result into
/// `ctx.rax` and marks the task Runnable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitOn {
    None,
    /// wait4(-1): any child of this task.
    Any,
    /// wait4 on a specific child pid.
    Child(u64),
    /// read() on an empty pipe: `wait_ptr/len` is the user buffer.
    PipeRead(usize),
    /// write() on a full pipe: `wait_ptr/len` is the user buffer.
    PipeWrite(usize),
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
    /// One end of a pipe — the ring lives in PIPES[id], shared by
    /// whatever tasks inherited the fds. `write` selects the end.
    Pipe { id: usize, write: bool },
}

/// Max concurrent pipes — a hard bound on kernel frame pressure,
/// same discipline as the fd/OFD tables.
const MAX_PIPES: usize = 8;
/// Ring capacity: one frame per pipe, exactly PIPE_BUF — writes of
/// up to this size are atomic (the Linux promise).
const PIPE_SIZE: usize = 4096;

/// A pipe's shared state: a kernel-owned frame used as a ring with
/// monotonic head/tail (data = tail-head, offset = pos % PIPE_SIZE).
/// Endpoint counts are tracked here, NOT in OFD refs — dup'd fds
/// share one endpoint, so only the LAST close of an end decrements.
struct Pipe {
    /// phys frame, identity-mapped kernel side. Zeroed at alloc and
    /// never aliases file/page-cache memory — the Dirty Pipe
    /// (CVE-2022-0847) uninitialized-flags class doesn't exist here.
    buf: u64,
    head: usize,
    tail: usize,
    readers: usize,
    writers: usize,
}

static mut PIPES: [Option<Pipe>; MAX_PIPES] = [const { None }; MAX_PIPES];

/// The 512-byte fxsave64 image — per-task FPU/SSE state. 16-aligned
/// per the instruction's contract. Every context switch does
/// fxsave(old)/fxrstor(new) so SSE registers stay per-task: without
/// this, one task's xmm writes leak into the next task (NV-55-1).
#[repr(align(16))]
#[derive(Clone, Copy)]
struct FxState([u8; 512]);

/// The architectural fninit+fxsave capture taken once at boot —
/// every fresh task and every exec'd image starts from it.
static mut INIT_FX: FxState = FxState([0; 512]);

/// Capture INIT_FX and arm SSE. Called once before the first task
/// spawns; cpu::init already set CR0.MP + CR4.OSFXSR/OSXMMEXCPT.
pub fn fpu_init() {
    unsafe {
        core::arch::asm!("fninit");
        core::arch::asm!(
            "fxsave64 [{}]",
            in(reg) (*core::ptr::addr_of_mut!(INIT_FX)).0.as_mut_ptr()
        );
    }
}

/// Save the live FPU/SSE registers into the task's buffer.
fn fxsave(t: &mut Task) {
    // SAFETY: t.fx is 16-aligned, 512 bytes — the fxsave64 contract.
    unsafe { core::arch::asm!("fxsave64 [{}]", in(reg) t.fx.0.as_mut_ptr()) }
}

/// Restore a task's FPU/SSE registers — done on every resume so the
/// task sees only its own state.
fn fxrstor(fx: &FxState) {
    // SAFETY: fx is a valid 16-aligned 512-byte fxsave image.
    unsafe { core::arch::asm!("fxrstor64 [{}]", in(reg) fx.0.as_ptr()) }
}

// ---- signals ----

const SIG_DFL: u64 = 0;
const SIG_IGN: u64 = 1;
const SIGCHLD: usize = 17;
const SIGKILL: usize = 9;
const SIGSTOP: usize = 19;
const SIGSEGV: usize = 11;
/// Bytes pushed on the task's user stack per delivered signal:
/// signo (8) + the interrupted UCtx (160).
const SIGFRAME_SIZE: u64 = 168;
// The frame layout is only sound while UCtx stays 20 u64s — a size
// change must grow the frame and both copy sites with it.
const _: () = assert!(core::mem::size_of::<UCtx>() == 160);

/// Signals whose default is to do nothing — job-control and
/// informational signals; everything else kills by default.
fn sig_default_ignore(sig: usize) -> bool {
    matches!(sig, 17 | 18 | 19 | 20 | 23 | 28)
}

/// Write `data` into `pml4`'s user space byte-wise — each byte
/// translated through that task's tables (which may be inactive).
/// Returns false on the first unmapped or unwritable byte.
fn write_task_bytes(pml4: u64, va: u64, data: &[u8]) -> bool {
    for (i, &b) in data.iter().enumerate() {
        let Some(pa) = translate(pml4, va + i as u64) else {
            return false;
        };
        // SAFETY: pa is a present, writable user frame in pml4.
        unsafe { (pa as *mut u8).write(b) };
    }
    true
}

/// Apply one pending signal to `ctx` — the resume image of task `t`.
/// Handler installed: push [signo][interrupted UCtx] onto the task's
/// own stack, redirect rip to the handler, rdi=signo — sigreturn
/// pops it back. Ignore-default signals drop; every other default
/// kills with wait status = bare signo (the Linux wstatus shape).
/// An unwritable stack kills the task like Linux's delivery SEGV.
fn maybe_deliver(t: &mut Task, ctx: &mut UCtx) {
    while t.pending != 0 {
        let sig = t.pending.trailing_zeros() as usize + 1;
        t.pending &= !(1u32 << (sig - 1));
        match t.handlers.get(sig - 1).copied().unwrap_or(SIG_DFL) {
            SIG_IGN => {}
            SIG_DFL => {
                if sig_default_ignore(sig) {
                    continue;
                }
                t.state = TaskState::Exited;
                t.exit_status = sig as u64;
                return;
            }
            handler => {
                let frame = (ctx.rsp.wrapping_sub(SIGFRAME_SIZE)) & !15;
                let sig64 = sig as u64;
                // SAFETY: reinterprets the UCtx as its repr(C) bytes —
                // written verbatim into the task's own stack.
                let raw = unsafe {
                    core::slice::from_raw_parts(ctx as *const UCtx as *const u8, 160)
                };
                let ok = write_task_bytes(t.pml4, frame, &sig64.to_le_bytes())
                    && write_task_bytes(t.pml4, frame + 8, raw);
                if !ok {
                    t.state = TaskState::Exited;
                    t.exit_status = SIGSEGV as u64;
                    return;
                }
                ctx.rsp = frame;
                ctx.rip = handler;
                ctx.rdi = sig64;
                return; // one signal per resume, Linux-style
            }
        }
    }
}

/// A user-mode CPU fault whose signal is CAUGHT becomes real signal
/// delivery: cpu.rs rebuilt the interrupted UCtx from the trap
/// stubs' saved registers, we pend the signo and run it through the
/// same maybe_deliver frame-push/redirect a kill() would get. The
/// returned (rip, rsp, rdi) is spliced into the trap frame so the
/// exception iretq lands in the handler — sigreturn later restores
/// the fault-time context (re-faulting if unfixed, exactly like
/// Linux). A default or ignored disposition dies synchronously —
/// IGN on a synchronous fault can't resume (Linux force_sig).
pub fn fault_signal(signo: u64, ctx: &mut UCtx) -> (u64, u64, u64) {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    if let Some(t) = tasks.get_mut(cur).and_then(|s| s.as_mut()) {
        let caught = t
            .handlers
            .get(signo.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(SIG_DFL)
            > SIG_IGN;
        if caught {
            t.pending |= 1u32 << (signo - 1).min(31);
            maybe_deliver(t, ctx);
            if t.state == TaskState::Runnable {
                return (ctx.rip, ctx.rsp, ctx.rdi);
            }
            // Frame push failed — maybe_deliver already set
            // exit_status=SIGSEGV.
        } else {
            // Default-fatal or IGN on a synchronous fault — die with
            // the bare signo (Linux force_sig semantics).
            t.exit_status = signo;
        }
    }
    task_exit_inner()
}

/// Deliver pending signals into task n's saved ctx for a pick-time
/// resume; false when the delivery killed it (caller re-picks).
fn deliver_for(n: usize) -> bool {
    let t = unsafe { (*core::ptr::addr_of_mut!(TASKS))[n].as_mut().unwrap() };
    if t.pending != 0 {
        let mut c = t.ctx;
        maybe_deliver(t, &mut c);
        t.ctx = c;
    }
    t.state == TaskState::Runnable
}

/// pick_next + signal delivery: the chosen task's ctx is rewritten
/// before resume — a default-fatal signal marks it Exited and the
/// loop picks again.
fn pick_ready() -> Option<usize> {
    loop {
        let n = pick_next()?;
        if deliver_for(n) {
            return Some(n);
        }
    }
}

/// At the end of a syscall, apply pending signals to the return ctx.
/// A handler redirects the resume image; a default-fatal signal
/// dies exactly like exit() — this never returns to the dead task.
pub fn deliver_current(ctx: &mut UCtx) {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if cur == usize::MAX {
        return;
    }
    let dead = {
        let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
        match tasks.get_mut(cur).and_then(|s| s.as_mut()) {
            Some(t) => {
                maybe_deliver(t, ctx);
                t.state == TaskState::Exited
            }
            None => false,
        }
    };
    if dead {
        task_exit_inner() // exit_status already carries the signo
    }
}

/// kill(pid, sig): pend a signal on the target. A blocked target
/// wakes with -EINTR — the interrupted-syscall semantic (its saved
/// ctx then delivers the handler on resume). Zombie targets accept
/// the signal silently like Linux. ESRCH when no such pid.
pub fn kill(pid: u64, sig: u64) -> u64 {
    const ESRCH: u64 = (-3i64) as u64;
    if sig == 0 || sig > 31 {
        return EINVAL;
    }
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    let Some(t) = tasks.iter_mut().flatten().find(|t| t.pid == pid) else {
        return ESRCH;
    };
    if t.state == TaskState::Exited {
        return 0;
    }
    t.pending |= 1u32 << (sig - 1);
    // Wake a blocked target only when the signal actually interrupts:
    // an ignored disposition (SIG_IGN, or a default-ignore signal)
    // must not turn a sleeping syscall into a spurious EINTR.
    let h = t.handlers.get(sig as usize - 1).copied().unwrap_or(SIG_DFL);
    let ignored = h == SIG_IGN || (h == SIG_DFL && sig_default_ignore(sig as usize));
    if t.state == TaskState::Blocked && !ignored {
        t.ctx.rax = (-4i64) as u64; // -EINTR
        t.wait_on = WaitOn::None;
        t.wait_ptr = 0;
        t.wait_len = 0;
        t.state = TaskState::Runnable;
    }
    0
}

/// rt_sigaction(sig, act, oldact): read/swap the disposition word —
/// 0=SIG_DFL, 1=SIG_IGN, else a user handler VA (validated at
/// delivery, like Linux). SIGKILL/SIGSTOP stay uncatchable.
pub fn sigaction(sig: u64, act: u64, oldact: u64) -> u64 {
    if sig == 0 || sig > 31 {
        return EINVAL;
    }
    let Some(t) = cur_mut() else { return EINVAL };
    let i = (sig - 1) as usize;
    if oldact != 0 {
        if !range_ok_w(oldact, 8) {
            return EFAULT;
        }
        // SAFETY: validated writable user range.
        unsafe { (oldact as *mut u64).write_unaligned(t.handlers[i]) };
    }
    if act != 0 {
        if !range_ok(act, 8) {
            return EFAULT;
        }
        // SAFETY: validated readable user range.
        let h = unsafe { (act as *const u64).read_unaligned() };
        // SIGKILL/SIGSTOP refuse every disposition change — a task
        // that could ignore SIGKILL would be unkillable by signal.
        if sig as usize == SIGKILL || sig as usize == SIGSTOP {
            return EINVAL;
        }
        t.handlers[i] = h;
    }
    0
}

/// rt_sigreturn: restore the interrupted ctx the delivery pushed at
/// the caller's rsp — [signo][UCtx]. Every byte is user-owned, so
/// the privilege fields are pinned, not trusted — cs=0x33, ss=0x2b,
/// rip inside a user range, rsp inside a writable one, TF/NT/IOPL
/// cleared (the CVE-2014-9322 / SROP hardening). A bad frame kills
/// with SIGSEGV, like Linux.
pub fn sigreturn(ctx: &mut UCtx) -> u64 {
    let frame = ctx.rsp;
    if !range_ok(frame, SIGFRAME_SIZE) {
        die_signal(SIGSEGV as u64);
    }
    let mut saved = UCtx::ZERO;
    // SAFETY: validated user range in the current (caller's) space.
    unsafe {
        core::ptr::copy_nonoverlapping(
            (frame + 8) as *const u8,
            &mut saved as *mut UCtx as *mut u8,
            160,
        )
    };
    saved.cs = 0x33;
    saved.ss = 0x2b;
    saved.rflags = (saved.rflags | 0x2) & !(0x100 | 0x3000 | 0x4000);
    // rsp == stack_top is legal (empty stack): require the byte
    // BELOW rsp — the first push target — to be writable user space.
    if saved.rip >= 0x8000_0000_0000
        || saved.rsp >= 0x8000_0000_0000
        || !range_ok(saved.rip, 1)
        || !range_ok_w(saved.rsp.wrapping_sub(1), 1)
    {
        die_signal(SIGSEGV as u64);
    }
    *ctx = saved;
    ctx.rax // restored rax IS the return value
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
    /// The wait status wait4 reports: (exit_code << 8) normally, or
    /// the raw signo in the low byte for a signal death — the real
    /// Linux wstatus layout.
    exit_status: u64,
    /// What this task is Blocked on (None when not Blocked).
    wait_on: WaitOn,
    /// wait4 status VA, or the user buffer for a blocked pipe io.
    wait_ptr: u64,
    /// Blocked pipe io length (unused by wait4).
    wait_len: u64,
    /// Pending signal bitmask — bit (sig-1), sigs 1..=31.
    pending: u32,
    /// Per-signal disposition: 0=SIG_DFL, 1=SIG_IGN, else a user VA
    /// handler. SIGKILL/SIGSTOP can't be overridden (EINVAL).
    handlers: [u64; 31],
    /// Per-task FPU/SSE state -- fxsave'd on every way out,
    /// fxrstor'd on every resume so xmm state never crosses tasks.
    fx: FxState,
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
        for s in self.fds.iter_mut() {
            if let Some(i) = s.take() {
                ofd_release(i as usize);
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
        exit_status: 0,
        wait_on: WaitOn::None,
        wait_ptr: 0,
        wait_len: 0,
        pending: 0,
        handlers: [0; 31],
        // SAFETY: read of the init-time capture.
        fx: unsafe { *core::ptr::addr_of!(INIT_FX) },
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
pub fn fd_read(ctx: &mut UCtx, fd: u64, buf: u64, len: u64) -> u64 {
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
        FdKind::Pipe { id, write } => {
            let (id, write) = (*id, *write);
            if write {
                return EBADF; // wrong end
            }
            let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
            let Some(p) = pipes.get_mut(id).and_then(|s| s.as_mut()) else {
                return EBADF;
            };
            let avail = p.tail - p.head;
            if avail == 0 {
                if p.writers == 0 {
                    return 0; // EOF — every writer is gone
                }
                if len == 0 {
                    return 0;
                }
                // Block until a writer delivers. The waker copies
                // through OUR tables and deposits the count in
                // ctx.rax; wait_ptr's validity was verified now.
                if !range_ok_w(buf, 1) {
                    return EFAULT;
                }
                if t.pending != 0 {
                    return (-4i64) as u64; // -EINTR: signal before sleep
                }
                t.wait_on = WaitOn::PipeRead(id);
                t.wait_ptr = buf;
                t.wait_len = len;
                t.ctx = *ctx;
                t.state = TaskState::Blocked;
                block_switch()
            }
            let n = core::cmp::min(len as usize, avail);
            if !range_ok_w(buf, n as u64) {
                return EFAULT;
            }
            for i in 0..n {
                // SAFETY: ring index wraps inside the pipe-owned frame;
                // buf is validated user memory under the current cr3.
                unsafe {
                    let b = ((p.buf + ((p.head + i) % PIPE_SIZE) as u64) as *const u8).read();
                    ((buf + i as u64) as *mut u8).write(b);
                }
            }
            p.head += n;
            pipe_wake(id); // freed space may unblock writers
            n as u64
        }
    }
}

/// write(fd, buf, len) for non-console fds — pipes only; everything
/// else (initrd files are read-only, dirs, stdin, read-end) is EBADF.
/// Blocks when the ring can't take the write atomically.
pub fn fd_write(ctx: &mut UCtx, fd: u64, buf: u64, len: u64) -> u64 {
    const EPIPE: u64 = (-32i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    let Some(o) = ofd(t, fd) else { return EBADF };
    let FdKind::Pipe { id, write } = &o.kind else {
        return EBADF;
    };
    let (id, write) = (*id, *write);
    if !write {
        return EBADF; // wrong end
    }
    if len > PIPE_SIZE as u64 {
        return EINVAL; // single-frame ring; larger writes rejected
    }
    let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
    let Some(p) = pipes.get_mut(id).and_then(|s| s.as_mut()) else {
        return EBADF;
    };
    if p.readers == 0 {
        return EPIPE; // SIGPIPE would fire here once signals exist
    }
    if PIPE_SIZE - (p.tail - p.head) < len as usize {
        // Doesn't fit — <=PIPE_BUF writes are atomic, so block until
        // the ring drains enough. The waker pulls our buffer through
        // OUR tables and deposits the count in ctx.rax.
        if len > 0 && !range_ok(buf, 1) {
            return EFAULT;
        }
        if t.pending != 0 {
            return (-4i64) as u64; // -EINTR: signal before sleep
        }
        t.wait_on = WaitOn::PipeWrite(id);
        t.wait_ptr = buf;
        t.wait_len = len;
        t.ctx = *ctx;
        t.state = TaskState::Blocked;
        block_switch()
    }
    if !range_ok(buf, len) {
        return EFAULT;
    }
    for i in 0..len as usize {
        // SAFETY: buf is validated user memory under the current cr3;
        // the ring index wraps inside the pipe-owned frame.
        unsafe {
            let b = ((buf + i as u64) as *const u8).read();
            ((p.buf + ((p.tail + i) % PIPE_SIZE) as u64) as *mut u8).write(b);
        }
    }
    p.tail += len as usize;
    pipe_wake(id); // new data may unblock readers
    len
}

/// pipe2(fds, flags): a kernel ring with a read end and a write end,
/// each an OFD so the pair can be dup'd and fork-shared. flags must
/// be 0 (no O_NONBLOCK/O_CLOEXEC yet — flagged NV). ENFILE at the
/// pool bound, EMFILE at the fd/OFD bounds.
pub fn fd_pipe(fdptr: u64, flags: u64) -> u64 {
    const ENFILE_: u64 = (-23i64) as u64;
    if flags != 0 {
        return EINVAL;
    }
    if !range_ok_w(fdptr, 8) {
        return EFAULT;
    }
    let Some(t) = cur_mut() else { return EBADF };
    let Some(pi) = (unsafe { &*core::ptr::addr_of!(PIPES) })
        .iter()
        .position(Option::is_none)
    else {
        return ENFILE_;
    };
    let Some(buf) = kalloc::alloc_frame() else {
        return (-12i64) as u64;
    };
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    // Need two OFD slots and two fd slots — collect before committing.
    let mut ois = ofds
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_none())
        .map(|(i, _)| i);
    let (Some(ro), Some(wo)) = (ois.next(), ois.next()) else {
        kalloc::free_frame(buf);
        return EMFILE;
    };
    let mut fis = t
        .fds
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_none())
        .map(|(i, _)| i);
    let (Some(rf), Some(wf)) = (fis.next(), fis.next()) else {
        kalloc::free_frame(buf);
        return EMFILE;
    };
    {
        let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
        pipes[pi] = Some(Pipe {
            buf,
            head: 0,
            tail: 0,
            readers: 1,
            writers: 1,
        });
    }
    ofds[ro] = Some(OpenFile {
        kind: FdKind::Pipe { id: pi, write: false },
        pos: 0,
        refs: 1,
    });
    ofds[wo] = Some(OpenFile {
        kind: FdKind::Pipe { id: pi, write: true },
        pos: 0,
        refs: 1,
    });
    t.fds[rf] = Some(ro as u8);
    t.fds[wf] = Some(wo as u8);
    // SAFETY: fdptr validated writable for 8 bytes above.
    unsafe {
        (fdptr as *mut u32).write_unaligned(rf as u32);
        ((fdptr + 4) as *mut u32).write_unaligned(wf as u32);
    }
    0
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
    if let Some(prev) = t.fds[new as usize].take() {
        ofd_release(prev as usize);
    }
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
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
    ofd_release(oi as usize);
    0
}

/// Drop one reference on OFD `oi`; at zero, retire it — pipe ends
/// then run endpoint accounting (a dup'd end only counts once).
fn ofd_release(oi: usize) {
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    let Some(o) = ofds.get_mut(oi).and_then(|s| s.as_mut()) else {
        return;
    };
    o.refs -= 1;
    if o.refs > 0 {
        return;
    }
    // Take the kind out before clearing so endpoint teardown runs
    // after the OFD is already gone.
    let kind = core::mem::replace(&mut o.kind, FdKind::Stdin);
    ofds[oi] = None;
    if let FdKind::Pipe { id, write } = kind {
        pipe_end_closed(id, write);
    }
}

/// The last fd naming one pipe end went away. Decrement the endpoint
/// count; free the ring when both ends are gone; otherwise wake the
/// opposite side — writers see EPIPE with no readers left, readers
/// see EOF (0) when the writers are all gone.
fn pipe_end_closed(id: usize, write: bool) {
    let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
    let Some(p) = pipes.get_mut(id).and_then(|s| s.as_mut()) else {
        return;
    };
    if write {
        p.writers -= 1;
    } else {
        p.readers -= 1;
    }
    let gone = if write { p.writers == 0 } else { p.readers == 0 };
    if p.readers == 0 && p.writers == 0 {
        kalloc::free_frame(p.buf);
        pipes[id] = None;
        return;
    }
    if gone {
        // EOF to readers / EPIPE to writers.
        wake_pipe_waiters(
            if write {
                WaitOn::PipeRead(id)
            } else {
                WaitOn::PipeWrite(id)
            },
            if write { 0 } else { (-32i64) as u64 },
        );
    }
}

/// Wake every task blocked on `on` with `ret` as its syscall result
/// — the endpoint-teardown wake (EOF/EPIPE), no data moves.
fn wake_pipe_waiters(on: WaitOn, ret: u64) {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        if let Some(t) = s.as_mut() {
            if t.state == TaskState::Blocked && t.wait_on == on {
                t.ctx.rax = ret;
                t.wait_on = WaitOn::None;
                t.state = TaskState::Runnable;
            }
        }
    }
}

/// Feed blocked readers from pipe `id`'s ring, greedily: each waiter
/// gets min(its len, available) bytes copied through its own page
/// tables (per-byte translate — the sleeping task's space isn't
/// mapped under the waker's cr3). Returns how many tasks woke.
fn wake_pipe_readers(id: usize) -> usize {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
    let mut woke = 0;
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state != TaskState::Blocked || t.wait_on != WaitOn::PipeRead(id) {
            continue;
        }
        let Some(p) = pipes.get_mut(id).and_then(|x| x.as_mut()) else {
            break;
        };
        let avail = p.tail - p.head;
        if avail == 0 {
            break; // nothing left to hand out
        }
        let n = core::cmp::min(t.wait_len as usize, avail);
        let mut done = 0usize;
        while done < n {
            // SAFETY: ring frame is pipe-owned, index wraps in bounds.
            let b = unsafe {
                ((p.buf + ((p.head + done) % PIPE_SIZE) as u64) as *const u8).read()
            };
            match translate(t.pml4, t.wait_ptr + done as u64) {
                // SAFETY: pa is a present user frame in t's tables.
                Some(pa) => unsafe { (pa as *mut u8).write(b) },
                None => break,
            }
            done += 1;
        }
        p.head += done;
        t.ctx.rax = if done == 0 { (-14i64) as u64 } else { done as u64 };
        t.wait_on = WaitOn::None;
        t.state = TaskState::Runnable;
        woke += 1;
    }
    woke
}

/// Drain blocked writers into pipe `id`'s ring. A writer unblocks
/// only when its whole len fits — writes <= PIPE_BUF are atomic,
/// and len is capped at the frame size, so full-space means empty.
/// Returns how many tasks woke.
fn wake_pipe_writers(id: usize) -> usize {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    let pipes = unsafe { &mut *core::ptr::addr_of_mut!(PIPES) };
    let mut woke = 0;
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state != TaskState::Blocked || t.wait_on != WaitOn::PipeWrite(id) {
            continue;
        }
        let Some(p) = pipes.get_mut(id).and_then(|x| x.as_mut()) else {
            break;
        };
        if PIPE_SIZE - (p.tail - p.head) < t.wait_len as usize {
            continue; // still doesn't fit — stays blocked
        }
        let len = t.wait_len as usize;
        let mut done = 0usize;
        while done < len {
            match translate(t.pml4, t.wait_ptr + done as u64) {
                // SAFETY: pa is a present user frame in t's tables;
                // the ring index wraps inside our frame.
                Some(pa) => unsafe {
                    let b = (pa as *const u8).read();
                    ((p.buf + ((p.tail + done) % PIPE_SIZE) as u64) as *mut u8)
                        .write(b)
                },
                None => break,
            }
            done += 1;
        }
        p.tail += done;
        t.ctx.rax = if done == 0 { (-14i64) as u64 } else { done as u64 };
        t.wait_on = WaitOn::None;
        t.state = TaskState::Runnable;
        woke += 1;
    }
    woke
}

/// After a pipe's fill level changes, settle both wait directions.
/// A woken reader frees space (writers may fit); a woken writer adds
/// data (more readers may be fed). Every wake clears wait_on, so
/// this loop is monotonically terminating within MAX_TASKS.
fn pipe_wake(id: usize) {
    loop {
        if wake_pipe_readers(id) + wake_pipe_writers(id) == 0 {
            break;
        }
    }
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
    // exec resets caught handlers to default (SIG_IGN survives) —
    // a handler in the OLD image would be unmapped garbage here.
    for h in t.handlers.iter_mut() {
        if *h != SIG_IGN {
            *h = SIG_DFL;
        }
    }
    // SAFETY: read of the init-time capture -- exec resets FPU too.
    t.fx = unsafe { *core::ptr::addr_of!(INIT_FX) };
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
    let mut live = [0u64; MAX_TASKS];
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
    let Some(n) = pick_ready() else {
        return;
    };
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    cpu::init_syscall(syscall::entry_addr());
    let (ctx, pml4, kst) = {
        let t = unsafe { (*core::ptr::addr_of!(TASKS))[n].as_ref().unwrap() };
        fxrstor(&t.fx);
        (&t.ctx as *const UCtx, t.pml4, t.kstack_top)
    };
    syscall::set_kstack(kst);
    unsafe { syscall::sched_launch(ctx, pml4) };
}

/// Resume the chosen task: set its kstack for syscalls and iret
/// into its saved ctx. Never returns to the caller's context.
fn switch_to(n: usize) -> ! {
    // The outgoing task's FPU state is live in the registers --
    // save it before restoring n's (a dead task's save is harmless).
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if let Some(t) = unsafe { (*core::ptr::addr_of_mut!(TASKS))[cur].as_mut() } {
        fxsave(t);
    }
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    let (ctx, pml4, kst) = {
        let t = unsafe { (*core::ptr::addr_of!(TASKS))[n].as_ref().unwrap() };
        fxrstor(&t.fx);
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
    if let Some(t) = unsafe { (*core::ptr::addr_of_mut!(TASKS))[cur].as_mut() } {
        t.exit_status = (code & 0xff) << 8;
    }
    task_exit_inner()
}

/// The running task died to a signal: same teardown as exit, but the
/// wait status is the bare signo in the low byte — real wstatus.
fn die_signal(signo: u64) -> ! {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if let Some(t) = unsafe { (*core::ptr::addr_of_mut!(TASKS))[cur].as_mut() } {
        t.exit_status = signo;
    }
    task_exit_inner()
}

/// Shared exit/signal-death teardown: mark Exited, wake a matching
/// wait4 parent (status + pid), SIGCHLD the parent if it lives,
/// orphan children, then switch away or unwind.
fn task_exit_inner() -> ! {
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    let (cpid, cppid, wstatus) = {
        let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
        if let Some(t) = tasks.get_mut(cur).and_then(|s| s.as_mut()) {
            t.state = TaskState::Exited;
            (t.pid, t.ppid, t.exit_status)
        } else {
            (0, 0, 0)
        }
    };
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    // Wake a wait-blocked parent whose target just exited.
    let mut collected = false;
    for s in tasks.iter_mut() {
        let Some(p) = s else { continue };
        if p.state == TaskState::Blocked
            && p.pid == cppid
            && (p.wait_on == WaitOn::Any || p.wait_on == WaitOn::Child(cpid))
        {
            // wait4 returns the child's pid; its status was already
            // validated writable when the parent blocked.
            p.ctx.rax = cpid;
            if p.wait_ptr != 0 {
                if let Some(pa) = translate(p.pml4, p.wait_ptr) {
                    // SAFETY: pa is a translated, present user frame.
                    unsafe {
                        ((pa) as *mut u32)
                            .write_unaligned((wstatus & 0xffff) as u32)
                    };
                }
            }
            p.state = TaskState::Runnable;
            p.wait_on = WaitOn::None;
            p.wait_ptr = 0;
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
    // SIGCHLD to the parent — pending only; default disposition
    // ignores it, a handler sees it on the parent's next resume.
    if cppid != 0 {
        if let Some(p) = tasks.iter_mut().flatten().find(|t| t.pid == cppid) {
            p.pending |= 1 << (SIGCHLD - 1);
        }
    }
    if let Some(n) = pick_ready() {
        switch_to(n)
    }
    syscall::terminate()
}

/// The running task blocked in wait4: its syscall ctx is already
/// stored in `ctx`. Switch to any Runnable task; if none exists,
/// idle on hlt until a tick re-arms the pick loop.
fn block_switch() -> ! {
    loop {
        if let Some(n) = pick_ready() {
            switch_to(n)
        }
        // Nothing runnable — sleep one tick. A child exiting or any
        // wake makes the next pick find a Runnable task.
        // SAFETY: halts with interrupts enabled, masks again after.
        unsafe { core::arch::asm!("sti", "hlt", "cli", options(nostack)) };
    }
}

/// Resume task n from the IRQ path: set kstack + cr3 and return its
/// ctx for the dispatch stub to pop+iret.
fn resume_ctx(n: usize) -> *const UCtx {
    unsafe { *core::ptr::addr_of_mut!(CUR_I) = n };
    let t = unsafe { (*core::ptr::addr_of_mut!(TASKS))[n].as_mut().unwrap() };
    syscall::set_kstack(t.kstack_top);
    // Switch address space before returning the new task's ctx —
    // shared kernel slots make the swap safe mid-function.
    // SAFETY: pml4 is a live task-owned PML4.
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) t.pml4) };
    &t.ctx
}

/// Timer IRQ — called from the sched dispatch stub with the full
/// context image it pushed. A kernel frame (cs&3==0 — mid-syscall,
/// where IF is masked anyway, or any kernel context) just ticks.
/// A CPL3 frame is a preemptible task: snapshot its ctx, deliver
/// pending signals (a default-fatal one kills it here), round-robin
/// with delivery at the pick, and return the ctx to resume.
pub fn tick(img: &mut UCtx) -> *const UCtx {
    if img.cs & 3 != 3 {
        return img;
    }
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if cur == usize::MAX {
        return img;
    }
    // Snapshot the preempted task for a later resume -- GPRs into
    // ctx, the live FPU/SSE registers into its fx buffer.
    if let Some(t) = unsafe { (*core::ptr::addr_of_mut!(TASKS))[cur].as_mut() } {
        t.ctx = *img;
        fxsave(t);
    }
    deliver_for(cur);
    let Some(n) = pick_ready() else {
        // Nothing else runnable: if cur survived delivery it's the
        // only task — resume its (possibly redirected) ctx. If a
        // default-fatal signal killed it, the queue is empty —
        // unwind to the kernel frame like the last exit does.
        let tasks = unsafe { &*core::ptr::addr_of!(TASKS) };
        match tasks[cur].as_ref() {
            Some(t) if t.state == TaskState::Runnable => return &t.ctx,
            _ => syscall::terminate(),
        }
    };
    if n == cur {
        let tasks = unsafe { &*core::ptr::addr_of!(TASKS) };
        return &tasks[cur].as_ref().unwrap().ctx;
    }
    resume_ctx(n)
}

/// Translate a user VA to its identity-mapped physical address under
/// `pml4` — used to write into another task's address space while a
/// different task's tables are active (wait4 status, pipe io, signal
/// frames). Requires the leaf's WRITE bit: every caller writes, and
/// a read-only page must not be modified through this path.
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
    if e & PRESENT == 0 || e & WRITE == 0 {
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
        exit_status: 0,
        wait_on: WaitOn::None,
        wait_ptr: 0,
        wait_len: 0,
        // Dispositions inherit across fork (the Linux semantic);
        // the pending set does not.
        pending: 0,
        handlers: p.handlers,
        fx: p.fx,
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
                    .write_unaligned((t.exit_status & 0xffff) as u32)
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
        if t.pending != 0 {
            return (-4i64) as u64; // -EINTR: signal before sleep
        }
        t.wait_on = if pid == -1 { WaitOn::Any } else { WaitOn::Child(pid as u64) };
        t.wait_ptr = status_va;
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
