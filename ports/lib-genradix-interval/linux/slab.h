#pragma once
/* slab.h shim for the lib-genradix-interval harness.
 * kzalloc()/kfree() map to calloc()/free(); genradix_alloc_budget is a
 * harness hook that makes allocation deterministically fail after the
 * given number of calls (negative = never fail), so the recorded C
 * behavior covers __genradix_ptr_alloc()'s ENOMEM paths too. */
#include <stdlib.h>
#include <linux/types.h>

extern int genradix_alloc_budget;

static inline void *kzalloc(size_t size, gfp_t gfp)
{
	(void)gfp;
	if (genradix_alloc_budget == 0)
		return NULL;
	if (genradix_alloc_budget > 0)
		genradix_alloc_budget--;
	return calloc(1, size);
}

static inline void kfree(const void *p)
{
	free((void *)p);
}
