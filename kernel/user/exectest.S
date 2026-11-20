# Rustux userspace: execve image-replacement selftest.
# nr in rax; args rdi,rsi,rdx. execve=59, exit=60.
# Exit codes: 0 via the exec'd image, 1=bad errno on ENOENT,
# 2=bad errno on ENOEXEC, 3=execve returned after success.
    .section .text
    .globl _start
_start:
    # execve("/nope") -> -2 ENOENT: a failed exec must return to
    # THIS image with its context intact.
    mov $59, %eax
    lea bad(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-2, %rax
    je 1f
    mov $1, %edi
    mov $60, %eax
    syscall
1:
    # execve("/motd.txt") -> -8 ENOEXEC: parsed before the reset,
    # so the task keeps its address space and resumes here.
    mov $59, %eax
    lea notelf(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-8, %rax
    je 2f
    mov $2, %edi
    mov $60, %eax
    syscall
2:
    # marker — proves which image emitted which line in the log
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    # execve("/init") -> replace this image; init prints hello and
    # exits 0. sysret must land on init's entry, never here.
    mov $59, %eax
    lea good(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    mov $3, %edi                # execve returned after success?!
    mov $60, %eax
    syscall
    ud2                         # unreachable
good:
    .ascii "/init\0"
bad:
    .ascii "/nope\0"
notelf:
    .ascii "/motd.txt\0"
msg:
    .ascii "exectest: execing /init\n"
msg_end:
