#pragma once
/* linux/kernel.h shim for the lib-assoc-array harness: the arithmetic
 * helpers assoc_array.c uses.  Semantics match upstream:
 *   __ffs()        is 0-based (upstream __ffs, NOT libc ffs)
 *   ilog2()        floor(log2(n)); the priv header only feeds it
 *                  compile-time constants (16 -> 4, 64 -> 6)
 *   round_up/down  power-of-two alignment, as upstream
 *   struct_size / flex_array_size for the shortcut's trailing
 *   index_key[] flexible array. */
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/bug.h>
#include <string.h>

#define ARRAY_SIZE(a)		(sizeof(a) / sizeof((a)[0]))

#define round_up(x, y)		(((x) + (y) - 1) & ~((y) - 1))
#define round_down(x, y)	((x) & ~((y) - 1))

#define ilog2(n)		(63 - __builtin_clzll((unsigned long long)(n)))
#define __ffs(x)		(__builtin_ffsl((long)(x)) - 1)
#define __ffs64(x)		(__builtin_ffsll((long long)(x)) - 1)
#define ffs(x)			__builtin_ffs((int)(x))

#define container_of(ptr, type, member) \
	((type *)((char *)(ptr) - offsetof(type, member)))

#define smp_wmb()		barrier()
#define smp_mb()		barrier()
#define smp_rmb()		barrier()

/* printk: pr_devel compiles to nothing without DEBUG, like upstream. */
#define pr_devel(fmt, ...)	do { } while (0)
#define pr_err(fmt, ...)	do { } while (0)
#define printk(fmt, ...)	do { } while (0)

/* flexible-array sizing, matching upstream overflow.h */
#define flex_array_size(p, member, count) \
	(sizeof((p)->member[0]) * (count))
#define struct_size(p, member, count) \
	(sizeof(*(p)) + flex_array_size(p, member, count))
#define struct_size_t(type, member, count) \
	struct_size((type *)NULL, member, count)
