#pragma once
/* linux/bug.h shim for the lib-assoc-array harness.
 * BUG_ON()/BUG() abort loudly: the upstream contract never passes
 * arguments that trip them; a trip means the harness is broken, so
 * dying is correct.  assoc_array.c has many internal BUG_ON invariant
 * checks (slot bounds, pointer types, leaf counts); under the harness
 * they must never fire. */
#include <stdlib.h>
#include <linux/compiler.h>

#define BUG_ON(c)	do { if (unlikely(c)) abort(); } while (0)
#define BUG()		abort()
#define BUILD_BUG_ON(e)	((void)sizeof(char[1 - 2 * !!(e)]))
