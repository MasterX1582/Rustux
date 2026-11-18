/* Userspace shim for <linux/compiler.h>. */
#pragma once

#define likely(x) __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define __always_inline inline __attribute__((__always_inline__))
#define __attribute_const__ __attribute__((__const__))
