#pragma once
/* ilog2/round[up|down]_pow_of_two over the u32/u16 ranges this unit
 * uses. */
#include <linux/types.h>

static inline int ilog2_u32(u32 n)
{
	return 31 - __builtin_clz(n);
}
#define ilog2(n) ilog2_u32(n)

static inline unsigned long roundup_pow_of_two(unsigned long n)
{
	return 1UL << (n == 1 ? 0 : (32 - __builtin_clz((unsigned int)n - 1)));
}

static inline unsigned long rounddown_pow_of_two(unsigned long n)
{
	return 1UL << (31 - __builtin_clz((unsigned int)n | 1));
}
