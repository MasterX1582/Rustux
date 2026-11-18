#pragma once
#include <linux/types.h>
#include <linux/compiler.h>

/* Bit rotation — same wrap semantics as the kernel rol/ror helpers. */
static __always_inline u32 rol32(u32 word, unsigned int shift)
{
	return (word << (shift & 31)) | (word >> ((-shift) & 31));
}

static __always_inline u32 ror32(u32 word, unsigned int shift)
{
	return (word >> (shift & 31)) | (word << ((-shift) & 31));
}

static __always_inline u64 rol64(u64 word, unsigned int shift)
{
	return (word << (shift & 63)) | (word >> ((-shift) & 63));
}

static __always_inline u64 ror64(u64 word, unsigned int shift)
{
	return (word >> (shift & 63)) | (word << ((-shift) & 63));
}
