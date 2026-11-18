#pragma once
/* Spinlock no-ops; also carries the bucket_locks.c public prototypes
 * (they live in spinlock.h upstream). */
#include <linux/types.h>
#include <linux/lockdep.h>

struct spinlock {
	int _unused;
};
typedef struct spinlock spinlock_t;

#define spin_lock_init(l)	do { } while (0)
#define spin_lock(l)		do { } while (0)
#define spin_unlock(l)		do { } while (0)
#define spin_lock_bh(l)		do { } while (0)
#define spin_unlock_bh(l)	do { } while (0)

int __alloc_bucket_spinlocks(spinlock_t **locks, unsigned int *lock_mask,
			     size_t max_size, unsigned int cpu_mult,
			     gfp_t gfp, const char *name,
			     struct lock_class_key *key);

#define alloc_bucket_spinlocks(locks, lock_mask, max_size, cpu_mult, gfp)    \
	({								     \
		static struct lock_class_key key;			     \
		int ret;						     \
									     \
		ret = __alloc_bucket_spinlocks(locks, lock_mask, max_size,   \
					       cpu_mult, gfp, #locks, &key); \
		ret;							     \
	})

void free_bucket_spinlocks(spinlock_t *locks);
