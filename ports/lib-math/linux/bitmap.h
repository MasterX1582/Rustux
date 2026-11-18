/* Userspace shim for <linux/bitmap.h>: faithful single-threaded bitmap ops
 * over unsigned long words, LP64. Bit n lives in word n/BITS_PER_LONG at
 * offset n%BITS_PER_LONG, matching the kernel's layout. */
#pragma once

#include <linux/types.h>
#include <linux/bitops.h>
#include <string.h>

static inline void bitmap_fill(unsigned long *dst, unsigned int nbits)
{
	unsigned int nwords = (nbits + BITS_PER_LONG - 1) / BITS_PER_LONG;

	memset(dst, 0xff, nwords * sizeof(unsigned long));
}

static inline void bitmap_copy(unsigned long *dst, const unsigned long *src,
			       unsigned int nbits)
{
	unsigned int nwords = (nbits + BITS_PER_LONG - 1) / BITS_PER_LONG;

	memcpy(dst, src, nwords * sizeof(unsigned long));
}

static inline unsigned long bitmap_size(unsigned long nbits)
{
	return ((nbits + BITS_PER_LONG - 1) / BITS_PER_LONG) * sizeof(unsigned long);
}

static inline void __clear_bit(unsigned long nr, unsigned long *addr)
{
	addr[nr / BITS_PER_LONG] &= ~(1UL << (nr % BITS_PER_LONG));
}

static inline int test_bit(unsigned long nr, const unsigned long *addr)
{
	return !!(addr[nr / BITS_PER_LONG] & (1UL << (nr % BITS_PER_LONG)));
}

/* find_next_bit: lowest set bit at or after `offset`, below `size`;
 * returns `size` when none is set. */
static inline unsigned long find_next_bit(const unsigned long *addr,
					  unsigned long size,
					  unsigned long offset)
{
	unsigned long i;

	for (i = offset; i < size; i++) {
		if (test_bit(i, addr))
			return i;
	}
	return size;
}
