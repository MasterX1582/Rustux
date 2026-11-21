    .intel_syntax noprefix
    .section .text
    .globl _start
# rtest — rseq(334) registration ABI: register, cpu_id fields,
# EBUSY/EINVAL honesty, unregister, re-register. Needs a non-glibc
# task: glibc registers its own rseq area at startup (an EBUSY on
# a second call is the proof the dynamic path engaged too — see
# dyntest). Exit code = failure count.
_start:
    xor %r15d, %r15d

    # rseq(&area, 32, 0, RSEQ_SIG) -> 0
    lea rdi, [rip+area]
    mov esi, 32
    xor edx, edx
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    test rax, rax
    jz 1f
    inc %r15d
1:
    # cpu_id_start == 0 and cpu_id == 0 (single CPU)
    cmp dword ptr [rip+area], 0
    je 2f
    inc %r15d
2:
    cmp dword ptr [rip+area+4], 0
    je 3f
    inc %r15d
3:
    # second register -> -16 EBUSY
    lea rdi, [rip+area]
    mov esi, 32
    xor edx, edx
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    cmp rax, -16
    je 4f
    inc %r15d
4:
    # bad sig -> -22 EINVAL
    lea rdi, [rip+area]
    mov esi, 32
    xor edx, edx
    mov r10d, 0x12345678
    mov eax, 334
    syscall
    cmp rax, -22
    je 5f
    inc %r15d
5:
    # unaligned area -> -22 EINVAL
    lea rdi, [rip+area2+4]
    mov esi, 32
    xor edx, edx
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    cmp rax, -22
    je 6f
    inc %r15d
6:
    # short len -> -22 EINVAL
    lea rdi, [rip+area]
    mov esi, 16
    xor edx, edx
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    cmp rax, -22
    je 7f
    inc %r15d
7:
    # unknown flags -> -22 EINVAL
    lea rdi, [rip+area]
    mov esi, 32
    mov edx, 2
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    cmp rax, -22
    je 8f
    inc %r15d
8:
    # unregister of a different area -> -22 EINVAL
    lea rdi, [rip+area2]
    mov esi, 32
    mov edx, 1
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    cmp rax, -22
    je 9f
    inc %r15d
9:
    # unregister ours -> 0
    lea rdi, [rip+area]
    mov esi, 32
    mov edx, 1
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    test rax, rax
    jz 10f
    inc %r15d
10:
    # re-register -> 0 (proves the unregister cleared the slot)
    lea rdi, [rip+area]
    mov esi, 32
    xor edx, edx
    mov r10d, 0x53053053
    mov eax, 334
    syscall
    test rax, rax
    jz 11f
    inc %r15d
11:
    # report + exit(failures)
    mov eax, 1
    mov edi, 1
    lea rsi, [rip+msg]
    mov edx, msg_end - msg
    syscall
    mov eax, 60
    mov edi, %r15d
    syscall
    ud2

    .section .bss
    .balign 32
area:
    .space 64
area2:
    .space 64

    .section .rodata
msg:
    .ascii "rtest: rseq registration checks done\n"
msg_end:
