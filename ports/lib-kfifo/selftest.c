/* Independent sanity checks for the lib/kfifo.c + kfifo.h port unit:
 * canonical values, not the recorded corpus.  Prints FAIL lines and
 * returns the failure count. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <linux/dma-mapping.h>
#include <linux/err.h>
#include <linux/export.h>
#include <linux/kfifo.h>
#include <linux/log2.h>
#include <linux/scatterlist.h>
#include <linux/slab.h>
#include <linux/uaccess.h>

static int fails;
#define CK(expr) do {						\
	if (!(expr)) {						\
		printf("FAIL %d: %s\n", __LINE__, #expr);	\
		fails++;					\
	}							\
} while (0)

int main(void)
{
	static unsigned char buf[64];
	static unsigned char io[64];
	static struct scatterlist sgl[4];
	struct __kfifo f;
	unsigned int n, tail, copied;
	int err;

	/* --- plain byte fifo: init 8, in 5, out 3 ------------------ */
	CK(__kfifo_init(&f, buf, 8, 1) == 0);
	CK(f.mask == 7 && f.esize == 1 && f.in == 0 && f.out == 0);
	CK(__kfifo_in(&f, "abcde", 5) == 5);
	CK(f.in == 5 && f.out == 0);
	CK(__kfifo_out(&f, io, 3) == 3);
	CK(!memcmp(io, "abc", 3));
	CK(f.in == 5 && f.out == 3);
	CK(f.in - f.out == 2);			/* kfifo_len */
	CK(__kfifo_out_peek(&f, io, 8) == 2);	/* clamps to content */
	CK(!memcmp(io, "de", 2));
	CK(f.out == 3);				/* peek leaves out */

	/* full fifo: in returns 0 */
	CK(__kfifo_in(&f, "vwxyz123", 8) == 6);	/* 6 slots left */
	CK(f.in - f.out == 8);
	CK(__kfifo_in(&f, "ZZ", 2) == 0);

	/* empty fifo: out returns 0 */
	CK(__kfifo_out(&f, io, 64) == 8);
	CK(__kfifo_out(&f, io, 1) == 0);

	/* --- wraparound of the ring + u32 counters ----------------- */
	f.in = f.out = 0xfffffff0u;
	CK(__kfifo_in(&f, "0123456789", 10) == 8);	/* clamped to cap */
	CK(f.in == 0xfffffff8u && f.out == 0xfffffff0u);
	CK(__kfifo_out(&f, io, 10) == 8);
	CK(f.out == 0xfffffff8u);
	CK(f.in - f.out == 0);
	/* push counters across the u32 boundary */
	CK(__kfifo_in(&f, io, 8) == 8);
	f.out += 2;
	CK(__kfifo_in(&f, io, 8) == 2);		/* 6 used -> 2 free */
	CK(f.in == 2 && f.out == 0xfffffffau);	/* wrapped past 2^32 */
	CK(f.in - f.out == 8);

	/* --- __kfifo_init rounding -------------------------------- */
	CK(__kfifo_init(&f, buf, 12, 1) == 0);	/* 12 -> rounddown 8 */
	CK(f.mask == 7);
	CK(__kfifo_init(&f, buf, 16, 1) == 0);
	CK(f.mask == 15);
	CK(__kfifo_init(&f, buf, 1, 1) == -EINVAL);
	/* size 0 is upstream UB (rounddown_pow_of_two(0)) — not covered */

	/* --- element addressing: esize 4 --------------------------- */
	CK(__kfifo_init(&f, buf, 32, 4) == 0);	/* 8 elements of 4 */
	CK(f.mask == 7 && f.esize == 4);
	{
		uint32_t v[3] = { 0xdeadbeef, 0x11223344, 0xa5a5a5a5 };
		uint32_t w[3] = { 0, 0, 0 };
		CK(__kfifo_in(&f, v, 3) == 3);
		CK(__kfifo_out(&f, w, 2) == 2);
		CK(w[0] == 0xdeadbeef && w[1] == 0x11223344);
	}

	/* --- out_linear: contiguous run to end of buffer ------------ */
	CK(__kfifo_init(&f, buf, 8, 1) == 0);
	CK(__kfifo_in(&f, "abcdefgh", 8) == 8);
	f.out += 5; f.in = f.out + 8;		/* in near end: out=5 */
	/* out=5..7 linear run is 3 bytes, then wraps */
	CK(__kfifo_in(&f, "ABCDEFGH", 0) == 0);
	f.in = 8; f.out = 5;			/* 3 bytes: f,g,h */
	n = __kfifo_out_linear(&f, &tail, 8);
	CK(n == 3 && tail == 5);		/* capped at end of buffer */
	n = __kfifo_out_linear(&f, &tail, 2);
	CK(n == 2 && tail == 5);		/* capped by request */

	/* --- record fifo, recsize 1 -------------------------------- */
	CK(__kfifo_init(&f, buf, 16, 1) == 0);
	CK(__kfifo_in_r(&f, "abc", 3, 1) == 3);	/* len-hdr + 3 data */
	CK(f.in == 4);
	CK(__kfifo_len_r(&f, 1) == 3);		/* record length */
	CK(__kfifo_in_r(&f, "de", 2, 1) == 2);
	CK(f.in == 7);
	CK(__kfifo_out_peek_r(&f, io, 8, 1) == 3);
	CK(!memcmp(io, "abc", 3));
	CK(__kfifo_out_r(&f, io, 8, 1) == 3);	/* consumes whole rec */
	CK(f.out == 4);
	CK(__kfifo_out_r(&f, io, 8, 1) == 2);
	CK(!memcmp(io, "de", 2));
	CK(f.in == f.out);
	CK(__kfifo_out_r(&f, io, 8, 1) == 0);	/* empty -> 0 */

	/* record too long for the space: in_r returns 0, all-or-nothing */
	CK(__kfifo_in_r(&f, io, 16, 1) == 0);	/* 16+1 > 16 */
	CK(f.in == f.out);
	CK(__kfifo_max_r(100, 1) == 100);
	CK(__kfifo_max_r(300, 1) == 255);	/* u8 header clamps */
	CK(__kfifo_max_r(70000, 2) == 65535);
	CK(__kfifo_max_r(200, 2) == 200);

	/* recsize 2 header */
	CK(__kfifo_init(&f, buf, 16, 1) == 0);
	CK(__kfifo_in_r(&f, "hello", 5, 2) == 5);
	CK(f.in == 7);				/* 2-byte header + 5 */
	CK(__kfifo_len_r(&f, 2) == 5);
	__kfifo_skip_r(&f, 2);
	CK(f.out == 7);

	/* --- user copies (memcpy shims, always succeed) ------------- */
	CK(__kfifo_init(&f, buf, 16, 1) == 0);
	err = __kfifo_from_user(&f, "xy", 2, &copied);
	CK(err == 0 && copied == 2 && f.in == 2);
	err = __kfifo_to_user(&f, io, 4, &copied);
	CK(err == 0 && copied == 2 && !memcmp(io, "xy", 2));
	CK(f.out == 2);
	err = __kfifo_from_user_r(&f, "pq", 2, &copied, 1);
	CK(err == 0 && copied == 2 && f.in == 5);	/* hdr + 2 */
	err = __kfifo_to_user_r(&f, io, 8, &copied, 1);
	CK(err == 0 && copied == 2 && !memcmp(io, "pq", 2));
	CK(f.out == 5);

	/* --- DMA scatterlists --------------------------------------- */
	CK(__kfifo_init(&f, buf, 8, 1) == 0);
	f.in = 7; f.out = 6;			/* 1 used, 7 free, in=7 */
	/* dma in len 4 at in=7: splits 1 byte [7..8) + 3 bytes [0..3) */
	n = __kfifo_dma_in_prepare(&f, sgl, 4, 4, DMA_MAPPING_ERROR);
	CK(n == 2);
	CK(sgl[0].length == 1 && sgl[1].length == 3);
	CK((unsigned char *)sgl[0].buf == buf + 7);
	CK((unsigned char *)sgl[1].buf == buf);
	f.in += 4;
	/* dma out 5 at out=6: 2 bytes [6..8) + 3 bytes [0..3) */
	n = __kfifo_dma_out_prepare(&f, sgl, 2, 5, DMA_MAPPING_ERROR);
	CK(n == 2);
	CK(sgl[0].length == 2 && sgl[1].length == 3);
	/* len clamps to available */
	n = __kfifo_dma_out_prepare(&f, sgl, 4, 99, DMA_MAPPING_ERROR);
	CK(n == 2);				/* avail=5 -> same split */
	/* mapped variant fills dma fields */
	n = __kfifo_dma_out_prepare(&f, sgl, 2, 5, 0x1000);
	CK(sg_dma_address(&sgl[0]) == 0x1000 + 6);
	CK(sg_dma_len(&sgl[0]) == 2);
	CK(sg_dma_address(&sgl[1]) == 0x1000);
	CK(sg_dma_len(&sgl[1]) == 3);
	/* nents limits entries: 1 entry -> only the first run */
	n = __kfifo_dma_out_prepare(&f, sgl, 1, 5, 0x1000);
	CK(n == 1 && sgl[0].length == 2);

	/* record dma: prepare pokes nothing, finish writes the header */
	CK(__kfifo_init(&f, buf, 16, 1) == 0);
	n = __kfifo_dma_in_prepare_r(&f, sgl, 4, 4, 1, DMA_MAPPING_ERROR);
	CK(n == 1 && sgl[0].length == 4);
	CK((unsigned char *)sgl[0].buf == buf + 1); /* off in+recsize */
	__kfifo_dma_in_finish_r(&f, 4, 1);
	CK(f.in == 5 && __kfifo_len_r(&f, 1) == 4);
	n = __kfifo_dma_out_prepare_r(&f, sgl, 4, 4, 1, DMA_MAPPING_ERROR);
	CK(n == 1 && sgl[0].length == 4);

	/* --- alloc / free ------------------------------------------- */
	CK(__kfifo_alloc_node(&f, 5, 1, GFP_KERNEL, NUMA_NO_NODE) == 0);
	CK(f.mask == 7 && f.data != NULL);	/* 5 -> roundup 8 */
	__kfifo_free(&f);
	CK(f.data == NULL && f.mask == 0 && f.in == 0 && f.out == 0);
	CK(__kfifo_alloc_node(&f, 1, 1, GFP_KERNEL, NUMA_NO_NODE) == -EINVAL);
	CK(f.data == NULL);
	CK(__kfifo_alloc_node(&f, 0, 1, GFP_KERNEL, NUMA_NO_NODE) == -EINVAL);
	CK(__kfifo_alloc_node(&f, 8, 4, GFP_KERNEL, NUMA_NO_NODE) == 0);
	CK(f.mask == 7 && f.esize == 4);
	__kfifo_free(&f);

	/* --- typed header layer -------------------------------------- */
	{
		static struct kfifo tf;
		static struct kfifo_rec_ptr_1 rf;
		static unsigned char tb[16];
		unsigned char v = 0;

		CK(kfifo_init(&tf, tb, 16) == 0);
		CK(kfifo_initialized(&tf) && kfifo_size(&tf) == 16);
		CK(kfifo_is_empty(&tf) && !kfifo_is_full(&tf));
		CK(kfifo_esize(&tf) == 1 && kfifo_recsize(&tf) == 0);
		CK(kfifo_put(&tf, 0x41) == 1);
		CK(kfifo_put(&tf, 0x42) == 1);
		CK(kfifo_len(&tf) == 2 && kfifo_avail(&tf) == 14);
		CK(kfifo_peek(&tf, &v) == 1 && v == 0x41);
		CK(kfifo_get(&tf, &v) == 1 && v == 0x41);
		CK(kfifo_len(&tf) == 1);
		CK(kfifo_in(&tf, "xyz", 3) == 3);
		CK(kfifo_len(&tf) == 4);
		CK(kfifo_out(&tf, io, 2) == 2 && !memcmp(io, "Bx", 2));
		kfifo_reset(&tf);
		CK(kfifo_is_empty(&tf));
		CK(kfifo_init(&tf, tb, 12) == 0);	/* 12 -> 8 */
		CK(kfifo_size(&tf) == 8);

		CK(kfifo_init(&rf, tb, 16) == 0);
		CK(kfifo_recsize(&rf) == 1);
		CK(kfifo_in(&rf, "ab", 2) == 2);	/* dispatches to _r */
		CK(kfifo_peek_len(&rf) == 2);
		CK(kfifo_len(&rf) == 3);		/* hdr + data */
		CK(kfifo_out(&rf, io, 8) == 2 && !memcmp(io, "ab", 2));
		CK(kfifo_is_empty(&rf));
	}

	/* in-place fifo: INIT_KFIFO + direct-buf element path */
	{
		static STRUCT_KFIFO(unsigned char, 8) itf;
		unsigned char v = 0;

		INIT_KFIFO(itf);
		CK(itf.kfifo.mask == 7 && itf.kfifo.data == itf.buf);
		CK(kfifo_put(&itf, 0x55) == 1);
		CK(itf.buf[0] == 0x55);
		CK(kfifo_get(&itf, &v) == 1 && v == 0x55);
		CK(kfifo_init(&itf, buf, 8) == -EINVAL); /* not a ptr fifo */
	}

	if (!fails)
		printf("selftest: all checks passed\n");
	return fails;
}
