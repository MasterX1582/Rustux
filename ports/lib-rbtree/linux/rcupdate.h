#pragma once
/* RCU no-ops: in the single-threaded userspace harness every RCU
 * primitive is the plain assignment/dereference it compiles to on
 * !CONFIG_RCU builds. */

#define rcu_assign_pointer(p, v)	((p) = (v))
#define rcu_dereference_raw(p)		(p)
#define rcu_dereference(p)		(p)
#define RCU_INIT_POINTER(p, v)		((p) = (v))

#define rcu_read_lock()			do { } while (0)
#define rcu_read_unlock()		do { } while (0)
#define rcu_read_lock_held()		1
#define synchronize_rcu()		do { } while (0)
