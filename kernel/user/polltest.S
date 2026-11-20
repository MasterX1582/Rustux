# Rustux userspace: poll(7) readiness selftest.
# pipe=22 poll=7 open=2 close=3 fork=57 wait4=61 exit=60 write=1 read=0.
# pollfd = {fd:u32, events:u16, revents:u16} = 8 bytes.
# POLLIN=1 POLLOUT=4 POLLERR=8 POLLHUP=16 POLLNVAL=32.
# Exit code = failed checks.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures
    # pipe(fds) -> rfd@fds, wfd@fds+4
    lea fds(%rip), %rdi
    xor %esi, %esi
    mov $22, %eax
    syscall
    test %rax, %rax
    jns 1f
    mov $60, %eax
    mov $1, %edi
    syscall
    ud2
1:
    # poll([{rfd,POLLIN}], 1, 0) -> 0 (empty pipe, timeout 0)
    mov fds(%rip), %eax
    mov %eax, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    test %rax, %rax
    jz 2f
    mov $60, %eax
    mov $2, %edi
    syscall
    ud2
2:
    # poll([{99,POLLIN}], 1, 0) -> 1 + POLLNVAL
    movl $99, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    cmp $1, %rax
    je 3f
    mov $60, %eax
    mov $3, %edi
    syscall
    ud2
3:
    cmpw $0x20, pf+6(%rip)        # revents == POLLNVAL
    je 4f
    mov $60, %eax
    mov $4, %edi
    syscall
    ud2
4:
    # timed empty poll -> 0 (writers alive, no data, 50ms)
    mov fds(%rip), %eax
    mov %eax, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    mov $50, %edx
    mov $7, %eax
    syscall
    test %rax, %rax
    jz 5f
    mov $60, %eax
    mov $5, %edi
    syscall
    ud2
5:
    # open /motd.txt -> poll POLLIN -> 1 (files never block)
    mov $2, %eax
    lea path(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    test %rax, %rax
    js 6f
    mov %eax, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    cmp $1, %rax
    jne 6f
    cmpw $1, pf+6(%rip)           # revents == POLLIN
    je 7f
6:  mov $60, %eax
    mov $6, %edi
    syscall
    ud2
7:
    # fork: child writes 'x' then exits (its wfd close -> POLLHUP);
    # parent polls with timeout -1 -> wakes POLLIN, then after the
    # byte drains + child exit -> POLLHUP
    mov $57, %eax
    syscall
    test %rax, %rax
    js 8f
    jz child
    mov %rax, %r12
    mov fds(%rip), %eax
    mov %eax, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    mov $-1, %rdx                 # infinite
    mov $7, %eax
    syscall
    cmp $1, %rax
    jne f20
    testw $1, pf+6(%rip)          # POLLIN set (POLLHUP may race-join)
    jz f21
    # read the byte out
    mov fds(%rip), %edi
    lea -64(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne f22
    # wait4 the child
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    jne f23
    # close OUR wfd too -> writers == 0 -> next poll: POLLHUP
    mov fds+4(%rip), %edi
    mov $3, %eax
    syscall
    mov fds(%rip), %eax
    mov %eax, pf(%rip)
    movw $1, pf+4(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    cmp $1, %rax
    jne f24
    testw $0x10, pf+6(%rip)       # POLLHUP set
    jz f25
    jmp report
8:
f20: mov $60, %eax
    mov $20, %edi
    syscall
    ud2
f21: mov $60, %eax
    mov $21, %edi
    syscall
    ud2
f22: mov $60, %eax
    mov $22, %edi
    syscall
    ud2
f23: mov $60, %eax
    mov $23, %edi
    syscall
    ud2
f24: mov $60, %eax
    mov $24, %edi
    syscall
    ud2
f25: mov $60, %eax
    mov $25, %edi
    syscall
    ud2

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
    mov fds+4(%rip), %edi
    lea x(%rip), %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    mov $60, %eax
    xor %edi, %edi
    syscall
    ud2

    .section .data
    .balign 4
fds:
    .long 0, 0
pf:
    .long 0
    .word 0, 0
path:
    .ascii "/motd.txt\0"
x:
    .byte 'x'

    .section .text
msg:
    .ascii "polltest: poll readiness checks done\n"
msg_end:
