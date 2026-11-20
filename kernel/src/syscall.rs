//! SYSCALL/SYSRET path: the ring-3 <-> ring-0 boundary.
//!
//! SYSCALL itself does NOT save or switch rsp, so the entry stub's
//! first act is swapping to a dedicated kernel stack. It then
//! marshals the Linux syscall ABI (nr=rax, args rdi/rsi/rdx) into a
//! Rust call, restores the user registers sysretq needs (rcx=rip,
//! r11=rflags), and returns.
//!
//! user_enter drops to ring 3 via iretq after saving a kernel frame;
//! sys_exit unwinds that frame (user_terminate) so control returns to
//! the caller of user::run — a one-shot process model until a real
//! task scheduler exists.

use crate::{elf, log, user};
use core::arch::global_asm;

/// Kernel stack top for the CURRENT task's syscall entries, set by
/// user::run before each ring-3 drop. SYSCALL doesn't touch TSS.rsp0,
/// so the stub loads this explicitly. TSS.rsp0 separately serves
/// interrupts arriving while CPL=3.
#[no_mangle]
static mut CUR_KSTACK: u64 = 0;

/// Where the stub stashes the user rsp for the return leg.
#[no_mangle]
static mut USER_RSP: u64 = 0;
/// Kernel rsp/cr3 saved by user_enter, restored by user_terminate.
#[no_mangle]
static mut KSP: u64 = 0;
#[no_mangle]
static mut KCR3: u64 = 0;

/// Point the entry stub at `top` — the current task's private kernel
/// stack. Called by user::run immediately before each ring-3 entry;
/// the serial run-to-completion model makes the singleton sound.
pub fn set_kstack(top: u64) {
    // SAFETY: sole writer, main context, before any user code runs.
    unsafe { *core::ptr::addr_of_mut!(CUR_KSTACK) = top };
}

global_asm!(
    "
    .globl syscall_entry
syscall_entry:
    // rcx=user rip, r11=user rflags; rsp is still the USER stack.
    mov qword ptr [rip + USER_RSP], rsp
    mov rsp, qword ptr [rip + CUR_KSTACK]
    and rsp, -16               // kstack top is 16-aligned; belt&braces
    // Build a full UCtx on the kstack — every user register plus the
    // iret frame — so a syscall can copy it (fork) or rewrite it
    // (exec) with exactly the semantics a task switch resumes.
    push 0x2b                  // ss
    push qword ptr [rip + USER_RSP]
    push r11                   // rflags
    push 0x33                  // cs
    push rcx                   // rip
    push rax                   // ctx.rax = nr (handler overwrites w/ ret)
    push rcx                   // ctx.rcx = cpu-saved rip (user rcx = rip)
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11                   // ctx.r11 = user rflags value
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov rcx, rsp               // arg: &mut UCtx (Windows x64 ABI)
    call handle_syscall
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax                    // ctx.rax = the return value
    pop rcx                    // frame rip — exec may have redirected it
    add rsp, 8                 // skip cs slot
    pop r11                    // frame rflags
    pop rsp                    // back to the user stack
    sysretq

    .globl sched_launch
sched_launch:
    // First scheduler entry. Windows x64 args: rcx = &UCtx,
    // rdx = pml4. Saves the kernel context the last task's exit
    // unwinds to — Windows callee-saves rbx,rbp,rdi,rsi,r12-r15 and
    // the caller may hold live pointers in rdi/rsi across this call.
    push rbx
    push rbp
    push rdi
    push rsi
    push r12
    push r13
    push r14
    push r15
    mov rax, cr3
    mov [rip + KCR3], rax
    mov [rip + KSP], rsp
    // Fall into sched_iret — rcx/rdx still carry the arguments.
    .globl sched_iret
sched_iret:
    // The single resume path for every switch: adopt the target
    // task's page table, pop its saved GPRs, iretq into its frame.
    mov cr3, rdx
    mov rsp, rcx
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq

    .globl user_terminate
user_terminate:
    // Called from sys_exit while on the syscall stack. Unwind to the
    // frame user_enter saved: kernel page table, kernel stack, the
    // callee-saved set, then return to user::run's caller.
    mov rax, qword ptr [rip + KCR3]
    mov cr3, rax
    mov rsp, qword ptr [rip + KSP]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rsi
    pop rdi
    pop rbp
    pop rbx
    ret
"
);

unsafe extern "C" {
    fn user_terminate() -> !;
    fn syscall_entry();
    /// Resume a task ctx: cr3 = pml4, rsp = ctx, pop GPRs, iretq.
    /// Never returns — the caller's stack is abandoned.
    pub fn sched_iret(ctx: *const user::UCtx, pml4: u64) -> !;
    /// First scheduler entry — saves the kernel continuation the
    /// final exit unwinds to. Returns only via user_terminate.
    pub fn sched_launch(ctx: *const user::UCtx, pml4: u64);
}

/// Address the LSTAR MSR should point at.
pub fn entry_addr() -> u64 {
    syscall_entry as *const () as u64
}

/// Unwind to the kernel frame sched_launch saved — the last task's
/// exit path back to main. Never returns to its caller's frame.
pub fn terminate() -> ! {
    unsafe { user_terminate() }
}

// Linux syscall numbers.
const NR_READ: u64 = 0;
const NR_WRITE: u64 = 1;
const NR_OPEN: u64 = 2;
const NR_CLOSE: u64 = 3;
const NR_LSEEK: u64 = 8;
const NR_DUP: u64 = 32;
const NR_DUP2: u64 = 33;
const NR_GETPID: u64 = 39;
const NR_FORK: u64 = 57;
const NR_EXECVE: u64 = 59;
const NR_EXIT: u64 = 60;
const NR_WAIT4: u64 = 61;
const NR_GETPPID: u64 = 110;
const NR_GETDENTS: u64 = 217;

// errno values (negative returns per the kernel ABI).
const ENOENT: u64 = (-2i64) as u64;
const EBADF: u64 = (-9i64) as u64;
const EFAULT: u64 = (-14i64) as u64;
const EINVAL: u64 = (-22i64) as u64;
const ENAMETOOLONG: u64 = (-36i64) as u64;
const ENOEXEC: u64 = (-8i64) as u64;
const ENOSYS: u64 = (-38i64) as u64;

/// Longest single copy from user space — a hostile len can't stall
/// the kernel or exhaust memory.
const MAX_COPY: u64 = 4096;

/// ctx=rcx — the asm stub pushed a full UCtx and hands us &mut to it
/// (Windows x64 call convention). Args live in ctx.{rdi,rsi,rdx}; the
/// return value goes back through ctx.rax. Handlers may also copy the
/// whole ctx (fork), rewrite it (exec), block on it (wait4), or
/// diverge entirely (exit).
#[no_mangle]
extern "C" fn handle_syscall(c: *mut user::UCtx) {
    // SAFETY: the stub built this UCtx on the current task's kstack.
    let ctx = unsafe { &mut *c };
    let (nr, a1, a2, a3) = (ctx.rax, ctx.rdi, ctx.rsi, ctx.rdx);
    ctx.rax = match nr {
        // fd_read validates the write-target range itself because only
        // it knows how many bytes the source will actually produce;
        // the transfer cap is enforced here exactly like sys_write.
        NR_READ => {
            if a3 > MAX_COPY {
                EINVAL
            } else {
                user::fd_read(a1, a2, a3)
            }
        }
        NR_WRITE => sys_write(a1, a2, a3),
        NR_OPEN => sys_open(a1, a2, a3),
        NR_CLOSE => user::fd_close(a1),
        // off arrives as u64; the fd layer interprets it as i64 —
        // negative seeks are how SEEK_CUR/END move backwards.
        NR_LSEEK => user::fd_lseek(a1, a2 as i64, a3),
        NR_DUP => user::fd_dup(a1),
        NR_DUP2 => user::fd_dup2(a1, a2),
        NR_GETPID => user::getpid(),
        NR_FORK => user::fork(ctx),
        NR_WAIT4 => user::wait4(ctx, a1 as i64, a2, a3),
        NR_GETPPID => user::getppid(),
        NR_GETDENTS => user::fd_getdents(a1, a2, a3),
        NR_EXECVE => sys_execve(ctx, a1, a2, a3),
        NR_EXIT => sys_exit(a1),
        _ => ENOSYS,
    };
}

/// Copy a NUL-terminated user pathname into `name` under the pointer
/// gate — the getname discipline (CVE-2025-68814 class): byte-wise
/// validation, bounded, EFAULT on unmapped bytes, ENAMETOOLONG past
/// the cap. Returns the name length (excluding the NUL) or an errno.
fn copy_user_path(path: u64, name: &mut [u8; 257]) -> Result<usize, u64> {
    let mut n = 0usize;
    loop {
        if n == 256 {
            return Err(ENAMETOOLONG);
        }
        if !user::range_ok(path + n as u64, 1) {
            return Err(EFAULT);
        }
        // SAFETY: range_ok proved this byte is inside a mapped range.
        let b = unsafe { *((path + n as u64) as *const u8) };
        if b == 0 {
            break;
        }
        name[n] = b;
        n += 1;
    }
    Ok(n)
}

/// open(path, flags, mode): O_RDONLY only, path is a NUL-terminated
/// user string.
fn sys_open(path: u64, flags: u64, _mode: u64) -> u64 {
    if flags != 0 {
        return EINVAL;
    }
    let mut name = [0u8; 257];
    let n = match copy_user_path(path, &mut name) {
        Ok(n) => n,
        Err(e) => return e,
    };
    // Tolerate the conventional leading '/' — the archive namespace
    // is flat and root-relative.
    let name = if name[0] == b'/' { &name[1..n] } else { &name[..n] };
    user::fd_open(name)
}

/// execve(path, argv, envp): resolve the name in the initrd, verify it
/// is a loadable ELF BEFORE touching the address space, then let
/// user::exec rebuild it. argv/envp are accepted for ABI shape but
/// not yet consumed — a stack-args rung will land with fork
/// (CVE-2022-49264's argv==NULL discipline applies then).
fn sys_execve(ctx: &mut user::UCtx, path: u64, _argv: u64, _envp: u64) -> u64 {
    let mut name = [0u8; 257];
    let n = match copy_user_path(path, &mut name) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let name = if name[0] == b'/' { &name[1..n] } else { &name[..n] };
    let Some(img) = user::fs_lookup(name) else {
        return ENOENT;
    };
    // Parse before reset: a bad image must not cost the task its
    // address space — Linux returns ENOEXEC and the process lives.
    if elf::parse(img).is_none() {
        return ENOEXEC;
    }
    if !user::exec(img, ctx) {
        // The address space is already torn down; the task cannot
        // safely resume. Kill it — the run() loop reaps as usual.
        sys_exit(127);
    }
    0 // unreachable on success — sysret lands in the new image
}

/// write(fd, buf, len): console fds only, buffer fully validated
/// against the process's mapped user ranges before a single byte is
/// read — this is the trust boundary, not a convenience check.
fn sys_write(fd: u64, buf: u64, len: u64) -> u64 {
    if !user::fd_is_console(fd) {
        return EBADF;
    }
    if len == 0 {
        return 0;
    }
    if len > MAX_COPY {
        return EINVAL;
    }
    if !user::range_ok(buf, len) {
        return EFAULT;
    }
    // SAFETY: range_ok proved [buf, buf+len) lies inside a mapped
    // user range under the current (process) page table.
    let s = unsafe { core::slice::from_raw_parts(buf as *const u8, len as usize) };
    match core::str::from_utf8(s) {
        Ok(t) => log::printk(format_args!("{t}")),
        Err(_) => log::printk(format_args!("rustux: user write: {len}B non-utf8")),
    }
    // Flush now — main is parked in sched_launch for the whole task
    // phase, so without a syscall-side drain the small ring wraps and
    // user output is lost before it reaches the console.
    log::drain_task();
    len
}

/// exit(code): hand off to the scheduler — resume the next Runnable
/// task's ctx, or unwind to the saved kernel frame when none remain.
fn sys_exit(code: u64) -> ! {
    log::printk(format_args!("rustux: user: exit({code})"));
    log::drain_task();
    user::task_exit(code)
}
