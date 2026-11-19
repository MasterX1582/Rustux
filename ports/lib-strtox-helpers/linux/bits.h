#pragma once
/* BIT/GENMASK over a 64-bit unsigned long (LP64 harness). */
#define BIT(nr)			(1UL << (nr))
#define GENMASK(h, l) \
	(((~0UL) << (l)) & ((~0UL) >> (BITS_PER_LONG - 1 - (h))))
