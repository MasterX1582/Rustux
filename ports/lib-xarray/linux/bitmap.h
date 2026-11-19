#pragma once
/* linux/bitmap.h shim: the handful of bit/bitmap helpers xarray.c and
 * xarray.h use, on real unsigned long words (XA_MARK_LONGS == 1, but
 * written generically anyway).  Semantics identical to lib/bitmap.c /
 * asm/bitops: find_next_bit returns the smallest set bit in
 * [offset,size) or size; __ffs is the 0-based first-set index. */
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/string.h>
#include <linux/minmax.h>

#define DECLARE_BITMAP(name, n) unsigned long name[BITS_TO_LONGS(n)]

static inline unsigned long __ffs(unsigned long word)
{
	return (unsigned long)__builtin_ctzl(word);
}

static inline unsigned long ffz(unsigned long word)
{
	return (unsigned long)__builtin_ctzl(~word);
}

static inline void __set_bit(unsigned int nr, volatile unsigned long *addr)
{
	addr[nr / BITS_PER_LONG] |= 1UL << (nr % BITS_PER_LONG);
}

static inline void __clear_bit(unsigned int nr, volatile unsigned long *addr)
{
	addr[nr / BITS_PER_LONG] &= ~(1UL << (nr % BITS_PER_LONG));
}

static inline bool test_bit(unsigned int nr, const volatile unsigned long *addr)
{
	return (addr[nr / BITS_PER_LONG] >> (nr % BITS_PER_LONG)) & 1UL;
}

/* returns the previous value of the bit */
static inline bool __test_and_set_bit(unsigned int nr,
				      volatile unsigned long *addr)
{
	bool old = test_bit(nr, addr);
	__set_bit(nr, addr);
	return old;
}

static inline bool __test_and_clear_bit(unsigned int nr,
					volatile unsigned long *addr)
{
	bool old = test_bit(nr, addr);
	__clear_bit(nr, addr);
	return old;
}

static inline bool bitmap_empty(const unsigned long *addr, unsigned int nbits)
{
	unsigned int i;
	for (i = 0; i < nbits / BITS_PER_LONG; i++)
		if (addr[i])
			return false;
	if (nbits % BITS_PER_LONG)
		return !(addr[nbits / BITS_PER_LONG] &
			 ((1UL << (nbits % BITS_PER_LONG)) - 1));
	return true;
}

static inline void bitmap_fill(unsigned long *addr, unsigned int nbits)
{
	unsigned int i;
	for (i = 0; i < nbits / BITS_PER_LONG; i++)
		addr[i] = ~0UL;
	if (nbits % BITS_PER_LONG)
		addr[nbits / BITS_PER_LONG] = (1UL << (nbits % BITS_PER_LONG)) - 1;
}

static inline void bitmap_clear(unsigned long *addr, unsigned int start,
				unsigned int len)
{
	unsigned int end = start + len, b;
	for (b = start; b < end; b++)
		__clear_bit(b, addr);
}

static inline unsigned long find_next_bit(const unsigned long *addr,
		unsigned long size, unsigned long offset)
{
	unsigned long b;
	for (b = offset; b < size; b++)
		if (test_bit((unsigned int)b, addr))
			return b;
	return size;
}

static inline unsigned long find_next_zero_bit(const unsigned long *addr,
		unsigned long size, unsigned long offset)
{
	unsigned long b;
	for (b = offset; b < size; b++)
		if (!test_bit((unsigned int)b, addr))
			return b;
	return size;
}
