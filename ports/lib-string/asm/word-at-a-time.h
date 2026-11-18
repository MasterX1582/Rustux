#pragma once
#include <string.h>
#include <linux/types.h>
#include <linux/compiler.h>

/* Userspace port of arch/x86/include/asm/word-at-a-time.h.
 * The recorded reference is the x86-64 kernel build, which selects
 * DCACHE_WORD_ACCESS, 64BIT, HAVE_EFFICIENT_UNALIGNED_ACCESS and
 * ARCH_HAS_FAST_MULTIPLIER (all !KMSAN paths).  Defining them here keeps
 * lib/string.c byte-identical to upstream. */
#define CONFIG_64BIT 1
#define CONFIG_DCACHE_WORD_ACCESS 1
#define CONFIG_HAVE_EFFICIENT_UNALIGNED_ACCESS 1
#define CONFIG_ARCH_HAS_FAST_MULTIPLIER 1

_Static_assert(sizeof(unsigned long) == 8,
	       "word-at-a-time shim assumes LP64");

/* From <linux/bitops.h>: position of the least/most significant set bit. */
#define __ffs(x) __builtin_ctzl(x)
#define __fls(x) (63 - __builtin_clzl(x))

/* From <linux/wordpart.h>. */
#define REPEAT_BYTE(x) (~0ul / 0xff * (x))

struct word_at_a_time {
	const unsigned long one_bits, high_bits;
};

#define WORD_AT_A_TIME_CONSTANTS { REPEAT_BYTE(0x01), REPEAT_BYTE(0x80) }

/* Return nonzero if it has a zero */
static inline unsigned long has_zero(unsigned long a, unsigned long *bits,
				     const struct word_at_a_time *c)
{
	unsigned long mask = ((a - c->one_bits) & ~a) & c->high_bits;
	*bits = mask;
	return mask;
}

static inline unsigned long prep_zero_mask(unsigned long a,
					   unsigned long bits,
					   const struct word_at_a_time *c)
{
	return bits;
}

#ifdef CONFIG_64BIT

/* Keep the initial has_zero() value for both bitmask and size calc */
#define create_zero_mask(bits) (bits)

static inline unsigned long zero_bytemask(unsigned long bits)
{
	bits = (bits - 1) & ~bits;
	return bits >> 7;
}

#define find_zero(bits) (__ffs(bits) >> 3)

#else	/* 32-bit case, unused: the harness is LP64-only */

static inline unsigned long create_zero_mask(unsigned long bits)
{
	bits = (bits - 1) & ~bits;
	return bits >> 7;
}

#define zero_bytemask(mask) (mask)

static inline unsigned long find_zero(unsigned long mask)
{
	long a = (0x0ff0001 + mask) >> 23;
	return a & mask;
}

#endif

/* The x86 asm version loads an unaligned word and zero-pads on a
 * page-crossing fault.  Every harness buffer is fully mapped, so a
 * plain unaligned load yields the same value. */
static inline unsigned long load_unaligned_zeropad(const void *addr)
{
	unsigned long ret;

	memcpy(&ret, addr, sizeof(ret));
	return ret;
}

/* This is the last header lib/string.c includes before its
 * `#ifdef __BIG_ENDIAN` guard for ALLBUTLAST_BYTE_MASK.  glibc's
 * <endian.h> (pulled in transitively by <sys/types.h>/<stdlib.h>)
 * defines __BIG_ENDIAN as the constant 4321 on every platform, so a raw
 * `#ifdef` would silently take the big-endian branch.  The kernel only
 * defines __BIG_ENDIAN on BE targets; undef it to restore the x86-64
 * little-endian build. */
#undef __BIG_ENDIAN
