#pragma once
#include <linux/types.h>
#include <linux/compiler.h>
#include <string.h>

/* Unaligned and endian accessors, written byte-wise so they are correct
 * regardless of host alignment rules.  The reference target is x86-64
 * (little-endian): cpu_to_be*() byteswaps, get/put_unaligned_be*()
 * marshal big-endian regardless. */

#define __get_unaligned_t(type, ptr) ({				\
	const void *__gu_p = (ptr);				\
	type __val;						\
	memcpy(&__val, __gu_p, sizeof(type));			\
	__val;							\
})

#define get_unaligned(ptr) __get_unaligned_t(__typeof__(*(ptr)), (ptr))

#define __put_unaligned_t(type, val, ptr) do {			\
	void *__pu_p = (ptr);					\
	type __val = (val);					\
	memcpy(__pu_p, &__val, sizeof(type));			\
} while (0)

#define put_unaligned(val, ptr) __put_unaligned_t(__typeof__(val), (val), (ptr))

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

static __always_inline u32 get_unaligned_be32(const void *p)
{
	const u8 *b = p;
	return ((u32)b[0] << 24) | ((u32)b[1] << 16) |
	       ((u32)b[2] << 8) | (u32)b[3];
}

static __always_inline u64 get_unaligned_be64(const void *p)
{
	const u8 *b = p;
	return ((u64)b[0] << 56) | ((u64)b[1] << 48) |
	       ((u64)b[2] << 40) | ((u64)b[3] << 32) |
	       ((u64)b[4] << 24) | ((u64)b[5] << 16) |
	       ((u64)b[6] << 8) | (u64)b[7];
}

static __always_inline void put_unaligned_be32(u32 v, void *p)
{
	u8 *b = p;
	b[0] = (u8)(v >> 24);
	b[1] = (u8)(v >> 16);
	b[2] = (u8)(v >> 8);
	b[3] = (u8)v;
}

static __always_inline void put_unaligned_be64(u64 v, void *p)
{
	u8 *b = p;
	b[0] = (u8)(v >> 56);
	b[1] = (u8)(v >> 48);
	b[2] = (u8)(v >> 40);
	b[3] = (u8)(v >> 32);
	b[4] = (u8)(v >> 24);
	b[5] = (u8)(v >> 16);
	b[6] = (u8)(v >> 8);
	b[7] = (u8)v;
}

static __always_inline __be16 cpu_to_be16(u16 v)
{
	return (__be16)__builtin_bswap16(v);
}

static __always_inline __be32 cpu_to_be32(u32 v)
{
	return (__be32)__builtin_bswap32(v);
}

static __always_inline __be64 cpu_to_be64(u64 v)
{
	return (__be64)__builtin_bswap64(v);
}

static __always_inline u16 be16_to_cpu(__be16 v)
{
	return __builtin_bswap16((u16)v);
}

static __always_inline u32 be32_to_cpu(__be32 v)
{
	return __builtin_bswap32((u32)v);
}

static __always_inline u64 be64_to_cpu(__be64 v)
{
	return __builtin_bswap64((u64)v);
}

static __always_inline __le16 cpu_to_le16(u16 v) { return (__le16)v; }
static __always_inline __le32 cpu_to_le32(u32 v) { return (__le32)v; }
static __always_inline __le64 cpu_to_le64(u64 v) { return (__le64)v; }
static __always_inline u16 le16_to_cpu(__le16 v) { return (u16)v; }
static __always_inline u32 le32_to_cpu(__le32 v) { return (u32)v; }
static __always_inline u64 le64_to_cpu(__le64 v) { return (u64)v; }
