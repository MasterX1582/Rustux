#pragma once
/* linux/rcupdate.h shim for the lib-assoc-array harness.
 * RCU is a no-op in the single-threaded harness.  call_rcu() invokes
 * the callback immediately: upstream defers assoc_array_rcu_cleanup()
 * to after a grace period, but the edit and its excised metadata are
 * never touched again after apply, so running it synchronously is
 * behaviour-identical and keeps frees inside the recorded op. */
#include <linux/types.h>

#ifndef CONFIG_ASSOCIATIVE_ARRAY
#define CONFIG_ASSOCIATIVE_ARRAY 1
#endif

struct rcu_head {
	void *next;
	void (*func)(struct rcu_head *head);
};

typedef void (*rcu_callback_t)(struct rcu_head *head);

#define rcu_assign_pointer(p, v)		((p) = (v))
#define rcu_dereference_raw(p)			(p)
#define rcu_dereference(p)			(p)
#define rcu_dereference_check(p, c)		(p)
#define rcu_dereference_protected(p, c)		(p)
#define RCU_INIT_POINTER(p, v)			((p) = (v))

#define rcu_read_lock()				do { } while (0)
#define rcu_read_unlock()			do { } while (0)
#define rcu_read_lock_held()			1
#define synchronize_rcu()			do { } while (0)

static inline void call_rcu(struct rcu_head *head, rcu_callback_t func)
{
	func(head);
}
