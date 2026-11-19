#pragma once
/* spinlock no-ops: the sequential harness is the only thread.  The
 * irqsave "flags" variable is still written (upstream macro semantics:
 * local_irq_save stores the flag word) so xas_lock_irqsave users see a
 * defined value. */
#include <linux/types.h>

typedef int spinlock_t;

#define __SPIN_LOCK_UNLOCKED(name) 0
#define spin_lock_init(l) do { } while (0)
#define spin_lock(l) do { } while (0)
#define spin_unlock(l) do { } while (0)
#define spin_trylock(l) 1
#define spin_lock_bh(l) do { } while (0)
#define spin_unlock_bh(l) do { } while (0)
#define spin_lock_irq(l) do { } while (0)
#define spin_unlock_irq(l) do { } while (0)
#define spin_lock_irqsave(l, f) ((f) = 0)
#define spin_unlock_irqrestore(l, f) do { } while (0)
#define spin_lock_nested(l, s) do { } while (0)
#define spin_lock_bh_nested(l, s) do { } while (0)
#define spin_lock_irq_nested(l, s) do { } while (0)
#define spin_lock_irqsave_nested(l, f, s) ((f) = 0)
