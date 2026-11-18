// SPDX-License-Identifier: GPL-2.0
/*
 * Scalar wrappers for the record/diff harness.
 *
 * The real units in this directory export APIs the ctypes bridge cannot
 * express (pointer out-params, struct returns, flexible-array descriptors).
 * Each function here has a scalar-only signature, calls the real exported
 * function inside the same shared object, and returns one scalar field of
 * its result — so `record` on the C side captures the real unit's behavior
 * and `diff` verifies the Rust port transitively.
 */

#include <linux/types.h>
#include <linux/rational.h>
#include <linux/reciprocal_div.h>
#include <linux/cordic.h>
#include <linux/polynomial.h>

unsigned long rational_n(unsigned long gn, unsigned long gd,
			 unsigned long mn, unsigned long md)
{
	unsigned long n, d;

	rational_best_approximation(gn, gd, mn, md, &n, &d);
	return n;
}

unsigned long rational_d(unsigned long gn, unsigned long gd,
			 unsigned long mn, unsigned long md)
{
	unsigned long n, d;

	rational_best_approximation(gn, gd, mn, md, &n, &d);
	return d;
}

uint32_t reciprocal_m(uint32_t d)
{
	return reciprocal_value(d).m;
}

uint32_t reciprocal_sh1(uint32_t d)
{
	return reciprocal_value(d).sh1;
}

uint32_t reciprocal_sh2(uint32_t d)
{
	return reciprocal_value(d).sh2;
}

uint32_t reciprocal_adv_m(uint32_t d, uint32_t prec)
{
	return reciprocal_value_adv(d, (uint8_t)prec).m;
}

uint32_t reciprocal_adv_sh(uint32_t d, uint32_t prec)
{
	return reciprocal_value_adv(d, (uint8_t)prec).sh;
}

uint32_t reciprocal_adv_exp(uint32_t d, uint32_t prec)
{
	return reciprocal_value_adv(d, (uint8_t)prec).exp;
}

uint32_t reciprocal_adv_wide(uint32_t d, uint32_t prec)
{
	return reciprocal_value_adv(d, (uint8_t)prec).is_wide_m;
}

int32_t cordic_i(int32_t theta)
{
	return cordic_calc_iq(theta).i;
}

int32_t cordic_q(int32_t theta)
{
	return cordic_calc_iq(theta).q;
}

/*
 * Fixed-arity descriptor for polynomial_calc: quadratic c0*x^2 + c1*x + c2
 * with per-degree dividers/leftovers, terminated by the deg==0 term
 * (which is also the constant coefficient, matching the kernel contract).
 */
long polynomial_deg2(long td,
		     long c0, long dv0, long lo0,
		     long c1, long dv1, long lo1,
		     long c2, long dv2, long lo2, long data)
{
	struct {
		long total_divider;
		struct polynomial_term terms[3];
	} p;

	p.total_divider = td;
	p.terms[0] = (struct polynomial_term){2, c0, dv0, lo0};
	p.terms[1] = (struct polynomial_term){1, c1, dv1, lo1};
	p.terms[2] = (struct polynomial_term){0, c2, dv2, lo2};
	return polynomial_calc((struct polynomial *)&p, data);
}
