/* Scalar wrappers around lib/find_bit.c, lib/bitmap.c and the
 * include/linux/{bitmap,find}.h inline layer, so the standard scalar
 * bridge can drive them.
 *
 * Deterministic seeded bitmaps, same convention as the other ports:
 *   - 16-word (1024-bit) static bitmaps bm1/bm2/bm3/dst; every wrapper
 *     refills them from scalar seeds before each call.
 *   - nbits_of(v) = |v| % 513 (0..512: hits every word residue incl. 0),
 *     start_of(v) = |v| % 640 (can exceed nbits -> "not found" paths).
 *   - sel_fill() gives the second/third bitmap an adversarial fill
 *     (copy / invert / zero / ones of bm1, or its own seed) so true
 *     branches random seeds almost never reach get exercised.
 *   - dst is always junk-filled first; dst-writing ops return an
 *     order-sensitive FNV checksum over all 16 words packed with the
 *     op's own return value (multiplier chosen >= value range, so the
 *     packing is injective) — untouched-tail behavior is verified too.
 *   - *_c variants pass a compile-time nbits so the header's
 *     small_const_nbits / memcmp / memset fast paths run instead of the
 *     general out-of-line path that runtime nbits selects.
 *   - get_random_u32_below() is deterministic by design: linux/random.h
 *     implements it as splitmix64 over a state the wrapper seeds per
 *     call (rseed), keeping find_random_bit() verifiable.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/bitops.h>
#include <linux/bitmap.h>
#include <linux/random.h>
#include <linux/device.h>

#define NW 16	/* unsigned longs per bitmap = 1024 bits */
#define N32 16	/* u32 slots for arr32 conversions */

static unsigned long bm1[NW], bm2[NW], bm3[NW], dst[NW];
static u32 buf32[N32];
static u64 buf64[NW];

/* backing state for the shims */
u64 __harness_rand_state;
struct devm_action __harness_devm_actions[DEVM_MAX_ACTIONS];
int __harness_devm_nactions;

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static uint64_t iabs(int64_t v)
{
	return v < 0 ? 0ULL - (uint64_t)v : (uint64_t)v;
}

static unsigned int nbits_of(int64_t v) { return (unsigned)(iabs(v) % 513); }
static unsigned int start_of(int64_t v) { return (unsigned)(iabs(v) % 640); }

static void fill(unsigned long *b, uint64_t seed)
{
	int i;
	rng_state = seed | 1;
	for (i = 0; i < NW; i++)
		b[i] = xs64();
}

static void fill32(uint64_t seed)
{
	int i;
	rng_state = seed | 1;
	for (i = 0; i < N32; i++)
		buf32[i] = (u32)xs64();
}

static void fill64(uint64_t seed)
{
	int i;
	rng_state = seed | 1;
	for (i = 0; i < NW; i++)
		buf64[i] = xs64();
}

/* Adversarial fill for a derived bitmap: sel picks ones/zeros/copy/
 * invert of @a or an independent seed so non-random branches (equal,
 * subset, empty, full, identity-remap, w==0) are reachable. */
static void sel_fill(unsigned long *b, const unsigned long *a,
		     uint64_t seed, uint64_t sel)
{
	int i;
	switch (sel % 5) {
	case 1: memcpy(b, a, sizeof bm1); break;
	case 2: for (i = 0; i < NW; i++) b[i] = ~a[i]; break;
	case 3: memset(b, 0, sizeof bm1); break;
	case 4: memset(b, 0xff, sizeof bm1); break;
	default: fill(b, seed); break;
	}
}

static uint64_t cksum(const unsigned long *b)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;
	for (i = 0; i < NW; i++)
		h = (h ^ b[i]) * 0x100000001b3ULL;
	return h;
}

static uint64_t cksum32(const u32 *b)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;
	for (i = 0; i < N32; i++)
		h = (h ^ b[i]) * 0x100000001b3ULL;
	return h;
}

static uint64_t cksum64(const u64 *b)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;
	for (i = 0; i < NW; i++)
		h = (h ^ b[i]) * 0x100000001b3ULL;
	return h;
}

/* =============== lib/find_bit.c out-of-line exports ================= */
/* "raw_" prefix = the _/_-prefixed library symbols verbatim. */

