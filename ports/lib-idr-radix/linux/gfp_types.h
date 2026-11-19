#pragma once
/* linux/gfp_types.h shim: the enum/bit definitions live in gfp.h for
 * this harness; radix-tree.c only needs GFP_ZONEMASK beyond what the
 * lib-xarray shim already provides (zone bits 0..3). */
#include <linux/gfp.h>
