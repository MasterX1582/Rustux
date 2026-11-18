#pragma once
/* Mutex no-ops for the sequential harness. */
#include <linux/lockdep.h>
#include <linux/spinlock.h>

struct mutex {
	int _unused;
};

#define mutex_init(m)		do { } while (0)
#define mutex_init_with_key(m, key)	do { } while (0)
#define mutex_lock(m)		do { } while (0)
#define mutex_unlock(m)		do { } while (0)
#define mutex_is_locked(m)	1