uint64_t wrap_raw_find_first_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return _find_first_bit(bm1, nbits_of(size));
}
uint64_t wrap_raw_find_first_and_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	fill(bm1, s1); fill(bm2, s2);
	return _find_first_and_bit(bm1, bm2, nbits_of(size));
}
uint64_t wrap_raw_find_first_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	fill(bm1, s1); fill(bm2, s2);
	return _find_first_andnot_bit(bm1, bm2, nbits_of(size));
}
uint64_t wrap_raw_find_first_and_and_bit(uint64_t s1, uint64_t s2, uint64_t s3,
					 int64_t size)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return _find_first_and_and_bit(bm1, bm2, bm3, nbits_of(size));
}
uint64_t wrap_raw_find_first_zero_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return _find_first_zero_bit(bm1, nbits_of(size));
}
uint64_t wrap_raw_find_next_bit(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return _find_next_bit(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_raw_find_nth_bit(uint64_t s, int64_t size, int64_t n)
{
	fill(bm1, s);
	return __find_nth_bit(bm1, nbits_of(size), start_of(n));
}
uint64_t wrap_raw_find_nth_and_bit(uint64_t s1, uint64_t s2, int64_t size,
				   int64_t n)
{
	fill(bm1, s1); fill(bm2, s2);
	return __find_nth_and_bit(bm1, bm2, nbits_of(size), start_of(n));
}
uint64_t wrap_raw_find_nth_and_andnot_bit(uint64_t s1, uint64_t s2,
					  uint64_t s3, int64_t size, int64_t n)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return __find_nth_and_andnot_bit(bm1, bm2, bm3, nbits_of(size),
					 start_of(n));
}
uint64_t wrap_raw_find_next_and_bit(uint64_t s1, uint64_t s2, int64_t size,
				    int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return _find_next_and_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_raw_find_next_andnot_bit(uint64_t s1, uint64_t s2, int64_t size,
				       int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return _find_next_andnot_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_raw_find_next_or_bit(uint64_t s1, uint64_t s2, int64_t size,
				   int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return _find_next_or_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_raw_find_next_zero_bit(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return _find_next_zero_bit(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_raw_find_last_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return _find_last_bit(bm1, nbits_of(size));
}
/* offset return and clump out-param packed: off*256 + clump (clump<256). */
uint64_t wrap_find_next_clump8(uint64_t s, int64_t size, int64_t start)
{
	unsigned long clump = 0, off;
	fill(bm1, s);
	off = find_next_clump8(&clump, bm1, nbits_of(size), start_of(start));
	return off * 256 + (clump & 0xff);
}
uint64_t wrap_find_first_clump8(uint64_t s, int64_t size)
{
	unsigned long clump = 0, off;
	fill(bm1, s);
	off = find_first_clump8(&clump, bm1, nbits_of(size));
	return off * 256 + (clump & 0xff);
}
/* find_random_bit: rseed feeds the deterministic get_random_u32_below;
 * mode picks the bitmap fill so the w==0 / w==1 switch arms run too. */
uint64_t wrap_find_random_bit(uint64_t s, int64_t size, uint64_t rseed,
			      uint64_t mode)
{
	unsigned long n = nbits_of(size);
	fill(bm1, s);
	switch (mode % 3) {
	case 1: memset(bm1, 0, sizeof bm1); break;
	case 2: memset(bm1, 0, sizeof bm1);
		if (n) bm1[0] = 1UL << (s % 64);
		break;
	}
	__harness_seed_random(rseed);
	return find_random_bit(bm1, n);
}

/* ============ include/linux/find.h public finders (runtime) ========= */

uint64_t wrap_find_first_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return find_first_bit(bm1, nbits_of(size));
}
uint64_t wrap_find_next_bit(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return find_next_bit(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_find_nth_bit(uint64_t s, int64_t size, int64_t n)
{
	fill(bm1, s);
	return find_nth_bit(bm1, nbits_of(size), start_of(n));
}
uint64_t wrap_find_first_and_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_first_and_bit(bm1, bm2, nbits_of(size));
}
uint64_t wrap_find_next_and_bit(uint64_t s1, uint64_t s2, int64_t size,
				int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_and_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_find_nth_and_bit(uint64_t s1, uint64_t s2, int64_t size,
			       int64_t n)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_nth_and_bit(bm1, bm2, nbits_of(size), start_of(n));
}
uint64_t wrap_find_first_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_first_andnot_bit(bm1, bm2, nbits_of(size));
}
uint64_t wrap_find_next_andnot_bit(uint64_t s1, uint64_t s2, int64_t size,
				   int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_andnot_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_find_nth_and_andnot_bit(uint64_t s1, uint64_t s2, uint64_t s3,
				      int64_t size, int64_t n)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return find_nth_and_andnot_bit(bm1, bm2, bm3, nbits_of(size),
				       start_of(n));
}
uint64_t wrap_find_first_and_and_bit(uint64_t s1, uint64_t s2, uint64_t s3,
				     int64_t size)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return find_first_and_and_bit(bm1, bm2, bm3, nbits_of(size));
}
uint64_t wrap_find_next_or_bit(uint64_t s1, uint64_t s2, int64_t size,
			       int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_or_bit(bm1, bm2, nbits_of(size), start_of(start));
}
uint64_t wrap_find_first_zero_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return find_first_zero_bit(bm1, nbits_of(size));
}
uint64_t wrap_find_next_zero_bit(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return find_next_zero_bit(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_find_last_bit(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return find_last_bit(bm1, nbits_of(size));
}
uint64_t wrap_find_next_bit_wrap(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return find_next_bit_wrap(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_find_next_and_bit_wrap(uint64_t s1, uint64_t s2, int64_t size,
				     int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_and_bit_wrap(bm1, bm2, nbits_of(size),
				      start_of(start));
}
/* LE finders: on little-endian find.h maps them to the plain finders. */
uint64_t wrap_find_next_bit_le(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return find_next_bit_le(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_find_next_zero_bit_le(uint64_t s, int64_t size, int64_t start)
{
	fill(bm1, s);
	return find_next_zero_bit_le(bm1, nbits_of(size), start_of(start));
}
uint64_t wrap_find_first_zero_bit_le(uint64_t s, int64_t size)
{
	fill(bm1, s);
	return find_first_zero_bit_le(bm1, nbits_of(size));
}
uint64_t wrap___for_each_wrap(uint64_t s, int64_t size, int64_t start,
			      int64_t n)
{
	fill(bm1, s);
	return __for_each_wrap(bm1, nbits_of(size), start_of(start),
			       start_of(n));
}

/* ===== include/linux/find.h public finders, nbits = 40 constant ===== */
/* small_const_nbits(40) is true at compile time: the header's single-word
 * GENMASK fast paths run. start/n bounded to 45 so >= size happens too. */
#define CN 40

uint64_t wrap_find_first_bit_c(uint64_t s)
{
	fill(bm1, s);
	return find_first_bit(bm1, CN);
}
uint64_t wrap_find_next_bit_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return find_next_bit(bm1, CN, iabs(start) % 45);
}
uint64_t wrap_find_nth_bit_c(uint64_t s, int64_t n)
{
	fill(bm1, s);
	return find_nth_bit(bm1, CN, iabs(n) % 45);
}
uint64_t wrap_find_first_and_bit_c(uint64_t s1, uint64_t s2)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_first_and_bit(bm1, bm2, CN);
}
uint64_t wrap_find_next_and_bit_c(uint64_t s1, uint64_t s2, int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_and_bit(bm1, bm2, CN, iabs(start) % 45);
}
uint64_t wrap_find_nth_and_bit_c(uint64_t s1, uint64_t s2, int64_t n)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_nth_and_bit(bm1, bm2, CN, iabs(n) % 45);
}
uint64_t wrap_find_first_andnot_bit_c(uint64_t s1, uint64_t s2)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_first_andnot_bit(bm1, bm2, CN);
}
uint64_t wrap_find_next_andnot_bit_c(uint64_t s1, uint64_t s2, int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_andnot_bit(bm1, bm2, CN, iabs(start) % 45);
}
uint64_t wrap_find_nth_and_andnot_bit_c(uint64_t s1, uint64_t s2, uint64_t s3,
					int64_t n)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return find_nth_and_andnot_bit(bm1, bm2, bm3, CN, iabs(n) % 45);
}
uint64_t wrap_find_first_and_and_bit_c(uint64_t s1, uint64_t s2, uint64_t s3)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3);
	return find_first_and_and_bit(bm1, bm2, bm3, CN);
}
uint64_t wrap_find_next_or_bit_c(uint64_t s1, uint64_t s2, int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_or_bit(bm1, bm2, CN, iabs(start) % 45);
}
uint64_t wrap_find_first_zero_bit_c(uint64_t s)
{
	fill(bm1, s);
	return find_first_zero_bit(bm1, CN);
}
uint64_t wrap_find_next_zero_bit_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return find_next_zero_bit(bm1, CN, iabs(start) % 45);
}
uint64_t wrap_find_last_bit_c(uint64_t s)
{
	fill(bm1, s);
	return find_last_bit(bm1, CN);
}
uint64_t wrap_find_next_bit_wrap_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return find_next_bit_wrap(bm1, CN, iabs(start) % 45);
}
uint64_t wrap_find_next_and_bit_wrap_c(uint64_t s1, uint64_t s2, int64_t start)
{
	fill(bm1, s1); fill(bm2, s2);
	return find_next_and_bit_wrap(bm1, bm2, CN, iabs(start) % 45);
}
uint64_t wrap_find_next_bit_le_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return find_next_bit_le(bm1, CN, iabs(start) % 45);
}
uint64_t wrap_find_next_zero_bit_le_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return find_next_zero_bit_le(bm1, CN, iabs(start) % 45);
}
uint64_t wrap_find_first_zero_bit_le_c(uint64_t s)
{
	fill(bm1, s);
	return find_first_zero_bit_le(bm1, CN);
}

