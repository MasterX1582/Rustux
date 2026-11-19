#pragma once
/* linux/atomic.h shim: the sequential model makes every atomic op a
 * plain load/store/compare-and-swap on the backing word (same
 * convention as the other ports). _relaxed/_acquire/_release variants
 * alias the plain ops; the smp_* barriers they pair with are no-ops
 * (linux/smp.h). */
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/smp.h>

typedef struct {
	long counter;
} atomic_long_t;

typedef struct {
	int counter;
} atomic_t;

#define ATOMIC_INIT(i) { (i) }
#define ATOMIC_LONG_INIT(i) { (i) }

#define atomic_long_read(v) READ_ONCE((v)->counter)
#define atomic_long_read_acquire(v) READ_ONCE((v)->counter)
#define atomic_long_set(v, i) WRITE_ONCE((v)->counter, (long)(i))
#define atomic_long_set_release(v, i) WRITE_ONCE((v)->counter, (long)(i))
#define atomic_long_inc(v) ((void)(v)->counter++)
#define atomic_long_dec(v) ((void)(v)->counter--)

static inline long atomic_long_cmpxchg(atomic_long_t *v, long old,
				       long new)
{
	long prev = v->counter;
	if (prev == old)
		v->counter = new;
	return prev;
}

#define atomic_long_cmpxchg_relaxed(v, o, n) atomic_long_cmpxchg(v, o, n)
#define atomic_long_cmpxchg_acquire(v, o, n) atomic_long_cmpxchg(v, o, n)
#define atomic_long_cmpxchg_release(v, o, n) atomic_long_cmpxchg(v, o, n)

/* try_cmpxchg semantics: on success *oldp's target was swapped for new
 * and the return is true; on failure *oldp is overwritten with the
 * current value and the return is false. */
static inline bool atomic_long_try_cmpxchg(atomic_long_t *v, long *oldp,
					   long new)
{
	long prev = v->counter;
	if (prev == *oldp) {
		v->counter = new;
		return true;
	}
	*oldp = prev;
	return false;
}

#define atomic_long_try_cmpxchg_relaxed(v, o, n) \
	atomic_long_try_cmpxchg(v, o, n)
#define atomic_long_try_cmpxchg_acquire(v, o, n) \
	atomic_long_try_cmpxchg(v, o, n)
#define atomic_long_try_cmpxchg_release(v, o, n) \
	atomic_long_try_cmpxchg(v, o, n)
