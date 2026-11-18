#pragma once
/* linux/log2.h shim — non-constant power-of-2 helpers mirroring
 * upstream semantics (fls/fls64/fls_long via compiler builtins). */
#include <linux/types.h>

static __always_inline __attribute__((const))
int fls(unsigned int x)
{
	return x ? 32 - __builtin_clz(x) : 0;
}

static __always_inline __attribute__((const))
int fls64(u64 x)
{
	return x ? 64 - __builtin_clzll(x) : 0;
}

static __always_inline __attribute__((const))
int fls_long(unsigned long x)
{
	return x ? (int)sizeof(unsigned long) * 8 - __builtin_clzl(x) : 0;
}

static __always_inline __attribute__((const))
int __ilog2_u32(u32 n)
{
	return fls(n) - 1;
}

static __always_inline __attribute__((const))
int __ilog2_u64(u64 n)
{
	return fls64(n) - 1;
}

/* upstream: n - 1 < (n ^ (n - 1)) — zero is not a power of two */
static __always_inline __attribute__((const))
bool is_power_of_2(unsigned long n)
{
	return n - 1 < (n ^ (n - 1));
}

static __always_inline __attribute__((const))
unsigned long __roundup_pow_of_two(unsigned long n)
{
	return 1UL << fls_long(n - 1);
}

static __always_inline __attribute__((const))
unsigned long __rounddown_pow_of_two(unsigned long n)
{
	return 1UL << (fls_long(n) - 1);
}

#define ilog2(n) \
( \
	__builtin_constant_p(n) ?	\
	((n) < 2 ? 0 :			\
	 63 - __builtin_clzll(n)) :	\
	(sizeof(n) <= 4) ?		\
	__ilog2_u32(n) :		\
	__ilog2_u64(n)			\
 )

#define roundup_pow_of_two(n)			\
(						\
	__builtin_constant_p(n) ? (		\
		((n) == 1) ? 1 :		\
		(1UL << (ilog2((n) - 1) + 1))	\
				   ) :		\
	__roundup_pow_of_two(n)			\
 )

#define rounddown_pow_of_two(n)			\
(						\
	__builtin_constant_p(n) ? (		\
		(1UL << ilog2(n))) :		\
	__rounddown_pow_of_two(n)		\
 )
