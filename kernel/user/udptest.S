# udptest.S - UDP socket checks: socket/bind/sendto/recvfrom/poll.
# End-to-end: a real DNS query to QEMU slirp's resolver (10.0.2.3:53)
# comes back through our RX ring -> demux -> socket queue/wake.
# socket=41 sendto=44 recvfrom=45 bind=49 poll=7 write=1 exit=60.
# Failure exit codes pinpoint the check.
    .global _start
    .text
_start:
    xor %r15d, %r15d            # unused
    # --- socket(AF_INET=2, SOCK_DGRAM=2) -> fd ---
    mov $41, %eax
    mov $2, %edi
    mov $2, %esi
    xor %edx, %edx
    syscall
    test %rax, %rax
    js e1
    mov %rax, %r12              # r12 = fd
    # --- bind(fd, {2, htons(40000), 0.0.0.0}) -> 0 ---
    mov %eax, %edi
    lea sa_b(%rip), %rsi
    mov $16, %edx
    mov $49, %eax
    syscall
    test %rax, %rax
    jnz e2
    # --- socket + bind same port -> EADDRINUSE(-98) ---
    mov $41, %eax
    mov $2, %edi
    mov $2, %esi
    xor %edx, %edx
    syscall
    test %rax, %rax
    js e3
    mov %rax, %r13              # fd2
    mov %eax, %edi
    lea sa_b(%rip), %rsi
    mov $16, %edx
    mov $49, %eax
    syscall
    cmp $-98, %rax
    jne e4
    # --- socket(AF_UNIX=1,...) -> EAFNOSUPPORT(-97) ---
    mov $41, %eax
    mov $1, %edi
    mov $2, %esi
    xor %edx, %edx
    syscall
    cmp $-97, %rax
    jne e5
    # --- sendto(fd, dnsq1, qlen, 0, sa_dns, 16) -> qlen ---
    mov %r12d, %edi
    lea dnsq1(%rip), %rsi
    mov $(dnsq1_end - dnsq1), %edx
    xor %r10d, %r10d
    lea sa_dns(%rip), %r8
    mov $16, %r9d
    mov $44, %eax
    syscall
    cmp $(dnsq1_end - dnsq1), %rax
    jne e6
    # --- poll(fd, POLLIN, 8s) -> 1 with POLLIN ---
    mov %r12d, pf(%rip)
    movw $1, pf+4(%rip)
    movw $0, pf+6(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    mov $8000, %edx
    mov $7, %eax
    syscall
    cmp $1, %rax
    jne e7
    testw $1, pf+6(%rip)
    jz e8
    # --- recvfrom -> reply bytes + sockaddr ---
    mov %r12d, %edi
    lea rbuf(%rip), %rsi
    mov $512, %edx
    xor %r10d, %r10d
    lea ssrc(%rip), %r8
    xor %r9d, %r9d
    mov $45, %eax
    syscall
    test %rax, %rax
    js e9
    test %rax, %rax
    jz e9
    # txid echo: rbuf[0..2] == 0x15 0x82
    cmpb $0x15, rbuf(%rip)
    jne e10
    cmpb $0x82, rbuf+1(%rip)
    jne e10
    # QR bit set in flags byte
    testb $0x80, rbuf+3(%rip)
    jz e11
    # ssrc: family==2, port==htons(53)
    cmpw $2, ssrc(%rip)
    jne e12
    cmpb $0, ssrc+2(%rip)
    jne e12
    cmpb $53, ssrc+3(%rip)
    jne e12
    # --- second query: blocked recvfrom proves the wake path ---
    mov %r12d, %edi
    lea dnsq2(%rip), %rsi
    mov $(dnsq2_end - dnsq2), %edx
    xor %r10d, %r10d
    lea sa_dns(%rip), %r8
    mov $16, %r9d
    mov $44, %eax
    syscall
    cmp $(dnsq2_end - dnsq2), %rax
    jne e13
    # recvfrom blocks until the demux wakes us with the reply
    mov %r12d, %edi
    lea rbuf(%rip), %rsi
    mov $512, %edx
    xor %r10d, %r10d
    xor %r8d, %r8d              # no sockaddr out
    xor %r9d, %r9d
    mov $45, %eax
    syscall
    test %rax, %rax
    js e14
    test %rax, %rax
    jz e14
    cmpb $0x15, rbuf(%rip)
    jne e15
    cmpb $0x83, rbuf+1(%rip)
    jne e15
    # --- drained queue: poll timeout 0 -> 0 ---
    mov %r12d, pf(%rip)
    movw $1, pf+4(%rip)
    movw $0, pf+6(%rip)
    lea pf(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    test %rax, %rax
    jnz e16
    jmp report
e1:  mov $60, %eax
    mov $1, %edi
    syscall
    ud2
e2:  mov $60, %eax
    mov $2, %edi
    syscall
    ud2
e3:  mov $60, %eax
    mov $3, %edi
    syscall
    ud2
e4:  mov $60, %eax
    mov $4, %edi
    syscall
    ud2
e5:  mov $60, %eax
    mov $5, %edi
    syscall
    ud2
e6:  mov $60, %eax
    mov $6, %edi
    syscall
    ud2
e7:  mov $60, %eax
    mov $7, %edi
    syscall
    ud2
e8:  mov $60, %eax
    mov $8, %edi
    syscall
    ud2
e9:  mov $60, %eax
    mov $9, %edi
    syscall
    ud2
e10: mov $60, %eax
    mov $10, %edi
    syscall
    ud2
e11: mov $60, %eax
    mov $11, %edi
    syscall
    ud2
e12: mov $60, %eax
    mov $12, %edi
    syscall
    ud2
e13: mov $60, %eax
    mov $13, %edi
    syscall
    ud2
e14: mov $60, %eax
    mov $14, %edi
    syscall
    ud2
e15: mov $60, %eax
    mov $15, %edi
    syscall
    ud2
e16: mov $60, %eax
    mov $16, %edi
    syscall
    ud2

report:
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    mov $60, %eax
    xor %edi, %edi
    syscall

    .data
msg:    .ascii "udptest: udp socket checks done\n"
msg_end:
# DNS A-query for example.com, txid 0x1582
dnsq1:  .byte 0x15,0x82, 0x01,0x00, 0x00,0x01, 0x00,0x00, 0x00,0x00, 0x00,0x00
        .byte 7,'e','x','a','m','p','l','e', 3,'c','o','m', 0
        .byte 0,1, 0,1
dnsq1_end:
dnsq2:  .byte 0x15,0x83, 0x01,0x00, 0x00,0x01, 0x00,0x00, 0x00,0x00, 0x00,0x00
        .byte 7,'e','x','a','m','p','l','e', 3,'c','o','m', 0
        .byte 0,1, 0,1
dnsq2_end:
# sockaddr_in {family=2, port=htons(53), ip=10.0.2.3, pad}
sa_dns: .word 2
        .byte 0,53
        .byte 10,0,2,3
        .zero 8
# sockaddr_in {family=2, port=htons(40000), ip=0}
sa_b:   .word 2
        .byte 0x9c,0x40
        .byte 0,0,0,0
        .zero 8
pf:     .long 0
        .word 0
        .word 0
ssrc:   .zero 16
rbuf:   .zero 512
