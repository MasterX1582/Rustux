#pragma once
/* linux/sprintf.h shim for the lib-seqbuf harness: a DETERMINISTIC
 * MINI-PRINTER standing in for kernel vsnprintf/vscnprintf/bstr_printf.
 *
 * seq_buf_vprintf() and seq_buf_hex_dump() call vsnprintf(); this shim
 * implements the exact subset of the format language the ported code
 * (and the harness's own printf ops) can produce:
 *
 *   flags:    - 0 + space #
 *   width:    digits or * (int arg)
 *   precision: .digits or .* (int arg)
 *   length:   (none)=int, l=long, ll=long long, h/hh=int, z=size_t, t=ptrdiff_t
 *   conv:     % c s d i u x X o p
 *
 * Semantics are libc snprintf's for this subset: writes at most
 * size-1 chars plus a NUL (when size > 0), returns the number of chars
 * that WOULD have been written (excluding NUL).  printf formatting
 * itself is a separate unported unit — this printer is shared verbatim
 * with the Rust port, so both sides produce byte-identical output by
 * construction.
 *
 * %p convention: the harness passes only pointers into its `data` pool
 * to %p (seq_buf_hex_dump's DUMP_PREFIX_ADDRESS prints `ptr + i`).
 * The shim prints `0x` + lowercase hex of (arg - kshim_ptr_base), which
 * is deterministic; the Rust port models the same pointer as a pool
 * offset and prints the identical digits.
 */
#include <linux/types.h>
#include <linux/compiler.h>
#include <stdarg.h>
#include <string.h>

extern uintptr_t kshim_ptr_base;	/* %p bias: base of the data pool */

struct kshim_fmtbuf {
	char *buf;
	size_t size;
	size_t pos;		/* would-be length */
};

static inline void kshim_putc(struct kshim_fmtbuf *f, char c)
{
	if (f->pos + 1 < f->size)
		f->buf[f->pos] = c;
	f->pos++;
}

static inline void kshim_emitn(struct kshim_fmtbuf *f, char c, int n)
{
	while (n-- > 0)
		kshim_putc(f, c);
}

/* emit `s` up to NUL or `prec` chars (prec < 0 = unlimited) */
static inline void kshim_emitstr(struct kshim_fmtbuf *f, const char *s,
				 long prec)
{
	long i = 0;

	while (s[i] && (prec < 0 || i < prec)) {
		kshim_putc(f, s[i]);
		i++;
	}
}

static inline int kshim_vsnprintf(char *buf, size_t size, const char *fmt,
				  va_list ap)
{
	struct kshim_fmtbuf f = { buf, size, 0 };
	const char *p;

	for (p = fmt; *p; p++) {
		int left = 0, zero = 0, plus = 0, space = 0, alt = 0;
		long width = 0, prec = -1;
		int length = 0;			/* 0=int, 1=l, 2=ll, 3=z, 4=t */
		char conv, tmp[32], sign = 0;
		const char *digits;
		uint64_t mag = 0;
		int neg = 0, ndig, pad;

		if (*p != '%') {
			kshim_putc(&f, *p);
			continue;
		}
		p++;
		/* flags */
		for (;; p++) {
			if (*p == '-') left = 1;
			else if (*p == '0') zero = 1;
			else if (*p == '+') plus = 1;
			else if (*p == ' ') space = 1;
			else if (*p == '#') alt = 1;
			else break;
		}
		/* width */
		if (*p == '*') {
			width = va_arg(ap, int);
			if (width < 0) {
				left = 1;
				width = -width;
			}
			p++;
		} else {
			while (*p >= '0' && *p <= '9')
				width = width * 10 + (*p++ - '0');
		}
		/* precision */
		if (*p == '.') {
			p++;
			if (*p == '*') {
				prec = va_arg(ap, int);
				p++;
			} else {
				prec = 0;
				while (*p >= '0' && *p <= '9')
					prec = prec * 10 + (*p++ - '0');
			}
		}
		/* length */
		if (*p == 'l') {
			length = (*++p == 'l') ? (p++, 2) : 1;
		} else if (*p == 'h') {
			if (*++p == 'h')
				p++;
			length = 0;
		} else if (*p == 'z') {
			length = 3; p++;
		} else if (*p == 't') {
			length = 4; p++;
		}
		conv = *p;
		switch (conv) {
		case 'c': {
			char c = (char)va_arg(ap, int);
			if (!left)
				kshim_emitn(&f, ' ', (int)(width - 1));
			kshim_putc(&f, c);
			if (left)
				kshim_emitn(&f, ' ', (int)(width - 1));
			break;
		}
		case 's': {
			const char *s = va_arg(ap, const char *);
			long slen = 0;

			while (s[slen] && (prec < 0 || slen < prec))
				slen++;
			if (!left)
				kshim_emitn(&f, ' ', (int)(width - slen));
			kshim_emitstr(&f, s, prec);
			if (left)
				kshim_emitn(&f, ' ', (int)(width - slen));
			break;
		}
		case 'p': {
			uintptr_t v = (uintptr_t)va_arg(ap, void *);
			int i;

			mag = v - kshim_ptr_base;
			digits = "0123456789abcdef";
			ndig = 0;
			do {
				tmp[ndig++] = digits[mag & 15];
				mag >>= 4;
			} while (mag);
			if (!left)
				kshim_emitn(&f, ' ', (int)(width - ndig - 2));
			kshim_putc(&f, '0');
			kshim_putc(&f, 'x');
			for (i = ndig - 1; i >= 0; i--)
				kshim_putc(&f, tmp[i]);
			if (left)
				kshim_emitn(&f, ' ', (int)(width - ndig - 2));
			break;
		}
		case 'd':
		case 'i': {
			int64_t v;
			if (length == 2)      v = va_arg(ap, long long);
			else if (length == 1) v = va_arg(ap, long);
			else if (length == 3) v = va_arg(ap, ssize_t);
			else if (length == 4) v = va_arg(ap, ptrdiff_t);
			else                  v = va_arg(ap, int);
			if (v < 0) {
				neg = 1;
				mag = (uint64_t)0 - (uint64_t)v;
			} else {
				mag = (uint64_t)v;
			}
			if (neg) sign = '-';
			else if (plus) sign = '+';
			else if (space) sign = ' ';
			goto emit_num;
		}
		case 'u':
		case 'x':
		case 'X':
		case 'o': {
			uint64_t v;
			unsigned int base;
			if (length == 2)      v = va_arg(ap, unsigned long long);
			else if (length == 1) v = va_arg(ap, unsigned long);
			else if (length == 3) v = va_arg(ap, size_t);
			else if (length == 4) v = va_arg(ap, ptrdiff_t);
			else                  v = va_arg(ap, unsigned int);
			mag = v;
			if (conv == 'o') base = 8;
			else if (conv == 'u') base = 10;
			else base = 16;
			digits = (conv == 'X') ? "0123456789ABCDEF"
					     : "0123456789abcdef";
			sign = alt && v ? '#' : 0;
			goto emit_num_base;
		emit_num:
			base = 10;
			digits = "0123456789";
		emit_num_base:
			ndig = 0;
			do {
				tmp[ndig++] = digits[mag % base];
				mag /= base;
			} while (mag);
			/* sign: '#' means 0x/0X/0 prefix for x/X/o */
			pad = (prec > ndig) ? (int)(prec - ndig) : 0;
			{
				int prefix = (sign == '#') ?
					((base == 16) ? 2 : 1) :
					(sign ? 1 : 0);
				int total = ndig + pad + prefix;
				if (!left && !(zero && prec < 0))
					kshim_emitn(&f, ' ',
						    (int)(width - total));
				if (sign == '#') {
					if (base == 16) {
						kshim_putc(&f, '0');
						kshim_putc(&f, conv);
					} else {
						kshim_putc(&f, '0');
					}
				} else if (sign) {
					kshim_putc(&f, sign);
				}
				if (!left && zero && prec < 0)
					kshim_emitn(&f, '0',
						    (int)(width - total));
				kshim_emitn(&f, '0', pad);
				while (ndig)
					kshim_putc(&f, tmp[--ndig]);
				if (left)
					kshim_emitn(&f, ' ',
						    (int)(width - total));
			}
			break;
		}
		case '%':
			kshim_putc(&f, '%');
			break;
		case 0:
			p--;		/* trailing % at NUL: stop */
			break;
		default:
			/* unknown conversion: emit it literally */
			kshim_putc(&f, '%');
			kshim_putc(&f, conv);
			break;
		}
		if (!*p)
			break;
	}
	if (size) {
		size_t end = f.pos < size ? f.pos : size - 1;
		buf[end] = '\0';
	}
	return (int)f.pos;
}

static inline int kshim_snprintf(char *buf, size_t size, const char *fmt, ...)
{
	va_list ap;
	int ret;

	va_start(ap, fmt);
	ret = kshim_vsnprintf(buf, size, fmt, ap);
	va_end(ap);
	return ret;
}

/* seq_buf.c calls the libc names; route them to the deterministic
 * mini-printer.  Defined AFTER the implementations so the names inside
 * this header are literal. */
#define vsnprintf kshim_vsnprintf
#define snprintf kshim_snprintf
#define vscnprintf(buf, size, fmt, args) \
	kshim_vsnprintf(buf, size, fmt, args)
