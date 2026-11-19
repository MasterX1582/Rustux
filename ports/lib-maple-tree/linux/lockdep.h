#pragma once
/* lockdep no-ops: the sequential harness holds no real locks, and every
 * _protected/_check dereference site just wants "held". */
#include <linux/types.h>

struct lockdep_map {
	int unused;
};

#define lockdep_is_held(lock) 1
#define lockdep_assert_held(lock) do { } while (0)
#define lock_is_held(map) 1
