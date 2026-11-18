#pragma once
#include <stdio.h>
#include <stdlib.h>
#include <linux/compiler.h>

/* Kernel BUG_ON() panics; the harness never reaches it (the strlcat
 * wrapper always passes count > strlen(dest)).  Abort so a violated
 * invariant is loud instead of silent UB. */
#define BUG_ON(c) do { if (unlikely(c)) abort(); } while (0)

#define WARN_ON_ONCE(c) ({						\
	static int __seen;						\
	int __cond = !!(c);						\
	if (__cond && !__seen) {					\
		__seen = 1;						\
		fprintf(stderr, "WARN_ON_ONCE at %s:%d\n",		\
			__FILE__, __LINE__);				\
	}								\
	__cond;								\
})
