#pragma once
#include <linux/types.h>
#include <linux/minmax.h>
#include <linux/container_of.h>
#include <linux/compiler.h>
#include <linux/errno.h>

#define ARRAY_SIZE(a)		(sizeof(a) / sizeof((a)[0]))

#define BUILD_BUG_ON(cond)	((void)sizeof(char[1 - 2 * !!(cond)]))
#define BUG_ON(cond)		do { if (cond) __builtin_trap(); } while (0)
#define BUG()			__builtin_trap()
#define WARN_ON(cond)		(!!(cond))
#define WARN_ON_ONCE(cond)	(!!(cond))

/* Sequential cmpxchg: same semantics single-threaded. */
#define cmpxchg(ptr, old, new) ({					\
	__typeof__(*(ptr)) __old = (old);				\
	__typeof__(*(ptr)) __cur = *(ptr);				\
	if (__cur == __old)						\
		*(ptr) = (__typeof__(*(ptr)))(new);			\
	__cur;								\
})

#define container_of_safe(ptr, type, member) ({				\
	typeof(ptr) _ptr = (ptr);					\
	_ptr ? container_of(_ptr, type, member) : (type *)NULL; })
