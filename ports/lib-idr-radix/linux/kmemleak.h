#pragma once
/* linux/kmemleak.h shim: kmemleak_update_trace() annotates debug
 * metadata upstream; a no-op here. */
#include <linux/types.h>

static inline void kmemleak_update_trace(const void *ptr)
{
	(void)ptr;
}
