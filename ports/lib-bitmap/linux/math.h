#pragma once
/* include/linux/math.h subset used by find_bit.c/bitmap.c. */
#include <linux/types.h>

#ifndef __KERNEL_DIV_ROUND_UP
#define __KERNEL_DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))
#endif

#define __round_mask(x, y) ((__typeof__(x))((y) - 1))
#define round_up(x, y) ((((x) - 1) | __round_mask(x, y)) + 1)
#define round_down(x, y) ((x) & ~__round_mask(x, y))

#define DIV_ROUND_UP __KERNEL_DIV_ROUND_UP
#define DIV_ROUND_UP_ULL(ll, d) \
	({ unsigned long long _tmp = (ll) + (d) - 1; _tmp / (d); })
