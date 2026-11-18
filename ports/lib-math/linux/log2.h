/* Userspace shim for <linux/log2.h>. */
#pragma once

#include <linux/bitops.h>

static inline int ilog2(unsigned long n) { return n ? fls(n) - 1 : -1; }
