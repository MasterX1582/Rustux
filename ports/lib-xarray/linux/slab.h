#pragma once
/* linux/slab.h shim for the lib-xarray harness.
 *
 * The unit boundary is lib/radix-tree.c's node cache: xarray.c only
 * ever calls kmem_cache_alloc_lru(radix_tree_node_cachep, lru, gfp).
 * The harness implements that cache as a deterministic index pool of
 * real struct xa_node (see wrappers.c): allocation pops the lowest
 * free slot or returns NULL when the pool is exhausted or when the
 * seeded fail-mask nominates this alloc call — the -ENOMEM paths in
 * xas_alloc/xas_nomem/xas_split_alloc/xas_try_split are real coverage.
 */
#include <linux/types.h>
#include <linux/gfp.h>

struct list_lru;
struct xa_node;

struct kmem_cache {
	unsigned int obj_size;
};

void *kmem_cache_alloc_lru(struct kmem_cache *cachep, struct list_lru *lru,
			   gfp_t gfp);
void kmem_cache_free(struct kmem_cache *cachep, void *obj);
