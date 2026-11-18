/* Scalar/buffer wrappers around lib/string.c for the rust-anything
 * differential port, same convention as ports/lib-hash.
 *
 * Every export returns a scalar: pointer-returning functions yield offsets
 * into the seeded buffer (-1 for NULL); buffer-mutating functions fold the
 * resulting buffer through fnv1a so the whole output is observable; the
 * scalars and checksum are folded together with mix().
 *
 * Determinism: all content comes from a seeded xorshift64.  record forks
 * per case so the static buffers start fresh, but every wrapper re-fills
 * what it reads anyway.
 *
 * String construction: make_str() writes `n` deterministic non-zero body
 * bytes and a forced NUL at index n; the rest of the SSZ buffer keeps
 * deterministic tail bytes (count-limited readers like strnstr scan past
 * the first NUL).  mode&1 plants an extra NUL at a seeded position inside
 * the body (embedded-NUL cases); mode&2 fills with 'A'+x%58 (case-fold
 * collisions) instead of x|1.
 */
#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <string.h>
#include <strings.h>
#include <sys/types.h>
#include "linux/compiler.h"	/* __aligned */

/* lib/string.c functions with no libc prototype. */
ssize_t sized_strscpy(char *dest, const char *src, size_t count);
char *strnchrnul(const char *s, size_t count, int c);
char *strnchr(const char *s, size_t count, int c);
char *strnstr(const char *s1, const char *s2, size_t len);
void *memscan(void *addr, int c, size_t size);
void *memchr_inv(const void *start, int c, size_t bytes);
void *memset16(uint16_t *s, uint16_t v, size_t count);
void *memset32(uint32_t *s, uint32_t v, size_t count);
void *memset64(uint64_t *s, uint64_t v, size_t count);

#define SSZ 640		/* string buffers: body bytes + forced NUL + tail */
#define DSZ 1280	/* destination buffer for mutating ops */

static char sbuf[SSZ] __aligned(16);
static char sbuf2[SSZ] __aligned(16);
static char dbuf[DSZ] __aligned(16);
static uint16_t w16[640];
static uint32_t w32[640];
static uint64_t w64[160];

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
static void fill_raw(char *b, size_t n, uint64_t seed)
{
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < n; i++)
		b[i] = (char)xs64();
}

static void fill_typed(uint64_t seed)
{
	size_t i;
	rng_state = seed | 1;
	for (i = 0; i < 640; i++) {
		w16[i] = (uint16_t)xs64();
		w32[i] = (uint32_t)xs64();
	}
	for (i = 0; i < 160; i++)
		w64[i] = xs64();
}

/* Build a string in the first SSZ bytes of b: n non-zero body bytes + a
 * forced NUL at n, over a deterministic raw tail.  See the header comment
 * for the mode bits. */
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

/* String destination in dbuf: deterministic fill of the whole DSZ buffer
 * under the string content. */
static void make_dstr(size_t n, uint64_t seed, int mode)
{
	fill_raw(dbuf, DSZ, seed ^ 0x9e3779b97f4a7c15ULL);
	make_str(dbuf, n, seed, mode);
}

/* s1 in sbuf, s2 in sbuf2; mode&16 makes s2 a verbatim copy of s1
 * (equal / prefix-relation cases). */
static void two_strs(uint64_t sa, uint64_t sb, size_t na, size_t nb,
		     int mode)
{
	make_str(sbuf, na, sa, mode & 3);
	if (mode & 16)
		memcpy(sbuf2, sbuf, SSZ);
	else
		make_str(sbuf2, nb, sb, (mode >> 2) & 3);
}

/* Byte to search for inside a C string: mode&8 -> 0 (the terminator is
 * searchable for the strchr family); mode&4 -> a byte that is definitely
 * in the string; otherwise a raw byte. */
static int pick_str_c(int64_t csel, const char *s, int mode)
{
	size_t n = strlen(s);
	if (mode & 8)
		return 0;
	if ((mode & 4) && n)
		return (unsigned char)s[blen(csel, n)];
	return (int)(csel & 0xff);
}

