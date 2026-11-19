#pragma once
/* linux/mutex.h stub: percpu_counter.c includes it but this unit uses
 * no mutexes. */
#include <linux/types.h>

struct mutex {
	int m;
};

#define mutex_init(m) do { } while (0)
#define mutex_lock(m) do { } while (0)
#define mutex_unlock(m) do { } while (0)
