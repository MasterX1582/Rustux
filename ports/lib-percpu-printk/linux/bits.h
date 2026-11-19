#pragma once
/* linux/bits.h shim: BIT macros; BITS_PER_LONG lives in types.h. */
#include <linux/types.h>

#define BIT(nr) (1UL << (nr))
#define BIT_ULL(nr) (1ULL << (nr))
#define GENMASK(h, l) (((~0UL) >> (BITS_PER_LONG - 1 - (h))) & ((~0UL) << (l)))