/* Byte to search for inside a raw memory region of n bytes. */
static int pick_mem_c(int64_t csel, const char *b, size_t n, int mode)
{
	if ((mode & 4) && n)
		return (unsigned char)b[blen(csel, n)];
	return (int)(csel & 0xff);
}

/* ---- case-insensitive compare -------------------------------------------- */

int64_t wrap_strncasecmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
			 int64_t cnt, int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strncasecmp(sbuf, sbuf2, blen(cnt, SSZ));
}

int64_t wrap_strcasecmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
			int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strcasecmp(sbuf, sbuf2);
}

/* ---- copy / concat -------------------------------------------------------- */

uint64_t wrap_strcpy(uint64_t sa, int64_t la, int64_t mode)
{
	size_t na = blen(la, 600);
	make_str(sbuf, na, sa, (int)(mode & 3));
	fill_raw(dbuf, DSZ, sa ^ 0x9e3779b97f4a7c15ULL);
	strcpy(dbuf, sbuf);
	return mix(strlen(dbuf), fnv1a(dbuf, DSZ), na);
}

uint64_t wrap_sized_strscpy(uint64_t sa, int64_t la, int64_t cnt,
			    int64_t mode)
{
	size_t na = blen(la, SSZ - 1);
	/* mode&4 drives count past INT_MAX: WARN_ON_ONCE -> -E2BIG. */
	size_t count = (mode & 4) ? (size_t)INT_MAX + 1 : blen(cnt, SSZ);
	ssize_t res;
	make_str(sbuf, na, sa, (int)(mode & 3));
	fill_raw(dbuf, DSZ, sa ^ 0x9e3779b97f4a7c15ULL);
	res = sized_strscpy(dbuf, sbuf, count);
	return mix((uint64_t)(res + 16), fnv1a(dbuf, DSZ), count);
}

uint64_t wrap_stpcpy(uint64_t sa, int64_t la, int64_t mode)
{
	size_t na = blen(la, 600);
	char *r;
	make_str(sbuf, na, sa, (int)(mode & 3));
	fill_raw(dbuf, DSZ, sa ^ 0x9e3779b97f4a7c15ULL);
	r = stpcpy(dbuf, sbuf);
	return mix((uint64_t)(r - dbuf), fnv1a(dbuf, DSZ), na);
}

uint64_t wrap_strcat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t mode)
{
	make_dstr(blen(la, 600), sa, (int)(mode & 3));
	make_str(sbuf, blen(lb, SSZ - 1), sb, (int)((mode >> 2) & 3));
	strcat(dbuf, sbuf);
	return mix(strlen(dbuf), fnv1a(dbuf, DSZ), 0);
}

uint64_t wrap_strncat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		      int64_t cnt, int64_t mode)
{
	size_t count = blen(cnt, 600);
	make_dstr(blen(la, 600), sa, (int)(mode & 3));
	make_str(sbuf, blen(lb, SSZ - 1), sb, (int)((mode >> 2) & 3));
	strncat(dbuf, sbuf, count);
	return mix(strlen(dbuf), fnv1a(dbuf, DSZ), count);
}

uint64_t wrap_strlcat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		      int64_t cnt, int64_t mode)
{
	size_t na = blen(la, 600);
	size_t count;
	size_t res;
	make_dstr(na, sa, (int)(mode & 3));
	make_str(sbuf, blen(lb, SSZ - 1), sb, (int)((mode >> 2) & 3));
	/* BUG_ON(dsize >= count): keep count > strlen(dest), which is <= na */
	count = na + 1 + blen(cnt, DSZ - na - 1);
	res = strlcat(dbuf, sbuf, count);
	return mix(res, fnv1a(dbuf, DSZ), count);
}

/* ---- compare -------------------------------------------------------------- */

int64_t wrap_strcmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		    int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strcmp(sbuf, sbuf2);
}

