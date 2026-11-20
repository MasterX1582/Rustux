# Rustux userspace: fd-table + initrd-filesystem selftest.
# Exercises open/read/close semantics from actual CPL3 code.
# nr in rax; args rdi, rsi, rdx. read=0 write=1 open=2 close=3 exit=60.
# Exit code = number of failed checks.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # open("/motd.txt", O_RDONLY) -> fd 3
    mov $2, %eax
    lea path(%rip), %rdi
    xor %esi, %esi              # O_RDONLY
    xor %edx, %edx
    syscall
    cmp $3, %rax
    je 1f
    inc %r15d
1:
    mov %rax, %r14              # the file fd

    # read(fd, buf, 128) -> n > 0; buf lives on the writable stack
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $128, %edx
    xor %eax, %eax
    syscall
    test %rax, %rax
    jg 2f
    inc %r15d
2:
    # echo the file's content: write(1, buf, n)
    mov %rax, %rdx
    lea -256(%rsp), %rsi
    mov $1, %edi
    mov $1, %eax
    syscall

    # read(1, buf, 4) -> EBADF: console fd is not readable
    mov $1, %edi
    lea -256(%rsp), %rsi
    mov $4, %edx
    xor %eax, %eax
    syscall
    cmp $-9, %rax
    je 3f
    inc %r15d
3:
    # open("/nope.txt") -> -ENOENT
    mov $2, %eax
    lea badpath(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-2, %rax
    je 4f
    inc %r15d
4:
    # write(file_fd, ...) -> EBADF: initrd files are read-only
    mov %r14, %rdi
    lea msg(%rip), %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    cmp $-9, %rax
    je 5f
    inc %r15d
5:
    # close(file_fd) -> 0; then read(file_fd) -> EBADF
    mov %r14, %rdi
    mov $3, %eax
    syscall
    test %rax, %rax
    je 6f
    inc %r15d
6:
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $4, %edx
    xor %eax, %eax
    syscall
    cmp $-9, %rax
    je 7f
    inc %r15d
7:
    # report + exit(failures)
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    mov $60, %eax
    mov %r15d, %edi
    syscall
    ud2                         # unreachable
path:
    .ascii "/motd.txt\0"
badpath:
    .ascii "/nope.txt\0"
msg:
    .ascii "fstest: fd + initrd-fs checks done\n"
msg_end:
