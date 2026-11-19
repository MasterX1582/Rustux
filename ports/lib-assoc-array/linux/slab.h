#pragma once
/* linux/slab.h shim for the lib-assoc-array harness.
 *
 * The unit boundary is the kmalloc family: assoc_array.c allocates
 * nodes with kzalloc_obj(), shortcuts with kzalloc_flex()/kmalloc_flex()
 * (a trailing index_key[] flexible array) and edit scripts with
 * kzalloc_obj().  They all funnel to kmalloc()/kzalloc(), which
 * wrappers.c implements over deterministic index pools with a seeded
 * alloc-failure mask.  kfree() routes by pool address range.
 *
 * The *_obj/*_flex macros take the same arguments the modern typed
 * upstream macros do (a type or *expr for the object, the flexible
 * member name and element count for flex), minus the optional gfp
 * argument which the harness fixes at GFP_KERNEL. */
#include <linux/types.h>
#include <linux/kernel.h>

typedef unsigned int gfp_t;
#define GFP_KERNEL	0
#define GFP_ATOMIC	1
#define GFP_NOWAIT	2

void *kmalloc(size_t size, gfp_t gfp);
void *kzalloc(size_t size, gfp_t gfp);
void kfree(const void *ptr);

#define kmalloc_obj(P, ...)	kmalloc(sizeof(typeof(P)), GFP_KERNEL)
#define kzalloc_obj(P, ...)	kzalloc(sizeof(typeof(P)), GFP_KERNEL)
#define kmalloc_flex(P, FAM, COUNT, ...) \
	kmalloc(struct_size_t(typeof(P), FAM, COUNT), GFP_KERNEL)
#define kzalloc_flex(P, FAM, COUNT, ...) \
	kzalloc(struct_size_t(typeof(P), FAM, COUNT), GFP_KERNEL)
