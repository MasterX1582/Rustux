/* Scalar wrappers around lib/seq_buf.c and the include/linux/seq_buf.h
 * inline layer, so the standard scalar bridge can drive them.
 *
 * Buffer-arg friendly model: struct seq_buf points at a static BSZ
 * backing store; data source for putmem/putmem_hex/hex_dump is a static
 * `data` pool (a "pointer" is data + offset — %p prints the offset,
 * see linux/sprintf.h); strings for puts/printf/prefix live in a static
 * `strpool` (pointer = strpool + offset); to_user copies into `ubuf`
 * through the deterministic copy_to_user shim (kshim_uaccess_ok is the
 * number of bytes the fake user mapping accepts, SIZE_MAX = no fault).
 * printk output lands in a bounded klog which is folded too.
 *
 * Every wrapper returns one order-sensitive checksum folded over each
 * op's return value, then s.len/overflow flags, then FNV-1a of sbuf,
 * ubuf and klog — so buffer bytes, len bookkeeping and printk text are
 * all observable.
 *
 * Out of scope (need other subsystems, documented skips):
 *   seq_buf_print_seq — needs a live struct seq_file
 *   seq_buf_path      — needs d_path()/seq_mangle_path() (VFS)
 *   seq_buf_bprintf   — CONFIG_BINARY_PRINTF left undefined, matching
 *                       the harness config; bstr_printf is its own unit
 * The stubs below keep the .so self-contained; reaching one folds a
 * marker so an accidental call diverges loudly instead of silently.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdarg.h>
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/sprintf.h>
#include <linux/seq_buf.h>
#include <linux/seq_file.h>
#include <linux/printk.h>
#include <linux/uaccess.h>
#include <linux/errno.h>

#define BSZ 256		/* seq_buf backing store capacity */
#define DSZ 192		/* data pool for putmem/hex_dump */
#define SPSZ 512	/* string pool */
#define USZ 128		/* user buffer */
#define KLOG 4096	/* printk sink */

static char sbuf[BSZ];
static char data[DSZ];
static char strpool[SPSZ];
static char ubuf[USZ];
static char klog[KLOG];
static size_t klog_len;

uintptr_t kshim_ptr_base;
size_t kshim_uaccess_ok;

static struct seq_buf s;

/* ---- deterministic kernel-environment shims (see linux/*.h) ---- */

int kshim_printk(const char *fmt, ...)
{
	va_list ap;
	size_t rem = KLOG - klog_len;
	int ret;

	va_start(ap, fmt);
	ret = kshim_vsnprintf(klog + klog_len, rem, fmt, ap);
	va_end(ap);
	klog_len += (size_t)ret < rem ? (size_t)ret : (rem ? rem - 1 : 0);
	return ret;
}

/* ---- unreachable VFS/seq_file stubs (see linux/seq_file.h) ---- */
static uint64_t g_h;
static void fold(uint64_t v);

int seq_write(struct seq_file *m, const void *v, size_t len)
{
	(void)m; (void)v;
	fold(0xdead0001);
	return (int)len;
}
char *d_path(const struct path *path, char *buf, int buflen)
{
	(void)path;
	fold(0xdead0002);
	return buflen > 0 ? (buf[0] = '\0', buf) : buf;
}
char *seq_mangle_path(char *sp, const char *p, const char *esc)
{
	(void)p; (void)esc;
	fold(0xdead0003);
	return sp;
}

/* ---- rng / checksum plumbing (same scheme as the other ports) ---- */

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

static void fold(uint64_t v)
{
	g_h = (g_h ^ v) * 0x100000001b3ULL;
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

static void fold_state(void)
{
	fold(s.len);
	fold(s.size);
	fold(seq_buf_has_overflowed(&s));
	fold(seq_buf_buffer_left(&s));
	fold(seq_buf_used(&s));
}

static void fold_all(void)
{
	fold_state();
	fold(fnv1a(sbuf, BSZ));
	fold(fnv1a(ubuf, USZ));
	fold(fnv1a(klog, klog_len));
	fold(klog_len);
}

/* lay a string of `n` body bytes + NUL into strpool at off; returns off */
static size_t lay_str(size_t off, size_t n)
{
	size_t i;
	for (i = 0; i < n; i++)
		strpool[off + i] = (char)('A' + (xs64() % 58));
	strpool[off + n] = '\0';
	return off;
}

static void reset_all(void)
{
	int i;
	rng_state = 0;
	memset(sbuf, 0, sizeof sbuf);
	memset(data, 0, sizeof data);
	memset(strpool, 0, sizeof strpool);
	memset(ubuf, 0, sizeof ubuf);
	memset(klog, 0, sizeof klog);
	klog_len = 0;
	kshim_ptr_base = (uintptr_t)data;
	kshim_uaccess_ok = (size_t)-1;
	g_h = 0x9e3779b97f4a7c15ULL;
	for (i = 0; i < BSZ; i++)
		sbuf[i] = (char)(0x40 + (i * 37 % 59));	/* deterministic stale bytes */
}

/* printf format table index -> call site; arg types differ per fmt, so
 * each is spelled out (same sites the Rust port mirrors). */
static int do_printf(unsigned int fidx, uint64_t a, uint64_t b, size_t soff)
{
	switch (fidx % 12) {
	case 0: return seq_buf_printf(&s, "x=%d y=%u", (int)a, (unsigned)b);
	case 1: return seq_buf_printf(&s, "s=%s n=%x", strpool + soff,
				      (unsigned)b);
	case 2: return seq_buf_printf(&s, "%5d|%-8x|%c", (int)a,
				      (unsigned)b, (int)(a & 0x7f));
	case 3: return seq_buf_printf(&s, "%.3s:%.*s", strpool + soff,
				      (int)(b % 40), strpool + soff);
	case 4: return seq_buf_printf(&s, "p=%p", (void *)(data + (a % DSZ)));
	case 5: return seq_buf_printf(&s, "%08lx", (unsigned long)(a | 1));
	case 6: return seq_buf_printf(&s, "%%done %04u", (unsigned)(b % 10000));
	case 7: return seq_buf_printf(&s, "%*d", (int)(b % 12), (int)a);
	case 8: return seq_buf_printf(&s, "%-+6i", (int)a);
	case 9: return seq_buf_printf(&s, "%#x %#o", (unsigned)a, (unsigned)b);
	case 10: return seq_buf_printf(&s, "%llu/%llx", (unsigned long long)a,
				       (unsigned long long)b);
	default: return seq_buf_printf(&s, "%.8x:%-10.5s|", (unsigned)b,
				       strpool + soff);
	}
}

/* ============== seeded op-mix driver ==============
 * seed     -> rng
 * bsz_i    -> seq buffer size, iabs % (BSZ+1): covers 0/1/2/exact-fit
 * nops_i   -> op count, iabs % 150
 */
uint64_t wrap_seqbuf_seq(uint64_t seed, int64_t bsz_i, int64_t nops_i)
{
	size_t size = iabs(bsz_i) % (BSZ + 1);
	int nops = (int)(iabs(nops_i) % 150);
	int i;

	reset_all();
	rng_state = seed | 1;
	for (i = 0; i < DSZ; i++)
		data[i] = (char)xs64();
	for (i = 0; i < SPSZ; i++)
		strpool[i] = (char)xs64();
	strpool[SPSZ - 1] = 0;		/* bound stray scans in-pool */

	seq_buf_init(&s, sbuf, (unsigned int)size);

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int op = (int)(r % 16);

		switch (op) {
		case 0: case 1:		/* putc */
			fold(seq_buf_putc(&s, (unsigned char)(r >> 8)));
			break;
		case 2: {		/* puts */
			size_t n = (r >> 8) % 40;
			size_t off = lay_str((r >> 16) % (SPSZ - 64), n);
			fold(seq_buf_puts(&s, strpool + off));
			break;
		}
		case 3: case 4: {	/* putmem */
			size_t off = (r >> 8) % DSZ;
			size_t n = (r >> 20) % 24;
			if (n > DSZ - off)
				n = DSZ - off;
			fold(seq_buf_putmem(&s, data + off, (unsigned)n));
			break;
		}
		case 5: {		/* putmem_hex */
			size_t off = (r >> 8) % DSZ;
			size_t n = (r >> 20) % 20;
			if (n > DSZ - off)
				n = DSZ - off;
			fold(seq_buf_putmem_hex(&s, data + off, (unsigned)n));
			break;
		}
		case 6: {		/* printf */
			size_t off = lay_str((r >> 8) % (SPSZ - 64),
					     (r >> 20) % 40);
			fold(do_printf((unsigned)((r >> 28) % 12),
				       xs64(), xs64(), off));
			break;
		}
		case 7: {		/* get_buf + commit */
			char *p = NULL;
			size_t n = seq_buf_get_buf(&s, &p);
			size_t k = n ? (r >> 8) % (n + 1) : 0;
			size_t j;
			fold(n);
			fold(p ? (uint64_t)(p - sbuf) : 0xffffu);
			for (j = 0; j < k; j++)
				p[j] = (char)xs64();
			if (seq_buf_has_overflowed(&s) || (r & 0x20))
				/* committing >=0 while len > size is a kernel
				 * BUG_ON — out of contract for the harness */
				seq_buf_commit(&s, -1);
			else
				seq_buf_commit(&s, (int)k);
			break;
		}
		case 8:			/* pop */
			fold(seq_buf_pop(&s));
			break;
		case 9:			/* clear */
			seq_buf_clear(&s);
			fold_state();
			break;
		case 10: {		/* init_append w/ seeded content */
			size_t k = (r >> 8) % (size + 2);
			size_t j;
			for (j = 0; j < size; j++)
				sbuf[j] = (char)('a' + ((r + j) % 26));
			if (k < size)
				sbuf[k] = '\0';
			seq_buf_init_append(&s, sbuf, (unsigned int)size);
			fold_state();
			break;
		}
		case 11: {		/* set_overflow / overflowed */
			if (r & 0x40)
				seq_buf_set_overflow(&s);
			fold(seq_buf_has_overflowed(&s));
			fold(seq_buf_buffer_left(&s));
			fold(seq_buf_used(&s));
			break;
		}
		case 12: {		/* str/strlen/terminate */
			switch ((r >> 8) % 3) {
			case 0: {
				const char *t = seq_buf_str(&s);
				fold(fnv1a(t, strlen(t)));
				fold(strlen(t));
				break;
			}
			case 1:
				fold(seq_buf_strlen(&s));
				break;
			default:
				seq_buf_terminate(&s);
				fold(0x5e);
				break;
			}
			fold_state();
			break;
		}
		case 13: {		/* to_user */
			size_t start = (r >> 8) % (size + 2);
			int cnt = (int)((r >> 20) % (USZ + 1));
			if (r & 0x40)
				kshim_uaccess_ok = (r >> 28) % (cnt + 1);
			else
				kshim_uaccess_ok = (size_t)-1;
			fold(seq_buf_to_user(&s, ubuf, start, cnt));
			kshim_uaccess_ok = (size_t)-1;
			fold(fnv1a(ubuf, USZ));
			break;
		}
		case 14: {		/* hex_dump */
			static const int rows[4] = { 16, 32, 8, 24 };
			static const int grps[6] = { 1, 2, 4, 8, 3, 16 };
			size_t off = (r >> 8) % DSZ;
			size_t n = (r >> 20) % 48;
			size_t poff = lay_str((r >> 32) % (SPSZ - 64), 4);
			if (n > DSZ - off)
				n = DSZ - off;
			fold(seq_buf_hex_dump(&s, strpool + poff,
					      (int)((r >> 40) % 4),
					      rows[(r >> 44) % 4],
					      grps[(r >> 48) % 6],
					      data + off, n,
					      (r >> 52) & 1));
			break;
		}
		default:		/* do_printk + state */
			lay_str(0, 4);		/* lvl string */
			seq_buf_do_printk(&s, strpool);
			fold(fnv1a(klog, klog_len));
			fold_state();
			break;
		}
	}
	fold_all();
	return g_h;
}

