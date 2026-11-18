/* Scalar/digest-fold wrappers around lib/crypto/{sha1,sha256,sha512}.c
 * for the rust-anything differential port, same convention as
 * ports/lib-string and ports/lib-hash.
 *
 * Every export returns a u64: digest bytes are folded through fnv1a and
 * mixed with the input descriptors via mix().  The ctypes bridge only
 * carries scalars; a u64 fold keeps both sides inside
 * #![forbid(unsafe_code)] — a char* hex return would need raw pointers
 * on the Rust side.  Fold collisions are ~2^-64 per case pair.  The
 * inspectable ground truth lives in selftest.c (canonical vectors), so a
 * diff mismatch can always be reproduced against published test values.
 *
 * Determinism: all content comes from a seeded xorshift64.  record forks
 * per case so the static buffers start fresh, and every wrapper re-fills
 * what it reads.
 *
 * Visibility: the crypto headers are included inside a hidden-visibility
 * pragma so every kernel API call inside this shared object binds to our
 * definitions instead of going through the interposable global scope —
 * same interposition hazard class as the lib-string wave (other crypto
 * libraries export names like sha256_init/sha1_update).  libc string.h is
 * included first at default visibility so its declarations stay
 * interposable-free but resolvable (this unit defines no libc symbol).
 */
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <string.h>
#include <sys/types.h>
#include "linux/compiler.h"	/* __aligned */
#include "linux/kernel.h"	/* ARRAY_SIZE */

#pragma GCC visibility push(hidden)
#include <crypto/sha1.h>
#include <crypto/sha2.h>
#include <crypto/hmac.h>
#pragma GCC visibility pop

#define BSZ 1536	/* message buffers: len is mapped into 0..1500 */
#define KSZ 256		/* HMAC key buffer: key_len is mapped into 0..200 */
#define PSZ 256		/* pre-feed buffer for sha256_finup_2x ctx state */

static u8 buf[BSZ] __aligned(16);
static u8 buf2[BSZ] __aligned(16);
static u8 kbuf[KSZ] __aligned(16);
static u8 pbuf[PSZ] __aligned(16);

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static uint64_t uabs64(int64_t v)
{
	return v < 0 ? 0ULL - (uint64_t)v : (uint64_t)v;
}

static size_t blen(int64_t v, size_t cap)
{
	return (size_t)(uabs64(v) % cap);
}

static uint64_t fnv1a_h(uint64_t h, const void *p, size_t n)
{
	const uint8_t *b = p;
	size_t i;
	for (i = 0; i < n; i++) {
		h ^= b[i];
		h *= 1099511628211ULL;
	}
	return h;
}

static uint64_t fnv1a(const void *p, size_t n)
{
	return fnv1a_h(14695981039346656037ULL, p, n);
}

/* Result mixer: every recorded output depends on all three inputs. */
static uint64_t mix(uint64_t a, uint64_t b, uint64_t c)
{
	uint64_t h = a * 0x9E3779B97F4A7C15ULL;
	h = (h ^ b) * 0xC2B2AE3D27D4EB4FULL;
	h = (h ^ c) * 0x165667B19E3779F9ULL;
	h ^= h >> 32;
	return h;
}

/* Deterministic raw bytes (may contain zeros). */
static void fill_raw(u8 *b, size_t n, uint64_t seed)
{
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < n; i++)
		b[i] = (u8)xs64();
}

/* Length selection.  mode&1 picks from a boundary table instead of the
 * uniform 0..1500 range: for the 64-byte-block digests (SHA-1/224/256)
 * the padding edge sits at 56 and the buffer seam at 64; for the
 * 128-byte-block digests (SHA-384/512) at 112/128.  The tables hit both
 * edges +/-1, whole-block multiples, and the buffer cap. */
static const size_t edge64[] = {
	0, 1, 2, 7, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129,
	191, 192, 256, 511, 512, 1024, 1500,
};
static const size_t edge128[] = {
	0, 1, 2, 7, 55, 56, 111, 112, 113, 119, 120, 127, 128, 129,
	239, 240, 255, 256, 257, 384, 1024, 1500,
};

static size_t pick_len(int64_t v, int64_t mode, const size_t *tab,
		       size_t ntab)
{
	if (mode & 1)
		return tab[blen(v, ntab)];
	return blen(v, BSZ - 35);	/* 0..1500 */
}

/* HMAC key lengths: includes values over each block size (64 and 128)
 * so the raw_key_len > blocksize key-hashing path is hit, plus 0. */
