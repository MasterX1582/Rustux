#pragma once
/* linux/kernel.h shim: the small kernel.h surface the two units use.
 * abs() is the kernel's long-long aware macro; ALIGN/min/max come from
 * minmax.h; pr_* logging degrades to nothing (diagnostics, not state —
 * WARN_ON_ONCE in linux/bug.h is the observable variant). */
#include <linux/types.h>
#include <linux/stddef.h>
#include <linux/minmax.h>
#include <linux/bug.h>
#include <linux/compiler.h>
#include <linux/string.h>
#include <linux/errno.h>
#include <linux/smp.h>

#undef abs
#define abs(x) ({			\
	long long __x = (x);		\
	(__x < 0) ? -__x : __x;		\
})

#define __ALIGN_KERNEL(x, a) \
	(((__typeof__(x))(x)) + ((__typeof__(x))(a)) - 1 & ~((__typeof__(x))(a) - 1))
#define __ALIGN_KERNEL_MASK(x, mask) \
	(((__typeof__(x))(x)) + ((__typeof__(mask))(mask)) & ~(__typeof__(mask))(mask))
#define ALIGN(x, a) __ALIGN_KERNEL((x), (a))
#define ALIGN_DOWN(x, a) __ALIGN_KERNEL((x) - ((a) - 1), (a))
#define PTR_ALIGN(p, a) ((__typeof__(p))ALIGN((unsigned long)(p), (a)))

#define pr_fmt(fmt) fmt
#define printk(...) do { } while (0)
#define pr_warn(...) do { } while (0)
#define pr_warn_once(...) do { } while (0)
#define pr_info(...) do { } while (0)
#define printk_deferred(...) do { } while (0)

#define U64_MAX ((u64)~0ULL)
