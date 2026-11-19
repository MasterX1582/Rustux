#pragma once
/* linux/local_lock.h shim: local locks serialise the per-cpu preload
 * list; the sequential harness is the only "CPU", so they are no-ops.
 * INIT_LOCAL_LOCK() must be a valid struct-member initialiser. */
#include <linux/types.h>

typedef int local_lock_t;

#define INIT_LOCAL_LOCK(name)	0
#define local_lock(l)		do { (void)(l); } while (0)
#define local_unlock(l)		do { (void)(l); } while (0)
#define local_lock_irq(l)	do { (void)(l); } while (0)
#define local_unlock_irq(l)	do { (void)(l); } while (0)
