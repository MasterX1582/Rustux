/* Userspace shim for <linux/bitops.h>: only the fls family int_sqrt.c needs. */
#pragma once

#define BITS_PER_LONG (__SIZEOF_LONG__ * 8)

/* __fls: index of the most significant set bit, undefined for 0 (kernel contract). */
static inline int __fls(unsigned long word)
{
	return (int)(BITS_PER_LONG - 1) - __builtin_clzl(word);
}

/* fls64: position of the most significant set bit, 1-based; 0 for 0. */
static inline int fls64(unsigned long long word)
{
	return 64 - __builtin_clzll(word);
}
