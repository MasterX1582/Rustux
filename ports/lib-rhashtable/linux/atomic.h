#pragma once
/* Plain-counter atomics for the sequential harness. */
typedef struct {
	int counter;
} atomic_t;

#define ATOMIC_INIT(i)	{ (i) }

#define atomic_read(v)		((v)->counter)
#define atomic_set(v, i)	((v)->counter = (i))
#define atomic_inc(v)		((v)->counter++)
#define atomic_dec(v)		((v)->counter--)
#define atomic_inc_return(v)	(++(v)->counter)
#define atomic_dec_return(v)	(--(v)->counter)
#define atomic_add(i, v)	((v)->counter += (i))
#define atomic_sub(i, v)	((v)->counter -= (i))
#define atomic_cmpxchg(v, o, n)	__sync_val_compare_and_swap(&(v)->counter, (o), (n))
