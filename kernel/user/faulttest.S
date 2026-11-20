# Rustux userspace: fault isolation selftest.
# fork=57 wait4=61 exit=60 write=1.
# Three children each die to a CPU fault; the parent verifies each
# wait status is the bare signal number (wstatus low byte):
#   null write -> #PF -> SIGSEGV(11)
#   ud2        -> #UD -> SIGILL(4)
#   div by 0   -> #DE -> SIGFPE(8)
# Parent exit code = failed checks; each check is wait4 + status.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # ---- child 1: null write -> SIGSEGV ----
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    jz c_segv
    mov %rax, %r12
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne fail
    mov -256(%rsp), %eax
    cmp $11, %eax               # SIGSEGV
    jne fail

    # ---- child 2: ud2 -> SIGILL ----
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    jz c_ill
    mov %rax, %r12
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne fail
    mov -256(%rsp), %eax
    cmp $4, %eax                # SIGILL
    jne fail

    # ---- child 3: div by zero -> SIGFPE ----
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    jz c_fpe
    mov %rax, %r12
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne fail
    mov -256(%rsp), %eax
    cmp $8, %eax                # SIGFPE
    jne fail

    jmp report
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

c_segv:
    xor %eax, %eax
    movb $1, (%rax)             # write to page 0 -> #PF, task dies
    mov $60, %eax
    mov $99, %edi
    syscall
    ud2

c_ill:
    ud2                         # task dies SIGILL
    mov $60, %eax
    mov $98, %edi
    syscall
    ud2

c_fpe:
    xor %edx, %edx
    mov $1, %eax
    xor %ecx, %ecx
    div %ecx                    # #DE, task dies SIGFPE
    mov $60, %eax
    mov $97, %edi
    syscall
    ud2

msg:
    .ascii "faulttest: user faults kill the task, kernel lives\n"
msg_end:
