/* Scalar wrappers around lib/crypto/siphash.c, lib/xxhash.c and
 * include/linux/jhash.h so the standard scalar bridge can drive them.
 *
 * Deterministic seeded buffers, same convention as the other ports:
 * buf_len(len) = |len| % 513 with defined INT64_MIN negation.
 * `off` selects buffer alignment (buf is 16-byte aligned; off=1 makes the
 * data pointer unaligned -> siphash()/hsiphash() dispatch to both
 * backends on the same content). Keys are derived from scalars so the
 * full key space is reachable.
 */
#include <stdint.h>
#include <stddef.h>
#include "linux/siphash.h"
#include "linux/xxhash.h"
#include "linux/jhash.h"

/* 520 = 512 data + 8: wrap_{,h}siphash reads buf[off..off+n] with off<=7,
 * so a 512-element array would be indexed out of bounds at n=512. */
static uint8_t buf[520] __aligned(16);
static uint32_t wbuf[128];

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static size_t buf_len(int64_t len)
{
	uint64_t n = len < 0 ? 0ULL - (uint64_t)len : (uint64_t)len;
	return n % 513;
}

static void fill(uint64_t seed, size_t n)
{
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < n; i++)
		buf[i] = (uint8_t)xs64();
}

static void fill_words(uint64_t seed, size_t n)
{
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < n; i++)
		wbuf[i] = (uint32_t)xs64();
}

/* ---- siphash ---- */

uint64_t wrap_siphash(uint64_t k0, uint64_t k1, int64_t off,
		      uint64_t seed, int64_t len)
{
	siphash_key_t key = {{k0, k1}};
	size_t n = buf_len(len);
	/* Seed through buf[n..n+off): the unaligned dispatch hashes those
	 * bytes too, and leaving them stale made results call-order
	 * dependent. */
	fill(seed, n + ((uint64_t)off & 7));
	return siphash(buf + (off & 7), n, &key);
}

uint64_t wrap_siphash_1u64(uint64_t a, uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_1u64(a, &key);
}

uint64_t wrap_siphash_2u64(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_2u64(a, b, &key);
}

uint64_t wrap_siphash_3u64(uint64_t a, uint64_t b, uint64_t c,
			   uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_3u64(a, b, c, &key);
}

uint64_t wrap_siphash_4u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d,
			   uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_4u64(a, b, c, d, &key);
}

uint64_t wrap_siphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_1u32((u32)a, &key);
}

uint64_t wrap_siphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_2u32((u32)a, (u32)b, &key);
}

uint64_t wrap_siphash_3u32(uint64_t a, uint64_t b, uint64_t c,
			   uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_3u32((u32)a, (u32)b, (u32)c, &key);
}

uint64_t wrap_siphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d,
			   uint64_t k0, uint64_t k1)
{
	siphash_key_t key = {{k0, k1}};
	return siphash_4u32((u32)a, (u32)b, (u32)c, (u32)d, &key);
}

/* ---- hsiphash (SipHash1-3 on LP64) ---- */

uint64_t wrap_hsiphash(uint64_t k0, uint64_t k1, int64_t off,
		       uint64_t seed, int64_t len)
{
	hsiphash_key_t key = {{k0, k1}};
	size_t n = buf_len(len);
	fill(seed, n + ((uint64_t)off & 7));
	return hsiphash(buf + (off & 7), n, &key);
}

uint64_t wrap_hsiphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)
{
	hsiphash_key_t key = {{k0, k1}};
	return hsiphash_1u32((u32)a, &key);
}

uint64_t wrap_hsiphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)
{
	hsiphash_key_t key = {{k0, k1}};
	return hsiphash_2u32((u32)a, (u32)b, &key);
}

uint64_t wrap_hsiphash_3u32(uint64_t a, uint64_t b, uint64_t c,
			    uint64_t k0, uint64_t k1)
{
	hsiphash_key_t key = {{k0, k1}};
	return hsiphash_3u32((u32)a, (u32)b, (u32)c, &key);
}

uint64_t wrap_hsiphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d,
			    uint64_t k0, uint64_t k1)
{
	hsiphash_key_t key = {{k0, k1}};
	return hsiphash_4u32((u32)a, (u32)b, (u32)c, (u32)d, &key);
}

/* ---- xxhash ---- */

uint64_t wrap_xxh32(uint64_t seed, uint64_t bseed, int64_t len)
{
	size_t n = buf_len(len);
	fill(bseed, n);
	return xxh32(buf, n, (uint32_t)seed);
}

uint64_t wrap_xxh64(uint64_t seed, uint64_t bseed, int64_t len)
{
	size_t n = buf_len(len);
	fill(bseed, n);
	return xxh64(buf, n, seed);
}

/* Drives the streaming API: reset, feed the seeded buffer in
 * chunk-sized update calls (chunk mapped to 1..64), digest. */
uint64_t wrap_xxh64_stream(uint64_t seed, uint64_t bseed, int64_t len,
			   int64_t chunk)
{
	struct xxh64_state st;
	size_t n = buf_len(len);
	uint64_t c64 = chunk < 0 ? 0ULL - (uint64_t)chunk : (uint64_t)chunk;
	size_t c = (size_t)(c64 % 64) + 1;
	size_t i = 0;

	fill(bseed, n);
	xxh64_reset(&st, seed);
	while (i < n) {
		size_t step = n - i < c ? n - i : c;
		if (xxh64_update(&st, buf + i, step))
			return (uint64_t)-1;
		i += step;
	}
	return xxh64_digest(&st);
}

/* ---- jhash (header-only inline functions) ---- */

uint64_t wrap_jhash(uint64_t initval, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);
	fill(seed, n);
	return jhash(buf, (u32)n, (u32)initval);
}

uint64_t wrap_jhash2(uint64_t initval, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len) % 129;
	fill_words(seed, n);
	return jhash2(wbuf, (u32)n, (u32)initval);
}

uint64_t wrap_jhash_1word(uint64_t a, uint64_t initval)
{
	return jhash_1word((u32)a, (u32)initval);
}

uint64_t wrap_jhash_2words(uint64_t a, uint64_t b, uint64_t initval)
{
	return jhash_2words((u32)a, (u32)b, (u32)initval);
}

uint64_t wrap_jhash_3words(uint64_t a, uint64_t b, uint64_t c,
			   uint64_t initval)
{
	return jhash_3words((u32)a, (u32)b, (u32)c, (u32)initval);
}
