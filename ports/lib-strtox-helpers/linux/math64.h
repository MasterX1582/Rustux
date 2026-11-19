#pragma once
#include <linux/types.h>
#include <linux/limits.h>
#include <linux/minmax.h>	/* kstrtox.c's from_user macros use min() */

/* do_div(): divide u64 n by u32 base in place, return the u32 remainder.
 * Matches the kernel semantics for the u64/u32 case used here. */
#define do_div(n, base) ({					\
	unsigned int __base = (base);				\
	unsigned int __rem = (unsigned int)((n) % __base);	\
	(n) /= __base;						\
	__rem; })

/* int_pow - computes base^exp, wrapping like lib/math/int_pow.c. */
static inline u64 int_pow(u64 base, unsigned int exp)
{
	u64 result = 1;

	while (exp) {
		if (exp & 1)
			result *= base;
		exp >>= 1;
		base *= base;
	}
	return result;
}
