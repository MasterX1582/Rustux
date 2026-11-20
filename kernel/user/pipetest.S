# Rustux userspace: pipe + fork IPC selftest.
# A blocked read must wake when the child writes — the classic
# parent/child pipe demo, proven across two address spaces.
# nr in rax; args rdi,rsi,rdx. read=0 write=1 close=3 pipe=22
# fork=57 exit=60 wait4=61. Parent exit code = failed checks.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # pipe(&fds) -> 0; fds = [rfd, wfd] = [3, 4]
    mov $22, %eax
    lea -256(%rsp), %rdi
    xor %esi, %esi
    syscall
    test %rax, %rax
    je 1f
    inc %r15d
1:
    mov -256(%rsp), %r14d       # rfd
    mov -252(%rsp), %r13d       # wfd

    # pipe2(&fds, 1) -> EINVAL: flags unsupported
    mov $293, %eax
    lea -256(%rsp), %rdi
    mov $1, %esi
    syscall
    cmp $-22, %rax
    je 2f
    inc %r15d
2:
    # fork — child writes, parent blocks on read
    mov $57, %eax
    syscall
    test %rax, %rax
    js 21f                      # fork error
    jz child
    jmp parent
21: inc %r15d
    jmp report

parent:
    mov %rax, %r12              # child pid
    # read(rfd, buf, 5) — BLOCKS until child writes; must get "hello"
    mov %r14, %rdi
    lea -512(%rsp), %rsi
    mov $5, %edx
    xor %eax, %eax
    syscall
    cmp $5, %rax
    jne 31f
    mov -512(%rsp), %eax
    cmp $0x6c6c6568, %eax       # "hell" LE
    jne 31f
    cmpb $0x6f, -508(%rsp)      # 'o'
    je 3f
31: inc %r15d
3:
    # wait4(-1, &st, 0) -> child pid, status = 7<<8
    mov $-1, %rdi
    lea -512(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    je 4f
    inc %r15d
4:
    mov -512(%rsp), %eax
    cmp $0x700, %eax
    je 5f
    inc %r15d
5:
    # close(wfd) -> read(rfd,1) -> 0 (EOF: all writers gone)
    mov %r13, %rdi
    mov $3, %eax
    syscall
    mov %r14, %rdi
    lea -512(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    test %rax, %rax
    je 6f
    inc %r15d
6:
    # close(rfd) — pipe dies (both ends gone)
    mov %r14, %rdi
    mov $3, %eax
    syscall
    # second pipe: write end with no readers -> EPIPE
    mov $22, %eax
    lea -256(%rsp), %rdi
    xor %esi, %esi
    syscall
    test %rax, %rax
    je 7f
    inc %r15d
    jmp report
7:
    mov -256(%rsp), %r14d       # fresh rfd
    mov -252(%rsp), %r13d       # fresh wfd
    mov %r14, %rdi              # close the read end first
    mov $3, %eax
    syscall
    mov %r13, %rdi
    lea cmsg(%rip), %rsi        # any mapped buffer
    mov $1, %edx
    mov $1, %eax
    syscall
    cmp $-32, %rax              # EPIPE
    je 8f
    inc %r15d
8:
    mov %r13, %rdi              # close the write end; pipe dies
    mov $3, %eax
    syscall
    # read(1) -> EBADF: write-only end isn't readable
    mov %r14, %rdi              # rfd closed — wait, use a fresh one
    # (reuse the closed fd on purpose: EBADF is the assertion)
    lea -512(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $-9, %rax
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
    # write(wfd, "hello", 5) -> 5 — the parent's blocked read
    # must wake with these bytes in ITS address space.
    mov %r13, %rdi
    lea cmsg(%rip), %rsi
    mov $5, %edx
    mov $1, %eax
    syscall
    cmp $5, %rax
    jne cfail
    mov $60, %eax
    mov $7, %edi
    syscall
cfail:
    mov $60, %eax
    mov $99, %edi
    syscall
    ud2
msg:
    .ascii "pipetest: pipe + fork IPC checks done\n"
msg_end:
cmsg:
    .ascii "hello"
