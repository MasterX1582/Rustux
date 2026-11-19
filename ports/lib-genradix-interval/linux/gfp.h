#pragma once
/* gfp.h shim: allocation flags are inert in the userspace harness;
 * values match upstream so folded masks (if any) agree. */
#include <linux/types.h>
#include <linux/errno.h> /* upstream gfp.h pulls in errno via mmzone/debug chains */

#define ___GFP_DMA		0x01u
#define ___GFP_HIGHMEM		0x02u
#define ___GFP_MEMALLOC		0x04u
#define ___GFP_KSWAPD_RECLAIM	0x40u
#define ___GFP_ATOMIC		0x200u
#define ___GFP_ACCOUNT		0x400u
#define __GFP_ZERO		0x8000u

#define GFP_ATOMIC	(___GFP_KSWAPD_RECLAIM | ___GFP_ATOMIC)
#define GFP_KERNEL	(___GFP_KSWAPD_RECLAIM | ___GFP_MEMALLOC)
#define GFP_NOWAIT	0u
