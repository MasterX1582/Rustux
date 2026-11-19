#pragma once
/* linux/kernel.h shim for the lib-maple-tree harness: pulls together the
 * pieces maple_tree.h/maple_tree.c need (min/max, WARN/BUG, limits,
 * string, ARRAY_SIZE, swap, printk no-ops).  pr_*() return the int
 * printk() does upstream; in the sequential build nothing is printed. */
#include <linux/compiler.h>
#include <linux/types.h>
#include <linux/stddef.h>
#include <linux/limits.h>
#include <linux/minmax.h>
#include <linux/bug.h>
#include <linux/string.h>
#include <linux/container_of.h>
#include <linux/list.h>
#include <linux/errno.h>
#include <linux/err.h>

#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))

#define swap(a, b) \
	do { typeof(a) __tmp = (a); (a) = (b); (b) = __tmp; } while (0)

#define static_assert(expr, ...) __static_assert(expr, ##__VA_ARGS__, #expr)
#define __static_assert(expr, msg, ...) _Static_assert(expr, msg)

#define ASSERT_STATIC_STORAGE(name)

#define pr_emerg(fmt, ...)	0
#define pr_alert(fmt, ...)	0
#define pr_crit(fmt, ...)	0
#define pr_err(fmt, ...)	0
#define pr_warn(fmt, ...)	0
#define pr_notice(fmt, ...)	0
#define pr_info(fmt, ...)	0
#define pr_debug(fmt, ...)	0
#define pr_cont(fmt, ...)	0
#define printk(fmt, ...)	0
#define dump_stack()		abort()
