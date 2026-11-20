# Rustux userspace: brk/mmap/munmap/mprotect selftest.
# mmap=9 mprotect=10 munmap=11 brk=12 read=0 write=1 pipe=22 exit=60.
# PROT_READ=1 PROT_WRITE=2. MAP_PRIVATE=2 MAP_FIXED=0x10 MAP_ANON=0x20.
# MMAP arena: 0x6000_0000_0000 .. 0x7000_0000_0000.
# Exit code = first failing check number; 0 = all green.
    .section .text
    .globl _start
_start:
    # pipe first — read() into a buffer exercises the W-gate, and an
    # empty-pipe read would BLOCK, so every gate-test read is prefilled.
    lea fds(%rip), %rdi
    xor %esi, %esi
    mov $22, %eax
    syscall
    test %rax, %rax
    jns p_ok
    mov $1, %edi
    jmp fail
p_ok:
    # -- brk ------------------------------------------------------------
    # brk(0) -> b0 (r14): nonzero, page-aligned.
    xor %edi, %edi
    mov $12, %eax
    syscall
    test %rax, %rax
    jz f2
    test $0xfff, %rax
    jnz f2
    mov %rax, %r14
    # brk(b0) -> b0 (no-op returns current).
    mov %r14, %rdi
    mov $12, %eax
    syscall
    cmp %r14, %rax
    jne f3
    # brk(b0+0x2000) -> grows two pages, both writable.
    lea 0x2000(%r14), %rdi
    mov $12, %eax
    syscall
    lea 0x2000(%r14), %rcx
    cmp %rcx, %rax
    jne f4
    movb $0x41, (%r14)
    movb $0x42, 0x1fff(%r14)
    cmpb $0x41, (%r14)
    jne f5
    cmpb $0x42, 0x1fff(%r14)
    jne f5
    # read(pipe, heap, 3) — kernel write into a grown heap page.
    mov fds+4(%rip), %edi
    lea xyz(%rip), %rsi
    mov $3, %edx
    mov $1, %eax
    syscall
    mov fds(%rip), %edi
    mov %r14, %rsi
    mov $3, %edx
    xor %eax, %eax
    syscall
    cmp $3, %rax
    jne f6
    cmpb $'x', (%r14)
    jne f7
    # brk shrink to b0+0x1000 — page 2 leaves the VMA.
    lea 0x1000(%r14), %rdi
    mov $12, %eax
    syscall
    lea 0x1000(%r14), %rcx
    cmp %rcx, %rax
    jne f8
    mov fds+4(%rip), %edi
    lea xyz(%rip), %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    mov fds(%rip), %edi
    lea 0x1000(%r14), %rsi        # one byte PAST the new break page
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $-14, %rax                # EFAULT — outside the shrunk VMA
    jne f9
    # brk below base is refused, current break returned unchanged.
    lea -1(%r14), %rdi
    mov $12, %eax
    syscall
    lea 0x1000(%r14), %rcx
    cmp %rcx, %rax
    jne f10
    # -- mmap ------------------------------------------------------------
    # mmap(0, 0x2000, RW, ANON|PRIVATE) -> MMAP_LO on the empty arena.
    xor %edi, %edi
    mov $0x2000, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    mov $0x600000000000, %rcx
    cmp %rcx, %rax
    jne f11
    mov %rax, %r13
    movb $0x5a, (%r13)
    movb $0x5b, 0x1fff(%r13)
    cmpb $0x5a, (%r13)
    jne f12
    cmpb $0x5b, 0x1fff(%r13)
    jne f12
    # mmap(0, 0, ...) -> EINVAL.
    xor %edi, %edi
    xor %esi, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp $-22, %rax
    jne f13
    # mmap PRIVATE-but-not-ANONYMOUS -> ENODEV (no file VMAs yet).
    xor %edi, %edi
    mov $0x1000, %esi
    mov $3, %edx
    mov $2, %r10d
    xor %r8d, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp $-19, %rax
    jne f14
    # MAP_FIXED at 0x400000 — inside kernel-shared PML4 slots — must
    # be EINVAL. This is the line between a VMA and page-table
    # corruption; Linux refuses addr < mmap_min_addr the same way.
    mov $0x400000, %edi
    mov $0x1000, %esi
    mov $3, %edx
    mov $0x32, %r10d              # PRIVATE|FIXED|ANON
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp $-22, %rax
    jne f15
    # -- mprotect ---------------------------------------------------------
    # mprotect(va, 0x1000, PROT_READ) — W gate goes, R gate stays.
    mov %r13, %rdi
    mov $0x1000, %esi
    mov $1, %edx
    mov $10, %eax
    syscall
    test %rax, %rax
    jnz f16
    mov fds+4(%rip), %edi
    lea xyz(%rip), %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    mov fds(%rip), %edi
    mov %r13, %rsi
    mov $1, %edx
    xor %eax, %eax
    syscall
    cmp $-14, %rax                # EFAULT — kernel write refused
    jne f17
    mov $1, %edi                  # write(1, va, 1) still reads fine
    mov %r13, %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    cmp $1, %rax
    jne f18
    # -- munmap -----------------------------------------------------------
    # munmap whole 2-page VMA -> 0; the VA then EFAULTs the gate.
    mov %r13, %rdi
    mov $0x2000, %esi
    mov $11, %eax
    syscall
    test %rax, %rax
    jnz f19
    mov $1, %edi
    mov %r13, %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    cmp $-14, %rax
    jne f20
    # Fresh anonymous mmap lands in the freed hole (first-fit reuse).
    xor %edi, %edi
    mov $0x1000, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp %r13, %rax
    jne f21
    # MAP_FIXED at a chosen free address, then AGAIN over itself —
    # the replace path must succeed and return the same VA.
    lea 0x10000(%r13), %r12
    mov %r12, %rdi
    mov $0x1000, %esi
    mov $3, %edx
    mov $0x32, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp %r12, %rax
    jne f22
    mov %r12, %rdi
    mov $0x1000, %esi
    mov $3, %edx
    mov $0x32, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp %r12, %rax
    jne f23
    # -- VMA split --------------------------------------------------------
    # mmap 3 pages at A; cut the middle; neighbors keep their data;
    # a fresh mmap lands exactly in the middle hole.
    xor %edi, %edi
    mov $0x3000, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    mov $0x600000001000, %rcx     # hole after the 1-page remap
    cmp %rcx, %rax
    jne f24
    mov %rax, %rbx
    movb $0xaa, (%rbx)
    movb $0xbb, 0x1000(%rbx)
    movb $0xcc, 0x2000(%rbx)
    lea 0x1000(%rbx), %rdi
    mov $0x1000, %esi
    mov $11, %eax
    syscall
    test %rax, %rax
    jnz f25
    cmpb $0xaa, (%rbx)            # left survivor intact
    jne f26
    cmpb $0xcc, 0x2000(%rbx)      # right survivor intact
    jne f26
    mov $1, %edi                  # middle is EFAULT for kernel reads
    lea 0x1000(%rbx), %rsi
    mov $1, %edx
    mov $1, %eax
    syscall
    cmp $-14, %rax
    jne f27
    xor %edi, %edi                # first-fit must find the hole
    mov $0x1000, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8d
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    lea 0x1000(%rbx), %rcx
    cmp %rcx, %rax
    jne f28
    # -- bad args ---------------------------------------------------------
    mov %rbx, %rdi                # unaligned munmap -> EINVAL
    inc %rdi
    mov $0x1000, %esi
    mov $11, %eax
    syscall
    cmp $-22, %rax
    jne f29
    mov %rbx, %rdi                # len 0 -> EINVAL
    xor %esi, %esi
    mov $11, %eax
    syscall
    cmp $-22, %rax
    jne f30
    mov $0x600000080000, %rdi     # mprotect over a hole -> ENOMEM
    mov $0x1000, %esi
    mov $1, %edx
    mov $10, %eax
    syscall
    cmp $-12, %rax
    jne f31
    mov %rbx, %rdi                # unaligned mprotect -> EINVAL
    inc %rdi
    mov $0x1000, %esi
    mov $1, %edx
    mov $10, %eax
    syscall
    cmp $-22, %rax
    jne f32
    # -- big brk ------------------------------------------------------------
    # Grow the heap by 8 pages in one call and touch the last one —
    # catches partial-map bookkeeping bugs.
    lea 0x8000(%r14), %rdi
    mov $12, %eax
    syscall
    lea 0x8000(%r14), %rcx
    cmp %rcx, %rax
    jne f33
    movb $0x77, 0x7fff(%r14)
    cmpb $0x77, 0x7fff(%r14)
    jne f34
    # fork inherits the VMA map: child reads parent's heap bytes.
    mov $57, %eax
    syscall
    test %rax, %rax
    js f35
    jz child_ok
    mov $61, %eax                 # wait4(-1)
    mov $-1, %rdi
    lea wst(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    test %rax, %rax
    js f36
    mov wst(%rip), %eax           # child must have exited 0
    test %eax, %eax
    jnz f37
    jmp report
child_ok:
    cmpb $0x77, 0x7fff(%r14)      # deep-copied heap page
    jne cf
    xor %edi, %edi
    mov $60, %eax
    syscall
    ud2
cf: mov $40, %edi
    mov $60, %eax
    syscall
    ud2

report:
    mov $1, %eax
    mov $1, %edi
    lea msg(%rip), %rsi
    mov $(msg_end - msg), %edx
    syscall
    xor %edi, %edi
    mov $60, %eax
    syscall
    ud2

f2:  mov $2, %edi
     jmp fail
f3:  mov $3, %edi
     jmp fail
f4:  mov $4, %edi
     jmp fail
f5:  mov $5, %edi
     jmp fail
f6:  mov $6, %edi
     jmp fail
f7:  mov $7, %edi
     jmp fail
f8:  mov $8, %edi
     jmp fail
f9:  mov $9, %edi
     jmp fail
f10: mov $10, %edi
     jmp fail
f11: mov $11, %edi
     jmp fail
f12: mov $12, %edi
     jmp fail
f13: mov $13, %edi
     jmp fail
f14: mov $14, %edi
     jmp fail
f15: mov $15, %edi
     jmp fail
f16: mov $16, %edi
     jmp fail
f17: mov $17, %edi
     jmp fail
f18: mov $18, %edi
     jmp fail
f19: mov $19, %edi
     jmp fail
f20: mov $20, %edi
     jmp fail
f21: mov $21, %edi
     jmp fail
f22: mov $22, %edi
     jmp fail
f23: mov $23, %edi
     jmp fail
f24: mov $24, %edi
     jmp fail
f25: mov $25, %edi
     jmp fail
f26: mov $26, %edi
     jmp fail
f27: mov $27, %edi
     jmp fail
f28: mov $28, %edi
     jmp fail
f29: mov $29, %edi
     jmp fail
f30: mov $30, %edi
     jmp fail
f31: mov $31, %edi
     jmp fail
f32: mov $32, %edi
     jmp fail
f33: mov $33, %edi
     jmp fail
f34: mov $34, %edi
     jmp fail
f35: mov $35, %edi
     jmp fail
f36: mov $36, %edi
     jmp fail
f37: mov $37, %edi
     jmp fail
fail:
    mov $60, %eax
    syscall
    ud2

    .section .data
    .balign 4
fds:
    .long 0, 0
wst:
    .long 0
xyz:
    .ascii "xyz"

    .section .text
msg:
    .ascii "mmtest: brk/mmap/munmap/mprotect checks done\n"
msg_end:
