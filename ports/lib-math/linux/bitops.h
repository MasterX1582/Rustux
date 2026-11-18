/* Userspace shim for <linux/bitops.h>: the ffs/fls family used by lib/math. */
#pragma once

#define BITS_PER_LONG (__SIZEOF_LONG__ * 8)

/* __ffs: index of the least significant set bit, undefined for 0. */
static inline unsigned long __ffs(unsigned long word)
{
	return __builtin_ctzl(word);
}

/* __fls: index of the most significant set bit, undefined for 0. */
static inline int __fls(unsigned long word)
{
	return (int)(BITS_PER_LONG - 1) - __builtin_clzl(word);
}

/* fls: position of the most significant set bit, 1-based; 0 for 0. */
static inline int fls(unsigned long word)
{
	return word ? (int)BITS_PER_LONG - __builtin_clzl(word) : 0;
}

/* fls64: same for 64-bit words. */
static inline int fls64(unsigned long long word)
{
	return 64 - __builtin_clzll(word);
}
