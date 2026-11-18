#pragma once
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/bitops.h>
#include <linux/unaligned.h>
#include <linux/minmax.h>

#include <stdlib.h>

#define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]))

/* Kernel panic() aborts the boot; only reachable from the compiled-out
 * FIPS self-test blocks, but kept defined for completeness. */
#define panic(...) abort()
