/* Userspace shim for <linux/bitops.h>: the ffs/fls family used by lib/math. */
#pragma once

#define BITS_PER_LONG (__SIZEOF_LONG__ * 8)

#define BIT(nr) (1UL << (nr))

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

/* fls: position of the most significant set bit, 1-based; 0 for 0.
 * Upstream generic_fls takes `unsigned int` (32-bit), not unsigned long. */
static inline int fls(unsigned int word)
{
	return word ? (int)(sizeof(word) * 8) - __builtin_clz(word) : 0;
}

/* fls64: same for 64-bit words; 0 for 0 (upstream fls64 guards zero). */
static inline int fls64(unsigned long long word)
{
	return word ? 64 - __builtin_clzll(word) : 0;
}
