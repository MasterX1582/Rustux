# Rustux userspace: fork/getpid/getppid/wait4 selftest.
# Exercises process creation semantics from actual CPL3 code.
# nr in rax; args rdi, rsi, rdx. read=0 write=1 open=2 close=3
# getpid=39 fork=57 exit=60 wait4=61 getppid=110.
# Parent exit code = number of failed checks; child exits 42.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # getpid() -> >0, keep in r13 (the parent pid)
    mov $39, %eax
    syscall
    test %rax, %rax
    jg 1f
    inc %r15d
1:
    mov %rax, %r13

    # getppid() -> 0: initrd-spawned roots have no parent
    mov $110, %eax
    syscall
    test %rax, %rax
    je 2f
    inc %r15d
2:
    # open("/motd.txt") -> fd 3; read 1 byte 'M' -> pos = 1
    mov $2, %eax
    lea path(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $3, %rax
    je 3f
    inc %r15d
3:
    mov %rax, %r14              # fd3 — the fork will SHARE its OFD
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne 31f
    cmpb $0x4d, -256(%rsp)      # 'M'
    je 4f
31: inc %r15d
4:
    # fork() -> 0 in child, child pid in parent
    mov $57, %eax
    syscall
    test %rax, %rax
    js 41f                      # negative errno
    jz child
    jmp parent
41: inc %r15d
    jmp report

parent:
    mov %rax, %r12              # child pid
    # wait4(-1, &status, 0) -> child pid; status == 42<<8
    mov $-1, %rdi
    lea -256(%rsp), %rsi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp %r12, %rax
    je 5f
    inc %r15d
5:
    mov -256(%rsp), %eax
    cmp $0x2a00, %eax           # 42 << 8
    je 6f
    inc %r15d
6:
    # shared OFD proof: child read 'O' (pos->2); a shared offset
    # means OUR next read gets 'T'. A copied OFD would give 'O'.
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne 61f
    cmpb $0x54, -256(%rsp)      # 'T'
    je 7f
61: inc %r15d
7:
    # wait4(-1, NULL, 0) -> -ECHILD: nothing left to reap
    mov $-1, %rdi
    xor %esi, %esi
    xor %edx, %edx
    mov $61, %eax
    syscall
    cmp $-10, %rax
    je 8f
    inc %r15d
8:
    # getpid still returns OUR pid, not the child's
    mov $39, %eax
    syscall
    cmp %r13, %rax
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
    ud2                         # unreachable

child:
    # rax==0: we are the copy. getpid must differ from r13.
    mov $39, %eax
    syscall
    cmp %r13, %rax
    je cfail
    # getppid must equal the parent's pid
    mov $110, %eax
    syscall
    cmp %r13, %rax
    jne cfail
    # shared OFD: our first read on fd3 gets 'O' (parent left pos=1)
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne cfail
    cmpb $0x4f, -256(%rsp)      # 'O'
    jne cfail
    mov $1, %eax
    mov $1, %edi
    lea cmsg(%rip), %rsi
    mov $(cmsg_end - cmsg), %edx
    syscall
    mov $60, %eax
    mov $42, %edi               # the status wait4 must report
    syscall
cfail:
    mov $60, %eax
    mov $99, %edi               # parent sees 99<<8 -> counts it
    syscall
    ud2
path:
    .ascii "/motd.txt\0"
msg:
    .ascii "forktest: fork + wait4 checks done\n"
msg_end:
cmsg:
    .ascii "forktest: child ran\n"
cmsg_end:
