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
/// mmap's first-fit arena — 4 TiB of user window kept free of the
/// ELF image (~0x4000…) and the stack (0x5556…).
const MMAP_LO: u64 = 0x6000_0000_0000;
const MMAP_HI: u64 = 0x7000_0000_0000;
const USTACK_PAGES: u64 = 16;

/// Per-task kernel stack for SYSCALL entries — replaces the shared
/// static stack so a task's kernel state can't bleed into the next.
const KSTACK_SIZE: usize = 16384;

/// Maximum distinct user VA ranges recorded for syscall validation —
/// the vm_max_map_count analog: mmap/mprotect splits consume slots,
/// brk/ELF/stack hold permanent ones. Overflow fails the op (ENOMEM),
/// never silently maps unvalidated VA.
const MAX_RANGES: usize = 32;

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
    /// poll(): `wait_ptr` is the user pollfd array, `wait_len` the
    /// fd count. Any pipe-state change re-scans it; wait_deadline
    /// ticks (0 = none) bounds the sleep.
    Poll,
    /// recvfrom() on an empty UDP socket: `wait_ptr` is the user
    /// buffer, `wait_len` its cap, `wait_ptr2` the sockaddr out ptr.
    UdpRecv(usize),
    /// nanosleep(): pure deadline wait — wait_deadline bounds it,
    /// the return is always 0.
    Timer,
    /// connect() awaiting SYN-ACK; wait_deadline bounds it.
    TcpConn(usize),
    /// accept() awaiting an established child on the listener.
    TcpAccept(usize),
    /// recv/read on an empty TCP stream: `wait_ptr/len` the buffer,
    /// `wait_ptr2` the optional peer sockaddr out ptr.
    TcpRecv(usize),
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
    /// An AF_INET/SOCK_DGRAM socket — the state lives in UDPSOCKS[id],
    /// dup'd/fork'd fds share the one endpoint like pipe ends.
    Udp(usize),
    /// An AF_INET/SOCK_STREAM endpoint — listener or connection —
    /// the state machine lives in TCPSOCKS[id].
    Tcp(usize),
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
/// Lowest-numbered deliverable signal: pending AND unmasked. SIGKILL
/// is always deliverable — the mask can't stop it (Linux semantics:
/// sig_kernel_only signals ignore the blocked set on delivery).
#[inline]
fn unmasked_pending(t: &Task) -> Option<usize> {
    let unmasked = t.pending & (!t.sigmask | (1u32 << (SIGKILL - 1)));
    (unmasked != 0).then(|| unmasked.trailing_zeros() as usize + 1)
}

/// A signal that can be delivered NOW — pending and unmasked.
#[inline]
fn deliverable(t: &Task) -> bool {
    unmasked_pending(t).is_some()
}

fn maybe_deliver(t: &mut Task, ctx: &mut UCtx) {
    while let Some(sig) = unmasked_pending(t) {
        t.pending &= !(1u32 << (sig - 1));
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
    if deliverable(t) {
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
        // A nanosleep interrupted by this signal reports the unslept
        // remainder in rem (stored in wait_ptr) — Linux semantics.
        let mut rax: u64 = (-4i64) as u64; // -EINTR
        if matches!(t.wait_on, WaitOn::Timer) {
            if t.wait_ptr != 0 {
                let now = crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed);
                let left = t.wait_deadline.saturating_sub(now);
                let rem = t.wait_ptr;
                write_task_bytes(t.pml4, rem, &(left / 100).to_le_bytes());
                write_task_bytes(t.pml4, rem + 8, &((left % 100) * 10_000_000).to_le_bytes());
            }
            // clock_nanosleep's ABI returns EINTR positive.
            if t.wait_ptr2 != 0 {
                rax = 4;
            }
        }
        t.ctx.rax = rax;
        t.wait_on = WaitOn::None;
        t.wait_ptr = 0;
        t.wait_len = 0;
        t.wait_deadline = 0;
        t.wait_ptr2 = 0;
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
    /// [lo, hi) address ranges user pointers may legally reference,
    /// with access bits (RNG_R=1 / RNG_W=2). The syscall layer's
    /// user-pointer gate walks exactly this table — PROT_NONE entries
    /// (flags 0) reserve VA but fail both gates, like Linux EFAULT.
    ranges: [(u64, u64, u8); MAX_RANGES],
    nranges: usize,
    /// Program break bookkeeping: brk_base is the aligned end of the
    /// loaded image (heap start), brk_cur the current break in bytes
    /// (page granularity underneath, like Linux VMAs).
    brk_base: u64,
    brk_cur: u64,
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
    /// Second wait pointer — recvfrom's sockaddr out VA (0 = none).
    wait_ptr2: u64,
    /// apic::TICKS deadline for a timed Poll wait — 0 means none.
    wait_deadline: u64,
    /// Pending signal bitmask — bit (sig-1), sigs 1..=31.
    pending: u32,
    /// rt_sigprocmask mask — set bits block delivery of the matching
    /// pending signal (masked-pending stays pending for later). Fork
    /// inherits it, exec preserves it. The mask may contain SIGKILL's
    /// bit like Linux, but delivery ignores it — see unmasked_pending.
    sigmask: u32,
    /// Per-signal disposition: 0=SIG_DFL, 1=SIG_IGN, else a user VA
    /// handler. SIGKILL/SIGSTOP can't be overridden (EINVAL).
    handlers: [u64; 31],
    /// The image's cpio name — /proc/self/exe resolves to it. Copied
    /// at spawn, inherited at fork, replaced at exec.
    name: [u8; 16],
    /// The task's user FS base (TLS pointer) — set via arch_prctl,
    /// restored to MSR_FS_BASE on every switch like the fx image.
    /// Users can't desync it: FSGSBASE is never enabled.
    fs_base: u64,
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
/// The shared open-file-description table. 32 tasks hold 3 std OFDs
/// each (96 worst case) before a single pipe/socket exists — 64 was
/// a real transient-ENOMEM under scheduling-dependent occupancy.
const MAX_OFDS: usize = 128;

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
    t.brk_base = 0;
    t.brk_cur = 0;
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
        if t.nranges >= MAX_RANGES {
            return false;
        }
        let fl = ((p.flags & elf::PF_R != 0) as u8)
            | ((p.flags & elf::PF_W != 0) as u8) << 1;
        // The VMA covers the whole mapped page range, not just the
        // segment extent — Linux VMAs are page-aligned too. (Without
        // this, mprotect on a RELRO sub-page range ENOMEMs on the
        // page slack before p.va.)
        t.ranges[t.nranges] = (va0, va1, fl);
        t.nranges += 1;
        t.brk_base = t.brk_base.max((mend + PAGE - 1) & !(PAGE - 1));
    }
    for i in 0..USTACK_PAGES {
        if map_page(t, USTACK_VA + i * PAGE, USER | WRITE | NX).is_none() {
            return false;
        }
    }
    t.stack_top = USTACK_VA + USTACK_PAGES * PAGE;
    if t.nranges >= MAX_RANGES {
        return false;
    }
    t.ranges[t.nranges] = (USTACK_VA, t.stack_top, RNG_R | RNG_W);
    t.nranges += 1;
    t.brk_cur = t.brk_base;
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
        ranges: [(0, 0, 0); MAX_RANGES],
        nranges: 0,
        brk_base: 0,
        brk_cur: 0,
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
        wait_ptr2: 0,
        wait_len: 0,
        wait_deadline: 0,
        pending: 0,
        sigmask: 0,
        handlers: [0; 31],
        // SAFETY: read of the init-time capture.
        fx: unsafe { *core::ptr::addr_of!(INIT_FX) },
        name: [0; 16],
        fs_base: 0,
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

/// Range-table access bits — mirror PROT_READ/PROT_WRITE.
const RNG_R: u8 = 1;
const RNG_W: u8 = 2;

/// Is `[va, va+len)` wholly inside a mapped user range? This is THE
/// user-pointer gate for the syscall layer — checked arithmetic only.
/// `need_write` demands RNG_W (kernel writes into the buffer);
/// otherwise RNG_R suffices — a PROT_NONE VMA fails both.
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
    let want = if need_write { RNG_W } else { RNG_R };
    cur.is_some_and(|p| {
        p.ranges[..p.nranges]
            .iter()
            .any(|&(lo, hi, fl)| va >= lo && end <= hi && fl & want != 0)
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

// --- memory management (brk/mmap/munmap/mprotect) -------------------
//
// VMAs are the `ranges` table itself: every mapped interval has an
// entry, so the syscall gate, first-fit allocation, and teardown all
// see the same truth. Pages are mapped eagerly (alloc_frame zeroes);
// PROT_NONE is a present-but-supervisor leaf — the VMA reserves the
// VA, user access faults, mprotect can restore USER later.

/// Flush the TLB after user-page-table edits on the current address
/// space — same reload exec uses (user leaves carry no GLOBAL bit).
fn flush_tlb() {
    // SAFETY: reloading the current cr3 only flushes the TLB.
    unsafe { core::arch::asm!("mov {0}, cr3", "mov cr3, {0}", out(reg) _) };
}

/// Drop `va`'s leaf mapping in `t`'s address space: free the backing
/// frame and clear the PTE. Intermediate tables stay allocated —
/// teardown (reap/exec's free_user_mappings) reclaims them in bulk.
/// No-op on an absent or huge mapping.
fn unmap_page(t: &mut Task, va: u64) {
    let mut tab = t.pml4;
    for shift in [39u64, 30, 21] {
        let e = rd64(tab + 8 * ((va >> shift) & 0x1ff));
        if e & PRESENT == 0 || e & (1 << 7) != 0 {
            return;
        }
        tab = e & PHYS_MASK;
    }
    let ent = tab + 8 * ((va >> 12) & 0x1ff);
    let e = rd64(ent);
    if e & PRESENT != 0 {
        kalloc::free_frame(e & PHYS_MASK);
        wr64(ent, 0);
    }
}

/// PTE address of `va`'s present leaf under `t.pml4` — mprotect's
/// rewrite target. None on absent or huge-page traversal.
fn leaf_ent(t: &Task, va: u64) -> Option<u64> {
    let mut tab = t.pml4;
    for shift in [39u64, 30, 21] {
        let e = rd64(tab + 8 * ((va >> shift) & 0x1ff));
        if e & PRESENT == 0 || e & (1 << 7) != 0 {
            return None;
        }
        tab = e & PHYS_MASK;
    }
    let ent = tab + 8 * ((va >> 12) & 0x1ff);
    (rd64(ent) & PRESENT != 0).then_some(ent)
}

/// Append a range entry; false when the table is full — callers
/// pre-check capacity before mutating page tables so ops stay atomic.
fn range_add(t: &mut Task, lo: u64, hi: u64, fl: u8) -> bool {
    if lo >= hi {
        return true; // empty VMA is a no-op
    }
    if t.nranges >= MAX_RANGES {
        return false;
    }
    t.ranges[t.nranges] = (lo, hi, fl);
    t.nranges += 1;
    true
}

/// Compact `t`'s range table after a rewrite — drops empty entries
/// and merges adjacent intervals with identical flags, so repeated
/// split cycles can't permanently consume the bounded table.
fn range_compact(t: &mut Task) {
    let mut w = 0usize;
    let mut out = [(0u64, 0u64, 0u8); MAX_RANGES];
    for &(lo, hi, fl) in &t.ranges[..t.nranges] {
        if lo >= hi {
            continue;
        }
        if w > 0 {
            let p = &mut out[w - 1];
            if p.1 == lo && p.2 == fl {
                p.1 = hi;
                continue;
            }
        }
        out[w] = (lo, hi, fl);
        w += 1;
    }
    t.ranges = out;
    t.nranges = w;
}

/// Entries needed after clipping [a,b) out of `t`'s table — partial
/// intersections split in two. Used to fail ops atomically before a
/// single page table entry is touched (no half-applied munmap).
fn clip_entry_count(t: &Task, a: u64, b: u64) -> usize {
    let mut n = 0usize;
    for &(lo, hi, _) in &t.ranges[..t.nranges] {
        if lo >= b || hi <= a {
            n += 1; // untouched
        } else {
            n += (lo < a) as usize + (hi > b) as usize;
        }
    }
    n
}

/// Remove [a,b) from every range entry — splits partial overlaps,
/// drops covered ones, then compacts. Caller pre-checked capacity.
fn range_clip(t: &mut Task, a: u64, b: u64) {
    for i in 0..t.nranges {
        let (lo, hi, fl) = t.ranges[i];
        if lo >= b || hi <= a {
            continue;
        }
        t.ranges[i] = if lo < a {
            (lo, a, fl) // left remnant; right piece appended below
        } else if hi > b {
            (b, hi, fl)
        } else {
            (0, 0, 0)
        };
        if lo < a && hi > b {
            // Middle cut — the right remnant needs a fresh slot.
            t.ranges[t.nranges] = (b, hi, fl);
            t.nranges += 1;
        }
    }
    range_compact(t);
}

/// Is [a,b) fully covered by (possibly several) range entries?
/// mprotect's precondition — Linux ENOMEMs over unmapped VA.
fn range_covered(t: &Task, a: u64, b: u64) -> bool {
    let mut cur = a;
    while cur < b {
        let mut adv = cur;
        for &(lo, hi, _) in &t.ranges[..t.nranges] {
            if lo <= cur && hi > adv {
                adv = hi;
            }
        }
        if adv == cur {
            return false;
        }
        cur = adv;
    }
    true
}

/// Set access bits on the intersection of every range with [a,b).
/// Precondition: range_covered passed and capacity was pre-checked.
fn range_protect(t: &mut Task, a: u64, b: u64, fl: u8) {
    for i in 0..t.nranges {
        let (lo, hi, ofl) = t.ranges[i];
        if lo >= b || hi <= a || ofl == fl {
            continue;
        }
        // Emit up to three pieces: unchanged left, reprotected middle,
        // unchanged right — the extra slots were counted up front.
        if lo < a {
            t.ranges[i] = (lo, a, ofl);
            t.ranges[t.nranges] = (a, hi.min(b), fl);
            t.nranges += 1;
            if hi > b {
                t.ranges[t.nranges] = (b, hi, ofl);
                t.nranges += 1;
            }
        } else {
            t.ranges[i] = (a.max(lo), hi.min(b), fl);
            if hi > b {
                t.ranges[t.nranges] = (b, hi, ofl);
                t.nranges += 1;
            }
        }
    }
    range_compact(t);
}

/// Slots needed if [a,b) is reprotected on top of `t`'s table —
/// worst case one clipped entry becomes three.
fn protect_entry_count(t: &Task, a: u64, b: u64) -> usize {
    let mut n = 0usize;
    for &(lo, hi, _) in &t.ranges[..t.nranges] {
        if lo >= b || hi <= a {
            n += 1;
        } else {
            n += 1 + (lo < a) as usize + (hi > b) as usize;
        }
    }
    n
}

/// First-fit a `len`-page VMA in the mmap arena — holes are just
/// space between recorded ranges, so munmap'd VA is reused naturally.
fn mmap_find(t: &Task, len: u64) -> Option<u64> {
    let mut c = MMAP_LO;
    loop {
        let end = c.checked_add(len)?;
        if end > MMAP_HI {
            return None;
        }
        let mut bump = 0u64;
        for &(lo, hi, _) in &t.ranges[..t.nranges] {
            if lo < end && hi > c {
                bump = bump.max(hi);
            }
        }
        if bump == 0 {
            return Some(c);
        }
        c = (bump + PAGE - 1) & !(PAGE - 1);
    }
}

/// prot → leaf-flag bits for user PTEs: PROT_NONE yields a
/// present-supervisor page (no USER bit) — VMA-reserved, faulting.
fn prot_leaf(prot: u64) -> u64 {
    if prot == 0 {
        return 0; // PRESENT only — supervisor leaf
    }
    let mut fl = USER;
    if prot & 2 != 0 {
        fl |= WRITE;
    }
    if prot & 4 == 0 {
        fl |= NX;
    }
    fl
}

/// prot → range access bits for the syscall-pointer gate.
fn prot_rng(prot: u64) -> u8 {
    ((prot & 1 != 0) as u8) | ((prot & 2 != 0) as u8) << 1
}

/// brk(addr): set the program break — page-granular growth at
/// brk_base. Returns the NEW break on success, the CURRENT one on
/// any failure (the Linux contract — never an errno).
pub fn sys_brk(addr: u64) -> u64 {
    const BRK_CAP: u64 = 256 << 20; // bounded heap: 256 MiB
    let Some(t) = cur_mut() else { return 0 };
    if t.brk_base == 0 || addr < t.brk_base {
        return t.brk_cur;
    }
    let Some(new_end) = addr
        .checked_add(PAGE - 1)
        .map(|v| v & !(PAGE - 1))
    else {
        return t.brk_cur;
    };
    if new_end < t.brk_base || new_end - t.brk_base > BRK_CAP {
        return t.brk_cur;
    }
    // The heap may not grow into another mapping (Linux's collision
    // check) — the heap's own VMA (lo == brk_base) doesn't count.
    let old_end = (t.brk_cur + PAGE - 1) & !(PAGE - 1);
    if new_end > old_end
        && t.ranges[..t.nranges].iter().any(|&(lo, hi, _)| {
            lo != t.brk_base && lo < new_end && hi > old_end
        })
    {
        return t.brk_cur;
    }
    if new_end > old_end {
        // Grow: the heap VMA grows in place or is re-added (a munmap
        // may have eaten it). Ensure the slot exists BEFORE mapping
        // so a full table can't leave pages mapped-but-ungated.
        let has_vma = t.ranges[..t.nranges]
            .iter()
            .any(|&(lo, _, _)| lo == t.brk_base);
        if !has_vma && t.nranges >= MAX_RANGES {
            return t.brk_cur;
        }
        let mut v = old_end.max(t.brk_base);
        while v < new_end {
            if map_page(t, v, USER | WRITE | NX).is_none() {
                // Roll back this syscall's partial growth.
                let mut u = old_end.max(t.brk_base);
                while u < v {
                    unmap_page(t, u);
                    u += PAGE;
                }
                return t.brk_cur;
            }
            v += PAGE;
        }
        if has_vma {
            for r in t.ranges[..t.nranges].iter_mut() {
                if r.0 == t.brk_base {
                    r.1 = r.1.max(new_end);
                    r.2 = RNG_R | RNG_W;
                }
            }
        } else {
            range_add(t, t.brk_base, new_end, RNG_R | RNG_W);
        }
    } else if new_end < old_end {
        let mut v = new_end;
        while v < old_end {
            unmap_page(t, v);
            v += PAGE;
        }
        for r in t.ranges[..t.nranges].iter_mut() {
            if r.0 == t.brk_base {
                r.1 = new_end;
            }
        }
        flush_tlb();
    }
    t.brk_cur = addr;
    addr
}

/// mmap(addr, len, prot, flags, fd, off): anonymous private mappings
/// only — the malloc substrate. FD-backed and shared mappings are
/// ENODEV until file-backed VMAs exist. MAP_FIXED replaces whatever
/// is at `addr` (inside the user window — kernel slots are off
/// limits unconditionally).
pub fn sys_mmap(
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    _fd: u64,
    _off: u64,
) -> u64 {
    const ENODEV: u64 = (-19i64) as u64;
    const ENOMEM: u64 = (-12i64) as u64;
    const MAP_PRIVATE: u64 = 0x02;
    const MAP_FIXED: u64 = 0x10;
    const MAP_ANON: u64 = 0x20;
    let Some(t) = cur_mut() else { return ENOMEM };
    if len == 0 || prot > 7 || flags & MAP_PRIVATE == 0 {
        return EINVAL;
    }
    if flags & MAP_ANON == 0 {
        return ENODEV; // file-backed mmap is a later rung
    }
    let Some(len) = len.checked_add(PAGE - 1).map(|v| v & !(PAGE - 1))
    else {
        return ENOMEM;
    };
    let va = if addr != 0 {
        if addr & (PAGE - 1) != 0
            || addr < elf::USER_VA_LO
            || addr.checked_add(len).is_none_or(|e| e > elf::USER_VA_HI)
        {
            return if flags & MAP_FIXED != 0 { EINVAL } else { ENOMEM };
        }
        if flags & MAP_FIXED != 0 {
            // Replace semantics: free the overlapped pages AND clip
            // their VMA entries — map_page would otherwise union-keep
            // the stale frames. Atomic: capacity checked up front.
            if clip_entry_count(t, addr, addr + len) >= MAX_RANGES {
                return ENOMEM;
            }
            let mut v = addr;
            while v < addr + len {
                unmap_page(t, v);
                v += PAGE;
            }
            range_clip(t, addr, addr + len);
            // Freed frames' stale translations must go before user
            // code can run — else they alias whatever the buddy hands
            // those frames to next (cross-task UAF).
            flush_tlb();
            addr
        } else if t.ranges[..t.nranges]
            .iter()
            .any(|&(lo, hi, _)| lo < addr + len && hi > addr)
        {
            // Hint collides — fall back to first-fit like Linux.
            let Some(v) = mmap_find(t, len) else { return ENOMEM };
            v
        } else {
            addr
        }
    } else {
        let Some(v) = mmap_find(t, len) else { return ENOMEM };
        v
    };
    if t.nranges >= MAX_RANGES {
        return ENOMEM;
    }
    let leaf = prot_leaf(prot);
    let mut v = va;
    while v < va + len {
        if map_page(t, v, leaf).is_none() {
            let mut u = va;
            while u < v {
                unmap_page(t, u);
                u += PAGE;
            }
            return ENOMEM;
        }
        v += PAGE;
    }
    range_add(t, va, va + len, prot_rng(prot));
    va
}

/// munmap(addr, len): Linux never fails on an aligned range — holes
/// and foreign areas are silent no-ops. Pages are freed, range
/// entries clipped/split, one TLB flush at the end.
pub fn sys_munmap(addr: u64, len: u64) -> u64 {
    let Some(t) = cur_mut() else { return EINVAL };
    if len == 0 || addr & (PAGE - 1) != 0 {
        return EINVAL;
    }
    // Clip to the user window — wild VA is not an error, just
    // nothing-to-do (Linux's permissive semantic). Length rounds UP
    // to whole pages like Linux: unmapping 1 byte still zaps the page.
    let lo = addr.max(elf::USER_VA_LO);
    let len = match len.checked_add(PAGE - 1) {
        Some(v) => v & !(PAGE - 1),
        None => u64::MAX - (PAGE - 1),
    };
    let hi = addr.saturating_add(len).min(elf::USER_VA_HI);
    if lo >= hi {
        return 0;
    }
    if clip_entry_count(t, lo, hi) > MAX_RANGES {
        return (-12i64) as u64; // ENOMEM — atomic bail before touching
    }
    let mut v = lo;
    while v < hi {
        unmap_page(t, v);
        v += PAGE;
    }
    range_clip(t, lo, hi);
    flush_tlb();
    0
}

/// mprotect(addr, len, prot): every page in the range must be mapped
/// (Linux ENOMEMs over holes); flags flip on leaf and VMA alike,
/// with VMA splits where the coverage is partial.
pub fn sys_mprotect(addr: u64, len: u64, prot: u64) -> u64 {
    const ENOMEM: u64 = (-12i64) as u64;
    let Some(t) = cur_mut() else { return ENOMEM };
    if addr & (PAGE - 1) != 0 || prot > 7 {
        return EINVAL;
    }
    if len == 0 {
        return 0;
    }
    let Some(end) = addr.checked_add(len) else { return ENOMEM };
    if addr < elf::USER_VA_LO || end > elf::USER_VA_HI {
        return ENOMEM;
    }
    let hi = (end + PAGE - 1) & !(PAGE - 1);
    if !range_covered(t, addr, hi)
        || protect_entry_count(t, addr, hi) > MAX_RANGES
    {
        return ENOMEM;
    }
    let leaf = prot_leaf(prot);
    let mut v = addr;
    while v < hi {
        // A covered range implies a mapped page — an absent leaf
        // here means the invariant broke; fail rather than guess.
        let Some(ent) = leaf_ent(t, v) else { return ENOMEM };
        let e = rd64(ent);
        wr64(ent, (e & PHYS_MASK) | PRESENT | leaf);
        v += PAGE;
    }
    range_protect(t, addr, hi, prot_rng(prot));
    flush_tlb();
    0
}

// --- process image: exec stack, arch_prctl, misc ABI ----------------
//
// Real binaries need the Linux initial-stack contract: argc at [rsp],
// argv pointers, NULL, envp pointers, NULL, auxv pairs, AT_NULL — and
// %fs as TLS via arch_prctl. Collected argv strings must be copied
// BEFORE exec frees the old address space.

/// Bounded exec vectors — kernel-side copies made while the OLD
/// address space is still mapped. Limits are honest EINVAL/E2BIG
/// boundaries, not Linux's multi-MB ARG_MAX.
pub const MAX_EXEC_ARGS: usize = 16;
pub const MAX_EXEC_ENVS: usize = 8;
pub const EXEC_STR_CAP: usize = 256;

pub struct ExecArgs {
    pub argv: [[u8; EXEC_STR_CAP]; MAX_EXEC_ARGS],
    pub argv_len: [usize; MAX_EXEC_ARGS],
    pub argc: usize,
    pub envp: [[u8; EXEC_STR_CAP]; MAX_EXEC_ENVS],
    pub envp_len: [usize; MAX_EXEC_ENVS],
    pub envc: usize,
}

impl ExecArgs {
    pub const fn empty() -> Self {
        ExecArgs {
            argv: [[0; EXEC_STR_CAP]; MAX_EXEC_ARGS],
            argv_len: [0; MAX_EXEC_ARGS],
            argc: 0,
            envp: [[0; EXEC_STR_CAP]; MAX_EXEC_ENVS],
            envp_len: [0; MAX_EXEC_ENVS],
            envc: 0,
        }
    }
}

/// Read a NUL-terminated string from `pml4`'s address space —
/// translate-per-byte like the sleeper paths, so an unmapped string
/// can't fault the kernel. None on bad memory or overflow.
fn copy_task_str(pml4: u64, va: u64, dst: &mut [u8]) -> Option<usize> {
    for (i, slot) in dst.iter_mut().enumerate() {
        let b = read_task_u(pml4, va + i as u64, 1)? as u8;
        *slot = b;
        if b == 0 {
            return Some(i);
        }
    }
    None
}

/// Collect one NUL-terminated pointer array (argv or envp) from the
/// caller's address space — bounded, every pointer slot gate-checked
/// before the string walk.
fn collect_strs(
    pml4: u64,
    va_base: u64,
    dest: &mut [[u8; EXEC_STR_CAP]],
    destlen: &mut [usize],
) -> Result<usize, u64> {
    const E2BIG: u64 = (-7i64) as u64;
    if va_base == 0 {
        return Ok(0);
    }
    let mut n = 0usize;
    loop {
        if n >= dest.len() {
            return Err(E2BIG);
        }
        let slot = va_base + (n as u64) * 8;
        if !range_ok(slot, 8) {
            return Err(EFAULT);
        }
        let Some(ptr) = read_task_u(pml4, slot, 8) else {
            return Err(EFAULT);
        };
        if ptr == 0 {
            return Ok(n);
        }
        let Some(l) = copy_task_str(pml4, ptr, &mut dest[n]) else {
            return Err(EFAULT);
        };
        destlen[n] = l + 1;
        n += 1;
    }
}

/// Collect execve's argv/envp pointer arrays into `out` — bounded,
/// read while the OLD address space is still mapped.
pub fn collect_exec_args(argv_va: u64, envp_va: u64, out: &mut ExecArgs) -> u64 {
    const E2BIG: u64 = (-7i64) as u64;
    let Some(t) = cur_mut() else { return E2BIG };
    match collect_strs(t.pml4, argv_va, &mut out.argv, &mut out.argv_len) {
        Ok(n) => out.argc = n,
        Err(e) => return e,
    }
    match collect_strs(t.pml4, envp_va, &mut out.envp, &mut out.envp_len) {
        Ok(n) => out.envc = n,
        Err(e) => return e,
    }
    0
}

/// Build the Linux initial stack: strings top-down, 16B AT_RANDOM,
/// then [argc][argv ptrs][NULL][envp ptrs][NULL][auxv][AT_NULL].
/// Returns the final rsp — launch's ctx needs exactly this.
fn setup_stack(
    t: &mut Task,
    e: &elf::Elf,
    img: &[u8],
    args: &ExecArgs,
) -> Option<u64> {
    let mut sp = t.stack_top;
    let mut str_va = |s: &[u8]| -> u64 {
        sp -= s.len() as u64;
        write_task_bytes(t.pml4, sp, s);
        sp
    };
    let mut argv = [0u64; MAX_EXEC_ARGS];
    for (i, (s, &l)) in args.argv.iter().zip(args.argv_len.iter()).enumerate().take(args.argc) {
        argv[i] = str_va(&s[..l]);
    }
    let mut envv = [0u64; MAX_EXEC_ENVS];
    for (i, (s, &l)) in args.envp.iter().zip(args.envp_len.iter()).enumerate().take(args.envc) {
        envv[i] = str_va(&s[..l]);
    }
    // AT_RANDOM: 16 real bytes — glibc seeds its stack canary here.
    sp -= 16;
    let mut rnd = [0u8; 16];
    rnd[..8].copy_from_slice(&crate::rand::next_u64().to_le_bytes());
    rnd[8..].copy_from_slice(&crate::rand::next_u64().to_le_bytes());
    write_task_bytes(t.pml4, sp, &rnd);
    let at_random = sp;
    sp &= !15u64;
    // AT_PHDR needs the VA where the phdr table was mapped — the
    // PT_LOAD covering e_phoff (headers land in the first segment).
    let mut at_phdr = 0u64;
    for p in e.phdrs(img) {
        if p.ty == elf::PT_LOAD
            && (p.off..p.off + p.filesz).contains(&(e.phoff() as u64))
        {
            at_phdr = p.va + (e.phoff() as u64 - p.off);
        }
    }
    let auxv: [(u64, u64); 16] = [
        (3, at_phdr),          // AT_PHDR
        (4, e.phentsize() as u64),
        (5, e.phnum() as u64),
        (6, PAGE),             // AT_PAGESZ
        (7, 0),                // AT_BASE (no interp)
        (8, 0),                // AT_FLAGS
        (9, e.entry),          // AT_ENTRY
        (11, 0),               // AT_UID
        (12, 0),               // AT_EUID
        (13, 0),               // AT_GID
        (14, 0),               // AT_EGID
        (16, 0),               // AT_HWCAP
        (17, 100),             // AT_CLKTCK — apic runs at 100Hz
        (23, 0),               // AT_SECURE
        (25, at_random),       // AT_RANDOM
        (0, 0),                // AT_NULL
    ];
    let slots = 1 + args.argc + 1 + args.envc + 1 + auxv.len() * 2;
    sp -= (slots as u64) * 8;
    sp &= !15u64; // ABI: rsp 16-aligned at entry
    let w = |v: u64, i: usize| {
        write_task_bytes(t.pml4, sp + (i as u64) * 8, &v.to_le_bytes());
    };
    let mut i = 0;
    w(args.argc as u64, i);
    i += 1;
    for &a in argv.iter().take(args.argc) {
        w(a, i);
        i += 1;
    }
    w(0, i);
    i += 1;
    for &e in envv.iter().take(args.envc) {
        w(e, i);
        i += 1;
    }
    w(0, i);
    i += 1;
    for (k, v) in auxv {
        w(k, i);
        w(v, i + 1);
        i += 2;
    }
    Some(sp)
}

/// arch_prctl(code, addr): x86_64 TLS control. SET_FS installs the
/// task's FS base (validated inside the user window or zero); GET_FS
/// writes it through a validated pointer. GS ops are EINVAL — no user
/// GS protocol exists in this ABI.
pub fn sys_arch_prctl(code: u64, addr: u64) -> u64 {
    const EPERM: u64 = (-1i64) as u64;
    let Some(t) = cur_mut() else { return EPERM };
    match code {
        0x1002 => {
            // ARCH_SET_FS — non-canonical or outside the user window
            // can't be a TLS block.
            if addr != 0 && !(elf::USER_VA_LO..elf::USER_VA_HI).contains(&addr) {
                return EPERM;
            }
            t.fs_base = addr;
            // SAFETY: MSR write of the running task's own FS base.
            unsafe { cpu::wrmsr(cpu::MSR_FS_BASE, addr) };
            0
        }
        0x1003 => {
            // ARCH_GET_FS
            if !range_ok_w(addr, 8) {
                return EFAULT;
            }
            // SAFETY: validated writable user range, current task.
            unsafe { (addr as *mut u64).write_unaligned(t.fs_base) };
            0
        }
        _ => EINVAL,
    }
}

/// uname(buf): Linux struct utsname — 6 fields × 65 bytes.
pub fn sys_uname(buf: u64) -> u64 {
    if !range_ok_w(buf, 390) {
        return EFAULT;
    }
    let fields: [&[u8]; 6] = [
        b"Rustux",
        b"rustux",
        b"0.1.0",
        b"rustux kernel",
        b"x86_64",
        b"(none)",
    ];
    for (i, f) in fields.iter().enumerate() {
        // SAFETY: buf validated writable for 390 bytes; each field
        // starts zeroed at its 65-byte slot.
        unsafe {
            core::ptr::write_bytes((buf + i as u64 * 65) as *mut u8, 0, 65);
            core::ptr::copy_nonoverlapping(
                f.as_ptr(),
                (buf + i as u64 * 65) as *mut u8,
                f.len().min(64),
            );
        }
    }
    0
}

/// getrandom(buf, len, flags): ChaCha20 bytes into a validated user
/// range — capped per call like the copy paths.
pub fn sys_getrandom(buf: u64, len: u64, _flags: u64) -> u64 {
    if len > 4096 {
        return EINVAL;
    }
    if len != 0 && !range_ok_w(buf, len) {
        return EFAULT;
    }
    let mut i = 0u64;
    while i < len {
        let v = crate::rand::next_u64().to_le_bytes();
        let n = ((len - i) as usize).min(8);
        // SAFETY: buf validated writable for len bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(
                v.as_ptr(),
                (buf + i) as *mut u8,
                n,
            )
        };
        i += n as u64;
    }
    len
}

/// ioctl(fd, req, arg): nothing here is a tty — ENOTTY like Linux on
/// a device that doesn't speak ioctl.
pub fn sys_ioctl() -> u64 {
    (-25i64) as u64 // ENOTTY
}

/// gettid: no threads — the thread id is the process id.
pub fn sys_gettid() -> u64 {
    cur_mut().map(|t| t.pid).unwrap_or(0)
}

/// clone(flags, stack, ptid, ctid, tls): the fork-shaped subset only —
/// an exit-signal of SIGCHLD (or 0), a kernel-made stack, and the
/// child-TID writes glibc asks for (PARENT_SETTID -> *ptid in our
/// space, CHILD_SETTID -> *ctid in the CHILD's space via its pml4;
/// CHILD_CLEARTID registers a futex-wake address — no futexes, so it
/// is silently honored-as-noop like a task that dies with none).
/// Thread combos (CLONE_VM/THREAD/SETTLS/any other flag) honestly
/// ENOSYS. glibc's fork() lands here, not on nr 57.
pub fn sys_clone(
    ctx: &mut UCtx,
    flags: u64,
    stack: u64,
    ptid: u64,
    ctid: u64,
    _tls: u64,
) -> u64 {
    const ENOSYS_: u64 = (-38i64) as u64;
    const PARENT_SETTID: u64 = 0x0010_0000;
    const CHILD_CLEARTID: u64 = 0x0020_0000;
    const CHILD_SETTID: u64 = 0x0100_0000;
    const SIGMASK: u64 = 0xff;
    const KNOWN: u64 = SIGMASK | PARENT_SETTID | CHILD_CLEARTID | CHILD_SETTID;
    if stack != 0 || flags & !KNOWN != 0 {
        return ENOSYS_;
    }
    // The exit signal must be SIGCHLD or none — wait4 keys on it.
    if flags & SIGMASK != 0 && flags & SIGMASK != 17 {
        return ENOSYS_;
    }
    if ptid != 0 && flags & PARENT_SETTID != 0 && !range_ok_w(ptid, 4) {
        return EFAULT;
    }
    let child = fork(ctx);
    if (child as i64) < 0 {
        return child;
    }
    if ptid != 0 && flags & PARENT_SETTID != 0 {
        // SAFETY: ptid validated writable above.
        unsafe { (ptid as *mut u32).write_unaligned(child as u32) };
    }
    if ctid != 0 && flags & CHILD_SETTID != 0 {
        // *ctid lives in the child's (copied) address space — write
        // through its page tables.
        let tasks = unsafe { &*core::ptr::addr_of!(TASKS) };
        if let Some(c) = tasks.iter().flatten().find(|t| t.pid == child) {
            write_task_bytes(c.pml4, ctid, &(child as u32).to_ne_bytes());
        }
    }
    child
}

/// tgkill(tgid, tid, sig): no threads — tgid == tid == pid, so this
/// is kill() with an extra ABI arg. glibc's abort() lands here.
pub fn sys_tgkill(tgid: u64, _tid: u64, sig: u64) -> u64 {
    kill(tgid, sig)
}

/// rt_sigprocmask(how, set, oldset, sigsetsize): the per-task signal
/// mask. SIG_BLOCK=0 / SIG_UNBLOCK=1 / SIG_SETMASK=2; `how` is only
/// validated when set is non-NULL, matching kernel behavior. We
/// track 31 signals, so the u64 set is truncated — high bits are
/// harmless. A masked signal stays pending for later delivery; the
/// next syscall/schedule boundary runs it through maybe_deliver.
pub fn sys_rt_sigprocmask(how: u64, set: u64, old: u64, sigsetsize: u64) -> u64 {
    if sigsetsize != 8 {
        return EINVAL;
    }
    let Some(t) = cur_mut() else { return ENOENT };
    if old != 0 {
        if !range_ok_w(old, 8) {
            return EFAULT;
        }
        // SAFETY: old validated writable for 8 bytes.
        unsafe { (old as *mut u64).write_unaligned(t.sigmask as u64) };
    }
    if set != 0 {
        if !range_ok(set, 8) {
            return EFAULT;
        }
        // SAFETY: set validated readable for 8 bytes.
        let m = unsafe { (set as *const u64).read_unaligned() } as u32;
        t.sigmask = match how {
            0 => t.sigmask | m,
            1 => t.sigmask & !m,
            2 => m,
            _ => return EINVAL,
        };
    }
    0
}

/// readlinkat("/proc/self/exe") → the image's cpio name — the only
/// proc path that exists. Anything else is ENOENT like a procfs
/// miss. Returns the byte count (no NUL), capped at the buffer.
pub fn read_proc_self_exe(buf: u64, len: u64) -> u64 {
    if len == 0 || !range_ok_w(buf, len) {
        return EFAULT;
    }
    let Some(t) = cur_mut() else { return ENOENT };
    let n = t.name.iter().position(|&b| b == 0).unwrap_or(16);
    // glibc's _dl_get_origin asserts the link is absolute — the
    // archive namespace is flat, so "/" + name is the honest answer.
    let total = (n + 1).min(len as usize);
    // SAFETY: buf validated writable for len bytes.
    unsafe {
        (buf as *mut u8).write(b'/');
        (buf as *mut u8).add(1).copy_from(t.name.as_ptr(), total - 1)
    };
    total as u64
}

/// getcwd(buf, len): the archive is one flat root — always "/".
pub fn sys_getcwd(buf: u64, len: u64) -> u64 {
    const ERANGE: u64 = (-34i64) as u64;
    if len < 2 {
        return ERANGE;
    }
    if !range_ok_w(buf, 2) {
        return EFAULT;
    }
    // SAFETY: buf validated writable for 2 bytes.
    unsafe {
        (buf as *mut u8).write(b'/');
        (buf as *mut u8).add(1).write(0);
    }
    2
}

/// clock_getres(clk, res): the APIC ticks at 100Hz — 10ms.
pub fn sys_clock_getres(_clk: u64, res: u64) -> u64 {
    if res == 0 {
        return 0;
    }
    if !range_ok_w(res, 16) {
        return EFAULT;
    }
    // SAFETY: res validated writable for 16 bytes.
    unsafe {
        (res as *mut u64).write_unaligned(0);
        (res as *mut u64).add(1).write_unaligned(10_000_000);
    }
    0
}

/// sched_yield(): ctx-save then switch to another runnable task if
/// one exists — self just returns 0 when it wins the pick again.
pub fn sys_sched_yield(ctx: &mut UCtx) -> u64 {
    ctx.rax = 0;
    let Some(t) = cur_mut() else { return 0 };
    t.ctx = *ctx;
    let cur = unsafe { *core::ptr::addr_of!(CUR_I) };
    if let Some(n) = pick_ready() {
        if n != cur {
            switch_to(n)
        }
    }
    0
}

/// fstat(fd, statbuf): same 144B stat as newfstatat, mode by OFD
/// kind — chr for console, fifo for pipes, sock for sockets,
/// reg/dir for archive entries.
pub fn sys_fstat(fd: u64, statbuf: u64) -> u64 {
    const STAT_LEN: u64 = 144;
    if !range_ok_w(statbuf, STAT_LEN) {
        return EFAULT;
    }
    let Some(t) = cur_mut() else { return EBADF };
    let Some(o) = ofd(t, fd) else { return EBADF };
    let (mode, size) = match &o.kind {
        FdKind::Stdin | FdKind::Console => (0o20644u64, 0u64), // S_IFCHR
        FdKind::Dir => (0o40755, 0),
        FdKind::File(d) => (0o100644, d.len() as u64),
        FdKind::Pipe { .. } => (0o10644, 0), // S_IFIFO
        FdKind::Udp(_) | FdKind::Tcp(_) => (0o140777, 0), // S_IFSOCK
    };
    let mut st = [0u64; 18];
    st[2] = 1;
    st[3] = mode;
    st[6] = size;
    st[7] = 4096;
    st[8] = size.div_ceil(512);
    // SAFETY: statbuf validated writable for 144 bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(
            st.as_ptr() as *const u8,
            statbuf as *mut u8,
            STAT_LEN as usize,
        )
    };
    0
}

/// set_tid_address(ptr): we don't store the pointer (no clear-thread-
/// tid futex wake yet) — the return is the tid, which is our pid.
pub fn sys_set_tid_address() -> u64 {
    cur_mut().map(|t| t.pid).unwrap_or(0)
}

/// set_robust_list(head, len): record nothing — robust futex wake on
/// death needs futexes first. Linux EINVALs a wrong struct size.
pub fn sys_set_robust_list(_head: u64, len: u64) -> u64 {
    if len != 24 {
        return EINVAL;
    }
    0
}

/// prlimit64(pid, res, new, old): report RLIM_INFINITY for `old`;
/// `new` is accepted and ignored (bounded resources are enforced by
/// the allocator tables, not rlimit).
pub fn sys_prlimit64(_pid: u64, _res: u64, _new: u64, old: u64) -> u64 {
    if old != 0 {
        if !range_ok_w(old, 16) {
            return EFAULT;
        }
        // SAFETY: validated writable user range.
        unsafe {
            (old as *mut u64).write_unaligned(u64::MAX);
            (old as *mut u64).add(1).write_unaligned(u64::MAX);
        }
    }
    0
}

/// clock_gettime(clk, ts): APIC ticks at 100Hz. Any clock id gets the
/// same monotonic counter — time domains don't exist yet.
pub fn sys_clock_gettime(_clk: u64, ts: u64) -> u64 {
    if !range_ok_w(ts, 16) {
        return EFAULT;
    }
    let t = crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed);
    // SAFETY: ts validated writable for 16 bytes.
    unsafe {
        (ts as *mut u64).write_unaligned(t / 100);
        (ts as *mut u64).add(1).write_unaligned((t % 100) * 10_000_000);
    }
    0
}

/// gettimeofday(tv, tz): same counter, {sec,usec} layout.
pub fn sys_gettimeofday(tv: u64, _tz: u64) -> u64 {
    if tv != 0 {
        if !range_ok_w(tv, 16) {
            return EFAULT;
        }
        let t = crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed);
        // SAFETY: tv validated writable for 16 bytes.
        unsafe {
            (tv as *mut u64).write_unaligned(t / 100);
            (tv as *mut u64).add(1).write_unaligned((t % 100) * 10_000);
        }
    }
    0
}

/// nanosleep(req, rem): block until the deadline — WaitOn::Timer
/// ages in poll_expire alongside the other deadline waits. Signals
/// interrupt it (rem is zeroed — partial-remain bookkeeping needs
/// the deadline stored, which we keep simple).
pub fn sys_nanosleep(ctx: &mut UCtx, req: u64, rem: u64) -> u64 {
    const EINTR: u64 = (-4i64) as u64;
    if !range_ok(req, 16) {
        return EFAULT;
    }
    let Some(t) = cur_mut() else { return EINVAL };
    let sec = unsafe { (req as *const u64).read_unaligned() };
    let nsec = unsafe { (req as *const u64).add(1).read_unaligned() };
    if nsec >= 1_000_000_000 {
        return EINVAL;
    }
    let ticks = sec
        .saturating_mul(100)
        .saturating_add(nsec.div_ceil(10_000_000));
    if ticks == 0 {
        return 0;
    }
    if rem != 0 && !range_ok_w(rem, 16) {
        return EFAULT;
    }
    if deliverable(t) {
        return EINTR;
    }
    t.wait_on = WaitOn::Timer;
    // wait_ptr carries `rem` — the signal-wake path fills it with
    // the unslept remainder on EINTR (Linux semantics); a normal
    // deadline wake leaves it untouched.
    t.wait_ptr = rem;
    t.wait_ptr2 = 0; // kernel-style -errno returns
    t.wait_deadline = crate::apic::TICKS
        .load(core::sync::atomic::Ordering::Relaxed)
        .saturating_add(ticks);
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// clock_nanosleep(clk, flags, req, rem): glibc's nanosleep() lands
/// here, not on nr 35. Unlike every other syscall this ABI returns
/// POSITIVE errno values (EINTR, EINVAL, EFAULT) directly — the
/// signal-wake path must know that, so wait_ptr2 flags the mode.
/// flags bit0 = TIMER_ABSTIME (req is an absolute deadline).
pub fn sys_clock_nanosleep(
    ctx: &mut UCtx,
    _clk: u64,
    flags: u64,
    req: u64,
    rem: u64,
) -> u64 {
    const EFAULT_P: u64 = 14;
    const EINVAL_P: u64 = 22;
    if !range_ok(req, 16) {
        return EFAULT_P;
    }
    let sec = unsafe { (req as *const u64).read_unaligned() };
    let nsec = unsafe { (req as *const u64).add(1).read_unaligned() };
    if nsec >= 1_000_000_000 || flags & !1 != 0 {
        return EINVAL_P;
    }
    let now = crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let ticks = if flags & 1 != 0 {
        // Absolute deadline measured in boot-time ticks.
        sec.saturating_mul(100)
            .saturating_add(nsec.div_ceil(10_000_000))
            .saturating_sub(now)
    } else {
        sec.saturating_mul(100)
            .saturating_add(nsec.div_ceil(10_000_000))
    };
    if ticks == 0 {
        return 0;
    }
    if rem != 0 && !range_ok_w(rem, 16) {
        return EFAULT_P;
    }
    let Some(t) = cur_mut() else { return EINVAL_P };
    if deliverable(t) {
        return 4; // EINTR, positive per this ABI
    }
    t.wait_on = WaitOn::Timer;
    t.wait_ptr = rem;
    t.wait_ptr2 = 1; // positive-errno wake
    t.wait_deadline = now.saturating_add(ticks);
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// readv — iovec {base:u64, len:u16 pairs as u64s}. Scatters one
/// logical read across the vector: after the first element, continue
/// only while the fd still reports data-ready — a second BLOCKING
/// read would exceed Linux's "return what's available" semantic.
/// (writev lives in syscall.rs where the console write path is.)
pub fn fd_readv(ctx: &mut UCtx, fd: u64, iov: u64, cnt: u64) -> u64 {
    if cnt == 0 {
        return 0;
    }
    if cnt > 32 || !range_ok(iov, cnt * 16) {
        return EINVAL;
    }
    let mut done = 0u64;
    for i in 0..cnt {
        let (base, len) = unsafe {
            (
                ((iov + i * 16) as *const u64).read_unaligned(),
                ((iov + i * 16 + 8) as *const u64).read_unaligned(),
            )
        };
        if len == 0 {
            continue;
        }
        if done > 0 {
            // POLLIN|POLLERR|POLLHUP = readable-or-terminal — anything
            // else means the next read would block.
            let ready = cur_mut()
                .map(|t| poll_revents(t, fd as i32, 1) & (1 | 8 | 16) != 0)
                .unwrap_or(false);
            if !ready {
                break;
            }
        }
        let n = fd_read(ctx, fd, base, len);
        if (n as i64) < 0 {
            return if done > 0 { done } else { n };
        }
        done += n;
        if n < len {
            break;
        }
    }
    done
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
        FdKind::Udp(i) => udp_recv_or_block(ctx, *i, buf, len, 0),
        FdKind::Tcp(i) => tcp_recv_or_block(ctx, *i, buf, len, 0),
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
                if deliverable(t) {
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
    if let FdKind::Tcp(i) = &o.kind {
        return tcp_send(*i, buf, len);
    }
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
        if deliverable(t) {
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

/// newfstatat(dirfd, path, statbuf, flags) -> Linux struct stat
/// (144 bytes, x86_64 layout) at a user pointer. The cpio archive
/// is the whole fs: ino is the entry's archive index, mode is
/// synthesized (S_IFREG|0644, S_IFDIR|0755 for "."), sizes come
/// from the member header. dirfd/flags are accepted for ABI shape —
/// a flat archive has no symlinks or per-dirfd namespaces to honor.
pub fn newfstatat(path: &[u8], statbuf: u64) -> u64 {
    const STAT_LEN: u64 = 144;
    if !range_ok_w(statbuf, STAT_LEN) {
        return EFAULT;
    }
    let mut st = [0u64; 18];
    if path.is_empty() || path == b"." {
        // The archive root: every member lives directly under it.
        st[2] = 2; // st_nlink
        st[3] = 0o40755; // st_mode: S_IFDIR | 0755 (u64 slot holds u32)
    } else {
        let Some(arc) = (unsafe { *core::ptr::addr_of!(INITRD) }) else {
            return ENOENT;
        };
        let Some((idx, (_, data))) = crate::cpio::entries(arc)
            .enumerate()
            .find(|(_, (n, _))| *n == path)
        else {
            return ENOENT;
        };
        st[1] = idx as u64 + 1; // st_ino
        st[2] = 1; // st_nlink
        st[3] = 0o100644; // st_mode: S_IFREG | 0644
        st[6] = data.len() as u64; // st_size
        st[8] = (data.len() as u64).div_ceil(512); // st_blocks
    }
    st[7] = 4096; // st_blksize
    // SAFETY: range_ok_w proved statbuf is a writable user range;
    // st is the 144-byte Linux x86_64 stat layout verbatim.
    unsafe {
        core::ptr::copy_nonoverlapping(
            st.as_ptr() as *const u8,
            statbuf as *mut u8,
            STAT_LEN as usize,
        )
    };
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
    if let FdKind::Udp(i) = kind {
        // The endpoint dies with its last fd — queued dgrams free
        // with it. A blocked recvfrom on this socket can't exist:
        // the blocker's own fd pins the endpoint alive.
        unsafe { (*core::ptr::addr_of_mut!(UDPSOCKS))[i] = None };
    }
    if let FdKind::Tcp(i) = kind {
        tcp_sock_close(i);
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
                t.wait_deadline = 0;
                t.state = TaskState::Runnable;
            }
        }
    }
    // Endpoint teardown also changes poll readiness (POLLHUP/ERR).
    wake_pollers();
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
/// this loop is monotonically terminating within MAX_TASKS. Poll
/// waiters re-scan on any pipe change (their fd set may name it).
fn pipe_wake(id: usize) {
    loop {
        if wake_pipe_readers(id) + wake_pipe_writers(id) == 0 {
            break;
        }
    }
    wake_pollers();
}

/// poll() event bits — the standard Linux POLL* values.
const POLLIN: u16 = 0x0001;
const POLLOUT: u16 = 0x0004;
const POLLERR: u16 = 0x0008;
const POLLHUP: u16 = 0x0010;
const POLLNVAL: u16 = 0x0020;
/// Bounded pollfd array — same fixed-table discipline as MAX_FDS.
const MAX_POLL: u64 = 32;

/// Read `len` user bytes from another space via per-byte translate —
/// small (<=8B) structured reads for pollfd fields on a sleeping task.
fn read_task_u(pml4: u64, va: u64, len: u64) -> Option<u64> {
    let mut v = 0u64;
    for i in 0..len {
        let pa = translate_r(pml4, va + i)?;
        // SAFETY: pa is a present user frame in pml4.
        v |= (unsafe { (pa as *const u8).read() } as u64) << (i * 8);
    }
    Some(v)
}

/// Write one u16 into another space (pollfd.revents on a sleeper).
fn write_task_u16(pml4: u64, va: u64, v: u16) {
    for i in 0..2u64 {
        if let Some(pa) = translate(pml4, va + i) {
            // SAFETY: pa is a present user frame in pml4.
            unsafe { (pa as *mut u8).write((v >> (i * 8)) as u8) };
        }
    }
}

/// Evaluate one pollfd's revents against kernel state. Requested
/// events are masked in; ERR/HUP/NVAL report unconditionally like
/// Linux (they describe the fd, not the request).
fn poll_revents(t: &Task, fd: i32, events: u16) -> u16 {
    if !(0..MAX_FDS as i32).contains(&fd) {
        return POLLNVAL;
    }
    let Some(oi) = t.fds[fd as usize] else {
        return POLLNVAL;
    };
    let ofds = unsafe { &*core::ptr::addr_of!(OFDS) };
    let Some(o) = ofds.get(oi as usize).and_then(|s| s.as_ref()) else {
        return POLLNVAL;
    };
    let pipes = unsafe { &*core::ptr::addr_of!(PIPES) };
    let r = match &o.kind {
        // Console writes never block; stdin reads return EOF so they
        // "never block" either — Linux reports POLLIN at EOF.
        FdKind::Stdin => POLLIN,
        FdKind::Console => POLLOUT,
        // File and dir reads never block (EOF reads return 0).
        FdKind::File(_) | FdKind::Dir => POLLIN,
        // UDP: readable when a datagram queues; send never blocks.
        FdKind::Udp(i) => {
            let socks = unsafe { &*core::ptr::addr_of!(UDPSOCKS) };
            let mut r = POLLOUT;
            if socks.get(*i).and_then(|s| s.as_ref()).is_some_and(|k| k.q.is_some()) {
                r |= POLLIN;
            }
            r
        }
        // TCP: listener readable when a child waits; a connection
        // readable on data/EOF, writable once established, ERR on
        // RST, HUP once fully closed.
        FdKind::Tcp(i) => {
            let socks = unsafe { &*core::ptr::addr_of!(TCPSOCKS) };
            match socks.get(*i).and_then(|s| s.as_ref()) {
                None => POLLNVAL,
                Some(k) if k.state == TcpState::Listen => {
                    if k.blen > 0 { POLLIN } else { 0 }
                }
                Some(k) => {
                    let mut r = 0;
                    if k.dead {
                        r |= POLLERR;
                    }
                    if k.rxl > 0 || k.fin_rx || k.state == TcpState::Closed {
                        r |= POLLIN;
                    }
                    if matches!(k.state, TcpState::Estab | TcpState::CloseWait) {
                        r |= POLLOUT;
                    }
                    if k.state == TcpState::Closed {
                        r |= POLLHUP;
                    }
                    r
                }
            }
        }
        FdKind::Pipe { id, write } => {
            match pipes.get(*id).and_then(|s| s.as_ref()) {
                None => POLLERR, // peer's frame freed — torn down
                Some(p) if !write => {
                    let mut r = 0;
                    if p.tail > p.head {
                        r |= POLLIN;
                    }
                    if p.writers == 0 {
                        r |= POLLHUP; // every writer gone — read won't block
                    }
                    r
                }
                Some(p) => {
                    let mut r = 0;
                    if p.readers == 0 {
                        r |= POLLERR; // write would EPIPE
                    }
                    if PIPE_SIZE - (p.tail - p.head) > 0 {
                        r |= POLLOUT;
                    }
                    r
                }
            }
        }
    };
    (r & events) | (r & (POLLERR | POLLHUP | POLLNVAL))
}

/// Scan a task's pollfd array through ITS page tables, writing each
/// revents. Returns the count of nonzero revents — works on the
/// current task and on a blocked one (translate walks the pml4).
fn poll_scan(t: &Task, fds: u64, nfds: u64) -> u64 {
    let mut ready = 0;
    for i in 0..nfds {
        let va = fds + i * 8;
        let (fd, events) = match (
            read_task_u(t.pml4, va, 4),
            read_task_u(t.pml4, va + 4, 2),
        ) {
            (Some(f), Some(e)) => (f as u32 as i32, e as u16),
            _ => {
                write_task_u16(t.pml4, va + 6, POLLNVAL);
                ready += 1;
                continue;
            }
        };
        let r = poll_revents(t, fd, events);
        write_task_u16(t.pml4, va + 6, r);
        if r != 0 {
            ready += 1;
        }
    }
    ready
}

/// Wake any blocked poller whose fd set is now ready — called after
/// every pipe-state change. The scan rewrites revents through the
/// sleeper's tables and the ready count lands in its syscall rax.
fn wake_pollers() {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state != TaskState::Blocked || t.wait_on != WaitOn::Poll {
            continue;
        }
        let n = poll_scan(t, t.wait_ptr, t.wait_len);
        if n > 0 {
            t.ctx.rax = n;
            t.wait_on = WaitOn::None;
            t.wait_deadline = 0;
            t.state = TaskState::Runnable;
        }
    }
}

/// Timer tick: expire timed poll waits — the return is 0 with
/// revents zeroed, the Linux timeout shape.
fn poll_expire() {
    let now = crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state == TaskState::Blocked
            && t.wait_deadline != 0
            && now >= t.wait_deadline
        {
            match t.wait_on {
                WaitOn::Poll => {
                    for i in 0..t.wait_len {
                        write_task_u16(t.pml4, t.wait_ptr + i * 8 + 6, 0);
                    }
                    t.ctx.rax = 0;
                }
                WaitOn::TcpConn(si) => {
                    // connect() timed out: ETIMEDOUT, the socket
                    // marked dead so further io fails fast.
                    t.ctx.rax = (-110i64) as u64;
                    if let Some(k) = tcp_socks()[si].as_mut() {
                        k.dead = true;
                        k.state = TcpState::Closed;
                    }
                }
                WaitOn::Timer => t.ctx.rax = 0, // nanosleep elapsed
                _ => continue,
            }
            t.wait_on = WaitOn::None;
            t.wait_deadline = 0;
            t.state = TaskState::Runnable;
        }
    }
}

/// poll(fds, nfds, timeout_ms): Linux pollfd {fd,events,revents} is
/// 8 bytes; revents write back through the caller's own tables.
/// Readiness is honest kernel state — pipes by ring level and
/// endpoint counts, files/dirs/console statically. timeout<0 waits
/// forever; nfds=0 with timeout>0 is a plain sleep.
pub fn fd_poll(ctx: &mut UCtx, fds: u64, nfds: u64, timeout_ms: i64) -> u64 {
    const EINTR: u64 = (-4i64) as u64;
    if nfds > MAX_POLL {
        return EINVAL;
    }
    let Some(t) = cur_mut() else { return EINVAL };
    if nfds > 0 && !range_ok_w(fds, nfds * 8) {
        return EFAULT;
    }
    let ready = if nfds == 0 { 0 } else { poll_scan(t, fds, nfds) };
    if ready > 0 || timeout_ms == 0 {
        return ready;
    }
    if deliverable(t) {
        return EINTR; // a signal landed before we slept
    }
    t.wait_on = WaitOn::Poll;
    t.wait_ptr = fds;
    t.wait_len = nfds;
    t.wait_deadline = if timeout_ms < 0 {
        0
    } else {
        crate::apic::TICKS.load(core::sync::atomic::Ordering::Relaxed)
            + (timeout_ms as u64).div_ceil(10)
    };
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

// ---- UDP sockets ---------------------------------------------------
// AF_INET/SOCK_DGRAM over the e1000 live ring. One datagram queue per
// socket — honest UDP: excess drops. The blocking recvfrom borrows the
// pipe-wait machinery: the waker (rx demux on the live ring) copies
// payload + sockaddr through the SLEEPER's page tables.

/// Max UDP sockets — same fixed-table discipline as MAX_PIPES.
const MAX_SOCKS: usize = 8;
/// A datagram payload cap: fits a full-size ethernet frame.
const MAX_DGRAM: usize = 1600;
/// First ephemeral port, Linux's default range start.
const EPHEM_BASE: u16 = 49152;

/// One queued datagram: payload + sender sockaddr for recvfrom.
struct Dgram {
    src: [u8; 4],
    sport: u16,
    len: u16,
    data: [u8; MAX_DGRAM],
}

/// A UDP endpoint: the bound local port (0 = unbound, assigned on
/// first send/bind) plus a one-deep receive queue.
struct UdpSock {
    port: u16,
    q: Option<Dgram>,
}

static mut UDPSOCKS: [Option<UdpSock>; MAX_SOCKS] = [const { None }; MAX_SOCKS];

/// Linux struct sockaddr_in, all 16 bytes initialized before the
/// copy (the CVE-2021-47597 uninit-struct copy_to_user class).
fn sockaddr_in(port: u16, ip: [u8; 4]) -> [u8; 16] {
    let mut s = [0u8; 16];
    s[0..2].copy_from_slice(&2u16.to_ne_bytes()); // AF_INET
    s[2..4].copy_from_slice(&port.to_be_bytes());
    s[4..8].copy_from_slice(&ip);
    s
}

/// Parse the first 8 bytes of a sockaddr_in at `sa` in pml4's space:
/// (port, ip). None on untranslatable or non-INET input.
fn parse_sockaddr(pml4: u64, sa: u64) -> Option<(u16, [u8; 4])> {
    let v = read_task_u(pml4, sa, 8)?;
    if v & 0xffff != 2 {
        return None; // sin_family must be AF_INET
    }
    let port = u16::from_be(((v >> 16) & 0xffff) as u16);
    let ip = ((v >> 32) as u32).to_le_bytes();
    Some((port, ip))
}

/// Lowest unbound local port >= EPHEM_BASE.
fn ephemeral_port() -> Option<u16> {
    let socks = unsafe { &*core::ptr::addr_of!(UDPSOCKS) };
    (EPHEM_BASE..=u16::MAX)
        .find(|&p| !socks.iter().flatten().any(|s| s.port == p))
}

/// RX-side demux, called from e1000::drain_desc on the live ring.
/// A blocked recvfrom gets the datagram copied straight into its
/// buffer (through ITS tables); otherwise it queues one deep —
/// a second arrival displaces it, honest UDP loss.
pub fn udp_rx(src_ip: [u8; 4], sport: u16, dport: u16, p: &[u8]) {
    let socks = unsafe { &mut *core::ptr::addr_of_mut!(UDPSOCKS) };
    let Some(si) = socks.iter().position(|s| {
        s.as_ref().is_some_and(|k| k.port == dport)
    }) else {
        return; // nobody bound to this port — drop
    };
    // A blocked receiver first: payload + sockaddr straight through
    // the sleeper's tables, count into its rax — same wake shape as
    // the pipe paths.
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state == TaskState::Blocked && t.wait_on == WaitOn::UdpRecv(si) {
            let n = core::cmp::min(p.len(), t.wait_len as usize);
            write_task_bytes(t.pml4, t.wait_ptr, &p[..n]);
            if t.wait_ptr2 != 0 {
                write_task_bytes(
                    t.pml4,
                    t.wait_ptr2,
                    &sockaddr_in(sport, src_ip),
                );
            }
            t.ctx.rax = n as u64;
            t.wait_on = WaitOn::None;
            t.wait_deadline = 0;
            t.wait_ptr2 = 0;
            t.state = TaskState::Runnable;
            return;
        }
    }
    // Queue one deep — an occupied slot drops the new datagram.
    let Some(sock) = socks[si].as_mut() else { return };
    if sock.q.is_none() {
        let mut d = Dgram {
            src: src_ip,
            sport,
            len: 0,
            data: [0; MAX_DGRAM],
        };
        let n = core::cmp::min(p.len(), MAX_DGRAM);
        d.data[..n].copy_from_slice(&p[..n]);
        d.len = n as u16;
        sock.q = Some(d);
    }
    wake_pollers(); // POLLIN may have just become true
}

/// Shared recv path for recvfrom() and read() on a socket fd —
/// `sa` is the sockaddr_out VA (0 = none). Queued dgram or block;
/// the waker fills buf/sa through our tables.
fn udp_recv_or_block(
    ctx: &mut UCtx,
    si: usize,
    buf: u64,
    len: u64,
    sa: u64,
) -> u64 {
    const EINTR_: u64 = (-4i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    // A datagram already in the ring? Drain once before looking.
    crate::e1000::rx_drain();
    let take = {
        let socks = unsafe { &mut *core::ptr::addr_of_mut!(UDPSOCKS) };
        socks.get_mut(si).and_then(|s| s.as_mut()).and_then(|k| k.q.take())
    };
    if let Some(d) = take {
        let n = core::cmp::min(len as usize, d.len as usize);
        if n > 0 && !range_ok_w(buf, n as u64) {
            return EFAULT;
        }
        if sa != 0 && !range_ok_w(sa, 16) {
            return EFAULT;
        }
        // SAFETY: both ranges validated writable under the current
        // (this task's) page table.
        unsafe {
            core::ptr::copy_nonoverlapping(
                d.data.as_ptr(),
                buf as *mut u8,
                n,
            );
        }
        if sa != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    sockaddr_in(d.sport, d.src).as_ptr(),
                    sa as *mut u8,
                    16,
                );
            }
        }
        return n as u64;
    }
    // Empty queue — validate the targets BEFORE sleeping, then
    // block: the demux writes through OUR tables on arrival.
    if !range_ok_w(buf, len.min(MAX_DGRAM as u64).max(1)) {
        return EFAULT;
    }
    if sa != 0 && !range_ok_w(sa, 16) {
        return EFAULT;
    }
    if deliverable(t) {
        return EINTR_; // a signal landed before we slept
    }
    t.wait_on = WaitOn::UdpRecv(si);
    t.wait_ptr = buf;
    t.wait_len = len.min(MAX_DGRAM as u64);
    t.wait_ptr2 = sa;
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// socket(AF_INET, type): a kernel endpoint behind an OFD, so it
/// dups/fork-shares like every other fd. SOCK_DGRAM -> UDPSOCKS,
/// SOCK_STREAM -> TCPSOCKS in the Init state.
pub fn fd_socket(domain: u64, ty: u64, _proto: u64) -> u64 {
    const EAFNOSUPPORT: u64 = (-97i64) as u64;
    const ENFILE_: u64 = (-23i64) as u64;
    if domain != 2 || !(ty == 1 || ty == 2) {
        return EAFNOSUPPORT;
    }
    let Some(t) = cur_mut() else { return EBADF };
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    let (Some(oi), Some(fi)) = (
        ofds.iter().position(Option::is_none),
        t.fds.iter().position(Option::is_none),
    ) else {
        return EMFILE;
    };
    let kind = if ty == 2 {
        let socks = unsafe { &mut *core::ptr::addr_of_mut!(UDPSOCKS) };
        let Some(si) = socks.iter().position(Option::is_none) else {
            return ENFILE_;
        };
        socks[si] = Some(UdpSock { port: 0, q: None });
        FdKind::Udp(si)
    } else {
        let socks = tcp_socks();
        let Some(si) = socks.iter().position(Option::is_none) else {
            return ENFILE_;
        };
        socks[si] = Some(TcpSock {
            port: 0,
            peer_ip: [0; 4],
            peer_port: 0,
            peer_mac: [0; 6],
            state: TcpState::Init,
            snd_nxt: 0,
            rcv_nxt: 0,
            fin_acked: false,
            fin_rx: false,
            backlog: [usize::MAX; TCP_BACKLOG],
            blen: 0,
            parent: usize::MAX,
            orphan: false,
            dead: false,
            rxb: [0; TCP_RX_CAP],
            rxl: 0,
        });
        FdKind::Tcp(si)
    };
    ofds[oi] = Some(OpenFile { kind, pos: 0, refs: 1 });
    t.fds[fi] = Some(oi as u8);
    fi as u64
}

/// bind(fd, sockaddr_in, len): claim a local port for a socket —
/// required before its datagrams demux. Port 0 picks an ephemeral.
pub fn fd_bind(fd: u64, sa: u64, len: u64) -> u64 {
    const EAFNOSUPPORT: u64 = (-97i64) as u64;
    const EADDRINUSE: u64 = (-98i64) as u64;
    const EADDRNOTAVAIL: u64 = (-99i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    if len < 8 {
        return EINVAL;
    }
    if !range_ok(sa, 8) {
        return EFAULT;
    }
    enum SockId {
        U(usize),
        T(usize),
    }
    let Some(sid) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Udp(i)) => Some(SockId::U(*i)),
        Some(FdKind::Tcp(i)) => Some(SockId::T(*i)),
        _ => None,
    }) else {
        return EBADF;
    };
    let Some((port, _ip)) = parse_sockaddr(t.pml4, sa) else {
        return EAFNOSUPPORT;
    };
    match sid {
        SockId::U(si) => {
            let socks = unsafe { &mut *core::ptr::addr_of_mut!(UDPSOCKS) };
            if socks[si].as_ref().is_none_or(|k| k.port != 0) {
                // None can't happen (the OFD pins the slot); a
                // nonzero port is a re-bind — EINVAL like Linux.
                return EINVAL;
            }
            let p = if port == 0 {
                let Some(e) = ephemeral_port() else {
                    return EADDRNOTAVAIL;
                };
                e
            } else {
                if socks.iter().flatten().any(|s| s.port == port) {
                    return EADDRINUSE;
                }
                port
            };
            if let Some(k) = socks[si].as_mut() {
                k.port = p;
            }
        }
        SockId::T(si) => {
            let socks = tcp_socks();
            if socks[si].as_ref().is_none_or(|k| k.port != 0) {
                return EINVAL;
            }
            let p = if port == 0 {
                let Some(e) = tcp_ephemeral() else {
                    return EADDRNOTAVAIL;
                };
                e
            } else {
                if socks.iter().flatten().any(|s| s.port == port) {
                    return EADDRINUSE;
                }
                port
            };
            if let Some(k) = socks[si].as_mut() {
                k.port = p;
            }
        }
    }
    0
}

/// sendto(fd, buf, len, flags, sockaddr_in, alen): one datagram out —
/// the destination MAC resolves via the ARP cache/gateway.
pub fn fd_sendto(
    fd: u64,
    buf: u64,
    len: u64,
    _flags: u64,
    sa: u64,
    alen: u64,
) -> u64 {
    const EAFNOSUPPORT: u64 = (-97i64) as u64;
    const EHOSTUNREACH: u64 = (-113i64) as u64;
    const EADDRNOTAVAIL: u64 = (-99i64) as u64;
    const EMSGSIZE: u64 = (-90i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    // sendto on a stream socket is a plain send; a non-null
    // destination on a connected socket is EISCONN like Linux.
    match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Tcp(i)) => {
            const EISCONN: u64 = (-106i64) as u64;
            return if sa == 0 { tcp_send(*i, buf, len) } else { EISCONN };
        }
        Some(FdKind::Udp(_)) => {}
        _ => return EBADF,
    }
    let Some(si) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Udp(i)) => Some(*i),
        _ => None,
    }) else {
        return EBADF;
    };
    if sa == 0 || alen < 8 {
        return EINVAL;
    }
    if !range_ok(sa, 8) {
        return EFAULT;
    }
    if len == 0 {
        return EINVAL;
    }
    // One ethernet frame: 14 + 20 + 8 + payload must fit a 1600B
    // staging buffer in udp_tx.
    if len > 1400 {
        return EMSGSIZE;
    }
    if !range_ok(buf, len) {
        return EFAULT;
    }
    let Some((dport, dip)) = parse_sockaddr(t.pml4, sa) else {
        return EAFNOSUPPORT;
    };
    if !crate::e1000::net_up() {
        return EHOSTUNREACH;
    }
    // Auto-bind on first send, like Linux.
    let sport = {
        let socks = unsafe { &mut *core::ptr::addr_of_mut!(UDPSOCKS) };
        let Some(k) = socks[si].as_mut() else { return EBADF };
        if k.port == 0 {
            let Some(e) = ephemeral_port() else {
                return EADDRNOTAVAIL;
            };
            k.port = e;
        }
        k.port
    };
    // Stage the payload kernel-side — the user buffer is read once.
    let mut pay = [0u8; 1400];
    // SAFETY: buf validated readable for len bytes under the
    // current page table.
    unsafe {
        core::ptr::copy_nonoverlapping(
            buf as *const u8,
            pay.as_mut_ptr(),
            len as usize,
        );
    }
    // Resolve the link-layer target: broadcast -> ff:ff; on-subnet
    // -> ARP the host; off-subnet or unresolved -> the gateway.
    let our = crate::e1000::our_ip();
    let mask = crate::e1000::our_mask();
    let mac = if dip == [0xff; 4] {
        Some([0xff; 6])
    } else {
        let on_link = (0..4).all(|i| dip[i] & mask[i] == our[i] & mask[i]);
        let r = if on_link {
            crate::e1000::arp_resolve(dip)
        } else {
            None
        };
        r.or_else(crate::e1000::gw_mac)
    };
    let Some(mac) = mac else { return EHOSTUNREACH };
    if !crate::e1000::udp_tx(mac, dip, sport, dport, &pay[..len as usize]) {
        return (-5i64) as u64; // EIO — tx ring rejected
    }
    len
}

/// recvfrom(fd, buf, len, flags, sockaddr_out, alen): the next queued
/// datagram, or block until the demux wakes us. alen is accepted for
/// ABI shape — the sockaddr is always written at full 16 bytes when
/// requested.
pub fn fd_recvfrom(
    ctx: &mut UCtx,
    fd: u64,
    buf: u64,
    len: u64,
    _flags: u64,
    sa: u64,
    _alen: u64,
) -> u64 {
    let Some(t) = cur_mut() else { return EBADF };
    match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Udp(i)) => udp_recv_or_block(ctx, *i, buf, len, sa),
        Some(FdKind::Tcp(i)) => tcp_recv_or_block(ctx, *i, buf, len, sa),
        _ => EBADF,
    }
}

// ---- TCP sockets (SOCK_STREAM) ----
//
// Minimal in-order stack: no retransmit, no out-of-order queue —
// out-of-window segments drop and that's honest loss. States are
// the textbook ones. Accepted children are first-class TCPSOCKS
// slots; the listener's backlog holds only indices, so nothing is
// cloned and there is no association to inherit (CVE-2026-99116's
// accept-clone UAF class is absent by design). Listener state is
// never carried across calls either — no stale-error accepts
// (CVE-2026-90138 class). ISNs come from the same SipHash-
// keyed construction as the wire selftest.

/// Bounds: endpoints, per-conn stream buffer, accept backlog.
const MAX_TCPS: usize = 8;
const TCP_RX_CAP: usize = 2048;
const TCP_BACKLOG: usize = 4;
/// connect() timeout, ~10s at 100Hz ticks.
const TCP_CONN_TICKS: u64 = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TcpState {
    /// Fresh socket — bound or not, never connected.
    Init,
    Listen,
    /// connect() sent SYN, awaiting SYN-ACK.
    SynSent,
    /// Listener child: SYN seen, SYN-ACK sent, awaiting the ACK.
    SynRecv,
    Estab,
    /// Peer's FIN arrived — our send side still open.
    CloseWait,
    /// We sent FIN; awaiting its ACK and/or the peer's FIN.
    FinWait,
    /// FINs crossed: ours ACKs the peer's, awaiting ours.
    LastAck,
    Closed,
}

struct TcpSock {
    port: u16,
    peer_ip: [u8; 4],
    peer_port: u16,
    peer_mac: [u8; 6],
    state: TcpState,
    snd_nxt: u32,
    rcv_nxt: u32,
    /// Our FIN was ACKed (FinWait/LastAck bookkeeping).
    fin_acked: bool,
    /// Peer FIN'd — reads hit EOF once the buffer drains.
    fin_rx: bool,
    /// Listener: established children not yet accept()ed.
    backlog: [usize; TCP_BACKLOG],
    blen: usize,
    /// SynRecv child: the listener slot that spawned us.
    parent: usize,
    /// All fds gone — teardown completes asynchronously.
    orphan: bool,
    /// RST seen — reads fail ECONNRESET, writes EPIPE.
    dead: bool,
    rxb: [u8; TCP_RX_CAP],
    rxl: usize,
}

static mut TCPSOCKS: [Option<TcpSock>; MAX_TCPS] = [const { None }; MAX_TCPS];

fn tcp_socks() -> &'static mut [Option<TcpSock>; MAX_TCPS] {
    unsafe { &mut *core::ptr::addr_of_mut!(TCPSOCKS) }
}

/// Predictable-ISN rejection: SipHash of the 4-tuple + a timestamp
/// under the per-boot key — same construction as tcp_selftest.
fn tcp_isn(dip: [u8; 4], sport: u16, dport: u16) -> u32 {
    let our = crate::e1000::our_ip();
    let t = unsafe { core::arch::x86_64::_rdtsc() } as u32;
    klib::hash::siphash_3u32(
        u32::from_be_bytes(our) ^ u32::from_be_bytes(dip),
        ((sport as u32) << 16) | dport as u32,
        t,
        crate::rand::isn_key(),
    ) as u32
}

/// Lowest unbound local TCP port >= EPHEM_BASE (separate namespace
/// from UDP, like Linux).
fn tcp_ephemeral() -> Option<u16> {
    let socks = unsafe { &*core::ptr::addr_of!(TCPSOCKS) };
    (EPHEM_BASE..=u16::MAX)
        .find(|&p| !socks.iter().flatten().any(|s| s.port == p))
}

/// Emit one segment for a connection slot.
fn tcp_seg(
    k: &TcpSock,
    flags: u8,
    payload: &[u8],
) -> bool {
    crate::e1000::tcp_tx(
        k.peer_mac, k.peer_ip, k.port, k.peer_port, k.snd_nxt,
        k.rcv_nxt, flags, payload,
    )
}

/// RST answering a segment nobody owns — never RST an RST.
#[allow(clippy::too_many_arguments)]
fn tcp_rst(
    src_ip: [u8; 4],
    src_mac: [u8; 6],
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    plen: usize,
) {
    if flags & crate::net::TCP_RST != 0 {
        return;
    }
    let (s, a, fl) = if flags & crate::net::TCP_ACK != 0 {
        (ack, 0, crate::net::TCP_RST)
    } else {
        let used = plen as u32
            + (flags & crate::net::TCP_SYN != 0) as u32
            + (flags & crate::net::TCP_FIN != 0) as u32;
        (0, seq.wrapping_add(used), crate::net::TCP_RST | crate::net::TCP_ACK)
    };
    let _ = crate::e1000::tcp_tx(
        src_mac, src_ip, dport, sport, s, a, fl, &[],
    );
}

/// Drop the first runnable task sleeping on `on`, hand it `ret`
/// and mark it Runnable — the generic non-data wake.
fn wake_waiter(on: WaitOn, ret: u64) {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state == TaskState::Blocked && t.wait_on == on {
            t.ctx.rax = ret;
            t.wait_on = WaitOn::None;
            t.wait_deadline = 0;
            t.state = TaskState::Runnable;
            return;
        }
    }
}

/// Mint a Tcp OFD+fd inside task `t` for connection `ci`.
/// Returns the fd, or EMFILE/ENFILE without consuming `ci`.
fn tcp_mint_fd(t: &mut Task, ci: usize) -> Result<u64, u64> {
    const ENFILE_: u64 = (-23i64) as u64;
    let ofds = unsafe { &mut *core::ptr::addr_of_mut!(OFDS) };
    let Some(oi) = ofds.iter().position(Option::is_none) else {
        return Err(ENFILE_);
    };
    let Some(fi) = t.fds.iter().position(Option::is_none) else {
        return Err(EMFILE);
    };
    ofds[oi] = Some(OpenFile {
        kind: FdKind::Tcp(ci),
        pos: 0,
        refs: 1,
    });
    t.fds[fi] = Some(oi as u8);
    Ok(fi as u64)
}

/// A connection reached Estab or died — wake a blocked accept() on
/// listener `li` by minting its fd inside the sleeper's table.
fn wake_accept(li: usize) {
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state == TaskState::Blocked && t.wait_on == WaitOn::TcpAccept(li) {
            let socks = tcp_socks();
            let Some(k) = socks[li].as_mut() else { return };
            if k.blen == 0 {
                continue;
            }
            let ci = k.backlog[0];
            for j in 1..k.blen {
                k.backlog[j - 1] = k.backlog[j];
            }
            k.blen -= 1;
            match tcp_mint_fd(t, ci) {
                Ok(fd) => t.ctx.rax = fd,
                Err(e) => {
                    k.backlog[k.blen] = ci;
                    k.blen += 1;
                    t.ctx.rax = e;
                }
            }
            if t.wait_ptr2 != 0 {
                // sockaddr_out requested — the peer's address.
                let socks = unsafe { &*core::ptr::addr_of!(TCPSOCKS) };
                if let Some(c) = socks[ci].as_ref() {
                    let _ = write_task_bytes(
                        t.pml4,
                        t.wait_ptr2,
                        &sockaddr_in(c.peer_port, c.peer_ip),
                    );
                }
                t.wait_ptr2 = 0;
            }
            t.wait_on = WaitOn::None;
            t.state = TaskState::Runnable;
            return;
        }
    }
}

/// Feed a blocked TcpRecv sleeper straight from the connection's
/// stream buffer through ITS page tables — or EOF/error without
/// data, matching what a re-run recv() would return.
fn wake_tcp_recv(ci: usize) {
    const ECONNRESET: u64 = (-104i64) as u64;
    let tasks = unsafe { &mut *core::ptr::addr_of_mut!(TASKS) };
    for s in tasks.iter_mut() {
        let Some(t) = s.as_mut() else { continue };
        if t.state == TaskState::Blocked && t.wait_on == WaitOn::TcpRecv(ci) {
            let socks = tcp_socks();
            let Some(k) = socks[ci].as_mut() else { return };
            if k.dead {
                t.ctx.rax = ECONNRESET;
            } else if k.rxl > 0 {
                let n = core::cmp::min(k.rxl, t.wait_len as usize);
                if !write_task_bytes(t.pml4, t.wait_ptr, &k.rxb[..n]) {
                    return; // sleeper's pages vanished — leave it
                }
                k.rxb.copy_within(n..k.rxl, 0);
                k.rxl -= n;
                t.ctx.rax = n as u64;
            } else if k.fin_rx {
                t.ctx.rax = 0;
            } else {
                return;
            }
            if t.wait_ptr2 != 0 {
                // recvfrom asked for the peer's sockaddr.
                let _ = write_task_bytes(
                    t.pml4,
                    t.wait_ptr2,
                    &sockaddr_in(k.peer_port, k.peer_ip),
                );
                t.wait_ptr2 = 0;
            }
            t.wait_on = WaitOn::None;
            t.wait_deadline = 0;
            t.state = TaskState::Runnable;
            return;
        }
    }
}

/// Free a connection slot, unlinking it from a listener backlog.
fn tcp_free(ci: usize) {
    let socks = tcp_socks();
    let parent = socks[ci].as_ref().map_or(usize::MAX, |k| k.parent);
    socks[ci] = None;
    if parent != usize::MAX {
        if let Some(l) = socks[parent].as_mut() {
            if let Some(j) = l.backlog[..l.blen].iter().position(|&c| c == ci)
            {
                for x in j + 1..l.blen {
                    l.backlog[x - 1] = l.backlog[x];
                }
                l.blen -= 1;
            }
        }
    }
}

/// Per-connection segment handling — the state machine proper.
/// `src_mac` is the sender's link address, kept as peer_mac so
/// replies track the freshest ARP truth.
#[allow(clippy::too_many_arguments)]
fn tcp_rx_conn(
    ci: usize,
    _src_ip: [u8; 4],
    src_mac: [u8; 6],
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    let socks = tcp_socks();
    let Some(k) = socks[ci].as_mut() else { return };
    k.peer_mac = src_mac;
    if flags & crate::net::TCP_RST != 0 {
        let was = k.state;
        k.dead = true;
        k.state = TcpState::Closed;
        if k.orphan || matches!(was, TcpState::SynRecv) {
            tcp_free(ci); // nothing else references it
            return;
        }
        wake_waiter(WaitOn::TcpConn(ci), (-111i64) as u64); // ECONNREFUSED
        wake_tcp_recv(ci);
        wake_pollers();
        return;
    }
    match k.state {
        TcpState::SynSent => {
            if flags & crate::net::TCP_SYN != 0
                && flags & crate::net::TCP_ACK != 0
                && ack == k.snd_nxt
            {
                k.rcv_nxt = seq.wrapping_add(1);
                k.state = TcpState::Estab;
                let _ = tcp_seg(k, crate::net::TCP_ACK, &[]);
                wake_waiter(WaitOn::TcpConn(ci), 0);
                wake_pollers();
            }
        }
        TcpState::SynRecv => {
            if flags & crate::net::TCP_ACK != 0 && ack == k.snd_nxt {
                let li = k.parent;
                let ok = {
                    let l = socks[li].as_mut();
                    l.is_some_and(|l| {
                        if l.state == TcpState::Listen && l.blen < TCP_BACKLOG {
                            l.backlog[l.blen] = ci;
                            l.blen += 1;
                            true
                        } else {
                            false
                        }
                    })
                };
                if ok {
                    socks[ci].as_mut().unwrap().state = TcpState::Estab;
                    wake_accept(li);
                    wake_pollers();
                } else {
                    // Listener gone or backlog full — RST the child.
                    let (pm, pi, sp, dp, sn) = {
                        let k = socks[ci].as_ref().unwrap();
                        (k.peer_mac, k.peer_ip, k.port, k.peer_port,
                         k.snd_nxt)
                    };
                    let _ = crate::e1000::tcp_tx(
                        pm, pi, sp, dp, sn, 0, crate::net::TCP_RST, &[],
                    );
                    tcp_free(ci);
                }
            }
        }
        TcpState::Estab | TcpState::CloseWait | TcpState::FinWait => {
            // ACK bookkeeping for our FIN before data handling.
            if ack == k.snd_nxt && k.state == TcpState::FinWait {
                k.fin_acked = true;
            }
            let mut close_me = false;
            if seq == k.rcv_nxt {
                let mut adv = 0u32;
                if !payload.is_empty()
                    && k.rxl + payload.len() <= TCP_RX_CAP
                {
                    k.rxb[k.rxl..k.rxl + payload.len()]
                        .copy_from_slice(payload);
                    k.rxl += payload.len();
                    adv += payload.len() as u32;
                }
                if flags & crate::net::TCP_FIN != 0 {
                    // FIN only counts once preceding bytes are ours.
                    if adv as usize == payload.len() {
                        adv += 1;
                        k.fin_rx = true;
                    }
                }
                if adv > 0 {
                    k.rcv_nxt = k.rcv_nxt.wrapping_add(adv);
                    let _ = tcp_seg(k, crate::net::TCP_ACK, &[]);
                }
                if k.fin_rx && k.state == TcpState::Estab {
                    k.state = TcpState::CloseWait;
                } else if k.fin_rx
                    && k.state == TcpState::FinWait
                    && k.fin_acked
                {
                    k.state = TcpState::Closed;
                    close_me = k.orphan;
                }
                wake_tcp_recv(ci);
                wake_pollers();
            }
            if close_me {
                tcp_free(ci);
            }
        }
        TcpState::LastAck if ack == k.snd_nxt => {
            let orphan = k.orphan;
            k.state = TcpState::Closed;
            if orphan {
                tcp_free(ci);
            } else {
                wake_tcp_recv(ci);
                wake_pollers();
            }
        }
        _ => {}
    }
}

/// TCP demux entry — wire path via e1000::ip_demux, loopback via
/// the same demux on injected frames. First match wins: an
/// established-ish 4-tuple connection, else a listener takes SYNs,
/// else we answer RST like a closed port should.
#[allow(clippy::too_many_arguments)]
pub fn tcp_rx(
    src_ip: [u8; 4],
    src_mac: [u8; 6],
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    let socks = tcp_socks();
    if let Some(ci) = socks.iter().position(|s| {
        s.as_ref().is_some_and(|k| {
            !matches!(k.state, TcpState::Listen | TcpState::Init)
                && k.port == dport
                && k.peer_port == sport
                && k.peer_ip == src_ip
        })
    }) {
        tcp_rx_conn(ci, src_ip, src_mac, seq, ack, flags, payload);
        return;
    }
    if flags & crate::net::TCP_SYN != 0 && flags & crate::net::TCP_ACK == 0 {
        let Some(li) = socks.iter().position(|s| {
            s.as_ref().is_some_and(|k| {
                k.state == TcpState::Listen && k.port == dport
            })
        }) else {
            // SYN to a closed port — refuse, so connect() sees it.
            tcp_rst(src_ip, src_mac, sport, dport, seq, ack, flags,
                    payload.len());
            return;
        };
        // Bound half-open + accepted children per listener.
        let kids = socks.iter().flatten()
            .filter(|k| k.parent == li).count();
        let Some(ci) = socks.iter().position(Option::is_none) else {
            return; // no slots — drop
        };
        if kids >= TCP_BACKLOG {
            return; // backlog bound — honest drop
        }
        let isn = tcp_isn(src_ip, dport, sport);
        socks[ci] = Some(TcpSock {
            port: dport,
            peer_ip: src_ip,
            peer_port: sport,
            peer_mac: src_mac,
            state: TcpState::SynRecv,
            snd_nxt: isn.wrapping_add(1),
            rcv_nxt: seq.wrapping_add(1),
            fin_acked: false,
            fin_rx: false,
            backlog: [usize::MAX; TCP_BACKLOG],
            blen: 0,
            parent: li,
            orphan: false,
            dead: false,
            rxb: [0; TCP_RX_CAP],
            rxl: 0,
        });
        // SYN-ACK from the child: seq=isn, ack=their seq+1.
        let _ = crate::e1000::tcp_tx(
            src_mac, src_ip, dport, sport, isn,
            seq.wrapping_add(1),
            crate::net::TCP_SYN | crate::net::TCP_ACK, &[],
        );
        // If the peer vanishes the child reaps when the listener
        // closes; the ACK completing it moves it to the backlog.
        return;
    }
    // Nothing to catch it — closed-port RST.
    tcp_rst(src_ip, src_mac, sport, dport, seq, ack, flags,
            payload.len());
}

/// Initiate a connection: SYN out, then block (with a ~10s
/// deadline) for the SYN-ACK — unless loopback completed it inline.
pub fn fd_connect(ctx: &mut UCtx, fd: u64, sa: u64, len: u64) -> u64 {
    const EAFNOSUPPORT: u64 = (-97i64) as u64;
    const EADDRNOTAVAIL: u64 = (-99i64) as u64;
    const ECONNREFUSED: u64 = (-111i64) as u64;
    const EHOSTUNREACH: u64 = (-113i64) as u64;
    const EISCONN: u64 = (-106i64) as u64;
    const EINTR_: u64 = (-4i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    if len < 8 || !range_ok(sa, 8) {
        return EFAULT;
    }
    let Some(si) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Tcp(i)) => Some(*i),
        _ => None,
    }) else {
        return EBADF;
    };
    let Some((dport, dip)) = parse_sockaddr(t.pml4, sa) else {
        return EAFNOSUPPORT;
    };
    if !crate::e1000::net_up() {
        return EHOSTUNREACH;
    }
    let Some(mac) = crate::e1000::route_mac(dip) else {
        return EHOSTUNREACH;
    };
    // Loopback peers canonicalize to our own address — replies on
    // the injected path carry our IP as their source, so the
    // 4-tuple match stays exact.
    let peer = if dip[0] == 127 { crate::e1000::our_ip() } else { dip };
    {
        let socks = tcp_socks();
        let Some(k) = socks[si].as_mut() else { return EBADF };
        if k.state != TcpState::Init {
            return EISCONN; // already connected/connecting
        }
        if k.port == 0 {
            let Some(e) = tcp_ephemeral() else {
                return EADDRNOTAVAIL;
            };
            k.port = e;
        }
        k.peer_ip = peer;
        k.peer_port = dport;
        k.peer_mac = mac;
        k.snd_nxt = tcp_isn(peer, k.port, dport).wrapping_add(1);
        k.state = TcpState::SynSent;
        let isn = k.snd_nxt.wrapping_sub(1);
        let _ = crate::e1000::tcp_tx(
            mac, dip, k.port, dport, isn, 0, crate::net::TCP_SYN, &[],
        );
        // Loopback may have completed the handshake inside the tx.
        match k.state {
            TcpState::Estab => return 0,
            _ if k.dead => return ECONNREFUSED,
            _ => {}
        }
    }
    let Some(t) = cur_mut() else { return EBADF };
    if deliverable(t) {
        return EINTR_;
    }
    t.wait_on = WaitOn::TcpConn(si);
    t.wait_deadline = crate::apic::TICKS
        .load(core::sync::atomic::Ordering::Relaxed)
        + TCP_CONN_TICKS;
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// listen(fd, backlog): a bound Init socket becomes passive.
/// backlog is capped at our queue depth, like Linux clamps it.
pub fn fd_listen(fd: u64, backlog: u64) -> u64 {
    const EADDRNOTAVAIL: u64 = (-99i64) as u64;
    const EOPNOTSUPP: u64 = (-95i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    let Some(si) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Tcp(i)) => Some(*i),
        _ => None,
    }) else {
        return EOPNOTSUPP; // non-TCP or bad fd
    };
    let socks = tcp_socks();
    let Some(k) = socks[si].as_mut() else { return EOPNOTSUPP };
    match k.state {
        TcpState::Listen => {
            k.blen = k.blen.min(TCP_BACKLOG.min(backlog as usize));
            return 0; // re-listen just shrinks the queue
        }
        TcpState::Init => {}
        _ => return EINVAL, // listened after connecting — EINVAL
    }
    if k.port == 0 {
        let Some(e) = tcp_ephemeral() else {
            return EADDRNOTAVAIL;
        };
        k.port = e;
    }
    k.blen = 0;
    k.state = TcpState::Listen;
    0
}