int64_t wrap_strncmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t cnt, int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strncmp(sbuf, sbuf2, blen(cnt, SSZ));
}

/* ---- character search ----------------------------------------------------- */

int64_t wrap_strchr(uint64_t sa, int64_t la, int64_t c, int64_t mode)
{
	char *r;
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	r = strchr(sbuf, pick_str_c(c, sbuf, (int)mode));
	return r ? (int64_t)(r - sbuf) : -1;
}

int64_t wrap_strchrnul(uint64_t sa, int64_t la, int64_t c, int64_t mode)
{
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	return (int64_t)(strchrnul(sbuf, pick_str_c(c, sbuf, (int)mode)) - sbuf);
}

int64_t wrap_strnchrnul(uint64_t sa, int64_t la, int64_t c, int64_t cnt,
			int64_t mode)
{
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	return (int64_t)(strnchrnul(sbuf, blen(cnt, SSZ),
				   pick_str_c(c, sbuf, (int)mode)) - sbuf);
}

int64_t wrap_strrchr(uint64_t sa, int64_t la, int64_t c, int64_t mode)
{
	char *r;
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	r = strrchr(sbuf, pick_str_c(c, sbuf, (int)mode));
	return r ? (int64_t)(r - sbuf) : -1;
}

int64_t wrap_strnchr(uint64_t sa, int64_t la, int64_t c, int64_t cnt,
		     int64_t mode)
{
	char *r;
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	r = strnchr(sbuf, blen(cnt, SSZ), pick_str_c(c, sbuf, (int)mode));
	return r ? (int64_t)(r - sbuf) : -1;
}

/* ---- length --------------------------------------------------------------- */

uint64_t wrap_strlen(uint64_t sa, int64_t la, int64_t mode)
{
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	return strlen(sbuf);
}

uint64_t wrap_strnlen(uint64_t sa, int64_t la, int64_t cnt, int64_t mode)
{
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	return strnlen(sbuf, blen(cnt, SSZ));
}

/* ---- span sets ------------------------------------------------------------ */

uint64_t wrap_strspn(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strspn(sbuf, sbuf2);
}

uint64_t wrap_strcspn(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		      int64_t mode)
{
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	return strcspn(sbuf, sbuf2);
}

int64_t wrap_strpbrk(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t mode)
{
	char *r;
	two_strs(sa, sb, blen(la, SSZ - 1), blen(lb, SSZ - 1), (int)mode);
	r = strpbrk(sbuf, sbuf2);
	return r ? (int64_t)(r - sbuf) : -1;
}

/* strsep takes char **: modelled as an offset cursor into dbuf, called
 * 1..4 times per wrapper call so the NULL-cursor transition is covered.
 * The returned fold includes each token offset, each new cursor, and the
 * mutated buffer. */
uint64_t wrap_strsep(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t cur, int64_t mode)
{
	size_t na = blen(la, 1000);
	size_t nb = blen(lb, 300);
	char *p;
	uint64_t acc = 0;
	int reps = 1 + (int)(uabs64(mode >> 5) & 3);
	int i;
	make_dstr(na, sa, (int)(mode & 3));
	make_str(sbuf2, nb, sb, (int)((mode >> 2) & 3));
	p = dbuf + blen(cur, na + 1);
	if (mode & 16)
		p = NULL;
	for (i = 0; i < reps; i++) {
		char *tok = strsep(&p, sbuf2);
		acc = mix(acc,
			  tok ? (uint64_t)(tok - dbuf) + 1 : 0,
			  p ? (uint64_t)(p - dbuf) + 1 : 0);
	}
	return mix(acc, fnv1a(dbuf, DSZ), na);
}

/* ---- memory fill / copy ---------------------------------------------------- */

