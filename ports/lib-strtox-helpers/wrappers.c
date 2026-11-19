/* Scalar wrappers around lib/kstrtox.c, lib/hexdump.c and
 * lib/string_helpers.c for the rust-anything differential port, same
 * convention as ports/lib-string and ports/lib-idr-radix.
 *
 * Every export returns a scalar: pointer-returning functions yield offsets
 * into the seeded buffer (-1 for NULL); buffer-mutating functions fold the
 * resulting buffer through fnv1a so the whole output is observable; the
 * scalars and checksum are folded together with mix().
 *
 * Determinism: all content comes from a seeded xorshift64.  record forks
 * per case so the static buffers start fresh, but every wrapper re-fills
 * what it reads anyway.  print_hex_dump()'s %p output is canonicalized to
 * PCANON + offset into hbuf by the printk() shim so the captured log is
 * identical across the C and Rust address spaces.
 */
#include <stdint.h>
#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <linux/types.h>
#include <linux/errno.h>
#include <linux/kernel.h>
#include <linux/string.h>
#include <linux/string_helpers.h>
#include <linux/kstrtox.h>
#include <linux/hex.h>
#include <linux/slab.h>
#include <linux/device.h>
#include <linux/fs.h>
#include <linux/mm.h>
#include <linux/err.h>

/* The byte-identical units under test. */
#include "kstrtox.h"	/* lib/kstrtox.h: _parse_integer* internals */

#define SSZ 640		/* string buffers: body bytes + forced NUL + tail */
#define DSZ 1280	/* destination buffer for mutating ops */
#define HBUF 96		/* hexdump source blob (3 full 32-byte rows) */
#define LB 256		/* hex_dump_to_buffer linebuf */
#define KLOG 8192	/* captured printk output */
#define NSTR 128	/* kstrtox input string */

static char sbuf[SSZ] __aligned(16);
static char sbuf2[SSZ] __aligned(16);
static char dbuf[DSZ] __aligned(16);
static char nbuf[NSTR] __aligned(16);
static uint8_t hbuf[HBUF] __aligned(16);
static unsigned char linebuf[LB] __aligned(16);
static char klog[KLOG] __aligned(16);
static size_t klog_len;

/* Canonical base for %p canonicalization inside printk(). */
#define PCANON 0xffff888000000000ULL

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

static uint64_t fnv1a(const void *p, size_t n)
{
	const uint8_t *b = p;
	uint64_t h = 14695981039346656037ULL;
	size_t i;
	for (i = 0; i < n; i++) {
		h ^= b[i];
		h *= 1099511628211ULL;
	}
	return h;
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
static void fill_raw(void *b, size_t n, uint64_t seed)
{
	uint8_t *p = b;
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < n; i++)
		p[i] = (uint8_t)xs64();
}

/* Build a string in the first SSZ bytes of b: n non-zero body bytes + a
 * forced NUL at n, over a deterministic raw tail.  mode&1 plants an extra
 * NUL at a seeded position inside the body; mode&2 fills with 'A'+x%58
 * (letter-ish bytes) instead of x|1. */
static size_t make_str(char *b, size_t n, uint64_t seed, int mode)
{
	size_t i;
	uint64_t x;
	fill_raw(b, SSZ, seed);
	rng_state = (seed ^ 0x5bd1e9959e3779b9ULL) | 1;
	for (i = 0; i < n; i++) {
		x = xs64();
		b[i] = (mode & 2) ? (char)('A' + x % 58) : (char)(x | 1);
	}
	if ((mode & 1) && n)
		b[blen((int64_t)xs64(), n)] = 0;
	b[n] = 0;
	return n;
}

/* ---- unreachable dependency stubs -----------------------------------------
 * The units under test reference these symbols in wrappers never call; they
 * exist only to satisfy the linker. */

int get_options(const char *str, int nints, int *ints)
{
	(void)str; (void)nints; (void)ints;
	return 0;
}

int get_cmdline(struct task_struct *task, char *buffer, int buflen)
{
	(void)task; (void)buffer; (void)buflen;
	return -ENOSYS;
}

char *file_path(struct file *filp, char *buf, int buflen)
{
	(void)filp; (void)buf; (void)buflen;
	return ERR_PTR(-ENOSYS);
}

char *kasprintf(gfp_t gfp, const char *fmt, ...)
{
	(void)gfp; (void)fmt;
	return NULL;
}

void *devres_alloc(dr_release_t release, size_t size, gfp_t gfp)
{
	(void)release; (void)size; (void)gfp;
	return NULL;
}

void devres_free(void *res)
{
	(void)res;
}

void devres_add(struct device *dev, void *res)
{
	(void)dev; (void)res;
}

/* ---- printk shim ----------------------------------------------------------
 * Captures into klog; %p is canonicalized to PCANON + offset into hbuf so
 * the emitted text is identical on the C and Rust sides.  Only the formats
 * print_hex_dump() can emit occur: %s, %p and %x-family conversions. */

static void kemit(char c)
{
	if (klog_len < sizeof(klog))
		klog[klog_len++] = (char)c;
}

static void kemits(const char *s)
{
	while (*s)
		kemit(*s++);
}

int printk(const char *fmt, ...)
{
	va_list ap;
	char spec[24];
	char num[32];

	va_start(ap, fmt);
	for (; *fmt; fmt++) {
		const char *f;
		size_t sl;
		char conv;
		unsigned long long v;

		if (*fmt != '%') {
			kemit(*fmt);
			continue;
		}
		fmt++;
		if (*fmt == 's') {
			kemits(va_arg(ap, const char *));
			continue;
		}
		if (*fmt == 'p') {
			uintptr_t off = (uintptr_t)
				((const char *)va_arg(ap, const void *) -
				 (const char *)hbuf);
			snprintf(num, sizeof num, "%016llx",
				 (unsigned long long)(PCANON + off));
			kemits(num);
			continue;
		}
		/* numeric conversion: copy the spec verbatim [%][flags][width]
		 * [.prec][ll|l]x and let snprintf format it. */
		f = fmt;
		while (*f && strchr("0123456789.*-+ #", *f))
			f++;
		if (f[0] == 'l' && f[1] == 'l')
			f += 2;
		else if (f[0] == 'l')
			f++;
		conv = *f;
		sl = (size_t)(f - fmt + 2);
		if (sl >= sizeof(spec))
			sl = sizeof(spec) - 1;
		spec[0] = '%';
		memcpy(spec + 1, fmt, sl - 2);
		spec[sl - 1] = conv;
		spec[sl] = '\0';
		if (strstr(spec, "ll")) {
			v = va_arg(ap, unsigned long long);
			snprintf(num, sizeof num, spec, v);
		} else {
			v = va_arg(ap, unsigned);
			snprintf(num, sizeof num, spec, (unsigned)v);
		}
		kemits(num);
		fmt = (char *)f;
	}
	va_end(ap);
	return 0;
}

/* ---- numeric-string generators -------------------------------------------- */

/* Curated boundary cases: every overflow edge for every integer type,
 * all the EINVAL classes and the base-detection corners. */
static const char *const num_cases[] = {
	"", " ", "+", "-", "+0", "-0", "0", "00", "0007", "0x", "0x0",
	"0xg", "0XFF", "0x7fffffffffffffff", "0x8000000000000000",
	"0xffffffffffffffff", "0x10000000000000000", "0XaB",
	"18446744073709551615", "18446744073709551616",
	"18446744073709551617", "184467440737095516150",
	"-9223372036854775808", "9223372036854775807",
	"-9223372036854775809", "9223372036854775808",
	"4294967295", "4294967296", "2147483647", "2147483648",
	"-2147483648", "-2147483649",
	"65535", "65536", "255", "256", "127", "128",
	"-127", "-128", "-129", "32767", "32768", "-32768", "-32769",
	"01777777777777777777777", "02000000000000000000000",
	"01000000000000000000000", "07777",
	"1111111111111111111111111111111111111111111111111111111111111111",
	"1000000000000000000000000000000000000000000000000000000000000000",
	"11111111111111111111111111111111111111111111111111111111111111111",
	"99999999999999999999", "9999999999999999999", "0xabcdef0123456789",
	"123\n", "123\n\n", "123x", "123 ", " 123", "12 3", "1,234",
	"+42", "-42", "+0x1f", "-0x1f", "0b101", "0o17",
	"g", "1a2", "12a", "a12", "\t1", "1\t", "\n", "1\n2",
	"++1", "--1", "+-1", "-+1", "0x-1", "-0x-1",
};
#define NNUMCASES (sizeof(num_cases) / sizeof(num_cases[0]))

/* Build a NUL-terminated numeric-string candidate into nbuf.
 * mode&0x400 selects a curated boundary string (seed picks which);
 * otherwise a seeded string assembled from whitespace/sign/prefix/digit
 * pool/trailer features.  The rest of nbuf keeps deterministic raw bytes
 * so from_user truncation reads defined memory. */
static void mknum(uint64_t seed, int mode)
{
	size_t i = 0, ndig, j;
	uint64_t x;
	static const char pools[][24] = {
		"0123456789",
		"0123456789abcdef",
		"0123456789ABCDEF",
		"01234567",
		"01",
		"0123456789abcdefABCDEF",
		"89",
		"0123456789abcdefgG_",
	};

	if (mode & 0x400) {
		const char *s = num_cases[seed % NNUMCASES];
		fill_raw(nbuf, NSTR, seed ^ 0xfeed5eedULL);
		strcpy(nbuf, s);
		return;
	}

	fill_raw(nbuf, NSTR, seed ^ 0xfeed5eedULL);
	rng_state = seed | 1;

	/* leading space -> always EINVAL */
	if (mode & 1)
		nbuf[i++] = ' ';
	/* sign */
	switch ((mode >> 1) & 3) {
	case 1: nbuf[i++] = '+'; break;
	case 2: nbuf[i++] = '-'; break;
	case 3: nbuf[i++] = ' '; break;
	}
	/* base prefix */
	switch ((mode >> 3) & 3) {
	case 1: nbuf[i++] = '0'; nbuf[i++] = 'x'; break;
	case 2: nbuf[i++] = '0'; nbuf[i++] = 'X'; break;
	case 3: nbuf[i++] = '0'; break;
	}
	/* digits: 1..70 chars from a pool chosen by mode; 70 hex digits
	 * always reaches ULLONG_MAX overflow. */
	ndig = 1 + xs64() % 70;
	for (j = 0; j < ndig && i < NSTR - 4; j++) {
		const char *pool = pools[(mode >> 5) & 7];
		size_t pn = strlen(pool);
		x = xs64();
		nbuf[i++] = pool[x % pn];
	}
	if (mode & 0x100)
		nbuf[i++] = 'g';
	switch ((mode >> 9) & 3) {
	case 1: nbuf[i++] = '\n'; break;
	case 2: nbuf[i++] = '\n'; nbuf[i++] = 'x'; break;
	case 3: nbuf[i++] = ' '; break;
	}
	nbuf[i] = 0;
}

/* Build a scaled-decimal candidate into nbuf: int digits, optional '.',
 * fraction digits, optional trailing newline/junk. */
static void mkdec(uint64_t seed, int mode)
{
	size_t i = 0, n, j;
	uint64_t x;

	static const char *const dec_cases[] = {
		"0", "0.0", ".5", "1.", "1.5", "-3.141592", "+2.75",
		"123.45", "0.999", "18446744073709551615.0",
		"18446744073709551616", "9.223372036854775807",
		"18446744073709551615", "18446744073709551616",
		"0.00000000000000000001", "1.000000000000000000000",
		".", "-", "", "-.5", "1.2x", "1.2 ", "1.2\n", " 1.2",
		"99999999999999999999.99999", "1.234567890123456789012345",
		"18446744073709551.615", "18446744073709551.616",
		"0.9999", "-0.5", "-18446744073709551615",
		"-18446744073709551616",
	};

	if (mode & 0x400) {
		const char *s = dec_cases[seed %
			(sizeof(dec_cases) / sizeof(dec_cases[0]))];
		fill_raw(nbuf, NSTR, seed ^ 0xfeed5eedULL);
		strcpy(nbuf, s);
		return;
	}

	fill_raw(nbuf, NSTR, seed ^ 0xfeed5eedULL);
	rng_state = seed | 1;

	if (mode & 1)
		nbuf[i++] = ' ';
	switch ((mode >> 1) & 3) {
	case 1: nbuf[i++] = '+'; break;
	case 2: nbuf[i++] = '-'; break;
	}
	n = xs64() % 22;
	for (j = 0; j < n && i < NSTR - 6; j++)
		nbuf[i++] = (char)('0' + xs64() % 10);
	if (mode & 4)
		nbuf[i++] = '.';
	n = xs64() % 24;
	for (j = 0; j < n && i < NSTR - 4; j++)
		nbuf[i++] = (char)('0' + xs64() % 10);
	switch ((mode >> 3) & 3) {
	case 1: nbuf[i++] = '\n'; break;
	case 2: nbuf[i++] = 'x'; break;
	case 3: nbuf[i++] = ' '; break;
	}
	nbuf[i] = 0;
	(void)x;
}

/* Build a kstrtobool candidate: one or two letters plus trailers. */
static void mkbool(uint64_t seed, int mode)
{
	static const char pool[] = "EeYyTt1DdNnFf0oOxXbB";
	size_t i = 0;
	uint64_t x;

	fill_raw(nbuf, NSTR, seed ^ 0xfeed5eedULL);
	rng_state = seed | 1;

	if (mode & 8) {		/* empty string */
		nbuf[0] = 0;
		return;
	}
	nbuf[i++] = pool[(x = xs64()) % (sizeof(pool) - 1)];
	switch ((mode >> 1) & 3) {
	case 1: {
		static const char c2[] = "nNfFxX";
		nbuf[i++] = c2[x % 6];
		break;
	}
	case 2:
		nbuf[i++] = (char)(x | 1);
		break;
	case 3:
		nbuf[i++] = '\n';
		break;
	}
	if (mode & 4)
		nbuf[i++] = 'Z';
	nbuf[i] = 0;
}

/* The `only` dictionary for string_escape_*: selected by sel%5. */
static const char *mkonly(int64_t sel, uint64_t seed)
{
	static char obuf[64];
	size_t i;
	uint64_t x;

	switch (blen(sel, 5)) {
	case 0:
		return NULL;
	case 1:
		return "";			/* is_dict false */
	case 2:
		return "\\\"abc\a\e";		/* specials + letters */
	case 3:
		return "\n\r\t\v\f0";		/* spaces + NUL */
	default:
		/* seeded subset of all 256 bytes */
		rng_state = seed | 1;
		for (i = 0; i < 32; i++) {
			x = xs64();
			obuf[i] = (char)((x >> ((i % 7) * 8)) & 0xff);
		}
		obuf[32] = 0;
		return obuf;
	}
}

/* Build an escape-sequence-heavy string for string_unescape: plain bytes
 * interleaved with \x, \NNN, \e-letter and trailing/empty backslashes. */
static void mkesc(size_t n, uint64_t seed, int mode)
{
	size_t i = 0, j;
	uint64_t x;
	static const char sp[] = "nrtvf";
	static const char hx[] = "0123456789abcdefABCDEFxg";
	static const char spec[] = "\\\"aeq";

	fill_raw(sbuf, SSZ, seed ^ 0xfeed5eedULL);
	rng_state = seed | 1;
	for (j = 0; j < n && i < SSZ - 6; j++) {
		x = xs64();
		switch (x % 8) {
		case 0:
			sbuf[i++] = '\\';
			sbuf[i++] = sp[x % 5];
			break;
		case 1:
			sbuf[i++] = '\\';
			sbuf[i++] = 'x';
			sbuf[i++] = hx[x % 24];
			if (x & 0x100)
				sbuf[i++] = hx[(x >> 8) % 24];
			break;
		case 2:
			sbuf[i++] = '\\';
			sbuf[i++] = (char)('0' + x % 9); /* 8/9 invalid */
			if (x & 0x100)
				sbuf[i++] = (char)('0' + (x >> 8) % 8);
			if (x & 0x200)
				sbuf[i++] = (char)('0' + (x >> 16) % 8);
			break;
		case 3:
			sbuf[i++] = '\\';
			sbuf[i++] = spec[x % 5];
			break;
		case 4:
			sbuf[i++] = '\\';
			sbuf[i++] = (char)((x >> 8) | 1);
			break;
		case 5:
			sbuf[i++] = '\\';
			break;
		default:
			sbuf[i++] = (char)(x | 1);
			break;
		}
	}
	if (mode & 2)
		sbuf[i++] = '\\';		/* trailing lone backslash */
	sbuf[i] = 0;
}

/* ==== kstrtox wrappers ================================================== */

/* Explicit definitions: the bridge's export scanner does not expand
 * macros, so these are spelled out (they all share the same body). */

uint64_t wrap_kstrtoull(uint64_t seed, int64_t base, int64_t mode)
{
	unsigned long long res = 0x5a5a5a5a5a5a5a5aULL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoull(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtoll(uint64_t seed, int64_t base, int64_t mode)
{
	long long res = 0x5a5a5a5a5a5a5a5aLL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoll(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap__kstrtoul(uint64_t seed, int64_t base, int64_t mode)
{
	unsigned long res = 0x5a5a5a5a5a5a5a5aUL;
	int ret;
	mknum(seed, (int)mode);
	ret = _kstrtoul(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap__kstrtol(uint64_t seed, int64_t base, int64_t mode)
{
	long res = 0x5a5a5a5a5a5a5a5aL;
	int ret;
	mknum(seed, (int)mode);
	ret = _kstrtol(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtouint(uint64_t seed, int64_t base, int64_t mode)
{
	unsigned int res = 0x5a5a5a5aU;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtouint(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtoint(uint64_t seed, int64_t base, int64_t mode)
{
	int res = 0x5a5a5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoint(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtou16(uint64_t seed, int64_t base, int64_t mode)
{
	u16 res = 0x5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou16(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtos16(uint64_t seed, int64_t base, int64_t mode)
{
	s16 res = 0x5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos16(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtou8(uint64_t seed, int64_t base, int64_t mode)
{
	u8 res = 0x5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou8(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtos8(uint64_t seed, int64_t base, int64_t mode)
{
	s8 res = 0x5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos8(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtou64(uint64_t seed, int64_t base, int64_t mode)
{
	u64 res = 0x5a5a5a5a5a5a5a5aULL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou64(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtos64(uint64_t seed, int64_t base, int64_t mode)
{
	s64 res = 0x5a5a5a5a5a5a5a5aLL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos64(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtou32(uint64_t seed, int64_t base, int64_t mode)
{
	u32 res = 0x5a5a5a5aU;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou32(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtos32(uint64_t seed, int64_t base, int64_t mode)
{
	s32 res = 0x5a5a5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos32(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtoul(uint64_t seed, int64_t base, int64_t mode)
{
	unsigned long res = 0x5a5a5a5a5a5a5a5aUL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoul(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_kstrtol(uint64_t seed, int64_t base, int64_t mode)
{
	long res = 0x5a5a5a5a5a5a5a5aL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtol(nbuf, (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)base);
}

uint64_t wrap_parse_integer_limit(uint64_t seed, int64_t base,
				  int64_t maxc, int64_t init, int64_t mode)
{
	unsigned long long res = 0x5a5a5a5a5a5a5a5aULL;
	unsigned int rv;

	mknum(seed, (int)mode);
	rv = _parse_integer_limit(nbuf, (unsigned int)base, &res,
				  (size_t)maxc, (unsigned long long)init);
	return mix((uint64_t)rv, res, (uint64_t)base);
}

uint64_t wrap_parse_integer_fixup_radix(uint64_t seed, int64_t base,
					int64_t mode)
{
	unsigned int b = (unsigned int)base;
	const char *r;

	mknum(seed, (int)mode);
	r = _parse_integer_fixup_radix(nbuf, &b);
	return mix((uint64_t)(r - nbuf), (uint64_t)b, 0);
}

uint64_t wrap_kstrtobool(uint64_t seed, int64_t mode)
{
	bool res = false;
	int ret;

	mkbool(seed, (int)mode);
	ret = kstrtobool((mode & 0x40) ? NULL : nbuf, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, 0);
}

/* Explicit from_user wrappers (macro-expanded for the export scanner). */

uint64_t wrap_kstrtoull_from_user(uint64_t seed, int64_t count,
				  int64_t base, int64_t mode)
{
	unsigned long long res = 0x5a5a5a5a5a5a5a5aULL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoull_from_user(nbuf, (size_t)count,
				  (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtoll_from_user(uint64_t seed, int64_t count,
				 int64_t base, int64_t mode)
{
	long long res = 0x5a5a5a5a5a5a5a5aLL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoll_from_user(nbuf, (size_t)count,
				 (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtoul_from_user(uint64_t seed, int64_t count,
				 int64_t base, int64_t mode)
{
	unsigned long res = 0x5a5a5a5a5a5a5a5aUL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoul_from_user(nbuf, (size_t)count,
				 (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtol_from_user(uint64_t seed, int64_t count,
				int64_t base, int64_t mode)
{
	long res = 0x5a5a5a5a5a5a5a5aL;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtol_from_user(nbuf, (size_t)count,
				(unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtouint_from_user(uint64_t seed, int64_t count,
				   int64_t base, int64_t mode)
{
	unsigned int res = 0x5a5a5a5aU;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtouint_from_user(nbuf, (size_t)count,
				   (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtoint_from_user(uint64_t seed, int64_t count,
				  int64_t base, int64_t mode)
{
	int res = 0x5a5a5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtoint_from_user(nbuf, (size_t)count,
				  (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtou16_from_user(uint64_t seed, int64_t count,
				  int64_t base, int64_t mode)
{
	u16 res = 0x5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou16_from_user(nbuf, (size_t)count,
				  (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtos16_from_user(uint64_t seed, int64_t count,
				  int64_t base, int64_t mode)
{
	s16 res = 0x5a5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos16_from_user(nbuf, (size_t)count,
				  (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtou8_from_user(uint64_t seed, int64_t count,
				 int64_t base, int64_t mode)
{
	u8 res = 0x5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtou8_from_user(nbuf, (size_t)count,
				 (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtos8_from_user(uint64_t seed, int64_t count,
				 int64_t base, int64_t mode)
{
	s8 res = 0x5a;
	int ret;
	mknum(seed, (int)mode);
	ret = kstrtos8_from_user(nbuf, (size_t)count,
				 (unsigned int)base, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res, (uint64_t)count);
}

uint64_t wrap_kstrtobool_from_user(uint64_t seed, int64_t count,
				   int64_t mode)
{
	bool res = false;
	int ret;

	mkbool(seed, (int)mode);
	ret = kstrtobool_from_user(nbuf, (size_t)count, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res,
		   (uint64_t)count);
}

uint64_t wrap_kstrtoudec64(uint64_t seed, int64_t scale, int64_t mode)
{
	u64 res = 0x5a5a5a5a5a5a5a5aULL;
	int ret;

	mkdec(seed, (int)mode);
	ret = kstrtoudec64(nbuf, (unsigned int)scale, &res);
	return mix((uint64_t)(ret + 16), res, (uint64_t)scale);
}

uint64_t wrap_kstrtodec64(uint64_t seed, int64_t scale, int64_t mode)
{
	s64 res = 0x5a5a5a5a5a5a5a5aLL;
	int ret;

	mkdec(seed, (int)mode);
	ret = kstrtodec64(nbuf, (unsigned int)scale, &res);
	return mix((uint64_t)(ret + 16), (uint64_t)res,
		   (uint64_t)scale);
}

/* ==== hexdump wrappers ================================================== */

int64_t wrap_hex_to_bin(int64_t c)
{
	return hex_to_bin((unsigned char)c);
}

uint64_t wrap_hex2bin(uint64_t seed, int64_t slen, int64_t cnt,
		      int64_t mode)
{
	static const char hd[] = "0123456789abcdefABCDEF";
	size_t i, n = blen(slen, 300);
	int ret;

	fill_raw(sbuf, SSZ, seed);
	rng_state = (seed ^ 0x5bd1e9959e3779b9ULL) | 1;
	for (i = 0; i < n; i++)
		sbuf[i] = hd[xs64() % 22];
	if ((mode & 1) && n)
		sbuf[blen((int64_t)xs64(), n)] = 'q';
	fill_raw(dbuf, DSZ, seed ^ 0x9e3779b97f4a7c15ULL);
	ret = hex2bin((u8 *)dbuf, sbuf, blen(cnt, 300));
	return mix((uint64_t)(ret + 16), fnv1a(dbuf, DSZ), n);
}

uint64_t wrap_bin2hex(uint64_t seed, int64_t slen, int64_t mode)
{
	size_t n = blen(slen, HBUF);
	char *r;

	fill_raw(hbuf, HBUF, seed);
	fill_raw(dbuf, DSZ, seed ^ 0x9e3779b97f4a7c15ULL);
	r = bin2hex(dbuf, hbuf, n);
	return mix((uint64_t)(r - dbuf), fnv1a(dbuf, DSZ),
		   (uint64_t)mode);
}

uint64_t wrap_hex_dump_to_buffer(uint64_t seed, int64_t len,
				 int64_t rowsize, int64_t groupsize,
				 int64_t linebuflen, int64_t ascii)
{
	size_t l = blen(len, HBUF + 1);
	size_t lblen = blen(linebuflen, LB + 1);
	int ret;

	fill_raw(hbuf, HBUF, seed);
	memset(linebuf, 0xa5, LB);
	ret = hex_dump_to_buffer(hbuf, l, (int)rowsize, (int)groupsize,
				 (char *)linebuf, lblen, ascii & 1);
	return mix((uint64_t)(ret + 16), fnv1a(linebuf, LB),
		   (uint64_t)l);
}

uint64_t wrap_print_hex_dump(uint64_t seed, int64_t ptype,
			     int64_t rowsize, int64_t groupsize,
			     int64_t len, int64_t ascii, int64_t mode)
{
	static const char *const levels[] = {
		KERN_DEBUG, KERN_INFO, KERN_WARNING, "", "X"
	};
	static const char *const pfxs[] = { "", "pfx: ", "data " };

	fill_raw(hbuf, HBUF, seed);
	klog_len = 0;
	print_hex_dump(levels[blen(mode, 5)],
		       pfxs[blen(mode / 8, 3)],
		       (int)ptype, (int)rowsize, (int)groupsize,
		       hbuf, blen(len, HBUF + 1), ascii & 1);
	return mix((uint64_t)klog_len, fnv1a(klog, klog_len), 0);
}

/* ==== string_helpers wrappers ============================================ */

uint64_t wrap_string_get_size(int64_t size, int64_t blk,
			      int64_t units, int64_t len)
{
	static char buf[64];
	int ret;

	memset(buf, 0xa5, sizeof buf);
	ret = string_get_size((u64)size, (u64)blk,
			      (enum string_size_units)units,
			      buf, (int)len);
	return mix((uint64_t)(ret + 16), fnv1a(buf, sizeof buf),
		   (uint64_t)size ^ (uint64_t)blk);
}

uint64_t wrap_string_escape_mem(uint64_t seed, int64_t isz,
				int64_t osz, int64_t flags,
				int64_t only)
{
	size_t is = blen(isz, SSZ + 1);
	size_t os = blen(osz, DSZ + 1);
	int ret;

	fill_raw(sbuf, SSZ, seed);
	memset(dbuf, 0xa5, DSZ);
	ret = string_escape_mem(sbuf, is, dbuf, os,
				(unsigned int)flags, mkonly(only, seed));
	return mix((uint64_t)(ret + 16), fnv1a(dbuf, DSZ), is);
}

uint64_t wrap_string_escape_str(uint64_t seed, int64_t slen,
				int64_t osz, int64_t flags,
				int64_t only, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);
	size_t os = blen(osz, DSZ + 1);
	int ret;

	make_str(sbuf, n, seed, (int)(mode & 3));
	memset(dbuf, 0xa5, DSZ);
	ret = string_escape_str(sbuf, dbuf, os,
				(unsigned int)flags, mkonly(only, seed));
	return mix((uint64_t)(ret + 16), fnv1a(dbuf, DSZ), n);
}

uint64_t wrap_string_unescape(uint64_t seed, int64_t slen,
			      int64_t size, int64_t flags,
			      int64_t mode)
{
	size_t n = blen(slen, 100);
	size_t sz = (mode & 8) ? 0 : blen(size, SSZ + 1);
	int r1, r2;

	mkesc(n, seed, (int)mode);
	memcpy(sbuf2, sbuf, SSZ);
	memset(dbuf, 0xa5, DSZ);
	r1 = string_unescape(sbuf, dbuf, sz, (unsigned int)flags);
	if (mode & 4)
		r2 = string_unescape(sbuf2, sbuf2, sz, (unsigned int)flags);
	else
		r2 = string_unescape(sbuf2, dbuf + 700, sz,
				     (unsigned int)flags);
	return mix((uint64_t)(r1 + 16) ^ (uint64_t)(r2 << 20),
		   fnv1a(dbuf, DSZ), (uint64_t)sz);
}

uint64_t wrap_skip_spaces(uint64_t seed, int64_t mode)
{
	size_t n = 1 + blen(seed, 60);
	size_t i, lead;
	char *r;

	make_str(sbuf, n, seed, 0);
	lead = (mode & 1) ? n : blen(seed >> 8, n);
	for (i = 0; i < lead; i++)
		sbuf[i] = ' ';
	r = skip_spaces(sbuf);
	return mix((uint64_t)(r - sbuf), fnv1a(sbuf, SSZ), n);
}

uint64_t wrap_strim(uint64_t seed, int64_t mode)
{
	size_t n = 1 + blen(seed, 60);
	size_t i, lead, trail;
	char *r;

	make_str(sbuf, n, seed, 0);
	lead = (mode & 1) ? n : blen(seed >> 8, n);
	trail = (mode & 2) ? n - lead : blen(seed >> 16, n - lead + 1);
	for (i = 0; i < lead; i++)
		sbuf[i] = ' ';
	for (i = 0; i < trail; i++)
		sbuf[n - 1 - i] = ' ';
	r = strim(sbuf);
	return mix((uint64_t)(r - sbuf), fnv1a(sbuf, SSZ), n);
}

uint64_t wrap_sysfs_streq(uint64_t seed, int64_t la, int64_t lb,
			  int64_t mode)
{
	size_t na = blen(la, SSZ - 2);
	size_t nb = blen(lb, SSZ - 2);
	bool r;

	make_str(sbuf, na, seed, 0);
	if (mode & 1) {
		memcpy(sbuf2, sbuf, SSZ);		/* identical */
	} else {
		make_str(sbuf2, nb, seed ^ 0x9e3779b97f4a7c15ULL, 0);
	}
	if (mode & 2) {
		size_t k = strlen(sbuf2);	/* s2 becomes "s2\n\0" */
		sbuf2[k] = '\n';
		sbuf2[k + 1] = '\0';
	}
	if (mode & 4) {
		size_t k = strlen(sbuf);	/* s1 becomes "s1\n\0" */
		sbuf[k] = '\n';
		sbuf[k + 1] = '\0';
	}
	r = sysfs_streq(sbuf, sbuf2);
	return mix((uint64_t)r, fnv1a(sbuf, na + 2), fnv1a(sbuf2, nb + 2));
}

uint64_t wrap_strreplace(uint64_t seed, int64_t slen, int64_t old,
			 int64_t new, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);
	char *r;

	make_str(sbuf, n, seed, (int)(mode & 3));
	if (mode & 4) {
		/* ensure `old` occurs: plant it at seeded positions */
		size_t k;
		for (k = 0; k < 3 && n; k++)
			sbuf[blen((int64_t)xs64(), n)] = (char)old;
	}
	r = strreplace(sbuf, (char)old, (char)new);
	return mix((uint64_t)(r - sbuf), fnv1a(sbuf, SSZ), n);
}

uint64_t wrap_memcpy_and_pad(uint64_t seed, int64_t dest_len,
			     int64_t count, int64_t pad)
{
	size_t dl = blen(dest_len, DSZ + 1);
	size_t ct = blen(count, SSZ + 1);

	fill_raw(sbuf, SSZ, seed);
	memset(dbuf, 0xa5, DSZ);
	memcpy_and_pad(dbuf, dl, sbuf, ct, (int)pad);
	return mix((uint64_t)dl, fnv1a(dbuf, DSZ), ct);
}

#define ARR_N 16
static char astr[ARR_N][96];
static const char *arr[ARR_N + 1];

static void mkarray(uint64_t seed, int mode)
{
	size_t i;

	rng_state = seed | 1;
	for (i = 0; i < ARR_N; i++) {
		size_t j, n = 1 + xs64() % 40;
		for (j = 0; j < n; j++)
			astr[i][j] = (char)('a' + xs64() % 58);
		astr[i][n] = 0;
		arr[i] = astr[i];
	}
	arr[ARR_N] = NULL;
	if (mode & 1) {
		/* plant a NULL terminator mid-array (first NULL stops scan) */
		size_t k = blen((int64_t)xs64(), ARR_N);
		arr[k] = NULL;
	}
}

uint64_t wrap_match_string(uint64_t seed, int64_t n, int64_t strsel,
			   int64_t mode)
{
	const char *q;
	int ret;

	mkarray(seed, (int)mode);
	if (mode & 2)
		q = astr[blen(strsel, ARR_N)];		/* hit candidate */
	else {
		size_t j;
		rng_state = (seed ^ 0x5bd1e9959e3779b9ULL) | 1;
		for (j = 0; j < 40; j++)
			sbuf[j] = (char)('a' + xs64() % 58);
		sbuf[40] = 0;
		q = sbuf;				/* miss candidate */
	}
	ret = match_string(arr, blen(n, ARR_N + 2), q);
	return mix((uint64_t)(ret + 4), (uint64_t)blen(n, ARR_N + 2), 0);
}

uint64_t wrap_sysfs_match_string(uint64_t seed, int64_t n, int64_t strsel,
				 int64_t mode)
{
	const char *q;
	int ret;

	mkarray(seed, (int)mode);
	if (mode & 2) {
		size_t k = blen(strsel, ARR_N);
		memcpy(sbuf, astr[k], 96);
		if (mode & 4)
			strcat(sbuf, "\n");	/* sysfs-style newline */
		q = sbuf;
	} else {
		size_t j;
		rng_state = (seed ^ 0x5bd1e9959e3779b9ULL) | 1;
		for (j = 0; j < 40; j++)
			sbuf[j] = (char)('a' + xs64() % 58);
		sbuf[40] = 0;
		if (mode & 4)
			strcat(sbuf, "\n");
		q = sbuf;
	}
	if (mode & 8) {
		/* array element carries a trailing newline too */
		size_t k = blen(strsel >> 8, ARR_N);
		size_t l = strlen(astr[k]);
		astr[k][l] = '\n';
		astr[k][l + 1] = 0;
	}
	ret = __sysfs_match_string(arr, blen(n, ARR_N + 2), q);
	return mix((uint64_t)(ret + 4), (uint64_t)blen(n, ARR_N + 2), 0);
}

uint64_t wrap_string_is_terminated(uint64_t seed, int64_t slen,
				   int64_t len, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);
	bool r;

	make_str(sbuf, n, seed, (int)(mode & 3));
	r = string_is_terminated(sbuf, (int)blen(len, SSZ));
	return mix((uint64_t)r, (uint64_t)n, 0);
}

uint64_t wrap_string_upper(uint64_t seed, int64_t slen, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);

	make_str(sbuf, n, seed, (int)(mode & 3));
	memset(dbuf, 0xa5, DSZ);
	string_upper(dbuf, sbuf);
	return mix((uint64_t)n, fnv1a(dbuf, DSZ), 0);
}

uint64_t wrap_string_lower(uint64_t seed, int64_t slen, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);

	make_str(sbuf, n, seed, (int)(mode & 3));
	memset(dbuf, 0xa5, DSZ);
	string_lower(dbuf, sbuf);
	return mix((uint64_t)n, fnv1a(dbuf, DSZ), 0);
}

uint64_t wrap_kstrdup_quotable(uint64_t seed, int64_t slen, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);
	char *r;
	uint64_t h;

	make_str(sbuf, n, seed, (int)(mode & 3));
	r = kstrdup_quotable((mode & 8) ? NULL : sbuf, GFP_KERNEL);
	h = r ? fnv1a(r, strlen(r) + 1) : 0;
	kfree(r);
	return mix((uint64_t)n, h, (uint64_t)(r != NULL));
}

uint64_t wrap_kstrdup_and_replace(uint64_t seed, int64_t slen,
				  int64_t old, int64_t new, int64_t mode)
{
	size_t n = blen(slen, SSZ - 1);
	char *r;
	uint64_t h;

	make_str(sbuf, n, seed, (int)(mode & 3));
	r = kstrdup_and_replace(sbuf, (char)old, (char)new, GFP_KERNEL);
	h = r ? fnv1a(r, strlen(r) + 1) : 0;
	kfree(r);
	return mix((uint64_t)n, h, (uint64_t)(r != NULL));
}