/// accept(fd, sockaddr_out, alen): the oldest established child
/// becomes a fresh fd; empty backlog blocks until one completes.
pub fn fd_accept(ctx: &mut UCtx, fd: u64, sa: u64, _alen: u64) -> u64 {
    const EOPNOTSUPP: u64 = (-95i64) as u64;
    const EINTR_: u64 = (-4i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    if sa != 0 && !range_ok_w(sa, 16) {
        return EFAULT;
    }
    let Some(li) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Tcp(i)) => Some(*i),
        _ => None,
    }) else {
        return EOPNOTSUPP;
    };
    crate::e1000::rx_drain();
    let socks = tcp_socks();
    let Some(k) = socks[li].as_mut() else { return EOPNOTSUPP };
    if k.state != TcpState::Listen {
        return EINVAL;
    }
    if k.blen > 0 {
        let ci = k.backlog[0];
        for j in 1..k.blen {
            k.backlog[j - 1] = k.backlog[j];
        }
        k.blen -= 1;
        let (pip, pport) = {
            let c = socks[ci].as_ref().unwrap();
            (c.peer_ip, c.peer_port)
        };
        let Some(t) = cur_mut() else { return EBADF };
        let fd = match tcp_mint_fd(t, ci) {
            Ok(f) => f,
            Err(e) => {
                let k = tcp_socks()[li].as_mut().unwrap();
                k.backlog[k.blen] = ci;
                k.blen += 1;
                return e;
            }
        };
        if sa != 0 {
            // SAFETY: sa validated writable above; all 16 bytes
            // come from the zeroed sockaddr_in.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    sockaddr_in(pport, pip).as_ptr(),
                    sa as *mut u8,
                    16,
                );
            }
        }
        return fd;
    }
    let Some(t) = cur_mut() else { return EBADF };
    if deliverable(t) {
        return EINTR_;
    }
    t.wait_on = WaitOn::TcpAccept(li);
    t.wait_ptr2 = sa;
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// Stream receive shared by read()/recvfrom() on a TCP fd.
/// `sa` (when nonzero) gets the peer's sockaddr.
fn tcp_recv_or_block(
    ctx: &mut UCtx,
    si: usize,
    buf: u64,
    len: u64,
    sa: u64,
) -> u64 {
    const ENOTCONN: u64 = (-107i64) as u64;
    const ECONNRESET: u64 = (-104i64) as u64;
    const EINTR_: u64 = (-4i64) as u64;
    if cur_mut().is_none() {
        return EBADF;
    }
    crate::e1000::rx_drain();
    {
        let socks = tcp_socks();
        let Some(k) = socks[si].as_mut() else { return EBADF };
        if k.dead {
            return ECONNRESET;
        }
        match k.state {
            TcpState::Estab
            | TcpState::CloseWait
            | TcpState::FinWait
            | TcpState::LastAck
            | TcpState::Closed => {}
            _ => return ENOTCONN, // no data stream yet
        }
        if k.rxl > 0 {
            let n = core::cmp::min(k.rxl, len as usize);
            if n > 0 && !range_ok_w(buf, n as u64) {
                return EFAULT;
            }
            // SAFETY: buf validated writable under this task's
            // page table; rxb holds received bytes only.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    k.rxb.as_ptr(),
                    buf as *mut u8,
                    n,
                );
            }
            k.rxb.copy_within(n..k.rxl, 0);
            k.rxl -= n;
            return n as u64;
        }
        if k.fin_rx || k.state == TcpState::Closed {
            return 0; // EOF
        }
    }
    if !range_ok_w(buf, len.min(TCP_RX_CAP as u64).max(1)) {
        return EFAULT;
    }
    let Some(t) = cur_mut() else { return EBADF };
    if deliverable(t) {
        return EINTR_;
    }
    t.wait_on = WaitOn::TcpRecv(si);
    t.wait_ptr = buf;
    t.wait_len = len.min(TCP_RX_CAP as u64);
    t.wait_ptr2 = sa;
    t.ctx = *ctx;
    t.state = TaskState::Blocked;
    block_switch()
}

/// Stream send shared by write()/sendto() on a TCP fd.
fn tcp_send(si: usize, buf: u64, len: u64) -> u64 {
    const ENOTCONN: u64 = (-107i64) as u64;
    const EMSGSIZE: u64 = (-90i64) as u64;
    const EPIPE_: u64 = (-32i64) as u64;
    if cur_mut().is_none() {
        return EBADF;
    }
    if len == 0 {
        return 0;
    }
    if len > 1400 {
        return EMSGSIZE;
    }
    if !range_ok(buf, len) {
        return EFAULT;
    }
    let (state, dead) = {
        let socks = unsafe { &*core::ptr::addr_of!(TCPSOCKS) };
        match socks[si].as_ref() {
            Some(k) => (k.state, k.dead),
            None => return EBADF,
        }
    };
    if dead {
        return EPIPE_;
    }
    if !matches!(state, TcpState::Estab | TcpState::CloseWait) {
        return ENOTCONN;
    }
    // SAFETY: buf validated readable for len under this task's
    // page table; the slice is consumed by tcp_tx before any
    // re-entrant drain could disturb it.
    let pay = unsafe {
        core::slice::from_raw_parts(buf as *const u8, len as usize)
    };
    // Snapshot what the segment needs — the tx path can re-enter
    // the table (loopback drain) so no borrow crosses it.
    let (pm, pi, sp, dp, sn, rn) = {
        let socks = unsafe { &*core::ptr::addr_of!(TCPSOCKS) };
        let k = socks[si].as_ref().unwrap();
        (k.peer_mac, k.peer_ip, k.port, k.peer_port, k.snd_nxt,
         k.rcv_nxt)
    };
    if !crate::e1000::tcp_tx(
        pm, pi, sp, dp, sn, rn,
        crate::net::TCP_PSH | crate::net::TCP_ACK,
        pay,
    ) {
        return (-5i64) as u64; // EIO — tx ring rejected
    }
    let socks = tcp_socks();
    match socks[si].as_mut() {
        Some(k) => {
            k.snd_nxt = k.snd_nxt.wrapping_add(len as u32);
            len
        }
        None => EPIPE_, // loopback teardown freed it mid-send
    }
}

/// shutdown(fd, how): how&2 closes the send side with a FIN.
/// how==1 alone marks reads EOF; how==2 does both.
pub fn fd_shutdown(fd: u64, how: u64) -> u64 {
    const ENOTCONN: u64 = (-107i64) as u64;
    let Some(t) = cur_mut() else { return EBADF };
    let Some(si) = (match ofd(t, fd).map(|o| &o.kind) {
        Some(FdKind::Tcp(i)) => Some(*i),
        _ => None,
    }) else {
        return EBADF;
    };
    if how > 2 {
        return EINVAL;
    }
    let socks = tcp_socks();
    let Some(k) = socks[si].as_mut() else { return EBADF };
    if how == 0 || how == 2 {
        k.fin_rx = true; // reads EOF now
    }
    if how >= 1 {
        match k.state {
            TcpState::Estab => {
                let _ = tcp_seg(k, crate::net::TCP_FIN | crate::net::TCP_ACK, &[]);
                let k = tcp_socks()[si].as_mut().unwrap();
                k.snd_nxt = k.snd_nxt.wrapping_add(1);
                k.state = TcpState::FinWait;
            }
            TcpState::CloseWait => {
                let _ = tcp_seg(k, crate::net::TCP_FIN | crate::net::TCP_ACK, &[]);
                let k = tcp_socks()[si].as_mut().unwrap();
                k.snd_nxt = k.snd_nxt.wrapping_add(1);
                k.state = TcpState::LastAck;
            }
            _ => return ENOTCONN,
        }
    }
    0
}

/// Last-fd teardown for a TCP socket: FIN the live connection and
/// orphan it to finish asynchronously; RST the half-open; listeners
/// drop their incomplete children. Established backlog children
/// survive — they're connections in their own right.
fn tcp_sock_close(si: usize) {
    enum A {
        Free,
        Fin(TcpState),
        Orphan,
    }
    let socks = tcp_socks();
    let Some(k) = socks[si].as_mut() else { return };
    let act = match k.state {
        TcpState::Estab => A::Fin(TcpState::FinWait),
        TcpState::CloseWait => A::Fin(TcpState::LastAck),
        TcpState::FinWait | TcpState::LastAck => A::Orphan,
        TcpState::SynSent | TcpState::SynRecv => {
            let _ = tcp_seg(k, crate::net::TCP_RST, &[]);
            A::Free
        }
        TcpState::Listen => {
            // RST + free the listener's incomplete children.
            let kids: [usize; MAX_TCPS] = {
                let mut v = [usize::MAX; MAX_TCPS];
                let mut n = 0;
                for (i, s) in socks.iter().enumerate() {
                    if s.as_ref().is_some_and(|c| c.parent == si
                        && c.state == TcpState::SynRecv)
                    {
                        v[n] = i;
                        n += 1;
                    }
                }
                v
            };
            for ci in kids {
                if ci != usize::MAX {
                    if let Some(c) = socks[ci].as_mut() {
                        let _ = tcp_seg(c, crate::net::TCP_RST, &[]);
                    }
                    tcp_free(ci);
                }
            }
            A::Free
        }
        _ => A::Free,
    };
    match act {
        A::Free => socks[si] = None,
        A::Orphan => {
            if let Some(k) = socks[si].as_mut() {
                k.orphan = true;
            }
        }
        A::Fin(next) => {
            let Some(k) = socks[si].as_mut() else { return };
            let _ = tcp_seg(k, crate::net::TCP_FIN | crate::net::TCP_ACK, &[]);
            let k = socks[si].as_mut().unwrap();
            k.snd_nxt = k.snd_nxt.wrapping_add(1);
            k.state = next;
            k.orphan = true;
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
pub fn exec(img: &[u8], ctx: &mut UCtx, args: &ExecArgs, name: &[u8]) -> bool {
    let Some(t) = cur_mut() else { return false };
    let Some(e) = elf::parse(img) else { return false };
    t.free_user_mappings();
    t.nranges = 0;
    t.stack_top = 0;
    t.fs_base = 0; // TLS dies with the old image — new one SET_FSs
    if !load_inner(t, &e, img) {
        return false;
    }
    t.name = [0; 16];
    let nl = name.len().min(15);
    t.name[..nl].copy_from_slice(&name[..nl]);
    let Some(rsp) = setup_stack(t, &e, img, args) else {
        return false;
    };
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
    *ctx = UCtx::launch(t.entry, rsp);
    true
}

// --- scheduler ----------------------------------------------------
// Round-robin over TASKS, driven by the LAPIC timer's dedicated
// dispatch path. Every switch is a UCtx resume; syscall context is
// non-preemptible (IF is masked on entry, and a tick landing on
// kernel CS never switches).

/// Enqueue `img` as a Runnable task — build its real initial stack
/// (argv0 = `name`, no env) and take the first free table slot.
/// False on bad image or full table.
pub fn spawn(img: &[u8], name: &str) -> bool {
    let Some(e) = elf::parse(img) else {
        return false;
    };
    let Some(mut t) = load(img) else {
        return false;
    };
    let mut args = ExecArgs::empty();
    let n = name.len().min(EXEC_STR_CAP - 1);
    args.argv[0][..n].copy_from_slice(&name.as_bytes()[..n]);
    args.argv_len[0] = n + 1;
    args.argc = 1;
    let nl = name.len().min(15);
    t.name[..nl].copy_from_slice(&name.as_bytes()[..nl]);
    let Some(rsp) = setup_stack(&mut t, &e, img, &args) else {
        t.reap();
        return false;
    };
    t.ctx = UCtx::launch(t.entry, rsp);
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
        // SAFETY: MSR write of a task-owned user FS base.
        unsafe { cpu::wrmsr(cpu::MSR_FS_BASE, t.fs_base) };
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
        // SAFETY: MSR write of a task-owned user FS base.
        unsafe { cpu::wrmsr(cpu::MSR_FS_BASE, t.fs_base) };
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
    // The tick saved the outgoing task's live FPU state before the
    // pick — restore the incoming task's image or it resumes with
    // the previous task's xmm registers. switch_to/launch do the
    // same; this path was missing it.
    fxrstor(&t.fx);
    // SAFETY: MSR write of a task-owned user FS base — TLS follows
    // the task across switches, same as launch/switch_to.
    unsafe { cpu::wrmsr(cpu::MSR_FS_BASE, t.fs_base) };
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
    poll_expire(); // timed poll waits age one tick
    crate::e1000::rx_drain(); // feed socket queues from the live ring
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
/// Walk `va` to its PA for a WRITABLE user page. Every caller is a
/// task-user-memory access, so the bound is applied here: the task's
/// pml4 shares kernel upper-half slots by reference, and a kernel VA
/// here would otherwise translate fine — a user-controlled pointer
/// (clone's ctid, a sigframe rsp, an argv walker) must never reach
/// kernel memory through this path.
fn translate(pml4: u64, va: u64) -> Option<u64> {
    if !(elf::USER_VA_LO..elf::USER_VA_HI).contains(&va) {
        return None;
    }
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

/// Read-side translate: same walk without the WRITE leaf gate —
/// for kernel reads of user memory (a read-only page is a legal
/// read source even though no write path may touch it). Bounded to
/// the user window for the same kernel-half reason as translate().
fn translate_r(pml4: u64, va: u64) -> Option<u64> {
    if !(elf::USER_VA_LO..elf::USER_VA_HI).contains(&va) {
        return None;
    }
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
        brk_base: p.brk_base,
        brk_cur: p.brk_cur,
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
        wait_ptr2: 0,
        wait_len: 0,
        wait_deadline: 0,
        // Dispositions inherit across fork (the Linux semantic);
        // the pending set does not.
        pending: 0,
        sigmask: p.sigmask,
        handlers: p.handlers,
        fx: p.fx,
        name: p.name,
        fs_base: p.fs_base,
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
        // Collected — reap now (Linux releases the task at wait):
        // the child's fds/map/pml4 free immediately, so shared
        // endpoint counts (pipe writers/readers) drop before we
        // return — a poll right after wait4 sees the child gone.
        t.reap();
        tasks[i] = None;
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
        if deliverable(t) {
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