/* ============== printf-family focus ============== */

uint64_t wrap_seqbuf_printf(uint64_t seed, int64_t bsz_i)
{
	size_t size = iabs(bsz_i) % (BSZ + 1);
	int i;

	reset_all();
	rng_state = seed | 1;
	for (i = 0; i < SPSZ; i++)
		strpool[i] = (char)xs64();
	strpool[SPSZ - 1] = 0;
	for (i = 0; i < DSZ; i++)
		data[i] = (char)xs64();

	seq_buf_init(&s, sbuf, (unsigned int)size);

	for (i = 0; i < 24; i++) {
		uint64_t a = xs64(), b = xs64();
		size_t off = lay_str(i * 17 % (SPSZ - 64), (size_t)(b % 48));
		fold(do_printf((unsigned)(a % 12), a, b, off));
		fold(seq_buf_puts(&s, strpool + (i * 13 % (SPSZ - 64))));
	}
	fold_all();
	return g_h;
}

/* ============== hex_dump focus: all prefixes/row/group/ascii ============== */

uint64_t wrap_seqbuf_hexdump(uint64_t seed, int64_t bsz_i, int64_t n_i)
{
	size_t size = iabs(bsz_i) % (BSZ + 1);
	int n = (int)(iabs(n_i) % 24);
	int i;

	reset_all();
	rng_state = seed | 1;
	for (i = 0; i < DSZ; i++)
		data[i] = (char)xs64();
	for (i = 0; i < SPSZ; i++)
		strpool[i] = (char)xs64();
	strpool[SPSZ - 1] = 0;

	seq_buf_init(&s, sbuf, (unsigned int)size);

	for (i = 0; i < n; i++) {
		static const int rows[4] = { 16, 32, 8, 48 };
		static const int grps[6] = { 1, 2, 4, 8, 3, 16 };
		uint64_t r = xs64();
		size_t off = (r >> 8) % DSZ;
		size_t len = (r >> 20) % 80;
		size_t poff = lay_str((r >> 32) % (SPSZ - 64),
				      (size_t)(r >> 52) % 12);
		if (len > DSZ - off)
			len = DSZ - off;
		fold(seq_buf_hex_dump(&s, strpool + poff,
				      (int)(i % 4) /* rotates all 3 + default */,
				      rows[(r >> 40) % 4],
				      grps[(r >> 44) % 6],
				      data + off, len, (r >> 48) & 1));
	}
	fold_all();
	return g_h;
}
