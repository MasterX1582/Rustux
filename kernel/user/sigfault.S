# Rustux userspace: fault-driven signal delivery selftest.
# rt_sigaction=13 rt_sigreturn=15 fork=57 wait4=61 exit=60 write=1.
# Installs a SIGSEGV handler, commits a null write; the handler
# verifies rdi==SIGSEGV, sets a flag, advances the saved rip in its
# own sigframe past the 3-byte faulting movb, and rt_sigreturn
# RESUMES the task -- proving catchable fault delivery end to end
# (fault -> frame -> handler -> patch ctx -> sigreturn -> resume).
    .section .text
    .globl _start
_start:
    mov $13, %eax               # rt_sigaction
    mov $11, %edi               # SIGSEGV
    lea act(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    test %rax, %rax
    jns 1f
    mov $60, %eax
    mov $2, %edi
    syscall
1:
    # fork a child that resets SIGSEGV to SIG_DFL (dispositions
    # inherit across fork, so it must explicitly restore default)
    # then faults: parent must see the classic bare-signo death.
    mov $57, %eax
    syscall
    test %rax, %rax
    jns 1f
    mov $60, %eax
    mov $3, %edi
    syscall
1:  test %rax, %rax
    jz nohandler_child
    mov %rax, %r12

    xor %eax, %eax
    movb $0x42, (%rax)          # #PF -> SIGSEGV -> handler (3-byte insn)
    # -- execution RESUMES here via sigreturn --
    cmpb $1, flag(%rip)
    jne fail4

    # collect the unhandled child: wait status must be bare 11
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne fail5
    cmpq $11, -256(%rsp)
    jne fail6

    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    mov $60, %eax
    xor %edi, %edi
    syscall
    ud2

nohandler_child:
    mov $13, %eax               # rt_sigaction(SIGSEGV, SIG_DFL)
    mov $11, %edi
    lea act_dfl(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    xor %eax, %eax
    movb $0x42, (%rax)          # dies SIGSEGV -- parent sees 11
    ud2

handler:
    cmp $11, %rdi               # arg must be the signo
    jne hfail
    movb $1, flag(%rip)
    # frame: [signo][UCtx]; UCtx.rip is field 15 -> offset 8+15*8=128
    addq $3, 128(%rsp)          # saved rip += sizeof(movb (rax))
    mov $15, %eax               # rt_sigreturn
    syscall
    ud2                         # unreachable
hfail:
    mov $60, %eax
    mov $8, %edi
    syscall
    ud2

fail4:
    mov $60, %eax
    mov $4, %edi
    syscall
    ud2
fail5:
    mov $60, %eax
    mov $5, %edi
    syscall
    ud2
fail6:
    mov $60, %eax
    mov $6, %edi
    syscall
    ud2
fail:
    mov $60, %eax
    mov $1, %edi
    syscall
    ud2

    .section .data
act:
    .quad handler               # sa_handler -- kernel reads word 0
    .quad 0, 0, 0
act_dfl:
    .quad 0                     # SIG_DFL
    .quad 0, 0, 0
flag:
    .byte 0

    .section .text
msg:
    .ascii "sigfault: caught SIGSEGV in handler, resumed\n"
msg_end:
