#pragma once
#include <linux/types.h>

#define BIT(nr)		(1UL << (nr))

static __always_inline u32 rol32(u32 word, unsigned int shift)
{
	return (word << (shift & 31)) | (word >> ((-shift) & 31));
}

static __always_inline u32 ror32(u32 word, unsigned int shift)
{
	return (word >> (shift & 31)) | (word << ((-shift) & 31));
}
