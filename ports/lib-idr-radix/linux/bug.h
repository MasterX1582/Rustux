#pragma once
/* linux/bug.h shim for the lib-idr-radix harness.
 * BUG_ON()/BUG() abort loudly: the contract-level callers never pass
 * arguments that trip them; a trip means the harness itself is wrong,
 * so dying is correct.
 *
 * WARN_ON()/WARN_ON_ONCE()/WARN() all return the condition (like
 * upstream) and bump kshim_warn_hits, which wrappers.c folds so a
 * C-side WARN is observable in the checksum.  Unlike upstream, the
 * _ONCE variants count EVERY hit: upstream's once-per-callsite
 * `static bool __warned` would persist across wrapper invocations and
 * make case N's checksum depend on which earlier cases ran in the
 * same process — nondeterminism across record/diff sessions.  The
 * Rust port mirrors the count-every-time rule exactly. */
#include <stdlib.h>
#include <linux/compiler.h>

extern unsigned long kshim_warn_hits;

#define BUG_ON(c) do { if (unlikely(c)) abort(); } while (0)
#define BUG() abort()

#define WARN_ON(c) ({ int __c = !!(c); if (__c) kshim_warn_hits++; unlikely(__c); })
#define WARN_ON_ONCE(c) WARN_ON(c)
#define WARN(c, ...) WARN_ON(c)
#define WARN_ONCE(c, ...) WARN_ON(c)
#define BUILD_BUG_ON(e) ((void)sizeof(char[1 - 2 * !!(e)]))
