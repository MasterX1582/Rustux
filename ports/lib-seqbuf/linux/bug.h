#pragma once
/* Minimal linux/bug.h shim for the lib-seqbuf harness.
 * Kernel WARN_ON()/WARN_ON_ONCE() return the condition (nonzero when
 * the check fired); BUG_ON() panics — the harness never reaches it
 * (seq_buf_commit is only driven with in-contract num values), so it
 * aborts loudly instead of falling into silent UB.  BUILD_BUG_ON is
 * the classic extern-array trick, like upstream's non-static branch. */
#include <stdio.h>
#include <stdlib.h>
#include <linux/compiler.h>

#define BUG_ON(c) do { if (unlikely(c)) abort(); } while (0)

#define WARN_ON(c) unlikely(c)

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

#define BUILD_BUG_ON(e) ((void)sizeof(char[1 - 2 * !!(e)]))
