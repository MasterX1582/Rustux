/* Userspace shim for <linux/polynomial.h> — verbatim from the kernel header. */
#pragma once

struct polynomial_term {
	unsigned int deg;
	long coef;
	long divider;
	long divider_leftover;
};

struct polynomial {
	long total_divider;
	struct polynomial_term terms[];
};

long polynomial_calc(const struct polynomial *poly, long data);