uint64_t wrap_memset(uint64_t sb, int64_t c, int64_t len, int64_t off,
		     int64_t mode)
{
	size_t o = blen(off, 640);
	size_t n = blen(len, 640);
	void *r;
	fill_raw(dbuf, DSZ, sb);
	if (mode & 1) {		/* full-buffer edge */
		o = 0;
		n = DSZ;
	}
	r = memset(dbuf + o, (int)c, n);
	return mix((uint64_t)((char *)r - dbuf), fnv1a(dbuf, DSZ),
		   ((uint64_t)(c & 0xff) << 16) | n);
}

uint64_t wrap_memset16(uint64_t sb, int64_t v, int64_t len, int64_t off,
		       int64_t mode)
{
	size_t o = blen(off, 320);
	size_t n = blen(len, 320);
	uint16_t *r;
	fill_typed(sb);
	if (mode & 1) {
		o = 0;
		n = 640;
	}
	r = memset16(w16 + o, (uint16_t)v, n);
	return mix((uint64_t)(r - w16), fnv1a(w16, sizeof w16),
		   ((uint64_t)(v & 0xffff) << 16) | n);
}

uint64_t wrap_memset32(uint64_t sb, int64_t v, int64_t len, int64_t off,
		       int64_t mode)
{
	size_t o = blen(off, 320);
	size_t n = blen(len, 320);
	uint32_t *r;
	fill_typed(sb);
	if (mode & 1) {
		o = 0;
		n = 640;
	}
	r = memset32(w32 + o, (uint32_t)v, n);
	return mix((uint64_t)(r - w32), fnv1a(w32, sizeof w32),
		   ((uint64_t)(v & 0xffffff) << 16) | n);
}

uint64_t wrap_memset64(uint64_t sb, int64_t v, int64_t len, int64_t off,
		       int64_t mode)
{
	size_t o = blen(off, 80);
	size_t n = blen(len, 80);
	uint64_t *r;
	fill_typed(sb);
	if (mode & 1) {
		o = 0;
		n = 160;
	}
	r = memset64(w64 + o, (uint64_t)v, n);
	return mix((uint64_t)(r - w64), fnv1a(w64, sizeof w64), n);
}

uint64_t wrap_memcpy(uint64_t sa, uint64_t sb, int64_t doff, int64_t soff,
		     int64_t len, int64_t mode)
{
	size_t so = blen(soff, 320);
	size_t n = blen(len, 320);
	size_t dof = blen(doff, DSZ - 320);
	void *r;
	fill_raw(sbuf, SSZ, sa);
	fill_raw(dbuf, DSZ, sb);
	if (mode & 1)		/* zero-length edge */
		n = 0;
	r = memcpy(dbuf + dof, sbuf + so, n);
	return mix((uint64_t)((char *)r - dbuf), fnv1a(dbuf, DSZ), n);
}

/* Overlapping moves inside one buffer: dof and so are independent, so both
 * dst>src and dst<src overlaps occur; mode&1 forces dst==src, mode&2
 * forces dst==src+1 (always overlapping when n>1). */
uint64_t wrap_memmove(uint64_t sb, int64_t doff, int64_t soff, int64_t len,
		      int64_t mode)
{
	size_t so = blen(soff, 640);
	size_t dof = blen(doff, 640);
	size_t n = blen(len, 640);
	void *r;
	fill_raw(dbuf, DSZ, sb);
	if (mode & 1)
		dof = so;
	if (mode & 2)
		dof = so + 1;
	r = memmove(dbuf + dof, dbuf + so, n);
	return mix((uint64_t)((char *)r - dbuf), fnv1a(dbuf, DSZ),
		   (dof << 16) | so);
}

/* ---- memory compare / scan ------------------------------------------------- */

int64_t wrap_memcmp(uint64_t sa, uint64_t sb, int64_t len, int64_t mode)
{
	size_t n = blen(len, SSZ);
	fill_raw(sbuf, SSZ, sa);
	fill_raw(sbuf2, SSZ, sb);
	if (mode & 16)
		memcpy(sbuf2, sbuf, n);		/* identical region */
	if ((mode & 32) && n)
		sbuf2[blen(len ^ 0x5a, n)] ^= 0xff; /* seeded single-byte diff */
	return memcmp(sbuf, sbuf2, n);
}

