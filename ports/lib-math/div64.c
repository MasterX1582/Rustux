// SPDX-License-Identifier: GPL-2.0
/*
 * Copyright (C) 2003 Bernardo Innocenti <bernie@develer.com>
 *
 * Based on former do_div() implementation from asm-parisc/div64.h:
 *	Copyright (C) 1999 Hewlett-Packard Co
 *	Copyright (C) 1999 David Mosberger-Tang <davidm@hpl.hp.com>
 *
 *
 * Generic C version of 64bit/32bit division and modulo, with
 * 64bit result and 32bit remainder.
 *
 * The fast case for (n>>32 == 0) is handled inline by do_div().
 *
 * Code generated for this function might be very inefficient
 * for some CPUs. __div64_32() can be overridden by linking arch-specific
 * assembly versions such as arch/ppc/lib/div64.S and arch/sh/lib/div64.S
 * or by defining a preprocessor macro in arch/include/asm/div64.h.
 */

#include <linux/bitops.h>
#include <linux/export.h>
#include <linux/math.h>
#include <linux/math64.h>
#include <linux/minmax.h>
#include <linux/log2.h>

/*
 * Harness normalization: this copy is the LP64-resolved extract of the
 * upstream file — the `#if BITS_PER_LONG == 32` region, the BITS_PER_ITER==16
 * branches and the preprocessor guards were removed to match what an LP64
 * gcc build of the unit actually compiles. The harness-wide LP64 check
 * lives in <linux/types.h>.
 */

/* Not needed on 64bit architectures */

/*
 * Iterative div/mod for use when dividend is not expected to be much
 * bigger than divisor.
 */
uint32_t iter_div_u64_rem(uint64_t dividend, uint32_t divisor, uint64_t *remainder)
{
	return __iter_div_u64_rem(dividend, divisor, remainder);
}
EXPORT_SYMBOL(iter_div_u64_rem);


#define mul_add(a, b, c) add_u64_u32(mul_u32_u32(a, b), c)

static inline uint64_t mul_u64_u64_add_u64(uint64_t *p_lo, uint64_t a, uint64_t b, uint64_t c)
{
	/* native 64x64=128 bits multiplication */
	u128 prod = (u128)a * b + c;

	*p_lo = prod;
	return prod >> 64;
}

#ifndef BITS_PER_ITER
#define BITS_PER_ITER (__LONG_WIDTH__ >= 64 ? 32 : 16)
#endif

#define mul_u64_long_add_u64(p_lo, a, b, c) mul_u64_u64_add_u64(p_lo, a, b, c)
#define add_u64_long(a, b) ((a) + (b))

uint64_t mul_u64_add_u64_div_u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d)
{
	unsigned long d_msig, q_digit;
	unsigned int reps, d_z_hi;
	uint64_t quotient, n_lo, n_hi;
	uint32_t overflow;

	n_hi = mul_u64_u64_add_u64(&n_lo, a, b, c);

	if (!n_hi)
		return div64_u64(n_lo, d);

	if (unlikely(n_hi >= d)) {
		/* trigger runtime exception if divisor is zero */
		if (d == 0) {
			unsigned long zero = 0;

			OPTIMIZER_HIDE_VAR(zero);
			return ~0UL/zero;
		}
		/* overflow: result is unrepresentable in a uint64_t */
		return ~0ULL;
	}

	/* Left align the divisor, shifting the dividend to match */
	d_z_hi = __builtin_clzll(d);
	if (d_z_hi) {
		d <<= d_z_hi;
		n_hi = n_hi << d_z_hi | n_lo >> (64 - d_z_hi);
		n_lo <<= d_z_hi;
	}

	reps = 64 / BITS_PER_ITER;
	/* Optimise loop count for small dividends */
	if (!(uint32_t)(n_hi >> 32)) {
		reps -= 32 / BITS_PER_ITER;
		n_hi = n_hi << 32 | n_lo >> 32;
		n_lo <<= 32;
	}

	/* Invert the dividend so we can use add instead of subtract. */
	n_lo = ~n_lo;
	n_hi = ~n_hi;

	/*
	 * Get the most significant BITS_PER_ITER bits of the divisor.
	 * This is used to get a low 'guestimate' of the quotient digit.
	 */
	d_msig = (d >> (64 - BITS_PER_ITER)) + 1;

	/*
	 * Now do a 'long division' with BITS_PER_ITER bit 'digits'.
	 * The 'guess' quotient digit can be low and BITS_PER_ITER+1 bits.
	 * The worst case is dividing ~0 by 0x8000 which requires two subtracts.
	 */
	quotient = 0;
	while (reps--) {
		q_digit = (unsigned long)(~n_hi >> (64 - 2 * BITS_PER_ITER)) / d_msig;
		/* Shift 'n' left to align with the product q_digit * d */
		overflow = n_hi >> (64 - BITS_PER_ITER);
		n_hi = add_u64_u32(n_hi << BITS_PER_ITER, n_lo >> (64 - BITS_PER_ITER));
		n_lo <<= BITS_PER_ITER;
		/* Add product to negated divisor */
		overflow += mul_u64_long_add_u64(&n_hi, d, q_digit, n_hi);
		/* Adjust for the q_digit 'guestimate' being low */
		while (overflow < 0xffffffff >> (32 - BITS_PER_ITER)) {
			q_digit++;
			n_hi += d;
			overflow += n_hi < d;
		}
		quotient = add_u64_long(quotient << BITS_PER_ITER, q_digit);
	}

	/*
	 * The above only ensures the remainder doesn't overflow,
	 * it can still be possible to add (aka subtract) another copy
	 * of the divisor.
	 */
	if ((n_hi + d) > n_hi)
		quotient++;
	return quotient;
}
EXPORT_SYMBOL(mul_u64_add_u64_div_u64);
