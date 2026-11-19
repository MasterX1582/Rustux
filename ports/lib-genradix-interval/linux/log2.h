#pragma once
/* linux/log2.h shim for the lib-genradix-interval harness.
 * Upstream ilog2() on a non-constant 64-bit value is fls64(n)-1, and
 * fls64(0)==0, so ilog2(0) == -1 -- __genradix_ptr_alloc() relies on
 * that to accept offset 0 into a depth-0 tree. */
#include <linux/types.h>

static inline __attribute__((__const__)) int ilog2_u64ish(unsigned long long n)
{
	return n ? 63 - __builtin_clzll(n) : -1;
}
#define ilog2(n) ilog2_u64ish((unsigned long long)(n))

static inline __attribute__((__const__)) int is_power_of_2(unsigned long n)
{
	return n != 0 && (n & (n - 1)) == 0;
}

#define roundup_pow_of_two(n) \
	(1ULL << (ilog2_u64ish((unsigned long long)(n) - 1) + 1))
