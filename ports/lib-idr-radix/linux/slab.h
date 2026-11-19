#pragma once
/* linux/slab.h shim for the lib-idr-radix harness.
 *
 * The unit boundary is lib/radix-tree.c's node cache: xarray.c calls
 * kmem_cache_alloc_lru() and radix-tree.c calls kmem_cache_alloc().
 * The harness implements both over one deterministic index pool of
 * real struct xa_node (see wrappers.c): allocation bumps a shared
 * alloc ordinal, returns NULL when the seeded fail-mask nominates the
 * call, then pops the lowest free pool slot.  That drives the real
 * -ENOMEM / preload / xas_nomem paths.
 *
 * idr.c additionally allocates struct ida_bitmap leaves through
 * kzalloc_obj() -> kzalloc(); wrappers.c backs those with a second
 * deterministic pool over the same alloc-ordinal/fail-mask stream.
 */
#include <linux/types.h>
#include <linux/gfp.h>

struct list_lru;
struct xa_node;

struct kmem_cache {
	unsigned int obj_size;
	void (*ctor)(void *);
};

#define SLAB_PANIC		0x00040000UL
#define SLAB_RECLAIM_ACCOUNT	0x00020000UL
#define SLAB_HWCACHE_ALIGN	0x00002000UL

struct kmem_cache *kmem_cache_create(const char *name, unsigned int size,
		unsigned int align, unsigned long flags,
		void (*ctor)(void *));
void kmem_cache_destroy(struct kmem_cache *cachep);

void *kmem_cache_alloc_lru(struct kmem_cache *cachep, struct list_lru *lru,
			   gfp_t gfp);
void *kmem_cache_alloc(struct kmem_cache *cachep, gfp_t gfp);
void kmem_cache_free(struct kmem_cache *cachep, void *obj);

void *kzalloc(size_t size, gfp_t gfp);
void *kmalloc(size_t size, gfp_t gfp);
void kfree(const void *ptr);

#define kzalloc_obj(P, gfp)	kzalloc(sizeof(P), gfp)
#define kmalloc_obj(P, gfp)	kmalloc(sizeof(P), gfp)
