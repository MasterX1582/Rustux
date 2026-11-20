# Rustux userspace milestone: ring-3 syscall-boundary selftest.
# Exercises the deny-by-default gates and the Linux register-clobber
# contract from actual CPL3 code. nr in rax; args rdi, rsi, rdx.
# write=1, exit=60. Exit code = number of failed checks.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures (callee-saved across syscalls)

    # EBADF: fd -1 is not 1/2
    mov $1, %eax
    mov $-1, %rdi
    lea msg(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $-9, %rax
    je 1f
    inc %r15d
1:
    # EFAULT: pointer outside the user VA window (PML4 slot 0)
    mov $1, %eax
    mov $1, %edi
    mov $0xdead, %esi
    shl $20, %rsi               # rsi = 0xdead00000
    mov $8, %edx
    syscall
    cmp $-14, %rax
    je 2f
    inc %r15d
2:
    # EINVAL: len 5000 exceeds the 4096 copy cap
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $5000, %edx
    syscall
    cmp $-22, %rax
    je 3f
    inc %r15d
3:
    # ENOSYS: unimplemented nr
    mov $9999, %eax
    xor %edi, %edi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-38, %rax
    je 4f
    inc %r15d
4:
    # Linux ABI: syscall clobbers only rax/rcx/r11. r10 is the
    # canary — a Windows-ABI handler would be free to trash it.
    mov $0x5151dead, %r10
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $0x5151dead, %r10
    je 5f
    inc %r15d
5:
    # success report + exit(failures)
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    mov $60, %eax
    mov %r15d, %edi
    syscall
    ud2                         # unreachable — exit must not return
msg:
    .ascii "selftest: syscall boundary checks done\n"
msg_end:
