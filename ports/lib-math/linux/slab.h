/* Userspace shim for <linux/slab.h>: kmalloc becomes malloc. GFP flags are
 * accepted and ignored; allocation failure still propagates as NULL so the
 * caller's fallback paths stay exercised. */
#pragma once

#include <stdlib.h>

#define GFP_KERNEL 0
#define __GFP_NOWARN 0

static inline void *kmalloc(size_t size, int flags)
{
	(void)flags;
	return malloc(size);
}

static inline void kfree(const void *p) { free((void *)p); }

/* Single-threaded: the RCU grace period is instantaneous. */
#define kfree_rcu(ptr, member) kfree(ptr)
