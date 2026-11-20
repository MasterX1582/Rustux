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
    # fd3 is closed now — reopen motd.txt for the OFD section
    mov $2, %eax
    lea path(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $3, %rax
    je 71f
    inc %r15d
71:
    mov %rax, %r14              # fresh fd3

    # lseek(fd, 0, SEEK_END) -> filesize (51, trailing CRLF)
    mov %r14, %rdi
    xor %esi, %esi
    mov $2, %edx
    mov $8, %eax
    syscall
    cmp $51, %rax
    je 8f
    inc %r15d
8:
    # lseek(fd, 0, SEEK_SET) -> 0
    mov %r14, %rdi
    xor %esi, %esi
    xor %edx, %edx
    mov $8, %eax
    syscall
    test %rax, %rax
    je 9f
    inc %r15d
9:
    # read(fd, buf, 1) -> 1 byte 'M' at offset 0
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne 91f
    cmpb $0x4d, -256(%rsp)
    je 10f
91: inc %r15d
10:
    # dup(fd3) -> fd4, the shared OFD
    mov %r14, %rdi
    mov $32, %eax
    syscall
    cmp $4, %rax
    je 11f
    inc %r15d
11:
    mov %rax, %r13              # fd4
    # read(fd4, buf, 1) -> 'O' — the dup shares pos (the real proof)
    mov %r13, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne 92f
    cmpb $0x4f, -256(%rsp)
    je 12f
92: inc %r15d
12:
    # lseek(1, 0, SEEK_SET) -> ESPIPE — console is not seekable
    mov $1, %edi
    xor %esi, %esi
    xor %edx, %edx
    mov $8, %eax
    syscall
    cmp $-29, %rax
    je 13f
    inc %r15d
13:
    # dup2(fd3, 9) -> 9
    mov %r14, %rdi
    mov $9, %esi
    mov $33, %eax
    syscall
    cmp $9, %rax
    je 14f
    inc %r15d
14:
    # close(fd4) then read(fd9,1) -> 'T': OFD survives with its
    # remaining refs and still shares the offset
    mov %r13, %rdi
    mov $3, %eax
    syscall
    mov $9, %edi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $1, %rax
    jne 93f
    cmpb $0x54, -256(%rsp)
    je 15f
93: inc %r15d
15:
    # lseek(fd, 100, SEEK_SET) -> 100 (past EOF is legal);
    # read -> 0 (EOF)
    mov %r14, %rdi
    mov $100, %esi
    xor %edx, %edx
    mov $8, %eax
    syscall
    cmp $100, %rax
    je 16f
    inc %r15d
16:
    mov %r14, %rdi
    lea -256(%rsp), %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    test %rax, %rax
    je 17f
    inc %r15d
17:
    # dup(99) -> EBADF; dup2(fd3, 9999) -> EBADF (table never grows)
    mov $99, %edi
    mov $32, %eax
    syscall
    cmp $-9, %rax
    je 18f
    inc %r15d
18:
    mov %r14, %rdi
    mov $9999, %esi
    mov $33, %eax
    syscall
    cmp $-9, %rax
    je 19f
    inc %r15d
19:
    # newfstatat(-100, "/motd.txt", buf) -> 0; S_IFREG|0644, ino!=0, size>0
    mov $-100, %rdi
    lea path(%rip), %rsi
    lea -512(%rsp), %rdx
    xor %r10d, %r10d
    mov $262, %eax
    syscall
    test %rax, %rax
    je 20f
    inc %r15d
20:
    cmpl $0100644, -512+24(%rsp)   # st_mode @ 24
    je 21f
    inc %r15d
21:
    cmpq $0, -512+8(%rsp)           # st_ino @ 8
    jne 22f
    inc %r15d
22:
    cmpq $0, -512+48(%rsp)          # st_size @ 48
    jg 23f
    inc %r15d
23:
    # newfstatat(-100, "/nope.txt", buf) -> -ENOENT
    mov $-100, %rdi
    lea badpath(%rip), %rsi
    lea -512(%rsp), %rdx
    xor %r10d, %r10d
    mov $262, %eax
    syscall
    cmp $-2, %rax
    je 24f
    inc %r15d
24:
    # newfstatat on "." -> S_IFDIR; stat to unmapped buf -> -EFAULT
    mov $-100, %rdi
    lea dot(%rip), %rsi
    lea -512(%rsp), %rdx
    xor %r10d, %r10d
    mov $262, %eax
    syscall
    test %rax, %rax
    je 25f
    inc %r15d
25:
    cmpl $040755, -512+24(%rsp)
    je 26f
    inc %r15d
26:
    mov $-100, %rdi
    lea path(%rip), %rsi
    mov $0xdead0000, %rdx           # unmapped -> EFAULT
    xor %r10d, %r10d
    mov $262, %eax
    syscall
    cmp $-14, %rax
    je 27f
    inc %r15d
27:
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
dot:
    .ascii ".\0"
msg:
    .ascii "fstest: fd + initrd-fs checks done\n"
msg_end:
