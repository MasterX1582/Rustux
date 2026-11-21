# Rustux userspace: signals selftest.
# rt_sigaction=13 rt_sigreturn=15 pipe=22 kill=62 fork=57 wait4=61.
# Proves: handler install, EINVAL paths, ESRCH, a blocked read
# interrupted by SIGUSR1 (handler runs, sigreturn -> -EINTR), and
# signal-death wstatus (bare signo) via wait4.
# Parent exit code = failed checks; child dies to SIGTERM.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # sigaction(SIGUSR1=10, {handler}) -> 0
    mov $13, %eax
    mov $10, %edi
    lea act(%rip), %rsi
    xor %edx, %edx
    syscall
    test %rax, %rax
    je 1f
    inc %r15d
1:
    # sigaction(32, ...) -> EINVAL
    mov $13, %eax
    mov $32, %edi
    lea act(%rip), %rsi
    xor %edx, %edx
    syscall
    cmp $-22, %rax
    je 2f
    inc %r15d
2:
    # sigaction(SIGKILL=9, handler) -> EINVAL: uncatchable
    mov $13, %eax
    mov $9, %edi
    lea act(%rip), %rsi
    xor %edx, %edx
    syscall
    cmp $-22, %rax
    je 3f
    inc %r15d
3:
    # kill(999999, 1) -> ESRCH
    mov $62, %eax
    mov $999999, %edi
    mov $1, %esi
    syscall
    cmp $-3, %rax
    je 4f
    inc %r15d
4:
    # kill(0-bad-sig): kill(1, 32) -> EINVAL
    mov $62, %eax
    mov $1, %edi
    mov $32, %esi
    syscall
    cmp $-22, %rax
    je 5f
    inc %r15d
5:
    # pipe -> rfd=3 wfd=4 (read stays empty: blocks)
    mov $22, %eax
    lea -256(%rsp), %rdi
    xor %esi, %esi
    syscall
    test %rax, %rax
    je 6f
    inc %r15d
6:
    mov -256(%rsp), %r14d       # rfd
    # fork — child signals, parent blocks on the empty pipe
    mov $57, %eax
    syscall
    test %rax, %rax
    js 61f
    jz child
    jmp parent
61: inc %r15d
    jmp report

parent:
    mov %rax, %r12              # child pid
    # read(rfd, buf, 1): blocks; child's kill -> handler ->
    # sigreturn -> -EINTR
    mov %r14, %rdi
    lea -512(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $-4, %rax               # -EINTR
    je 7f
    inc %r15d
7:
    # the handler ran before the EINTR return — marker == 'H'
    cmpb $0x48, marker(%rip)
    je 8f
    inc %r15d
8:
    # wait4 -> child died to SIGTERM: status = bare signo 15
    mov $-1, %rdi
    lea -512(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    je 9f
    inc %r15d
9:
    mov -512(%rsp), %eax
    cmp $15, %eax
    je report
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

child:
    # kill(ppid, SIGUSR1) — interrupts the parent's blocked read
    mov $110, %eax
    syscall                     # getppid
    mov %rax, %r14
    mov %r14, %rdi
    mov $10, %esi
    mov $62, %eax
    syscall
    test %rax, %rax
    js cfail
    # kill(self, SIGTERM) — default action: death, status = 15
    mov $39, %eax
    syscall                     # getpid
    mov %rax, %rdi
    mov $15, %esi
    mov $62, %eax
    syscall
    # unreachable — the signal delivers at this syscall's return
    mov $60, %eax
    mov $9, %edi
    syscall
cfail:
    mov $60, %eax
    mov $99, %edi
    syscall
    ud2

handler:
    movb $0x48, marker(%rip)    # 'H'
    ret                         # restorer stub -> rt_sigreturn

    .section .data
act:
    .quad handler               # sa_handler — we read word 0 only
    .quad 0, 0, 0
marker:
    .byte 0

    .section .text
msg:
    .ascii "sigtest: signal checks done\n"
msg_end:
