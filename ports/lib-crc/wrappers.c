/* Scalar wrappers: drive the pointer-arg CRC functions on deterministic
 * seeded buffers and return the scalar CRC. The record/diff pipeline
 * verifies the real functions transitively, including the generated
 * lookup tables. crc4() is scalar and bridged directly (no wrapper). */

#include <linux/types.h>
#include <stdint.h>

extern uint32_t crc32_le(uint32_t crc, const void *p, size_t len);
extern uint32_t crc32_be(uint32_t crc, const void *p, size_t len);
extern uint32_t crc32c(uint32_t crc, const void *p, size_t len);
extern uint64_t crc64_be(uint64_t crc, const void *p, size_t len);
extern uint64_t crc64_nvme(uint64_t crc, const void *p, size_t len);
extern uint16_t crc16(uint16_t crc, const uint8_t *p, size_t len);
extern uint16_t crc_ccitt(uint16_t crc, const uint8_t *buffer, size_t len);
extern uint16_t crc_itu_t(uint16_t crc, const uint8_t *buffer, size_t len);
extern uint8_t crc7_be(uint8_t crc, const uint8_t *buffer, size_t len);
extern uint16_t crc_t10dif_update(uint16_t crc, const uint8_t *p, size_t len);
extern void crc8_populate_msb(uint8_t table[256], uint8_t polynomial);
extern void crc8_populate_lsb(uint8_t table[256], uint8_t polynomial);
extern uint8_t crc8(const uint8_t table[256], const uint8_t *pdata,
		    size_t nbytes, uint8_t crc);

#define BUFSZ 512

static uint8_t buf[BUFSZ];

/* xorshift64: same stream in the C reference and the Rust port. */
static uint64_t rng_state;

static uint64_t xorshift(void)
{
	uint64_t x = rng_state;

	x ^= x << 13;
	x ^= x >> 7;
	x ^= x << 17;
	return rng_state = x;
}

static size_t buf_len(int64_t len)
{
	/* Defined-behavior absolute value; len%513 sweeps 0..512 covering
	 * the len=0 no-op, tiny buffers, and multi-word tails. */
	return (size_t)(len < 0 ? 0ULL - (uint64_t)len : (uint64_t)len) %
	       (BUFSZ + 1);
}

static void fill(uint64_t seed)
{
	size_t i;

	rng_state = seed | 1;
	for (i = 0; i < BUFSZ; i++)
		buf[i] = (uint8_t)xorshift();
}

/* crc32_le(crc, buf, len%513) */
uint64_t wrap_crc32_le(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc32_le((uint32_t)crc, buf, n);
}

/* crc32_be(crc, buf, len%513) */
uint64_t wrap_crc32_be(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc32_be((uint32_t)crc, buf, n);
}

/* crc32c(crc, buf, len%513) — Castagnoli polynomial table. */
uint64_t wrap_crc32c(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc32c((uint32_t)crc, buf, n);
}

/* crc64_be(crc, buf, len%513) — ECMA-182 table. */
uint64_t wrap_crc64_be(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc64_be(crc, buf, n);
}

/* crc64_nvme(crc, buf, len%513) — NVMe reflected table, ~crc in/out. */
uint64_t wrap_crc64_nvme(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc64_nvme(crc, buf, n);
}

/* crc16(crc, buf, len%513) */
uint64_t wrap_crc16(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc16((uint16_t)crc, buf, n);
}

/* crc_ccitt(crc, buf, len%513) */
uint64_t wrap_crc_ccitt(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc_ccitt((uint16_t)crc, buf, n);
}

/* crc_itu_t(crc, buf, len%513) */
uint64_t wrap_crc_itu_t(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc_itu_t((uint16_t)crc, buf, n);
}

/* crc7_be(crc, buf, len%513) */
uint64_t wrap_crc7_be(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc7_be((uint8_t)crc, buf, n);
}

/* crc_t10dif_update(crc, buf, len%513) */
uint64_t wrap_t10dif(uint64_t crc, uint64_t seed, int64_t len)
{
	size_t n = buf_len(len);

	fill(seed);
	return crc_t10dif_update((uint16_t)crc, buf, n);
}

/* crc8: populate_msb + populate_lsb on the same polynomial, then crc8()
 * over the seeded buffer with each table; checksum combines table
 * generation and the running CRC transitively. */
uint64_t wrap_crc8(uint64_t poly, uint64_t seed, int64_t len)
{
	uint8_t t_msb[256], t_lsb[256];
	size_t n = buf_len(len);
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	uint8_t c;
	size_t i;

	crc8_populate_msb(t_msb, (uint8_t)poly);
	crc8_populate_lsb(t_lsb, (uint8_t)poly);
	fill(seed);
	c = crc8(t_msb, buf, n, 0);
	h = (h ^ c) * 0x100000001b3ULL;
	c = crc8(t_lsb, buf, n, 0xff);
	h = (h ^ c) * 0x100000001b3ULL;
	for (i = 0; i < 8; i++)
		h = (h ^ t_msb[i * 31]) * 0x100000001b3ULL;
	for (i = 0; i < 8; i++)
		h = (h ^ t_lsb[i * 29 + 7]) * 0x100000001b3ULL;
	return h;
}
