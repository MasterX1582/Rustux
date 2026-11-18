/* Userspace shim for the RCU API surface prime_numbers.c uses. The harness
 * is single-threaded, so read sections are free and dereference/assign are
 * plain loads/stores. */
#pragma once

/* struct rcu_head comes from <linux/types.h> (as upstream). */
#include <linux/types.h>

#define RCU_INITIALIZER(v) (v)

static inline void rcu_read_lock(void) {}
static inline void rcu_read_unlock(void) {}

#define rcu_dereference(p) (p)
#define rcu_assign_pointer(p, v) ((p) = (v))
#define rcu_dereference_protected(p, c) (p)
