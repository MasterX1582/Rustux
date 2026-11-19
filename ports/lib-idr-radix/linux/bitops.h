#pragma once
/* linux/bitops.h shim for the lib-idr-radix harness: the handful of
 * bit helpers radix-tree.c and the radix-tree.h inline layer use
 * (__ffs/test_and_set family) live in linux/bitmap.h here, exactly as
 * upstream folds asm/bitops into the same logical API. */
#include <linux/bitmap.h>
