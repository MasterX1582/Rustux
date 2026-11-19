#pragma once
/* linux/sched/mm.h shim: might_alloc() is the might_sleep marker; a
 * no-op in the sequential harness. */
#include <linux/types.h>

static inline void might_alloc(gfp_t gfp_mask)
{
	(void)gfp_mask;
}

#define might_sleep_if(c) do { } while (0)
