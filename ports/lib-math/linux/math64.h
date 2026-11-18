/* Userspace shim for <linux/math64.h>: on LP64 the kernel's div_u64* inline
 * helpers reduce to plain division. */
#pragma once

#include <linux/types.h>
#include <linux/compiler.h>

/* Verbatim LP64 do_div from <asm-generic/div64.h> (base is u32-truncated,
 * matching <asm/div64.h>; kept here for TUs that only see math64.h). */
#define do_div(n, base) ({					\
	u32 __base = (base);					\
	u32 __rem;						\
	__rem = ((u64)(n)) % __base;				\
	(n) = ((u64)(n)) / __base;				\
	__rem;							\
})

static inline u64 div_u64(u64 a, u64 b) { return a / b; }
static inline s64 div_s64(s64 a, s64 b) { return a / b; }
static inline u64 div_u64_rem(u64 a, u64 b, u32 *rem) { *rem = (u32)(a % b); return a / b; }
static inline s64 div_s64_rem(s64 a, s64 b, s32 *rem) { *rem = (s32)(a % b); return a / b; }
static inline u64 div64_u64(u64 dividend, u64 divisor) { return dividend / divisor; }
static inline u64 div64_u64_rem(u64 dividend, u64 divisor, u64 *remainder)
{
	*remainder = dividend % divisor;
	return dividend / divisor;
}

/* Verbatim from <vdso/math64.h>: the iterative divider iter_div_u64_rem calls. */
static inline u32 __iter_div_u64_rem(u64 dividend, u32 divisor, u64 *remainder)
{
	u32 ret = 0;

	while (dividend >= divisor) {
		/*
		 * Prevent the compiler from optimising this loop into a
		 * modulo operation.
		 */
		OPTIMIZER_HIDE_VAR(dividend);

		dividend -= divisor;
		ret++;
	}

	*remainder = dividend;

	return ret;
}

/* Verbatim from <linux/math64.h>. */
static inline u64 mul_u32_u32(u32 a, u32 b)
{
	return (u64)a * b;
}

static inline u64 add_u64_u32(u64 a, u32 b)
{
	return a + b;
}
