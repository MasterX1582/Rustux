# spinA — preemption proof: writes 'a' with a spin delay long enough
# to cross several 100 Hz LAPIC ticks. Interleaved 'a's and 'b's in
# the serial log prove the timer IRQ switches between live tasks.
	.globl _start
_start:
	mov $300, %r15d
1:
	# write(1, ch, 1)
	mov $1, %eax
	mov $1, %edi
	lea ch(%rip), %rsi
	mov $1, %edx
	syscall
	# spin so the whole loop lasts several timer ticks
	mov $300000, %r14d
2:	dec %r14d
	jnz 2b
	dec %r15d
	jnz 1b
	# write(1, nl, 1)
	mov $1, %eax
	mov $1, %edi
	lea nl(%rip), %rsi
	mov $1, %edx
	syscall
	# exit(0)
	mov $60, %eax
	xor %edi, %edi
	syscall
ch:	.ascii "a"
nl:	.ascii "\n"
