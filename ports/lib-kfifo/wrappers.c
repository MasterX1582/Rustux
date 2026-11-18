/* Scalar wrappers around lib/kfifo.c + include/linux/kfifo.h for the
 * rust-anything differential port, same convention as ports/lib-rbtree:
 * the fifo is caller-buffer state, a seeded op sequence drives it, every
 * op's return value plus the in/out counters are folded through FNV-1a,
 * and a final checksum covers the whole backing buffer + {in,out,mask,
 * esize}.  Determinism: all content comes from a seeded xorshift64.
 *
 * recsize: 0 = plain fifo, 1 = u8 record header, 2 = u16.  recsize>0
 * forces esize 1 (the kernel only ever instantiates record fifos over
 * unsigned char).  Generated lengths deliberately exceed capacity,
 * record maxima (__kfifo_max_r), and remaining space, so the partial-op
 * and clamp paths all fire.
 *
 * Memory-safety contract kept by the driver (upstream relies on it —
 * breaking it reads/writes past the backing buffer):
 *   in - out <= mask + 1.
 * Ops that advance fifo->out by a *peeked header* length (out_r, skip_r,
 * to_user_r, the record skip inside dma_out_finish) are preflighted
 * against __kfifo_len_r(); explicit "out += count" skips are preflighted
 * against len.  Peek-style record ops run unguarded: their copy clamps
 * to the peeked record length, which is deterministic garbage at worst,
 * never a counter violation.  Record-mode out-path request lengths are
 * capped at mask+1 because copy_out wraps at most once.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/dma-mapping.h>
#include <linux/err.h>
#include <linux/export.h>
#include <linux/kfifo.h>
#include <linux/log2.h>
#include <linux/scatterlist.h>
#include <linux/slab.h>
#include <linux/uaccess.h>

#define BMAX	4096		/* fifo backing: 512 elements * max esize 8 */
#define IOMAX	8192		/* io buffer: also covers 1024*8 alloc'd fifo */
#define MAXSG	8
#define NOSG	0xffffffffu	/* folded when a guarded op doesn't run */

static unsigned char fbuf[BMAX] __aligned(64);
static unsigned char iobuf[IOMAX] __aligned(64);
static struct scatterlist sgl[MAXSG];
static struct __kfifo F;			/* __kfifo-level fifo */
static struct kfifo tf0;			/* typed ptr fifo, recsize 0 */
static struct kfifo_rec_ptr_1 tf1;		/* typed ptr fifo, recsize 1 */
static struct kfifo_rec_ptr_2 tf2;		/* typed ptr fifo, recsize 2 */
static STRUCT_KFIFO(unsigned char, 64) itf;	/* in-place fifo, recsize 0 */
static spinlock_t tlock;

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static uint64_t iabs64(int64_t v)
{
	return v < 0 ? 0ULL - (uint64_t)v : (uint64_t)v;
}

static uint64_t g_h;
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

static void fill_rng(void *b, size_t n)
{
	size_t i;
	for (i = 0; i < n; i++)
		((unsigned char *)b)[i] = (unsigned char)xs64();
}

static void reset_h(uint64_t seed)
{
	rng_state = seed | 1;
	g_h = 0x9e3779b97f4a7c15ULL;
}

/* The next record's declared length plus its header fits the occupied
 * space — i.e. consuming it keeps out <= in.  Mirrors the caller-side
 * contract real record-fifo users keep. */
static int rec_fits(struct __kfifo *f, unsigned int recsize)
{
	return f->in != f->out &&
	       __kfifo_len_r(f, recsize) + recsize <= f->in - f->out;
}

/* =================== __kfifo_* op-sequence driver =================== */

static void drive_seq(struct __kfifo *f, unsigned int recsize,
		      unsigned int nops)
{
	unsigned int cap = f->mask + 1;
	unsigned int i;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		unsigned int t = (unsigned)(r % 100);
		uint64_t big = xs64();
		/* request length: normally within [0, cap+64), sometimes
		 * far oversized so the record clamps / early-outs fire */
		unsigned int len = (unsigned)(xs64() % (cap + 64));
		unsigned int rlen = len % (cap + 1);
		unsigned int n = 0, copied = 0, tail = 0;
		int err = 0;

		if (big % 7 == 0)
			len += (unsigned)(big % 66000);

		if (t < 30) {				/* in */
			fill_rng(iobuf, IOMAX);
			if (recsize)
				n = __kfifo_in_r(f, iobuf, len, recsize);
			else
				n = __kfifo_in(f, iobuf, len);
			fold(n);
		} else if (t < 52) {			/* out */
			if (recsize) {
				if (rec_fits(f, recsize))
					n = __kfifo_out_r(f, iobuf, rlen,
							  recsize);
				else
					n = NOSG;
			} else {
				n = __kfifo_out(f, iobuf, len);
			}
			fold(n);
			if (n != NOSG)
				fold(fnv1a(iobuf, (size_t)n * f->esize));
		} else if (t < 64) {			/* out_peek */
			if (recsize)
				n = __kfifo_out_peek_r(f, iobuf, rlen,
						       recsize);
			else
				n = __kfifo_out_peek(f, iobuf, len);
			fold(n);
			fold(fnv1a(iobuf, (size_t)n * f->esize));
		} else if (t < 72) {			/* out_linear */
			if (recsize)
				n = __kfifo_out_linear_r(f, &tail, rlen,
							 recsize);
			else
				n = __kfifo_out_linear(f, &tail, len);
			fold(n);
			fold(tail);
			if (n) {
				unsigned int off = tail & f->mask;
				unsigned int run = n;
				if (off + run > cap)
					run = cap - off;
				fold(fnv1a((unsigned char *)f->data +
					   (size_t)off * f->esize,
					   (size_t)run * f->esize));
			}
		} else if (t < 80) {			/* skip */
			if (recsize) {
				if (rec_fits(f, recsize)) {
					__kfifo_skip_r(f, recsize);
					n = 1;
				} else {
					n = NOSG;
				}
			} else if (rlen <= f->in - f->out) {
				f->out += rlen;		/* kfifo_skip_count */
				n = rlen;
			} else {
				n = NOSG;
			}
			fold(n);
		} else if (t < 85) {			/* from_user */
			fill_rng(iobuf, IOMAX);
			if (recsize)
				err = __kfifo_from_user_r(f, iobuf, len,
							  &copied, recsize);
			else
				err = __kfifo_from_user(f, iobuf, len,
							&copied);
			fold((unsigned int)err);
			fold(copied);
		} else if (t < 90) {			/* to_user */
			if (recsize) {
				if (rec_fits(f, recsize)) {
					fill_rng(iobuf, IOMAX);
					err = __kfifo_to_user_r(f, iobuf,
						rlen, &copied, recsize);
				} else {
					err = -1;
					copied = NOSG;
				}
			} else {
				fill_rng(iobuf, IOMAX);
				err = __kfifo_to_user(f, iobuf, len, &copied);
			}
			fold((unsigned int)err);
			fold(copied);
			if (copied != NOSG)
				fold(fnv1a(iobuf,
					   copied < IOMAX ? copied : IOMAX));
		} else if (t < 95) {			/* queries */
			if (recsize) {
				fold(__kfifo_len_r(f, recsize));
				fold(__kfifo_max_r(len, recsize));
			} else {
				fold(f->in - f->out);	  /* kfifo_len */
				fold(cap - (f->in - f->out)); /* unused */
			}
		} else {				/* reset */
			if (t & 1) {
				f->out = f->in;		/* kfifo_reset_out */
				fold(0xdead);
			} else {
				f->in = f->out = 0;	/* kfifo_reset */
				fold(0xbeef);
			}
		}
		fold(f->in);
		fold(f->out);
	}
}

uint64_t wrap_kfifo_seq(uint64_t seed, int64_t sp_i, int64_t esize_i,
			int64_t recsize_i, int64_t nops_i, int64_t mode_i)
{
	uint64_t m = iabs64(mode_i);
	unsigned int sp = 3 + (unsigned)(iabs64(sp_i) % 7);
	unsigned int elems = 1u << sp;
	unsigned int esize = 1u << (iabs64(esize_i) % 4);
	unsigned int recsize = (unsigned)(iabs64(recsize_i) % 3);
	unsigned int nops = (unsigned)(iabs64(nops_i) % 300);
	unsigned int initsz;
	int ret;

	reset_h(seed);
	if (recsize)
		esize = 1;
	fill_rng(fbuf, BMAX);

	initsz = elems * esize;
	if (m & 1)
		initsz = (elems + elems / 2) * esize + 1;  /* -> rounddown */
	if (m & 2)
		initsz = esize;				 /* -> -EINVAL */
	ret = __kfifo_init(&F, fbuf, initsz, esize);
	fold((unsigned int)ret);
	fold(F.mask);
	if (m & 4)		/* counter wraparound: in/out near 2^32 */
		F.in = F.out = 0xffffff00u + (unsigned)(seed & 0xff);
	if (m & 8)
		fill_rng(fbuf, BMAX);	/* alternate buffer content */

	drive_seq(&F, recsize, nops);

	fold(F.in);
	fold(F.out);
	fold(F.mask);
	fold(F.esize);
	fold(fnv1a(fbuf, BMAX));
	return g_h;
}

/* ===================== DMA scatterlist sequences ===================== */

static void fold_sg(const struct scatterlist *sg)
{
	fold(sg_dma_address(sg));
	fold(sg_dma_len(sg));
	fold(sg->length);
	fold(sg->offset);
}

static void drive_dma(struct __kfifo *f, unsigned int recsize,
		      unsigned int nops, int mapped)
{
	unsigned int cap = f->mask + 1;
	unsigned int i;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		unsigned int t = (unsigned)(r % 100);
		uint64_t big = xs64();
		unsigned int len = (unsigned)(xs64() % (cap + 64));
		unsigned int nents = 1 + (unsigned)(xs64() % MAXSG);
		unsigned int n = 0, j;
		dma_addr_t dma;

		if (big % 7 == 0)
			len += (unsigned)(big % 66000);
		if (!recsize)
			nents = (unsigned)(xs64() % (MAXSG + 1));
		dma = mapped ? (0x40000000ULL + (big & 0xffffff))
			     : DMA_MAPPING_ERROR;

		if (t < 40) {			/* dma in */
			unsigned int unused = cap - (f->in - f->out);
			if (recsize)
				n = __kfifo_dma_in_prepare_r(f, sgl, nents,
						len, recsize, dma);
			else
				n = __kfifo_dma_in_prepare(f, sgl, nents,
							   len, dma);
			fold(n);
			for (j = 0; j < n; j++) {
				fold_sg(&sgl[j]);
				fill_rng(sgl[j].buf, sgl[j].length);
			}
			if (n) {
				if (recsize)
					__kfifo_dma_in_finish_r(f, len,
								recsize);
				else
					f->in += len < unused ? len : unused;
			}
			fold(f->in);
		} else if (t < 75) {		/* dma out */
			unsigned int avail = f->in - f->out;
			if (recsize)
				n = __kfifo_dma_out_prepare_r(f, sgl, nents,
						len, recsize, dma);
			else
				n = __kfifo_dma_out_prepare(f, sgl, nents,
							    len, dma);
			fold(n);
			for (j = 0; j < n; j++) {
				fold_sg(&sgl[j]);
				fold(fnv1a(sgl[j].buf, sgl[j].length));
			}
			if (n) {
				if (recsize) {
					if (rec_fits(f, recsize)) {
						__kfifo_skip_r(f, recsize);
						fold(1);
					} else {
						fold(NOSG);
					}
				} else {
					unsigned int l =
						len < avail ? len : avail;
					f->out += l;
					fold(l);
				}
			}
		} else {			/* plain in/out keep data moving */
			unsigned int n2 = (unsigned)(xs64() % (cap + 1));
			if (t & 1) {
				fill_rng(iobuf, n2);
				if (recsize)
					fold(__kfifo_in_r(f, iobuf, n2,
							  recsize));
				else
					fold(__kfifo_in(f, iobuf, n2));
			} else if (recsize) {
				if (rec_fits(f, recsize))
					fold(__kfifo_out_r(f, iobuf, n2,
							   recsize));
				else
					fold(NOSG);
			} else {
				fold(__kfifo_out(f, iobuf, n2));
			}
		}
		fold(f->in);
		fold(f->out);
	}
}

uint64_t wrap_kfifo_dma_seq(uint64_t seed, int64_t sp_i, int64_t esize_i,
			    int64_t recsize_i, int64_t nops_i, int64_t mode_i)
{
	uint64_t m = iabs64(mode_i);
	unsigned int sp = 3 + (unsigned)(iabs64(sp_i) % 7);
	unsigned int elems = 1u << sp;
	unsigned int esize = 1u << (iabs64(esize_i) % 4);
	unsigned int recsize = (unsigned)(iabs64(recsize_i) % 3);
	unsigned int nops = (unsigned)(iabs64(nops_i) % 300);
	int ret;

	reset_h(seed);
	if (recsize)
		esize = 1;
	fill_rng(fbuf, BMAX);
	sg_init_table(sgl, MAXSG);

	ret = __kfifo_init(&F, fbuf, elems * esize, esize);
	fold((unsigned int)ret);
	fold(F.mask);
	if (m & 4)
		F.in = F.out = 0xffffff00u + (unsigned)(seed & 0xff);

	drive_dma(&F, recsize, nops, !(m & 1));

	fold(F.in);
	fold(F.out);
	fold(F.mask);
	fold(F.esize);
	fold(fnv1a(fbuf, BMAX));
	return g_h;
}

/* ================= alloc/free lifecycle + mini sequence ============= */

uint64_t wrap_kfifo_alloc(uint64_t seed, int64_t size_i, int64_t esize_i,
			  int64_t nops_i, int64_t mode_i)
{
	struct __kfifo f;
	uint64_t m = iabs64(mode_i);
	unsigned int size = (unsigned)(iabs64(size_i) % 600);
	unsigned int esize = 1u << (iabs64(esize_i) % 4);
	unsigned int recsize = (unsigned)(m % 3);
	int ret;

	reset_h(seed);
	if (recsize)
		esize = 1;
	fold(size);
	fold(esize);
	ret = __kfifo_alloc_node(&f, size, esize, GFP_KERNEL, NUMA_NO_NODE);
	fold((unsigned int)ret);
	fold(f.mask);
	fold(f.esize);
	fold(f.data != NULL);
	if (!ret) {
		/* malloc'd content is indeterminate — seed it */
		fill_rng(f.data, (size_t)(f.mask + 1) * f.esize);
		drive_seq(&f, recsize, (unsigned)(iabs64(nops_i) % 60));
		fold(fnv1a(f.data, (size_t)(f.mask + 1) * f.esize));
		__kfifo_free(&f);
		fold(f.in);
		fold(f.out);
		fold(f.mask);
		fold(f.esize);
		fold(f.data != NULL);
	}
	return g_h;
}

/* ============== typed kfifo.h macro-layer sequence =================== */

/* REC_FITS_T: same contract as rec_fits() expressed through the typed
 * query macros — the next record (header + declared length) fits. */
#define REC_FITS_T(fp) \
	(!kfifo_is_empty(fp) && \
	 kfifo_peek_len(fp) + kfifo_recsize(fp) <= kfifo_len(fp))