static const size_t klens[] = {
	0, 1, 16, 20, 32, 48, 55, 63, 64, 65, 100, 111, 127, 128, 129,
	160, 192, 200,
};

static size_t pick_klen(int64_t v, int64_t mode)
{
	if (mode & 2)
		return klens[blen(v, ARRAY_SIZE(klens))];
	return blen(v, KSZ - 55);	/* 0..200 */
}

/* Chunked-update step selection.  mode&8 cycles a seeded walk through
 * the boundary table (each update call gets the next size), mode&16
 * fixes a tiny 1..16 step, otherwise a fixed 1..160 step. */
static const size_t chunk_tab[] = {
	1, 2, 3, 7, 16, 31, 55, 56, 63, 64, 65, 111, 112, 127, 128, 129,
};

static size_t chunk_at(int64_t c, int64_t mode, size_t i)
{
	if (mode & 8)
		return chunk_tab[(blen(c, ARRAY_SIZE(chunk_tab)) + i) %
				 ARRAY_SIZE(chunk_tab)];
	if (mode & 16)
		return blen(c, 16) + 1;
	return blen(c, 160) + 1;
}

/* Context folds, member-by-member so both sides hash the same byte
 * sequence without reproducing C's padding.  Every public final() has
 * memzero_explicit()'d the ctx by this point, so the fold verifies
 * zeroization on both sides too. */
#define FOLD(h, m) fnv1a_h((h), &(m), sizeof(m))

static uint64_t fold_sha1_ctx(const struct sha1_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->state);
	h = FOLD(h, c->bytecount);
	h = FOLD(h, c->buf);
	return h;
}

static uint64_t fold__sha256_ctx(const struct __sha256_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->state);
	h = FOLD(h, c->bytecount);
	h = FOLD(h, c->buf);
	return h;
}

static uint64_t fold__sha512_ctx(const struct __sha512_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->state);
	h = FOLD(h, c->bytecount_lo);
	h = FOLD(h, c->bytecount_hi);
	h = FOLD(h, c->buf);
	return h;
}

static uint64_t fold_hmac_sha1_ctx(const struct hmac_sha1_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->sha_ctx.state);
	h = FOLD(h, c->sha_ctx.bytecount);
	h = FOLD(h, c->sha_ctx.buf);
	h = FOLD(h, c->ostate);
	return h;
}

static uint64_t fold__hmac_sha256_ctx(const struct __hmac_sha256_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->sha_ctx.state);
	h = FOLD(h, c->sha_ctx.bytecount);
	h = FOLD(h, c->sha_ctx.buf);
	h = FOLD(h, c->ostate);
	return h;
}

static uint64_t fold__hmac_sha512_ctx(const struct __hmac_sha512_ctx *c)
{
	uint64_t h = 14695981039346656037ULL;
	h = FOLD(h, c->sha_ctx.state);
	h = FOLD(h, c->sha_ctx.bytecount_lo);
	h = FOLD(h, c->sha_ctx.bytecount_hi);
	h = FOLD(h, c->sha_ctx.buf);
	h = FOLD(h, c->ostate);
	return h;
}

/* ---- one-shot digests -------------------------------------------------- */

uint64_t wrap_sha1(uint64_t seed, int64_t len, int64_t mode)
{
	u8 out[SHA1_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(buf, n, seed);
	sha1(buf, n, out);
	return mix(fnv1a(out, sizeof out), n, 0x1);
}

uint64_t wrap_sha224(uint64_t seed, int64_t len, int64_t mode)
{
	u8 out[SHA224_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(buf, n, seed);
	sha224(buf, n, out);
	return mix(fnv1a(out, sizeof out), n, 0x224);
}

uint64_t wrap_sha256(uint64_t seed, int64_t len, int64_t mode)
{
	u8 out[SHA256_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(buf, n, seed);
	sha256(buf, n, out);
	return mix(fnv1a(out, sizeof out), n, 0x256);
}

uint64_t wrap_sha384(uint64_t seed, int64_t len, int64_t mode)
{
	u8 out[SHA384_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	fill_raw(buf, n, seed);
	sha384(buf, n, out);
	return mix(fnv1a(out, sizeof out), n, 0x384);
}

uint64_t wrap_sha512(uint64_t seed, int64_t len, int64_t mode)
{
	u8 out[SHA512_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	fill_raw(buf, n, seed);
	sha512(buf, n, out);
	return mix(fnv1a(out, sizeof out), n, 0x512);
}

/* ---- streaming digests -------------------------------------------------- */
/* init + update in chunked calls (chunk sizes from `chunk`/`mode`) + final.
 * Buffer-boundary bugs live in the partial-block path; chunk_at() walks
 * sizes that cross it from both sides.  After final() the kernel has
 * memzero_explicit()'d the ctx, so folding it verifies zeroization too. */

uint64_t wrap_sha1_stream(uint64_t seed, int64_t len, int64_t chunk,
			  int64_t mode)
{
	struct sha1_ctx ctx;
	u8 out[SHA1_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(buf, n, seed);
	sha1_init(&ctx);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		sha1_update(&ctx, buf + i, c);
		i += c;
	}
	if (mode & 4)
		sha1_update(&ctx, buf, 0);
	sha1_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold_sha1_ctx(&ctx),
		   (n << 8) | k);
}

uint64_t wrap_sha224_stream(uint64_t seed, int64_t len, int64_t chunk,
			    int64_t mode)
{
	struct sha224_ctx ctx;
	u8 out[SHA224_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(buf, n, seed);
	sha224_init(&ctx);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		__sha256_update(&ctx.ctx, buf + i, c);
		i += c;
	}
	if (mode & 4)
		sha224_update(&ctx, buf, 0);
	sha224_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__sha256_ctx(&ctx.ctx),
		   (n << 8) | k);
}

uint64_t wrap_sha256_stream(uint64_t seed, int64_t len, int64_t chunk,
			    int64_t mode)
{
	struct sha256_ctx ctx;
	u8 out[SHA256_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(buf, n, seed);
	sha256_init(&ctx);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		if (mode & 2)
			sha256_update(&ctx, buf + i, c);
		else
			__sha256_update(&ctx.ctx, buf + i, c);
		i += c;
	}
	if (mode & 4)
		__sha256_update(&ctx.ctx, buf, 0);
	sha256_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__sha256_ctx(&ctx.ctx),
		   (n << 8) | k);
}

uint64_t wrap_sha384_stream(uint64_t seed, int64_t len, int64_t chunk,
			    int64_t mode)
{
	struct sha384_ctx ctx;
	u8 out[SHA384_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(buf, n, seed);
	sha384_init(&ctx);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		__sha512_update(&ctx.ctx, buf + i, c);
		i += c;
	}
	if (mode & 4)
		sha384_update(&ctx, buf, 0);
	sha384_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__sha512_ctx(&ctx.ctx),
		   (n << 8) | k);
}

uint64_t wrap_sha512_stream(uint64_t seed, int64_t len, int64_t chunk,
			    int64_t mode)
{
	struct sha512_ctx ctx;
	u8 out[SHA512_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(buf, n, seed);
	sha512_init(&ctx);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		if (mode & 2)
			sha512_update(&ctx, buf + i, c);
		else
			__sha512_update(&ctx.ctx, buf + i, c);
		i += c;
	}
	if (mode & 4)
		__sha512_update(&ctx.ctx, buf, 0);
	sha512_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__sha512_ctx(&ctx.ctx),
		   (n << 8) | k);
}

/* ---- sha256_finup_2x ---------------------------------------------------- */
/* mode&1 passes ctx=NULL (the default-initial-context path); otherwise a
 * real ctx is used, pre-fed `pre` bytes when mode&2 (exercises the
 * sequential fallback on a ctx with a live partial block).  Also folds
 * sha256_finup_2x_is_optimized() — always false on the generic path. */

uint64_t wrap_sha256_finup_2x(uint64_t s1, uint64_t s2, int64_t len,
			      int64_t pre, int64_t mode)
{
	struct sha256_ctx ctx;
	const struct sha256_ctx *pctx = &ctx;
	u8 o1[SHA256_DIGEST_SIZE], o2[SHA256_DIGEST_SIZE];
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(buf, n, s1);
	fill_raw(buf2, n, s2);
	sha256_init(&ctx);
	if (mode & 1)
		pctx = NULL;
	else if (mode & 2) {
		size_t pl = blen(pre, PSZ);
		fill_raw(pbuf, pl, s1 ^ s2);
		sha256_update(&ctx, pbuf, pl);
	}
	sha256_finup_2x(pctx, buf, buf2, n, o1, o2);
	return mix(fnv1a(o1, sizeof o1), fnv1a(o2, sizeof o2),
		   (n << 2) | (uint64_t)sha256_finup_2x_is_optimized());
}

/* ---- HMAC: prepared-key streaming -------------------------------------- */

uint64_t wrap_hmac_sha1_prepared(uint64_t kseed, int64_t klen,
				 uint64_t seed, int64_t len, int64_t chunk,
				 int64_t mode)
{
	struct hmac_sha1_key key;
	struct hmac_sha1_ctx ctx;
	u8 out[SHA1_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha1_preparekey(&key, kbuf, kl);
	hmac_sha1_init(&ctx, &key);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha1_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha1_final(&ctx, out);
	/* final() zeroizes ctx; fold both outputs plus the dead ctx. */
	return mix(fnv1a(out, sizeof out), fold_hmac_sha1_ctx(&ctx),
		   mix(n, kl, 0x1));
}

uint64_t wrap_hmac_sha224_prepared(uint64_t kseed, int64_t klen,
				   uint64_t seed, int64_t len, int64_t chunk,
				   int64_t mode)
{
	struct hmac_sha224_key key;
	struct hmac_sha224_ctx ctx;
	u8 out[SHA224_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha224_preparekey(&key, kbuf, kl);
	hmac_sha224_init(&ctx, &key);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha224_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha224_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha256_ctx(&ctx.ctx),
		   mix(n, kl, 0x224));
}

uint64_t wrap_hmac_sha256_prepared(uint64_t kseed, int64_t klen,
				   uint64_t seed, int64_t len, int64_t chunk,
				   int64_t mode)
{
	struct hmac_sha256_key key;
	struct hmac_sha256_ctx ctx;
	u8 out[SHA256_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha256_preparekey(&key, kbuf, kl);
	hmac_sha256_init(&ctx, &key);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha256_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha256_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha256_ctx(&ctx.ctx),
		   mix(n, kl, 0x256));
}

uint64_t wrap_hmac_sha384_prepared(uint64_t kseed, int64_t klen,
				   uint64_t seed, int64_t len, int64_t chunk,
				   int64_t mode)
{
	struct hmac_sha384_key key;
	struct hmac_sha384_ctx ctx;
	u8 out[SHA384_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha384_preparekey(&key, kbuf, kl);
	hmac_sha384_init(&ctx, &key);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha384_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha384_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha512_ctx(&ctx.ctx),
		   mix(n, kl, 0x384));
}

uint64_t wrap_hmac_sha512_prepared(uint64_t kseed, int64_t klen,
				   uint64_t seed, int64_t len, int64_t chunk,
				   int64_t mode)
{
	struct hmac_sha512_key key;
	struct hmac_sha512_ctx ctx;
	u8 out[SHA512_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha512_preparekey(&key, kbuf, kl);
	hmac_sha512_init(&ctx, &key);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha512_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha512_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha512_ctx(&ctx.ctx),
		   mix(n, kl, 0x512));
}

/* ---- HMAC: raw-key streaming (init_usingrawkey) ------------------------- */

uint64_t wrap_hmac_sha1_rawkey(uint64_t kseed, int64_t klen,
			       uint64_t seed, int64_t len, int64_t chunk,
			       int64_t mode)
{
	struct hmac_sha1_ctx ctx;
	u8 out[SHA1_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha1_init_usingrawkey(&ctx, kbuf, kl);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha1_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha1_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold_hmac_sha1_ctx(&ctx),
		   mix(n, kl, 0x101));
}

uint64_t wrap_hmac_sha224_rawkey(uint64_t kseed, int64_t klen,
				 uint64_t seed, int64_t len, int64_t chunk,
				 int64_t mode)
{
	struct hmac_sha224_ctx ctx;
	u8 out[SHA224_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha224_init_usingrawkey(&ctx, kbuf, kl);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha224_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha224_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha256_ctx(&ctx.ctx),
		   mix(n, kl, 0x1224));
}

uint64_t wrap_hmac_sha256_rawkey(uint64_t kseed, int64_t klen,
				 uint64_t seed, int64_t len, int64_t chunk,
				 int64_t mode)
{
	struct hmac_sha256_ctx ctx;
	u8 out[SHA256_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha256_init_usingrawkey(&ctx, kbuf, kl);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha256_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha256_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha256_ctx(&ctx.ctx),
		   mix(n, kl, 0x1256));
}

uint64_t wrap_hmac_sha384_rawkey(uint64_t kseed, int64_t klen,
				 uint64_t seed, int64_t len, int64_t chunk,
				 int64_t mode)
{
	struct hmac_sha384_ctx ctx;
	u8 out[SHA384_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha384_init_usingrawkey(&ctx, kbuf, kl);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha384_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha384_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha512_ctx(&ctx.ctx),
		   mix(n, kl, 0x1384));
}

uint64_t wrap_hmac_sha512_rawkey(uint64_t kseed, int64_t klen,
				 uint64_t seed, int64_t len, int64_t chunk,
				 int64_t mode)
{
	struct hmac_sha512_ctx ctx;
	u8 out[SHA512_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	size_t i = 0, k = 0;
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	hmac_sha512_init_usingrawkey(&ctx, kbuf, kl);
	while (i < n) {
		size_t c = chunk_at(chunk, mode, k++);
		if (c > n - i)
			c = n - i;
		hmac_sha512_update(&ctx, buf + i, c);
		i += c;
	}
	hmac_sha512_final(&ctx, out);
	return mix(fnv1a(out, sizeof out), fold__hmac_sha512_ctx(&ctx.ctx),
		   mix(n, kl, 0x1512));
}

/* ---- HMAC: one-shot (prepared-key and usingrawkey) ---------------------- */

uint64_t wrap_hmac_sha1_oneshot(uint64_t kseed, int64_t klen,
				uint64_t seed, int64_t len, int64_t mode)
{
	struct hmac_sha1_key key;
	u8 out[SHA1_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	if (mode & 8) {
		hmac_sha1_usingrawkey(kbuf, kl, buf, n, out);
	} else {
		hmac_sha1_preparekey(&key, kbuf, kl);
		hmac_sha1(&key, buf, n, out);
	}
	return mix(fnv1a(out, sizeof out), mix(n, kl, mode & 8), 0x201);
}

uint64_t wrap_hmac_sha224_oneshot(uint64_t kseed, int64_t klen,
				  uint64_t seed, int64_t len, int64_t mode)
{
	struct hmac_sha224_key key;
	u8 out[SHA224_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	if (mode & 8) {
		hmac_sha224_usingrawkey(kbuf, kl, buf, n, out);
	} else {
		hmac_sha224_preparekey(&key, kbuf, kl);
		hmac_sha224(&key, buf, n, out);
	}
	return mix(fnv1a(out, sizeof out), mix(n, kl, mode & 8), 0x2224);
}

uint64_t wrap_hmac_sha256_oneshot(uint64_t kseed, int64_t klen,
				  uint64_t seed, int64_t len, int64_t mode)
{
	struct hmac_sha256_key key;
	u8 out[SHA256_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge64, ARRAY_SIZE(edge64));
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	if (mode & 8) {
		hmac_sha256_usingrawkey(kbuf, kl, buf, n, out);
	} else {
		hmac_sha256_preparekey(&key, kbuf, kl);
		hmac_sha256(&key, buf, n, out);
	}
	return mix(fnv1a(out, sizeof out), mix(n, kl, mode & 8), 0x2256);
}

uint64_t wrap_hmac_sha384_oneshot(uint64_t kseed, int64_t klen,
				  uint64_t seed, int64_t len, int64_t mode)
{
	struct hmac_sha384_key key;
	u8 out[SHA384_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	if (mode & 8) {
		hmac_sha384_usingrawkey(kbuf, kl, buf, n, out);
	} else {
		hmac_sha384_preparekey(&key, kbuf, kl);
		hmac_sha384(&key, buf, n, out);
	}
	return mix(fnv1a(out, sizeof out), mix(n, kl, mode & 8), 0x2384);
}

uint64_t wrap_hmac_sha512_oneshot(uint64_t kseed, int64_t klen,
				  uint64_t seed, int64_t len, int64_t mode)
{
	struct hmac_sha512_key key;
	u8 out[SHA512_DIGEST_SIZE];
	size_t kl = pick_klen(klen, mode);
	size_t n = pick_len(len, mode, edge128, ARRAY_SIZE(edge128));
	fill_raw(kbuf, kl, kseed);
	fill_raw(buf, n, seed);
	if (mode & 8) {
		hmac_sha512_usingrawkey(kbuf, kl, buf, n, out);
	} else {
		hmac_sha512_preparekey(&key, kbuf, kl);
		hmac_sha512(&key, buf, n, out);
	}
	return mix(fnv1a(out, sizeof out), mix(n, kl, mode & 8), 0x2512);
}

/* ---- *_zero_message_hash constants -------------------------------------- */
/* The kernel exports sha*_zero_message_hash from crypto/sha*.c (outside
 * this unit; the headers only declare them extern).  These are the same
 * published values verbatim; the wrapper folds each constant, the
 * computed empty-message digest, and whether they agree — so the Rust
 * port must produce both identical constants and identical digests. */

static const u8 ref_sha1_empty[SHA1_DIGEST_SIZE] = {
	0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d,
	0x32, 0x55, 0xbf, 0xef, 0x95, 0x60, 0x18, 0x90,
	0xaf, 0xd8, 0x07, 0x09
};
static const u8 ref_sha224_empty[SHA224_DIGEST_SIZE] = {
	0xd1, 0x4a, 0x02, 0x8c, 0x2a, 0x3a, 0x2b, 0xc9, 0x47,
	0x61, 0x02, 0xbb, 0x28, 0x82, 0x34, 0xc4, 0x15, 0xa2,
	0xb0, 0x1f, 0x82, 0x8e, 0xa6, 0x2a, 0xc5, 0xb3, 0xe4,
	0x2f
};
static const u8 ref_sha256_empty[SHA256_DIGEST_SIZE] = {
	0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
	0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
	0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
	0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55
};
static const u8 ref_sha384_empty[SHA384_DIGEST_SIZE] = {
	0x38, 0xb0, 0x60, 0xa7, 0x51, 0xac, 0x96, 0x38,
	0x4c, 0xd9, 0x32, 0x7e, 0xb1, 0xb1, 0xe3, 0x6a,
	0x21, 0xfd, 0xb7, 0x11, 0x14, 0xbe, 0x07, 0x43,
	0x4c, 0x0c, 0xc7, 0xbf, 0x63, 0xf6, 0xe1, 0xda,
	0x27, 0x4e, 0xde, 0xbf, 0xe7, 0x6f, 0x65, 0xfb,
	0xd5, 0x1a, 0xd2, 0xf1, 0x48, 0x98, 0xb9, 0x5b
};
static const u8 ref_sha512_empty[SHA512_DIGEST_SIZE] = {
	0xcf, 0x83, 0xe1, 0x35, 0x7e, 0xef, 0xb8, 0xbd,
	0xf1, 0x54, 0x28, 0x50, 0xd6, 0x6d, 0x80, 0x07,
	0xd6, 0x20, 0xe4, 0x05, 0x0b, 0x57, 0x15, 0xdc,
	0x83, 0xf4, 0xa9, 0x21, 0xd3, 0x6c, 0xe9, 0xce,
	0x47, 0xd0, 0xd1, 0x3c, 0x5d, 0x85, 0xf2, 0xb0,
	0xff, 0x83, 0x18, 0xd2, 0x87, 0x7e, 0xec, 0x2f,
	0x63, 0xb9, 0x31, 0xbd, 0x47, 0x41, 0x7a, 0x81,
	0xa5, 0x38, 0x32, 0x7a, 0xf9, 0x27, 0xda, 0x3e
};

uint64_t wrap_zero_message_hashes(uint64_t mode)
{
	u8 d[SHA512_DIGEST_SIZE];
	uint64_t acc = (uint64_t)mode;
	sha1(buf, 0, d);
	acc = mix(acc, fnv1a(d, SHA1_DIGEST_SIZE),
		  fnv1a(ref_sha1_empty, SHA1_DIGEST_SIZE) +
		  (memcmp(d, ref_sha1_empty, SHA1_DIGEST_SIZE) == 0));
	sha224(buf, 0, d);
	acc = mix(acc, fnv1a(d, SHA224_DIGEST_SIZE),
		  fnv1a(ref_sha224_empty, SHA224_DIGEST_SIZE) +
		  (memcmp(d, ref_sha224_empty, SHA224_DIGEST_SIZE) == 0));
	sha256(buf, 0, d);
	acc = mix(acc, fnv1a(d, SHA256_DIGEST_SIZE),
		  fnv1a(ref_sha256_empty, SHA256_DIGEST_SIZE) +
		  (memcmp(d, ref_sha256_empty, SHA256_DIGEST_SIZE) == 0));
	sha384(buf, 0, d);
	acc = mix(acc, fnv1a(d, SHA384_DIGEST_SIZE),
		  fnv1a(ref_sha384_empty, SHA384_DIGEST_SIZE) +
		  (memcmp(d, ref_sha384_empty, SHA384_DIGEST_SIZE) == 0));
	sha512(buf, 0, d);
	acc = mix(acc, fnv1a(d, SHA512_DIGEST_SIZE),
		  fnv1a(ref_sha512_empty, SHA512_DIGEST_SIZE) +
		  (memcmp(d, ref_sha512_empty, SHA512_DIGEST_SIZE) == 0));
	return acc;
}
