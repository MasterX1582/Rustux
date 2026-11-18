#pragma once
/* linux/slab.h shim: kmalloc family over malloc. gfp_t is in types.h.
 * Pulls kernel.h the way upstream slab.h drags in the core helpers —
 * kfifo.c gets min()/min3()/DIV_ROUND_UP() transitively through it. */
#include <linux/types.h>
#include <linux/kernel.h>
#include <stdlib.h>

#define GFP_KERNEL ((gfp_t)0)
#define __GFP_ZERO ((gfp_t)0x8000u)

static inline void *kmalloc_array(size_t n, size_t size, gfp_t flags)
{
	if (size && n > (size_t)-1 / size)
		return NULL;
	if (flags & __GFP_ZERO)
		return calloc(n, size);
	return malloc(n * size);
}

static inline void *kmalloc(size_t size, gfp_t flags)
{
	return kmalloc_array(1, size, flags);
}

static inline void *kmalloc_array_node(size_t n, size_t size, gfp_t flags,
				       int node)
{
	(void)node;
	return kmalloc_array(n, size, flags);
}

static inline void kfree(const void *p)
{
	free((void *)p);
}