/* ============ include/linux/find.h for_each iteration macros ======== */

uint64_t wrap_each_set_bit(uint64_t s, int64_t size)
{
	unsigned long b, h = 0;
	fill(bm1, s);
	for_each_set_bit(b, bm1, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_set_bit_from(uint64_t s, int64_t size, int64_t start)
{
	unsigned long b, h = 0;
	fill(bm1, s);
	b = start_of(start);
	for_each_set_bit_from(b, bm1, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_clear_bit(uint64_t s, int64_t size)
{
	unsigned long b, h = 0;
	fill(bm1, s);
	for_each_clear_bit(b, bm1, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_clear_bit_from(uint64_t s, int64_t size, int64_t start)
{
	unsigned long b, h = 0;
	fill(bm1, s);
	b = start_of(start);
	for_each_clear_bit_from(b, bm1, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_and_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	unsigned long b, h = 0;
	fill(bm1, s1); fill(bm2, s2);
	for_each_and_bit(b, bm1, bm2, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	unsigned long b, h = 0;
	fill(bm1, s1); fill(bm2, s2);
	for_each_andnot_bit(b, bm1, bm2, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_or_bit(uint64_t s1, uint64_t s2, int64_t size)
{
	unsigned long b, h = 0;
	fill(bm1, s1); fill(bm2, s2);
	for_each_or_bit(b, bm1, bm2, nbits_of(size))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_set_bitrange(uint64_t s, int64_t size)
{
	unsigned long b, e, h = 0;
	fill(bm1, s);
	for_each_set_bitrange(b, e, bm1, nbits_of(size))
		h = h * 131 + b * 17 + e;
	return h;
}
uint64_t wrap_each_set_bitrange_from(uint64_t s, int64_t size, int64_t start)
{
	unsigned long b, e, h = 0;
	fill(bm1, s);
	b = start_of(start);
	for_each_set_bitrange_from(b, e, bm1, nbits_of(size))
		h = h * 131 + b * 17 + e;
	return h;
}
uint64_t wrap_each_clear_bitrange(uint64_t s, int64_t size)
{
	unsigned long b, e, h = 0;
	fill(bm1, s);
	for_each_clear_bitrange(b, e, bm1, nbits_of(size))
		h = h * 131 + b * 17 + e;
	return h;
}
uint64_t wrap_each_clear_bitrange_from(uint64_t s, int64_t size, int64_t start)
{
	unsigned long b, e, h = 0;
	fill(bm1, s);
	b = start_of(start);
	for_each_clear_bitrange_from(b, e, bm1, nbits_of(size))
		h = h * 131 + b * 17 + e;
	return h;
}
uint64_t wrap_each_set_bit_wrap(uint64_t s, int64_t size, int64_t start)
{
	unsigned long b, h = 0;
	fill(bm1, s);
	for_each_set_bit_wrap(b, bm1, nbits_of(size), start_of(start))
		h = h * 131 + b;
	return h;
}
uint64_t wrap_each_set_clump8(uint64_t s, int64_t size)
{
	unsigned long b, clump = 0, h = 0;
	fill(bm1, s);
	for_each_set_clump8(b, clump, bm1, nbits_of(size))
		h = h * 131 + b * 256 + (clump & 0xff);
	return h;
}

/* ============ include/linux/bitmap.h ops (runtime nbits) ============ */

uint64_t wrap_bitmap_and(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)
{
	bool r;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	r = bitmap_and(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst) * 4 + r;
}
uint64_t wrap_bitmap_or(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	bitmap_or(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_xor(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	bitmap_xor(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_andnot(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)
{
	bool r;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	r = bitmap_andnot(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst) * 4 + r;
}
uint64_t wrap_bitmap_weighted_or(uint64_t s1, uint64_t s2, uint64_t ds,
				 int64_t nb)
{
	unsigned int w;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	w = bitmap_weighted_or(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst) * 1024 + w;
}
uint64_t wrap_bitmap_weighted_xor(uint64_t s1, uint64_t s2, uint64_t ds,
				  int64_t nb)
{
	unsigned int w;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	w = bitmap_weighted_xor(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst) * 1024 + w;
}
uint64_t wrap_bitmap_complement(uint64_t s, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_complement(dst, bm1, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_replace(uint64_t s1, uint64_t s2, uint64_t s3,
			     uint64_t ds, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3); fill(dst, ds);
	bitmap_replace(dst, bm1, bm2, bm3, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_equal(uint64_t s1, uint64_t s2, uint64_t sel, int64_t nb)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_equal(bm1, bm2, nbits_of(nb));
}
uint64_t wrap_bitmap_or_equal(uint64_t s1, uint64_t s2, uint64_t s3,
			      uint64_t sel, int64_t nb)
{
	int i;
	fill(bm1, s1); fill(bm2, s2);
	switch (sel % 4) {
	case 1: for (i = 0; i < NW; i++) bm3[i] = bm1[i] | bm2[i]; break;
	case 2: memset(bm3, 0, sizeof bm3); break;
	case 3: for (i = 0; i < NW; i++) bm3[i] = ~(bm1[i] | bm2[i]); break;
	default: fill(bm3, s3); break;
	}
	return bitmap_or_equal(bm1, bm2, bm3, nbits_of(nb));
}
uint64_t wrap_bitmap_intersects(uint64_t s1, uint64_t s2, uint64_t sel,
				int64_t nb)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_intersects(bm1, bm2, nbits_of(nb));
}
uint64_t wrap_bitmap_intersects_and(uint64_t s1, uint64_t s2, uint64_t s3,
				    uint64_t sel, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2);
	sel_fill(bm3, bm1, s3, sel);
	return bitmap_intersects_and(bm1, bm2, bm3, nbits_of(nb));
}
uint64_t wrap_bitmap_subset(uint64_t s1, uint64_t s2, uint64_t sel, int64_t nb)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_subset(bm1, bm2, nbits_of(nb));
}
uint64_t wrap_bitmap_empty(uint64_t s, uint64_t mode, int64_t nb)
{
	fill(bm1, s);
	if (mode % 3 == 1)
		memset(bm1, 0, sizeof bm1);
	else if (mode % 3 == 2)
		memset(bm1, 0xff, sizeof bm1);
	return bitmap_empty(bm1, nbits_of(nb));
}
uint64_t wrap_bitmap_full(uint64_t s, uint64_t mode, int64_t nb)
{
	fill(bm1, s);
	if (mode % 3 == 1)
		memset(bm1, 0, sizeof bm1);
	else if (mode % 3 == 2)
		memset(bm1, 0xff, sizeof bm1);
	return bitmap_full(bm1, nbits_of(nb));
}
uint64_t wrap_bitmap_weight(uint64_t s, int64_t nb)
{
	fill(bm1, s);
	return bitmap_weight(bm1, nbits_of(nb));
}
uint64_t wrap_bitmap_weight_and(uint64_t s1, uint64_t s2, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2);
	return bitmap_weight_and(bm1, bm2, nbits_of(nb));
}
uint64_t wrap_bitmap_weight_andnot(uint64_t s1, uint64_t s2, int64_t nb)
{
	fill(bm1, s1); fill(bm2, s2);
	return bitmap_weight_andnot(bm1, bm2, nbits_of(nb));
}
uint64_t wrap_bitmap_weight_from(uint64_t s, int64_t start, int64_t end)
{
	fill(bm1, s);
	return bitmap_weight_from(bm1, start_of(start), start_of(end));
}
uint64_t wrap_bitmap_set(uint64_t s, int64_t start, int64_t len)
{
	fill(dst, s);
	bitmap_set(dst, iabs(start) % 512, iabs(len) % 513);
	return cksum(dst);
}
uint64_t wrap_bitmap_clear(uint64_t s, int64_t start, int64_t len)
{
	fill(dst, s);
	bitmap_clear(dst, iabs(start) % 512, iabs(len) % 513);
	return cksum(dst);
}
uint64_t wrap_bitmap_shift_right(uint64_t s, uint64_t ds, int64_t shift,
				 int64_t nb)
{
	unsigned n = nbits_of(nb);
	unsigned sh = n ? (unsigned)(iabs(shift) % (n + 64)) : 0;
	fill(bm1, s); fill(dst, ds);
	bitmap_shift_right(dst, bm1, sh, n);
	return cksum(dst);
}
uint64_t wrap_bitmap_shift_left(uint64_t s, uint64_t ds, int64_t shift,
				int64_t nb)
{
	unsigned n = nbits_of(nb);
	unsigned sh = (unsigned)(iabs(shift) % 1024);
	fill(bm1, s); fill(dst, ds);
	bitmap_shift_left(dst, bm1, sh, n);
	return cksum(dst);
}
uint64_t wrap_bitmap_cut(uint64_t s, uint64_t ds, int64_t first, int64_t cut,
			 int64_t nb)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_cut(dst, bm1, (unsigned)(iabs(first) % 640),
		   (unsigned)(iabs(cut) % 513), nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_scatter(uint64_t s, uint64_t sm, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(bm2, sm); fill(dst, ds);
	bitmap_scatter(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_gather(uint64_t s, uint64_t sm, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(bm2, sm); fill(dst, ds);
	bitmap_gather(dst, bm1, bm2, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_remap(uint64_t s, uint64_t so, uint64_t sn, uint64_t sel,
			   uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(bm2, so);
	sel_fill(bm3, bm2, sn, sel);
	fill(dst, ds);
	bitmap_remap(dst, bm1, bm2, bm3, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_bitremap(uint64_t so, uint64_t sn, uint64_t sel,
			      int64_t oldbit, int64_t bits)
{
	int r;
	fill(bm2, so);
	sel_fill(bm3, bm2, sn, sel);
	r = bitmap_bitremap((int)(iabs(oldbit) % 640), bm2, bm3,
			    (int)nbits_of(bits));
	return (uint64_t)(int64_t)r;
}
uint64_t wrap_bitmap_onto(uint64_t s, uint64_t sr, uint64_t sel, uint64_t ds,
			  int64_t nb)
{
	fill(bm1, s); fill(bm2, sr);
	sel_fill(bm3, bm2, sr, sel);
	fill(dst, ds);
	bitmap_onto(dst, bm1, bm3, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_fold(uint64_t s, uint64_t ds, int64_t sz, int64_t nb)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_fold(dst, bm1, (unsigned)(iabs(sz) % 511 + 1), nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_find_next_zero_area(uint64_t s, int64_t size,
					 int64_t start, int64_t nr, int64_t am)
{
	fill(dst, s);
	return bitmap_find_next_zero_area(dst, nbits_of(size),
					  start_of(start),
					  (unsigned)(iabs(nr) % 65),
					  (1UL << (iabs(am) % 7)) - 1);
}
uint64_t wrap_bitmap_find_next_zero_area_off(uint64_t s, int64_t size,
					     int64_t start, int64_t nr,
					     int64_t am, int64_t ao)
{
	fill(dst, s);
	return bitmap_find_next_zero_area_off(dst, nbits_of(size),
					      start_of(start),
					      (unsigned)(iabs(nr) % 65),
					      (1UL << (iabs(am) % 7)) - 1,
					      iabs(ao) % 64);
}
uint64_t wrap_bitmap_zero(uint64_t ds, int64_t nb)
{
	fill(dst, ds);
	bitmap_zero(dst, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_fill(uint64_t ds, int64_t nb)
{
	fill(dst, ds);
	bitmap_fill(dst, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_copy(uint64_t s, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_copy(dst, bm1, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_copy_clear_tail(uint64_t s, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_copy_clear_tail(dst, bm1, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_copy_and_extend(uint64_t s, uint64_t ds, int64_t count,
				     int64_t size)
{
	unsigned n = nbits_of(size);
	unsigned c = n ? (unsigned)(iabs(count) % (n + 1)) : 0;
	fill(bm1, s); fill(dst, ds);
	bitmap_copy_and_extend(dst, bm1, c, n);
	return cksum(dst);
}
uint64_t wrap_bitmap_from_arr32(uint64_t s, uint64_t ds, int64_t nb)
{
	fill32(s); fill(dst, ds);
	bitmap_from_arr32(dst, buf32, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_to_arr32(uint64_t s, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill32(ds);
	bitmap_to_arr32(buf32, bm1, nbits_of(nb));
	return cksum32(buf32);
}
uint64_t wrap_bitmap_from_arr64(uint64_t s, uint64_t ds, int64_t nb)
{
	fill64(s); fill(dst, ds);
	bitmap_from_arr64(dst, buf64, nbits_of(nb));
	return cksum(dst);
}
uint64_t wrap_bitmap_to_arr64(uint64_t s, uint64_t ds, int64_t nb)
{
	fill(bm1, s); fill64(ds);
	bitmap_to_arr64(buf64, bm1, nbits_of(nb));
	return cksum64(buf64);
}
uint64_t wrap_bitmap_from_u64(uint64_t v, uint64_t ds)
{
	fill(dst, ds);
	bitmap_from_u64(dst, v);
	return cksum(dst);
}
uint64_t wrap_bitmap_read(uint64_t s, int64_t start, int64_t nb)
{
	fill(bm1, s);
	return bitmap_read(bm1, iabs(start) % 960, iabs(nb) % 65);
}
uint64_t wrap_bitmap_write(uint64_t s, int64_t start, int64_t nb, uint64_t v)
{
	fill(dst, s);
	bitmap_write(dst, v, iabs(start) % 960, iabs(nb) % 65);
	return cksum(dst);
}
/* nbits 64..67 — exercises the nbits > BITS_PER_LONG early-return arms. */
uint64_t wrap_bitmap_read_hi(uint64_t s, int64_t start, int64_t nb)
{
	fill(bm1, s);
	return bitmap_read(bm1, iabs(start) % 950, 64 + iabs(nb) % 4);
}
uint64_t wrap_bitmap_write_hi(uint64_t s, int64_t start, int64_t nb, uint64_t v)
{
	fill(dst, s);
	bitmap_write(dst, v, iabs(start) % 950, 64 + iabs(nb) % 4);
	return cksum(dst);
}
uint64_t wrap_bitmap_get_value8(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return bitmap_get_value8(bm1, iabs(start) % 960);
}
uint64_t wrap_bitmap_set_value8(uint64_t s, int64_t start, int64_t v)
{
	fill(dst, s);
	bitmap_set_value8(dst, iabs(v) % 300, iabs(start) % 960);
	return cksum(dst);
}
uint64_t wrap_bitmap_release_region(uint64_t s, int64_t pos, int64_t order)
{
	fill(dst, s);
	bitmap_release_region(dst, (unsigned)(iabs(pos) % 513),
			      (int)(iabs(order) % 8));
	return cksum(dst);
}
uint64_t wrap_bitmap_allocate_region(uint64_t s, int64_t pos, int64_t order)
{
	int r;
	fill(dst, s);
	r = bitmap_allocate_region(dst, (unsigned)(iabs(pos) % 513),
				   (int)(iabs(order) % 8));
	return cksum(dst) * 32 + (uint64_t)(r + 16);
}
uint64_t wrap_bitmap_find_free_region(uint64_t s, uint64_t mode, int64_t bits,
				      int64_t order)
{
	int r;
	fill(dst, s);
	if (mode % 3 == 1)
		memset(dst, 0, sizeof dst);
	else if (mode % 3 == 2)
		memset(dst, 0xff, sizeof dst);
	r = bitmap_find_free_region(dst, (unsigned)nbits_of(bits),
				    (int)(iabs(order) % 8));
	return cksum(dst) * 1024 + (uint64_t)(r + 13);
}

/* ===== bitmap.h constant-nbits variants (small_const_nbits paths) ==== */
/* CN=40 keeps the fast paths inside word 0. For bitmap_equal an aligned
 * constant (128) hits the memcmp middle path instead; bitmap_set/clear
 * get extra variants for their __set_bit / GENMASK / memset dispatches. */

uint64_t wrap_bitmap_and_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	bool r;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	r = bitmap_and(dst, bm1, bm2, CN);
	return cksum(dst) * 4 + r;
}
uint64_t wrap_bitmap_or_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	bitmap_or(dst, bm1, bm2, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_xor_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	bitmap_xor(dst, bm1, bm2, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_andnot_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	bool r;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	r = bitmap_andnot(dst, bm1, bm2, CN);
	return cksum(dst) * 4 + r;
}
uint64_t wrap_bitmap_weighted_or_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	unsigned int w;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	w = bitmap_weighted_or(dst, bm1, bm2, CN);
	return cksum(dst) * 1024 + w;
}
uint64_t wrap_bitmap_weighted_xor_c(uint64_t s1, uint64_t s2, uint64_t ds)
{
	unsigned int w;
	fill(bm1, s1); fill(bm2, s2); fill(dst, ds);
	w = bitmap_weighted_xor(dst, bm1, bm2, CN);
	return cksum(dst) * 1024 + w;
}
uint64_t wrap_bitmap_complement_c(uint64_t s, uint64_t ds)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_complement(dst, bm1, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_replace_c(uint64_t s1, uint64_t s2, uint64_t s3,
			       uint64_t ds)
{
	fill(bm1, s1); fill(bm2, s2); fill(bm3, s3); fill(dst, ds);
	bitmap_replace(dst, bm1, bm2, bm3, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_equal_c(uint64_t s1, uint64_t s2, uint64_t sel)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_equal(bm1, bm2, CN);
}
/* nbits=128 constant, multiple of 8: memcmp middle path. */
uint64_t wrap_bitmap_equal_c128(uint64_t s1, uint64_t s2, uint64_t sel)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_equal(bm1, bm2, 128);
}
uint64_t wrap_bitmap_or_equal_c(uint64_t s1, uint64_t s2, uint64_t s3,
				uint64_t sel)
{
	int i;
	fill(bm1, s1); fill(bm2, s2);
	switch (sel % 4) {
	case 1: for (i = 0; i < NW; i++) bm3[i] = bm1[i] | bm2[i]; break;
	case 2: memset(bm3, 0, sizeof bm3); break;
	case 3: for (i = 0; i < NW; i++) bm3[i] = ~(bm1[i] | bm2[i]); break;
	default: fill(bm3, s3); break;
	}
	return bitmap_or_equal(bm1, bm2, bm3, CN);
}
uint64_t wrap_bitmap_intersects_c(uint64_t s1, uint64_t s2, uint64_t sel)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_intersects(bm1, bm2, CN);
}
uint64_t wrap_bitmap_intersects_and_c(uint64_t s1, uint64_t s2, uint64_t s3,
				      uint64_t sel)
{
	fill(bm1, s1); fill(bm2, s2);
	sel_fill(bm3, bm1, s3, sel);
	return bitmap_intersects_and(bm1, bm2, bm3, CN);
}
uint64_t wrap_bitmap_subset_c(uint64_t s1, uint64_t s2, uint64_t sel)
{
	fill(bm1, s1);
	sel_fill(bm2, bm1, s2, sel);
	return bitmap_subset(bm1, bm2, CN);
}
uint64_t wrap_bitmap_empty_c(uint64_t s, uint64_t mode)
{
	fill(bm1, s);
	if (mode % 3 == 1)
		memset(bm1, 0, sizeof bm1);
	else if (mode % 3 == 2)
		memset(bm1, 0xff, sizeof bm1);
	return bitmap_empty(bm1, CN);
}
uint64_t wrap_bitmap_full_c(uint64_t s, uint64_t mode)
{
	fill(bm1, s);
	if (mode % 3 == 1)
		memset(bm1, 0, sizeof bm1);
	else if (mode % 3 == 2)
		memset(bm1, 0xff, sizeof bm1);
	return bitmap_full(bm1, CN);
}
uint64_t wrap_bitmap_weight_c(uint64_t s)
{
	fill(bm1, s);
	return bitmap_weight(bm1, CN);
}
uint64_t wrap_bitmap_weight_and_c(uint64_t s1, uint64_t s2)
{
	fill(bm1, s1); fill(bm2, s2);
	return bitmap_weight_and(bm1, bm2, CN);
}
uint64_t wrap_bitmap_weight_andnot_c(uint64_t s1, uint64_t s2)
{
	fill(bm1, s1); fill(bm2, s2);
	return bitmap_weight_andnot(bm1, bm2, CN);
}
uint64_t wrap_bitmap_weight_from_c(uint64_t s, int64_t start)
{
	fill(bm1, s);
	return bitmap_weight_from(bm1, (unsigned)(iabs(start) % 45), CN);
}
uint64_t wrap_bitmap_zero_c(uint64_t ds)
{
	fill(dst, ds);
	bitmap_zero(dst, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_fill_c(uint64_t ds)
{
	fill(dst, ds);
	bitmap_fill(dst, CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_copy_c(uint64_t s, uint64_t ds)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_copy(dst, bm1, CN);
	return cksum(dst);
}
/* bitmap_set/clear dispatch layers: nbits==1 -> __set_bit,
 * start+nbits<=64 const -> GENMASK, byte-aligned const -> memset. */
uint64_t wrap_bitmap_set_c1(uint64_t s, int64_t start)
{
	fill(dst, s);
	bitmap_set(dst, (unsigned)(iabs(start) % 640), 1);
	return cksum(dst);
}
uint64_t wrap_bitmap_clear_c1(uint64_t s, int64_t start)
{
	fill(dst, s);
	bitmap_clear(dst, (unsigned)(iabs(start) % 640), 1);
	return cksum(dst);
}
uint64_t wrap_bitmap_set_cs(uint64_t s)
{
	fill(dst, s);
	bitmap_set(dst, 10, 30);	/* GENMASK(39,10) path */
	return cksum(dst);
}
uint64_t wrap_bitmap_clear_cs(uint64_t s)
{
	fill(dst, s);
	bitmap_clear(dst, 10, 30);
	return cksum(dst);
}
uint64_t wrap_bitmap_set_cm(uint64_t s)
{
	fill(dst, s);
	bitmap_set(dst, 64, 128);	/* memset(map+8, 0xff, 16) path */
	return cksum(dst);
}
uint64_t wrap_bitmap_clear_cm(uint64_t s)
{
	fill(dst, s);
	bitmap_clear(dst, 64, 128);
	return cksum(dst);
}
uint64_t wrap_bitmap_shift_right_c(uint64_t s, uint64_t ds, int64_t shift)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_shift_right(dst, bm1, (unsigned)(iabs(shift) % 64), CN);
	return cksum(dst);
}
uint64_t wrap_bitmap_shift_left_c(uint64_t s, uint64_t ds, int64_t shift)
{
	fill(bm1, s); fill(dst, ds);
	bitmap_shift_left(dst, bm1, (unsigned)(iabs(shift) % 64), CN);
	return cksum(dst);
}

/* ============ bitop primitives (direct scalar coverage) ============= */

uint64_t wrap_test_bit(uint64_t s, int64_t pos)
{
	fill(bm1, s);
	return test_bit(iabs(pos) % 1024, bm1);
}
uint64_t wrap_set_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	set_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap_clear_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	clear_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap_change_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	change_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap___set_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	__set_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap___clear_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	__clear_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap___change_bit(uint64_t s, int64_t pos)
{
	fill(dst, s);
	__change_bit(iabs(pos) % 1024, dst);
	return cksum(dst);
}
uint64_t wrap___assign_bit(uint64_t s, int64_t pos, int64_t v)
{
	fill(dst, s);
	__assign_bit(iabs(pos) % 1024, dst, iabs(v) & 1);
	return cksum(dst);
}
uint64_t wrap_test_and_set_bit(uint64_t s, int64_t pos)
{
	bool r;
	fill(dst, s);
	r = test_and_set_bit(iabs(pos) % 1024, dst);
	return cksum(dst) * 4 + r;
}
uint64_t wrap_test_and_clear_bit(uint64_t s, int64_t pos)
{
	bool r;
	fill(dst, s);
	r = test_and_clear_bit(iabs(pos) % 1024, dst);
	return cksum(dst) * 4 + r;
}
uint64_t wrap_test_and_change_bit(uint64_t s, int64_t pos)
{
	bool r;
	fill(dst, s);
	r = test_and_change_bit(iabs(pos) % 1024, dst);
	return cksum(dst) * 4 + r;
}
uint64_t wrap_hweight_long(uint64_t w)
{
	return hweight_long(w);
}
uint64_t wrap_fns(uint64_t w, int64_t n)
{
	return fns(w, (unsigned)(iabs(n) % 65));
}
uint64_t wrap_ffs(int64_t v)
{
	return (uint64_t)ffs((int)(int32_t)iabs(v));
}
uint64_t wrap_fls(uint64_t v)
{
	return (uint64_t)fls((unsigned int)v);
}
