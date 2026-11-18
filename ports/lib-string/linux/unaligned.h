#pragma once
#include <linux/types.h>
#include <linux/compiler.h>
#include <string.h>

/* Unaligned accessors, written byte-wise so they are correct regardless
 * of the host's alignment rules.  The reference is little-endian
 * (x86-64); get_unaligned() uses native ordering via memcpy. */

#define __get_unaligned_t(type, ptr) ({				\
	const void *__gu_p = (ptr);				\
	type __val;						\
	memcpy(&__val, __gu_p, sizeof(type));			\
	__val;							\
})

#define get_unaligned(ptr) __get_unaligned_t(__typeof__(*(ptr)), (ptr))

static __always_inline u16 get_unaligned_le16(const void *p)
{
	const u8 *b = p;
	return (u16)b[0] | ((u16)b[1] << 8);
}

static __always_inline u32 get_unaligned_le32(const void *p)
{
	const u8 *b = p;
	return (u32)b[0] | ((u32)b[1] << 8) |
	       ((u32)b[2] << 16) | ((u32)b[3] << 24);
}

static __always_inline u64 get_unaligned_le64(const void *p)
{
	const u8 *b = p;
	return (u64)b[0] | ((u64)b[1] << 8) | ((u64)b[2] << 16) |
	       ((u64)b[3] << 24) | ((u64)b[4] << 32) | ((u64)b[5] << 40) |
	       ((u64)b[6] << 48) | ((u64)b[7] << 56);
}
