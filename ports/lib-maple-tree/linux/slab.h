#pragma once
/* linux/slab.h shim for the lib-maple-tree harness.
 *
 * The unit boundary is lib/maple_tree.c's maple_node_cache: the file
 * allocates struct maple_node objects (256B, 256B-aligned) through
 * kmem_cache_alloc(), single-node kfree()/kmem_cache_free(), bulk
 * kmem_cache_free_bulk(), and the sheaf API (prefill/refill/return/
 * alloc_from/size).  wrappers.c implements all of these over a
 * deterministic index pool with seeded fail-mask injection, so every
 * -ENOMEM path in mas_alloc_nodes/mas_pop_node/mas_nomem is real
 * coverage. */
#include <linux/types.h>
#include <linux/gfp.h>

struct slab_sheaf;

typedef unsigned int slab_flags_t;

#define _SLAB_PANIC 2
#define SLAB_PANIC ((slab_flags_t)(1U << _SLAB_PANIC))

struct kmem_cache {
	unsigned int obj_size;
	unsigned int align;
	unsigned int sheaf_capacity;
};

struct kmem_cache_args {
	unsigned int align;
	unsigned int sheaf_capacity;
};

struct kmem_cache *__kmem_cache_create_args(const char *name,
					    unsigned int size,
					    struct kmem_cache_args *args,
					    slab_flags_t flags);

#define kmem_cache_create(__name, __object_size, __args, ...)           \
	__kmem_cache_create_args(__name, __object_size, __args, __VA_ARGS__)

void *kmem_cache_alloc(struct kmem_cache *cachep, gfp_t gfp);
void kmem_cache_free(struct kmem_cache *cachep, void *obj);
void kmem_cache_free_bulk(struct kmem_cache *s, size_t size, void **p);

struct slab_sheaf *kmem_cache_prefill_sheaf(struct kmem_cache *s,
					    gfp_t gfp, unsigned int size);
int kmem_cache_refill_sheaf(struct kmem_cache *s, gfp_t gfp,
			    struct slab_sheaf **sheaf, unsigned int size);
void kmem_cache_return_sheaf(struct kmem_cache *s, gfp_t gfp,
			     struct slab_sheaf *sheaf);
void *kmem_cache_alloc_from_sheaf(struct kmem_cache *cachep, gfp_t gfp,
				  struct slab_sheaf *sheaf);
unsigned int kmem_cache_sheaf_size(struct slab_sheaf *sheaf);

void kfree(const void *objp);
