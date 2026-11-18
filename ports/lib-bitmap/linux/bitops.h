#pragma once
/* Generic bitops shim for the lib-bitmap harness — faithful userspace
 * versions of what include/linux/bitops.h and the asm-generic bitops
 * headers provide on x86-64. Do NOT copy include/linux/bitops.h: it
 * pulls in asm/bitops.h which does not exist here. */
#include <linux/types.h>
#include <linux/compiler.h>

typedef u64 __u64;

/* ---- include/linux/bits.h ---- */
#define BITS_PER_TYPE(type) (sizeof(type) * 8)
#define BIT(nr)         (1UL << (nr))
#define BIT_ULL(nr)     (1ULL << (nr))
#define BIT_MASK(nr)    (1UL << ((nr) % BITS_PER_LONG))
#define BIT_WORD(nr)    ((nr) / BITS_PER_LONG)

/* GENMASK(h, l): input checks (BUILD_BUG_ON_ZERO, const_true) omitted —
 * the runtime semantics are what the harness verifies. l <= h and
 * h < BITS_PER_LONG are preconditions upstream enforces at build time. */
#define GENMASK_TYPE(t, h, l) \
	((t)((((t)~0) << (l)) & ((t)~0 >> (BITS_PER_TYPE(t) - 1 - (h)))))
#define GENMASK(h, l)      GENMASK_TYPE(unsigned long, h, l)
#define GENMASK_ULL(h, l)  GENMASK_TYPE(unsigned long long, h, l)

/* ---- include/linux/bitops.h ---- */
#define __KERNEL_DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))
#define BITS_TO_LONGS(nr)   __KERNEL_DIV_ROUND_UP(nr, BITS_PER_TYPE(long))
#define BITS_TO_U64(nr)     __KERNEL_DIV_ROUND_UP(nr, BITS_PER_TYPE(u64))
#define BITS_TO_U32(nr)     __KERNEL_DIV_ROUND_UP(nr, BITS_PER_TYPE(u32))
#define BITS_TO_BYTES(nr)   __KERNEL_DIV_ROUND_UP(nr, BITS_PER_TYPE(char))

/* include/asm-generic/bitsperlong.h */
#define small_const_nbits(nbits) \
	(__builtin_constant_p(nbits) && (nbits) <= BITS_PER_LONG && (nbits) > 0)

/* ---- non-atomic bitops (generic-non-atomic.h semantics) ---- */
static __always_inline void
__set_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);

	*p |= mask;
}

static __always_inline void
__clear_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);

	*p &= ~mask;
}

static __always_inline void
__change_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);

	*p ^= mask;
}

static __always_inline bool
__test_and_set_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);
	unsigned long old = *p;

	*p = old | mask;
	return (old & mask) != 0;
}

static __always_inline bool
__test_and_clear_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);
	unsigned long old = *p;

	*p = old & ~mask;
	return (old & mask) != 0;
}

static __always_inline bool
__test_and_change_bit(unsigned long nr, volatile unsigned long *addr)
{
	unsigned long mask = BIT_MASK(nr);
	unsigned long *p = ((unsigned long *)addr) + BIT_WORD(nr);
	unsigned long old = *p;

	*p = old ^ mask;
	return (old & mask) != 0;
}

static __always_inline bool
_test_bit(unsigned long nr, const volatile unsigned long *addr)
{
	return 1UL & (addr[BIT_WORD(nr)] >> (nr & (BITS_PER_LONG - 1)));
}

/* Atomic-prefixed names: single-threaded harness, same semantics. */
#define set_bit(nr, addr)          __set_bit((nr), (addr))
#define clear_bit(nr, addr)        __clear_bit((nr), (addr))
#define change_bit(nr, addr)       __change_bit((nr), (addr))
#define test_and_set_bit(nr, addr) __test_and_set_bit((nr), (addr))
#define test_and_clear_bit(nr, addr) __test_and_clear_bit((nr), (addr))
#define test_and_change_bit(nr, addr) __test_and_change_bit((nr), (addr))
#define test_bit(nr, addr)         _test_bit((nr), (addr))

#define assign_bit(nr, addr, value) \
	((value) ? set_bit((nr), (addr)) : clear_bit((nr), (addr)))
#define __assign_bit(nr, addr, value) \
	((value) ? __set_bit((nr), (addr)) : __clear_bit((nr), (addr)))

/* ---- scan primitives (asm-generic builtin-* semantics) ---- */
/* __ffs: index of lowest set bit; undefined for word == 0. */
static __always_inline __attribute__((__const__)) unsigned int
__ffs(unsigned long word)
{
	return __builtin_ctzl(word);
}

/* ffz: index of lowest clear bit; undefined for word == ~0UL. */
#define ffz(x) __ffs(~(x))

#define ffs(x) __builtin_ffs(x)

/* fls: index of highest set bit + 1; fls(0) = 0. */
static __always_inline __attribute__((__const__)) int fls(unsigned int x)
{
	return x ? sizeof(x) * 8 - __builtin_clz(x) : 0;
}

/* fls64 */
static __always_inline __attribute__((__const__)) int fls64(u64 x)
{
	return x ? 64 - __builtin_clzll(x) : 0;
}

/* __fls: index of highest set bit; undefined for word == 0. */
static __always_inline __attribute__((__const__)) unsigned int
__fls(unsigned long word)
{
	return (sizeof(word) * 8) - 1 - __builtin_clzl(word);
}

static inline __attribute__((__const__)) unsigned int __ffs64(u64 word)
{
	return __ffs((unsigned long)word);
}

/* fns - find N'th set bit in a word (include/linux/bitops.h). */
static inline unsigned int fns(unsigned long word, unsigned int n)
{
	while (word && n--)
		word &= word - 1;

	return word ? __ffs(word) : BITS_PER_LONG;
}

/* ---- hamming weight (arch_hweight: popcount) ---- */
static inline unsigned int hweight8(unsigned int w)
{
	return __builtin_popcount(w & 0xff);
}
static inline unsigned int hweight16(unsigned int w)
{
	return __builtin_popcount(w & 0xffff);
}
static inline unsigned int hweight32(unsigned int w)
{
	return __builtin_popcount(w);
}
static inline unsigned long hweight64(u64 w)
{
	return __builtin_popcountll(w);
}
static __always_inline unsigned long hweight_long(unsigned long w)
{
	return sizeof(w) == 4 ? hweight32(w) : hweight64((__u64)w);
}

/* ---- rotations ---- */
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
