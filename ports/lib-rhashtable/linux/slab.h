#pragma once
/* Allocator shim: all kernel allocation entry points funnel into
 * kshim_alloc()/kshim_free(), implemented by the harness over a bump
 * arena with a seeded fail counter. */
#include <linux/types.h>
#include <linux/gfp.h>
#include <linux/compiler.h>

void *kshim_alloc(size_t size, gfp_t gfp);
void kshim_free(const void *p);

#define kmalloc_noprof(size, gfp)		kshim_alloc((size), (gfp))
#define kmalloc(size, gfp)		kshim_alloc((size), (gfp))
#define kzalloc(size, gfp)		kshim_alloc((size), (gfp) | __GFP_ZERO)
#define kfree(p)			kshim_free(p)

#define kvmalloc_node_align_noprof(size, align, flags, node)	\
		kshim_alloc((size), (flags))
#define kvmalloc_noprof(size, flags)	kshim_alloc((size), (flags))
#define kvmalloc(size, flags)		kshim_alloc((size), (flags))
#define kvfree(p)			kshim_free(p)
#define kvfree_atomic(p)		kshim_free(p)

#define kvmalloc_objs(P, COUNT, ...)	kshim_alloc(sizeof(P) * (COUNT), \
						    __VA_ARGS__ + 0)
#define kvcalloc(n, size, gfp)		kshim_alloc((n) * (size), \
						    (gfp) | __GFP_ZERO)

#define struct_size(p, member, n)	(sizeof(*(p)) + sizeof(*(p)->member) * (n))

/* alloc_tag plumbing is a transparent wrapper without
 * CONFIG_MEM_ALLOC_PROFILING. */
#define alloc_hooks(what)		(what)
#define alloc_hooks_tag(tag, what)	(what)
#define alloc_tag_record(tag)		do { } while (0)
