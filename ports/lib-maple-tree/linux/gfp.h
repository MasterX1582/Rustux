#pragma once
/* linux/gfp.h shim: real bit numbers from include/linux/gfp_types.h
 * (enum order with !LOCKDEP/!KASAN_HW_TAGS → ___GFP_LAST_BIT == 24).
 * XA_FLAGS_MARK() parks the per-array mark bits at __GFP_BITS_SHIFT,
 * so the constant must be the kernel's for xa_flags dumps to agree. */
#include <linux/types.h>
#include <linux/minmax.h>

#define ___GFP_DMA_BIT		0
#define ___GFP_HIGH_BIT		5
#define ___GFP_IO_BIT		6
#define ___GFP_FS_BIT		7
#define ___GFP_DIRECT_RECLAIM_BIT 10
#define ___GFP_KSWAPD_RECLAIM_BIT 11
#define ___GFP_NOWARN_BIT	13
#define ___GFP_NOFAIL_BIT	15
#define ___GFP_HARDWALL_BIT	20
#define ___GFP_ACCOUNT_BIT	22
#define ___GFP_LAST_BIT		24

#define __GFP_DMA		((__force gfp_t)(1U << ___GFP_DMA_BIT))
#define __GFP_HIGH		((__force gfp_t)(1U << ___GFP_HIGH_BIT))
#define __GFP_IO		((__force gfp_t)(1U << ___GFP_IO_BIT))
#define __GFP_FS		((__force gfp_t)(1U << ___GFP_FS_BIT))
#define __GFP_DIRECT_RECLAIM	((__force gfp_t)(1U << ___GFP_DIRECT_RECLAIM_BIT))
#define __GFP_KSWAPD_RECLAIM	((__force gfp_t)(1U << ___GFP_KSWAPD_RECLAIM_BIT))
#define __GFP_NOWARN		((__force gfp_t)(1U << ___GFP_NOWARN_BIT))
#define __GFP_NOFAIL		((__force gfp_t)(1U << ___GFP_NOFAIL_BIT))
#define __GFP_ZERO		((__force gfp_t)(1U << 8))
#define __GFP_HARDWALL		((__force gfp_t)(1U << ___GFP_HARDWALL_BIT))
#define __GFP_ACCOUNT		((__force gfp_t)(1U << ___GFP_ACCOUNT_BIT))
#define __GFP_RECLAIM		((__force gfp_t)(__GFP_DIRECT_RECLAIM | __GFP_KSWAPD_RECLAIM))

#define __GFP_BITS_SHIFT	___GFP_LAST_BIT
#define __GFP_BITS_MASK		((__force gfp_t)((1U << __GFP_BITS_SHIFT) - 1))

#define GFP_ATOMIC	(__GFP_HIGH | __GFP_KSWAPD_RECLAIM)
#define GFP_KERNEL	(__GFP_RECLAIM | __GFP_IO | __GFP_FS)
#define GFP_NOWAIT	(__GFP_KSWAPD_RECLAIM | __GFP_NOWARN)
#define GFP_NOIO	(__GFP_RECLAIM)

static inline bool gfpflags_allow_blocking(const gfp_t gfp_flags)
{
	return !!(gfp_flags & __GFP_DIRECT_RECLAIM);
}