int64_t wrap_bcmp(uint64_t sa, uint64_t sb, int64_t len, int64_t mode)
{
	size_t n = blen(len, SSZ);
	fill_raw(sbuf, SSZ, sa);
	fill_raw(sbuf2, SSZ, sb);
	if (mode & 16)
		memcpy(sbuf2, sbuf, n);
	if ((mode & 32) && n)
		sbuf2[blen(len ^ 0x5a, n)] ^= 0xff;
	return bcmp(sbuf, sbuf2, n);
}

int64_t wrap_memscan(uint64_t sb, int64_t c, int64_t len, int64_t mode)
{
	size_t n = blen(len, SSZ);
	void *r;
	fill_raw(sbuf, SSZ, sb);
	r = memscan(sbuf, pick_mem_c(c, sbuf, n, (int)mode), n);
	return (int64_t)((char *)r - sbuf);	/* n when not found */
}

int64_t wrap_memchr(uint64_t sb, int64_t c, int64_t len, int64_t mode)
{
	size_t n = blen(len, SSZ);
	void *r;
	fill_raw(sbuf, SSZ, sb);
	r = memchr(sbuf, pick_mem_c(c, sbuf, n, (int)mode), n);
	return r ? (int64_t)((char *)r - sbuf) : -1;
}

/* memchr_inv exercises all paths: alignment prefix ((mode>>4)&7 shifts the
 * start), the u64 word loop, and the trailing check_bytes8; mode&3 picks
 * uniform-c (no mismatch), uniform-c-with-one-flip, or random content. */
int64_t wrap_memchr_inv(uint64_t sb, int64_t c, int64_t len, int64_t mode)
{
	size_t off = blen(mode >> 4, 8);
	size_t n = blen(len, SSZ - 8);
	uint8_t cv = (uint8_t)c;
	void *r;
	fill_raw(dbuf, DSZ, sb);
	switch (mode & 3) {
	case 1:
		memset(dbuf + off, cv, n);
		break;
	case 2: {
		size_t k = n ? blen(len ^ 0x33, n) : 0;
		memset(dbuf + off, cv, n);
		dbuf[off + k] = (char)(cv + 1);
		break;
	}
	default:
		break;
	}
	r = memchr_inv(dbuf + off, cv, n);
	return r ? (int64_t)((char *)r - dbuf) : -1;
}

/* ---- substring search ------------------------------------------------------ */

/* mode&16 plants the needle inside the haystack at a seeded position when
 * it fits (guaranteed-match cases; an earlier NUL can still hide it). */
int64_t wrap_strstr(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		    int64_t mode)
{
	char *r;
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	make_str(sbuf2, blen(lb, SSZ - 1), sb, (int)((mode >> 2) & 3));
	if (mode & 16) {
		size_t l1 = strlen(sbuf), l2 = strlen(sbuf2);
		if (l2 && l1 >= l2)
			memcpy(sbuf + blen(mode >> 8, l1 - l2 + 1), sbuf2, l2);
	}
	r = strstr(sbuf, sbuf2);
	return r ? (int64_t)(r - sbuf) : -1;
}

int64_t wrap_strnstr(uint64_t sa, uint64_t sb, int64_t la, int64_t lb,
		     int64_t cnt, int64_t mode)
{
	char *r;
	make_str(sbuf, blen(la, SSZ - 1), sa, (int)(mode & 3));
	make_str(sbuf2, blen(lb, SSZ - 1), sb, (int)((mode >> 2) & 3));
	if (mode & 16) {
		size_t l1 = strlen(sbuf), l2 = strlen(sbuf2);
		if (l2 && l1 >= l2)
			memcpy(sbuf + blen(mode >> 8, l1 - l2 + 1), sbuf2, l2);
	}
	r = strnstr(sbuf, sbuf2, blen(cnt, SSZ));
	return r ? (int64_t)(r - sbuf) : -1;
}