#define TYPED_RUN(fp) do {                                                \
	unsigned int __cap = (fp)->kfifo.mask + 1;                      \
	unsigned int __i;                                             \
	for (__i = 0; __i < tnops; __i++) {                             \
		uint64_t r = xs64();                                  \
		unsigned int t = (unsigned)(r % 100);                 \
		unsigned int n = (unsigned)(xs64() % (__cap + 64));   \
		unsigned int rn = n % (__cap + 1);                    \
		unsigned int want = kfifo_recsize(fp) ? rn : n;       \
		unsigned char v = 0;                                  \
		if (t < 28) {                                         \
			fill_rng(iobuf, want);                        \
			fold(kfifo_in((fp), iobuf, want));            \
		} else if (t < 46) {                                  \
			if (kfifo_recsize(fp) ? REC_FITS_T(fp) : 1) { \
				unsigned int __n =                  \
					kfifo_out((fp), iobuf, want);   \
				fold(__n);                              \
				fold(fnv1a(iobuf, __n));              \
			} else {                                      \
				fold(NOSG);                           \
			}                                             \
		} else if (t < 54) {                                  \
			v = (unsigned char)xs64();                    \
			fold(kfifo_put((fp), v));                     \
		} else if (t < 62) {                                  \
			if (kfifo_recsize(fp) ? REC_FITS_T(fp) : 1) { \
				fold(kfifo_get((fp), &v));            \
				fold(v);                              \
			} else {                                      \
				fold(NOSG);                           \
			}                                             \
		} else if (t < 70) {                                  \
			fold(kfifo_peek((fp), &v));                   \
			fold(v);                                      \
		} else if (t < 78) {                                  \
			if (kfifo_recsize(fp) ? REC_FITS_T(fp) : 1) { \
				unsigned int __n =                  \
				    kfifo_out_peek((fp), iobuf, want);\
				fold(__n);                            \
				fold(fnv1a(iobuf, __n));              \
			} else {                                      \
				fold(kfifo_peek_len((fp)));           \
			}                                             \
		} else if (t < 86) {                                  \
			unsigned int c = rn % 4;                      \
			if (kfifo_recsize(fp) ? REC_FITS_T(fp) :      \
			    (c <= kfifo_len(fp))) {                   \
				kfifo_skip_count((fp), c);            \
				fold(c);                              \
			} else {                                      \
				fold(NOSG);                           \
			}                                             \
		} else if (t < 91) {                                  \
			unsigned int dn = rn % 8;                     \
			if (dn + kfifo_recsize(fp) <=                 \
			    kfifo_size(fp) - kfifo_len(fp)) {         \
				kfifo_dma_in_finish((fp), dn);        \
				fold(dn);                             \
			} else {                                      \
				fold(NOSG);                           \
			}                                             \
		} else if (t < 95) {                                  \
			if (kfifo_recsize(fp) ? REC_FITS_T(fp) :      \
			    (rn <= kfifo_len(fp))) {                  \
				kfifo_dma_out_finish((fp), rn);       \
				fold(rn);                             \
			} else {                                      \
				fold(NOSG);                           \
			}                                             \
		} else if (t < 98) {                                  \
			fill_rng(iobuf, 4);                           \
			fold(kfifo_in_spinlocked_noirqsave((fp),      \
							 iobuf, rn % 4,  \
							 &tlock));      \
			fold(kfifo_is_empty_spinlocked((fp), &tlock)); \
		} else {                                              \
			kfifo_reset((fp));                            \
			fold(0x7e57);                                 \
		}                                                     \
		fold(kfifo_len((fp)));                                \
		fold(kfifo_avail((fp)));                              \
		fold(kfifo_is_empty((fp)));                           \
		fold(kfifo_is_full((fp)));                            \
		fold(kfifo_peek_len((fp)));                           \
	}                                                             \
	fold(kfifo_size((fp)));                                       \
	fold(kfifo_esize((fp)));                                      \
	fold(kfifo_recsize((fp)));                                    \
	fold(kfifo_initialized((fp)) ? 1 : 0);                        \
	fold(fnv1a((fp)->kfifo.data,                                \
		   ((fp)->kfifo.mask + 1) * (fp)->kfifo.esize));      \
} while (0)

uint64_t wrap_kfifo_typed(uint64_t seed, int64_t sp_i, int64_t nops_i,
			  int64_t mode_i)
{
	uint64_t m = iabs64(mode_i);
	unsigned int sp = 3 + (unsigned)(iabs64(sp_i) % 7);
	unsigned int elems = 1u << sp;
	unsigned int tnops = (unsigned)(iabs64(nops_i) % 300);
	unsigned int kind = (unsigned)(m % 4);
	unsigned int initsz = elems;

	reset_h(seed);
	fill_rng(fbuf, BMAX);
	if (m & 8)
		initsz = elems + elems / 2 + 1;	/* non-pow2: rounds down */

	switch (kind) {
	case 0:
		fold(kfifo_init(&tf0, fbuf, initsz));
		if (m & 4)
			tf0.kfifo.in = tf0.kfifo.out =
				0xffffff00u + (unsigned)(seed & 0xff);
		TYPED_RUN(&tf0);
		break;
	case 1:
		fold(kfifo_init(&tf1, fbuf, initsz));
		if (m & 4)
			tf1.kfifo.in = tf1.kfifo.out =
				0xffffff00u + (unsigned)(seed & 0xff);
		TYPED_RUN(&tf1);
		break;
	case 2:
		fold(kfifo_init(&tf2, fbuf, initsz));
		if (m & 4)
			tf2.kfifo.in = tf2.kfifo.out =
				0xffffff00u + (unsigned)(seed & 0xff);
		TYPED_RUN(&tf2);
		break;
	default:
		/* in-place fifo: INIT_KFIFO sets it up; kfifo_init on an
		 * in-place object must fail with -EINVAL — fold both. */
		fold(kfifo_init(&itf, fbuf, initsz));
		INIT_KFIFO(itf);
		if (m & 4)
			itf.kfifo.in = itf.kfifo.out =
				0xffffff00u + (unsigned)(seed & 0xff);
		TYPED_RUN(&itf);
		break;
	}
	return g_h;
}
