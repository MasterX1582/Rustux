#pragma once
/* include/linux/swab.h subset. On little-endian hosts the *_le paths in
 * find_bit.c are not compiled and find.h maps the _le finders to the
 * plain ones, so swab() is only exercised indirectly — but it is defined
 * faithfully: byte-reverse the value. */
#include <linux/types.h>

static __always_inline __attribute__((__const__)) u16 __swab16(u16 x)
{
	return __builtin_bswap16(x);
}
static __always_inline __attribute__((__const__)) u32 __swab32(u32 x)
{
	return __builtin_bswap32(x);
}
static __always_inline __attribute__((__const__)) u64 __swab64(u64 x)
{
	return __builtin_bswap64(x);
}

static __always_inline __attribute__((__const__)) unsigned long
__swab(unsigned long x)
{
	return BITS_PER_LONG == 64 ? (unsigned long)__swab64(x)
				   : (unsigned long)__swab32(x);
}

#define swab16 __swab16
#define swab32 __swab32
#define swab64 __swab64
#define swab   __swab
