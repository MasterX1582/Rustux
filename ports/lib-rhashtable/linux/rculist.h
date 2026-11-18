#pragma once
/* Single-threaded RCU + list shim.  rcu_dereference* are plain reads;
 * call_rcu runs the callback immediately (the harness never reuses
 * freed memory: the allocator is a bump arena, so "freed" objects stay
 * readable, matching the deferred-free semantics the algorithm relies
 * on) and records that it ran so rcu_head_after_call_rcu() works. */
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/container_of.h>

#define rcu_assign_pointer(p, v)	((p) = (v))
#define rcu_dereference_raw(p)		(p)
#define rcu_dereference(p)		(p)
#define rcu_dereference_protected(p, c)	(p)
#define rcu_dereference_check(p, c)	(p)
#define rcu_dereference_all(p)		(p)
#define rcu_dereference_all_check(p, c)	(p)
#define rcu_access_pointer(p)		(p)
#define RCU_INIT_POINTER(p, v)		((p) = (v))

#define rcu_read_lock()			do { } while (0)
#define rcu_read_unlock()		do { } while (0)
#define rcu_read_lock_held()		1
#define synchronize_rcu()		do { } while (0)

struct rcu_head {
	void (*func)(struct rcu_head *head);
	unsigned long called;
};

#define rcu_head_init(h)	do { (h)->func = 0; (h)->called = 0; } while (0)

#define call_rcu(h, f)							\
	do {								\
		(h)->func = (f);					\
		(h)->called = 1;					\
		(f)(h);							\
	} while (0)

#define rcu_head_after_call_rcu(h, f)	((h)->called && (h)->func == (f))

/* ---- list_head, the subset rhashtable uses ---- */

struct list_head {
	struct list_head *next, *prev;
};

#define LIST_HEAD_INIT(name)	{ &(name), &(name) }
#define LIST_HEAD(name)		struct list_head name = LIST_HEAD_INIT(name)

#define INIT_LIST_HEAD(list)						\
	do {								\
		(list)->next = (list);					\
		(list)->prev = (list);					\
	} while (0)

static inline void __list_add(struct list_head *new,
			      struct list_head *prev,
			      struct list_head *next)
{
	next->prev = new;
	new->next = next;
	new->prev = prev;
	prev->next = new;
}

static inline void list_add(struct list_head *new, struct list_head *head)
{
	__list_add(new, head, head->next);
}

static inline void list_add_tail(struct list_head *new,
				 struct list_head *head)
{
	__list_add(new, head->prev, head);
}

static inline void __list_del(struct list_head *prev, struct list_head *next)
{
	next->prev = prev;
	prev->next = next;
}

static inline void list_del(struct list_head *entry)
{
	__list_del(entry->prev, entry->next);
	entry->next = entry;
	entry->prev = entry;
}

static inline int list_empty(const struct list_head *head)
{
	return head->next == head;
}

#define list_entry(ptr, type, member)	container_of(ptr, type, member)

#define list_for_each_entry(pos, head, member)				\
	for (pos = list_entry((head)->next, typeof(*pos), member);	\
	     &pos->member != (head);					\
	     pos = list_entry(pos->member.next, typeof(*pos), member))

#define list_for_each_entry_safe(pos, n, head, member)			\
	for (pos = list_entry((head)->next, typeof(*pos), member),	\
	     n = list_entry(pos->member.next, typeof(*pos), member);	\
	     &pos->member != (head);					\
	     pos = n, n = list_entry(n->member.next, typeof(*n), member))
