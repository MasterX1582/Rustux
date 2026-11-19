#pragma once
/* RCU no-ops for the single-threaded harness: every RCU primitive is
 * the plain assignment/dereference it compiles to, call_rcu() runs the
 * callback immediately, and kfree_rcu() frees at once.  Freed maple
 * nodes are only touched again through the pool allocator (upstream's
 * RCU grace-period guarantee degenerates to that here). */
#include <linux/types.h>

struct rcu_head {
	void *next;
	void (*func)(struct rcu_head *head);
};

struct lockdep_map;
extern struct lockdep_map rcu_callback_map;

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

void kfree(const void *objp);
#define kfree_rcu(ptr, rhf)		kfree(ptr)

static inline void call_rcu(struct rcu_head *head,
			    void (*func)(struct rcu_head *head))
{
	func(head);
}
