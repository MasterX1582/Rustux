#pragma once
/* lockdep no-ops: the sequential harness holds no real locks, and every
 * _protected/_check dereference sites in xarray.h just wants "held". */
#include <linux/types.h>

#define lockdep_is_held(lock) 1
#define lockdep_assert_held(lock) do { } while (0)
