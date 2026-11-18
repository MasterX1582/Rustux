#pragma once
/* gfp flags: only the two bits this unit observes are modelled —
 * allow_blocking decides the nested-table fallback in
 * bucket_table_alloc(), __GFP_NOFAIL suppresses injected failures,
 * __GFP_ZERO zeroes the allocation. */
#include <linux/types.h>
#include <linux/compiler.h>

#define __GFP_BLOCKING		((gfp_t)0x0001u)
#define __GFP_ZERO		((gfp_t)0x0100u)
#define __GFP_NOWARN		((gfp_t)0x0200u)
#define __GFP_NOFAIL		((gfp_t)0x0400u)
#define __GFP_ATOMIC		((gfp_t)0x0000u)

#define GFP_ATOMIC		(__GFP_ATOMIC)
#define GFP_KERNEL		(__GFP_BLOCKING)

static __always_inline bool gfpflags_allow_blocking(const gfp_t gfp_flags)
{
	return !!(gfp_flags & __GFP_BLOCKING);
}
