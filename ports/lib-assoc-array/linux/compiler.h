#pragma once
/* linux/compiler.h shim: READ_ONCE/WRITE_ONCE are plain volatile
 * accesses (the harness is single-threaded; they exist for RCU
 * address-dependency ordering upstream). */
#include <linux/types.h>

#define likely(x)	__builtin_expect(!!(x), 1)
#define unlikely(x)	__builtin_expect(!!(x), 0)

#define barrier()	__asm__ __volatile__("" ::: "memory")

#define READ_ONCE(x)		(*(volatile typeof(x) *)&(x))
#define WRITE_ONCE(x, v)	(*(volatile typeof(x) *)&(x) = (v))

#define __printf(a, b)
