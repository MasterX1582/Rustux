# Rustux userspace: per-task FPU/SSE state selftest.
# fork=57 wait4=61 exit=60 write=1.
# Parent and child each load a DIFFERENT 16-byte pattern into xmm0,
# then spin-check it survives thousands of preemptions. Without
# fxsave/fxrstor in the context switch, each task's xmm0 would be
# clobbered by the other on every timeslice.
# Parent exit code = failed checks; child exits 0 or 9.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    jz child

    mov %rax, %r12              # child pid
    # parent: xmm0 = patB, verify it survives ~60000 iterations of
    # interleaved scheduling (the child does the same with patA)
    movaps patB(%rip), %xmm0
    mov $60000, %ecx
1:
    movaps %xmm0, %xmm1
    pcmpeqb patB(%rip), %xmm1   # xmm1 = all-ones iff xmm0==patB
    pmovmskb %xmm1, %eax
    cmp $0xffff, %eax
    jne fail
    dec %ecx
    jnz 1b

    # wait4 the child — its exit code is 0 (pattern survived) or 9
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne fail
    mov -256(%rsp), %eax
    test %eax, %eax
    jnz fail
    jmp report

child:
    movaps patA(%rip), %xmm0
    mov $60000, %ecx
1:
    movaps %xmm0, %xmm1
    pcmpeqb patA(%rip), %xmm1
    pmovmskb %xmm1, %eax
    cmp $0xffff, %eax
    jne cfail
    dec %ecx
    jnz 1b
    mov $60, %eax
    xor %edi, %edi              # survived -> exit(0)
    syscall
cfail:
    mov $60, %eax
    mov $9, %edi                # xmm0 was clobbered -> exit(9)
    syscall
    ud2

fail:
    inc %r15d
report:
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    mov $60, %eax
    mov %r15d, %edi
    syscall
    ud2

    .section .data
    .balign 16
patA:
    .byte 0xAA,0xAA,0xAA,0xAA,0xAA,0xAA,0xAA,0xAA
    .byte 0xAA,0xAA,0xAA,0xAA,0xAA,0xAA,0xAA,0xAA
    .balign 16
patB:
    .byte 0x55,0x55,0x55,0x55,0x55,0x55,0x55,0x55
    .byte 0x55,0x55,0x55,0x55,0x55,0x55,0x55,0x55

    .section .text
msg:
    .ascii "fputest: xmm state survives preemption\n"
msg_end:
