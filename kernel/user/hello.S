# Rustux userspace milestone: ring-3 program on the SYSCALL ABI.
# nr in rax; args rdi, rsi, rdx. write=1, exit=60.
    .section .text
    .globl _start
_start:
    mov $1, %eax                # write
    mov $1, %edi                # fd 1
    lea msg(%rip), %rsi         # user buf
    mov $(msg_end - msg), %edx  # len
    syscall
    mov $60, %eax               # exit
    xor %edi, %edi              # code 0
    syscall
    ud2                         # unreachable — exit must not return
msg:
    .ascii "rustux user: hello from ring 3\n"
msg_end:
