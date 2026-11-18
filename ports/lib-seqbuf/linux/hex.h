#pragma once
/* linux/hex.h shim for the lib-seqbuf harness: the real hex_asc tables
 * plus a faithful userspace port of lib/hexdump.c's hex_dump_to_buffer()
 * (the only hexdump entry seq_buf.c needs; print_hex_dump() needs the
 * printk ring and is out of scope).
 *
 * Dependencies inside the ported body:
 *   snprintf()     -> the deterministic mini-printer (linux/sprintf.h)
 *   is_power_of_2() -> n && !(n & (n-1)), linux/log2.h semantics
 *   get_unaligned() -> native-endianness load (x86-64 LE reference build)
 *   isascii/isprint -> linux/ctype.h semantics (isprint = 0x20..0x7e)
 */
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/minmax.h>
#include <linux/sprintf.h>
#include <string.h>

/* glibc's <bits/endian.h> (pulled in transitively by libc headers)
 * defines __BIG_ENDIAN as the constant 4321 — defined on EVERY arch.
 * seq_buf.c's `#ifdef __BIG_ENDIAN` presence check would wrongly take
 * the big-endian branch of seq_buf_putmem_hex; the recorded reference
 * is the x86-64 little-endian build, so drop the leaked macro. */
#undef __BIG_ENDIAN

static const char hex_asc[] = "0123456789abcdef";
static const char hex_asc_upper[] = "0123456789ABCDEF";

#define hex_asc_lo(x)	hex_asc[((x) & 0x0f)]
#define hex_asc_hi(x)	hex_asc[((x) & 0xf0) >> 4]

static inline char *hex_byte_pack(char *buf, u8 byte)
{
	*buf++ = hex_asc_hi(byte);
	*buf++ = hex_asc_lo(byte);
	return buf;
}

#define hex_asc_upper_lo(x)	hex_asc_upper[((x) & 0x0f)]
#define hex_asc_upper_hi(x)	hex_asc_upper[((x) & 0xf0) >> 4]

static inline bool kshim_is_power_of_2(unsigned long n)
{
	return n != 0 && (n & (n - 1)) == 0;
}

static inline u16 kshim_get_unaligned_u16(const void *p)
{
	u16 v;
	memcpy(&v, p, sizeof(v));
	return v;
}
static inline u32 kshim_get_unaligned_u32(const void *p)
{
	u32 v;
	memcpy(&v, p, sizeof(v));
	return v;
}
static inline u64 kshim_get_unaligned_u64(const void *p)
{
	u64 v;
	memcpy(&v, p, sizeof(v));
	return v;
}

/* linux/ctype.h semantics on the u8 range used here. */
static inline bool kshim_isascii(u8 c) { return c <= 0x7f; }
static inline bool kshim_isprint(u8 c) { return c >= 0x20 && c <= 0x7e; }

/* Faithful port of lib/hexdump.c hex_dump_to_buffer(): same control
 * flow, same return values; get_unaligned becomes a memcpy load and
 * snprintf the deterministic mini-printer. */
static inline int hex_dump_to_buffer(const void *buf, size_t len,
				     int rowsize, int groupsize,
				     char *linebuf, size_t linebuflen,
				     bool ascii)
{
	const u8 *ptr = buf;
	int ngroups;
	u8 ch;
	int j, lx = 0;
	int ascii_column;
	int ret;

	if (rowsize != 16 && rowsize != 32)
		rowsize = 16;

	if (len > rowsize)		/* limit to one line at a time */
		len = rowsize;
	if (!kshim_is_power_of_2(groupsize) || groupsize > 8)
		groupsize = 1;
	if ((len % groupsize) != 0)	/* no mixed size output */
		groupsize = 1;

	ngroups = len / groupsize;
	ascii_column = rowsize * 2 + rowsize / groupsize + 1;

	if (!linebuflen)
		goto overflow1;

	if (!len)
		goto nil;

	if (groupsize == 8) {
		const u64 *ptr8 = buf;

		for (j = 0; j < ngroups; j++) {
			ret = snprintf(linebuf + lx, linebuflen - lx,
				       "%s%16.16llx", j ? " " : "",
				       (unsigned long long)kshim_get_unaligned_u64(ptr8 + j));
			if (ret >= linebuflen - lx)
				goto overflow1;
			lx += ret;
		}
	} else if (groupsize == 4) {
		const u32 *ptr4 = buf;

		for (j = 0; j < ngroups; j++) {
			ret = snprintf(linebuf + lx, linebuflen - lx,
				       "%s%8.8x", j ? " " : "",
				       (unsigned int)kshim_get_unaligned_u32(ptr4 + j));
			if (ret >= linebuflen - lx)
				goto overflow1;
			lx += ret;
		}
	} else if (groupsize == 2) {
		const u16 *ptr2 = buf;

		for (j = 0; j < ngroups; j++) {
			ret = snprintf(linebuf + lx, linebuflen - lx,
				       "%s%4.4x", j ? " " : "",
				       (unsigned int)kshim_get_unaligned_u16(ptr2 + j));
			if (ret >= linebuflen - lx)
				goto overflow1;
			lx += ret;
		}
	} else {
		for (j = 0; j < len; j++) {
			if (linebuflen < lx + 2)
				goto overflow2;
			ch = ptr[j];
			linebuf[lx++] = hex_asc_hi(ch);
			if (linebuflen < lx + 2)
				goto overflow2;
			linebuf[lx++] = hex_asc_lo(ch);
			if (linebuflen < lx + 2)
				goto overflow2;
			linebuf[lx++] = ' ';
		}
		if (j)
			lx--;
	}
	if (!ascii)
		goto nil;

	while (lx < ascii_column) {
		if (linebuflen < lx + 2)
			goto overflow2;
		linebuf[lx++] = ' ';
	}
	for (j = 0; j < len; j++) {
		if (linebuflen < lx + 2)
			goto overflow2;
		ch = ptr[j];
		linebuf[lx++] = (kshim_isascii(ch) && kshim_isprint(ch)) ? ch : '.';
	}
nil:
	linebuf[lx] = '\0';
	return lx;
overflow2:
	linebuf[lx++] = '\0';
overflow1:
	return ascii ? ascii_column + len : (groupsize * 2 + 1) * ngroups - 1;
}
