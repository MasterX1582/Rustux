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

use crate::{log, user};
use core::arch::global_asm;

#[repr(align(16))]
struct SysStack(#[allow(dead_code)] [u8; 8192]);

/// Dedicated kernel stack for SYSCALL entries. SYSCALL doesn't touch
/// TSS.rsp0, so this must be switched to explicitly by the stub.
/// TSS.rsp0 separately serves interrupts arriving while CPL=3.
#[no_mangle]
static SYS_STACK: SysStack = SysStack([0; 8192]);

/// Where the stub stashes the user rsp for the return leg.
#[no_mangle]
static mut USER_RSP: u64 = 0;
/// Kernel rsp/cr3 saved by user_enter, restored by user_terminate.
#[no_mangle]
static mut KSP: u64 = 0;
#[no_mangle]
static mut KCR3: u64 = 0;

global_asm!(
    "
    .globl syscall_entry
syscall_entry:
    // rcx=user rip, r11=user rflags; rsp is still the USER stack.
    mov qword ptr [rip + USER_RSP], rsp
    lea rsp, [rip + SYS_STACK]
    add rsp, 8192 - 8
    // Linux ABI: syscall may clobber only rax (ret), rcx and r11 —
    // every other user register must come back intact. The Rust
    // handler runs under the Windows x64 convention, which lets it
    // trash rdx/rsi/r8/r9/r10, so we save the full set it could
    // touch. rbx/rbp/r12-r15 are callee-saved under Windows and the
    // handler preserves them for free.
    push rcx
    push r11
    push rdi
    push rsi
    push rdx
    push r8
    push r9
    push r10
    sub rsp, 8                 // realign rsp to 16 for the call
    // Marshal the Linux syscall ABI (nr=rax, args rdi/rsi/rdx) into
    // the Windows x64 call this kernel uses: rcx,rdx,r8,r9.
    mov r9, rdx
    mov r8, rsi
    mov rdx, rdi
    mov rcx, rax
    call handle_syscall
    add rsp, 8
    pop r10
    pop r9
    pop r8
    pop rdx
    pop rsi
    pop rdi
    pop r11
    pop rcx
    mov rsp, qword ptr [rip + USER_RSP]
    sysretq

    .globl user_enter
user_enter:
    // Windows x64 args: rcx = user entry rip, rdx = user stack top,
    // r8 = user pml4.
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov rax, cr3
    mov [rip + KCR3], rax
    mov [rip + KSP], rsp
    mov cr3, r8
    mov rax, 0x2b              // ss = udata|3
    push rax
    push rdx                   // user rsp
    push 0x202                 // rflags: IF set in user mode
    mov rax, 0x33              // cs = ucode|3
    push rax
    push rcx                   // user rip
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
    pop rbp
    pop rbx
    ret
"
);

unsafe extern "C" {
    fn user_enter(entry: u64, ustack: u64, pml4: u64);
    fn user_terminate() -> !;
    fn syscall_entry();
}

/// Address the LSTAR MSR should point at.
pub fn entry_addr() -> u64 {
    syscall_entry as *const () as u64
}

/// Drop to ring 3: switch to the process page table and iretq into
/// the image's entry point. Returns only via sys_exit's unwind path.
pub fn enter(entry: u64, sp: u64, pml4: u64) {
    unsafe { user_enter(entry, sp, pml4) }
}

// Linux syscall numbers.
const NR_WRITE: u64 = 1;
const NR_EXIT: u64 = 60;

// errno values (negative returns per the kernel ABI).
const EBADF: u64 = (-9i64) as u64;
const EFAULT: u64 = (-14i64) as u64;
const EINVAL: u64 = (-22i64) as u64;
const ENOSYS: u64 = (-38i64) as u64;

/// Longest single copy from user space — a hostile len can't stall
/// the kernel or exhaust memory.
const MAX_COPY: u64 = 4096;

/// nr=rcx a1..a3=rdx,r8,r9 — entry from the asm stub (Windows x64
/// call convention), returns the syscall result in rax (negative
/// errno convention).
#[no_mangle]
extern "C" fn handle_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    match nr {
        NR_WRITE => sys_write(a1, a2, a3),
        NR_EXIT => sys_exit(a1),
        _ => ENOSYS,
    }
}

/// write(fd, buf, len): fd 1/2 only, buffer fully validated against
/// the process's mapped user ranges before a single byte is read —
/// this is the trust boundary, not a convenience check.
fn sys_write(fd: u64, buf: u64, len: u64) -> u64 {
    if fd != 1 && fd != 2 {
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
    len
}

/// exit(code): unwind to the kernel frame user_enter saved; never
/// returns to the user image.
fn sys_exit(code: u64) -> ! {
    log::printk(format_args!("rustux: user: exit({code})"));
    unsafe { user_terminate() }
}
