#pragma once
/* RCU no-ops for the single-threaded harness: every RCU primitive is
 * the plain assignment/dereference it compiles to, and call_rcu() runs
 * the callback immediately — xa_node_free() is only invoked once the
 * node is unlinked and never touched again before the free returns. */
#include <linux/types.h>

struct rcu_head {
	void *next;
	void (*func)(struct rcu_head *head);
};

#define rcu_assign_pointer(p, v)	((p) = (v))
#define rcu_dereference_raw(p)		(p)
#define rcu_dereference(p)		(p)
#define rcu_dereference_check(p, c)	(p)
#define rcu_dereference_protected(p, c)	(p)
#define RCU_INIT_POINTER(p, v)		((p) = (v))

#define rcu_read_lock()			do { } while (0)
#define rcu_read_unlock()		do { } while (0)
#define rcu_read_lock_held()		1
#define synchronize_rcu()		do { } while (0)

static inline void call_rcu(struct rcu_head *head,
			    void (*func)(struct rcu_head *head))
{
	func(head);
}
