#pragma once
#include <linux/types.h>
#include <linux/compiler.h>
#include <string.h>

/* Unaligned and little-endian accessors. The reference is little-endian
 * (x86-64); these are written byte-wise so they are correct regardless. */

#define __get_unaligned_t(type, ptr) ({                     \
	const void *__gu_p = (ptr);                             \
	type __val;                                             \
	memcpy(&__val, __gu_p, sizeof(type));                   \
	__val;                                                  \
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

static __always_inline __le16 cpu_to_le16(u16 v) { return (__le16)v; }
static __always_inline __le32 cpu_to_le32(u32 v) { return (__le32)v; }
static __always_inline __le64 cpu_to_le64(u64 v) { return (__le64)v; }
static __always_inline u16 le16_to_cpu(__le16 v) { return (u16)v; }
static __always_inline u32 le32_to_cpu(__le32 v) { return (u32)v; }
static __always_inline u64 le64_to_cpu(__le64 v) { return (u64)v; }

static __always_inline u16 le16_to_cpup(const __le16 *p)
{
	return get_unaligned_le16(p);
}

static __always_inline u32 le32_to_cpup(const __le32 *p)
{
	return get_unaligned_le32(p);
}

static __always_inline u64 le64_to_cpup(const __le64 *p)
{
	return get_unaligned_le64(p);
}
