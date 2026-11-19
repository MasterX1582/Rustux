#pragma once
/* linux/bug.h shim for the lib-xarray harness.
 * BUG_ON()/BUG() abort loudly: the contract-level callers in wrappers.c
 * never pass arguments that trip them; a trip means the harness itself
 * is wrong, so dying is correct.  WARN_ON()/WARN_ON_ONCE() return the
 * condition (like upstream) and bump kshim_warn_hits, which wrappers.c
 * folds so a C-side WARN is observable in the checksum (the Rust port
 * mirrors it as a counter, never a print). */
#include <stdlib.h>
#include <linux/compiler.h>

extern unsigned long kshim_warn_hits;

#define BUG_ON(c) do { if (unlikely(c)) abort(); } while (0)
#define BUG() abort()

#define WARN_ON(c) ({ int __c = !!(c); if (__c) kshim_warn_hits++; unlikely(__c); })

#define WARN_ON_ONCE(c) ({						\
	static int __seen;						\
	int __cond = !!(c);						\
	if (__cond && !__seen) {					\
		__seen = 1;						\
		kshim_warn_hits++;					\
	}								\
	__cond;								\
})

#define WARN_ONCE(c, ...) WARN_ON_ONCE(c)
#define BUILD_BUG_ON(e) ((void)sizeof(char[1 - 2 * !!(e)]))
