# Rustux userspace: TCP socket selftest over the loopback path.
# socket=41 connect=42 accept=43 shutdown=48 listen=50 bind=49
# poll=7 read=0 write=1 close=3 exit=60.
# The loopback injector completes the handshake inside connect(),
# so one task can be client AND server deterministically.
# Parent exit code = failed checks.
    .section .text
    .globl _start
_start:
    xor %r15d, %r15d            # failures

    # --- listener = socket(AF_INET, SOCK_STREAM, 0) ---
    mov $2, %edi
    mov $1, %esi
    xor %edx, %edx
    mov $41, %eax
    syscall
    test %rax, %rax
    js fail
    mov %rax, %r12              # listener fd

    # --- bind(listener, {2, 9090, 0.0.0.0}, 16) -> 0 ---
    mov %r12, %rdi
    lea sa_any(%rip), %rsi
    mov $16, %edx
    mov $49, %eax
    syscall
    test %rax, %rax
    jnz fail

    # --- listen(listener, 4) -> 0 ---
    mov %r12, %rdi
    mov $4, %esi
    mov $50, %eax
    syscall
    test %rax, %rax
    jnz fail

    # --- client = socket(2,1,0) ---
    mov $2, %edi
    mov $1, %esi
    xor %edx, %edx
    mov $41, %eax
    syscall
    test %rax, %rax
    js fail
    mov %rax, %r13              # client fd

    # --- connect(client, {2, 9090, 127.0.0.1}) -> 0 ---
    mov %r13, %rdi
    lea sa_lo(%rip), %rsi
    mov $16, %edx
    mov $42, %eax
    syscall
    test %rax, %rax
    jnz fail

    # --- accept(listener, out, alen) -> server fd ---
    mov %r12, %rdi
    lea sa_out(%rip), %rsi
    lea alen(%rip), %rdx
    mov $43, %eax
    syscall
    test %rax, %rax
    js fail
    mov %rax, %r14              # server fd

    # sockaddr_out: family 2, port in ephemeral range
    cmpw $2, sa_out(%rip)
    jne fail
    movzwl sa_out+2(%rip), %eax
    xchg %al, %ah               # BE port -> host order
    cmp $49152, %eax
    jb fail

    # --- write(client, "ping") -> 4; read(server) == "ping" ---
    mov %r13, %rdi
    lea ping(%rip), %rsi
    mov $4, %edx
    mov $1, %eax
    syscall
    cmp $4, %rax
    jne fail
    mov %r14, %rdi
    lea buf(%rip), %rsi
    mov $4, %edx
    xor %eax, %eax
    syscall
    cmp $4, %rax
    jne fail
    cmpl $0x676e6970, buf(%rip) # 'gnip' little-endian
    jne fail

    # --- write(server, "pong") -> 4; read(client) == "pong" ---
    mov %r14, %rdi
    lea pong(%rip), %rsi
    mov $4, %edx
    mov $1, %eax
    syscall
    cmp $4, %rax
    jne fail
    mov %r13, %rdi
    lea buf(%rip), %rsi
    mov $4, %edx
    xor %eax, %eax
    syscall
    cmp $4, %rax
    jne fail
    cmpl $0x676e6f70, buf(%rip) # 'gnop'
    jne fail

    # --- close(server) -> client's read hits EOF(0) ---
    mov %r14, %rdi
    mov $3, %eax
    syscall
    test %rax, %rax
    jnz fail
    mov %r13, %rdi
    lea buf(%rip), %rsi
    mov $4, %edx
    xor %eax, %eax
    syscall
    test %rax, %rax
    jnz fail                    # 0 bytes = EOF after peer FIN

    # --- poll(client, POLLIN|POLLOUT, 0) -> POLLIN visible ---
    movl %r13d, pfd(%rip)
    movw $5, pfd+4(%rip)
    movw $0, pfd+6(%rip)
    lea pfd(%rip), %rdi
    mov $1, %esi
    xor %edx, %edx
    mov $7, %eax
    syscall
    cmp $1, %rax
    jne fail
    movzwl pfd+6(%rip), %eax
    test $1, %eax               # POLLIN — EOF is readable
    jz fail

    # --- close(client) ---
    mov %r13, %rdi
    mov $3, %eax
    syscall
    test %rax, %rax
    jnz fail

    # --- connect to a closed port -> -ECONNREFUSED (-111) ---
    mov $2, %edi
    mov $1, %esi
    xor %edx, %edx
    mov $41, %eax
    syscall
    test %rax, %rax
    js fail
    mov %rax, %r14
    mov %r14, %rdi
    lea sa_dead(%rip), %rsi
    mov $16, %edx
    mov $42, %eax
    syscall
    cmp $-111, %rax
    jne fail
    mov %r14, %rdi
    mov $3, %eax
    syscall

    # --- send on a never-connected socket -> -ENOTCONN (-107) ---
    mov $2, %edi
    mov $1, %esi
    xor %edx, %edx
    mov $41, %eax
    syscall
    test %rax, %rax
    js fail
    mov %rax, %r14
    mov %r14, %rdi
    lea ping(%rip), %rsi
    mov $4, %edx
    mov $1, %eax
    syscall
    cmp $-107, %rax
    jne fail
    mov %r14, %rdi
    mov $3, %eax
    syscall

    jmp report
fail:
    inc %r15d
report:
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    mov $1, %eax
    syscall
    mov $60, %eax
    mov %r15d, %edi
    syscall
    ud2

    .section .data
    .balign 4
sa_any:                         # {AF_INET, 9090, 0.0.0.0}
    .short 2
    .byte 0x23, 0x82            # 9090 BE
    .byte 0, 0, 0, 0
    .quad 0
sa_lo:                          # {AF_INET, 9090, 127.0.0.1}
    .short 2
    .byte 0x23, 0x82
    .byte 127, 0, 0, 1
    .quad 0
sa_dead:                        # {AF_INET, 9, 127.0.0.1}
    .short 2
    .byte 0, 9
    .byte 127, 0, 0, 1
    .quad 0
alen:
    .quad 16
ping:
    .ascii "ping"
pong:
    .ascii "pong"
msg:
    .ascii "tcptest: tcp socket checks done\n"
msg_end:

    .section .bss
    .balign 4
sa_out:
    .skip 16
pfd:
    .skip 8
buf:
    .skip 16
