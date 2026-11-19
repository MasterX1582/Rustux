#pragma once
/* linux/math.h shim: DIV_ROUND_UP (used by RADIX_TREE_MAX_PATH and
 * friends) and ilog2 (used by idr.c's IDA_CHUNK_SHIFT). */
#include <linux/types.h>

#define DIV_ROUND_UP(n, d)	(((n) + (d) - 1) / (d))

#define ilog2(n)	((int)(63 - __builtin_clzll((unsigned long long)(n))))
