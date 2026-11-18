#pragma once
/* get_unaligned for jhash: byte-wise load so the result is the C
 * little-endian interpretation regardless of host alignment. */
#include <linux/types.h>
#include <string.h>

static inline u32 __get_unaligned_u32(const void *p)
{
	u32 v;
	memcpy(&v, p, sizeof(v));
	return v;
}

#define get_unaligned(ptr)	__get_unaligned_u32(ptr)
