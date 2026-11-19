/* Invariant self-test (not part of the shipped port): hand-computed
 * mini sequences for both units plus a seeded mix with a shadow model
 * for percpu_counter and a reader-consistency walk for the printk
 * ringbuffer.  Exit 0 = all good.
 *
 * Sequential model: kshim_cpu picks the simulated CPU, locks are
 * no-ops, panic flags stay off so every read path terminates.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/kernel.h>
#include <linux/percpu.h>
#include <linux/percpu_counter.h>
#include <linux/errno.h>
#include <linux/gfp.h>
#include <linux/bug.h>

#include "printk_ringbuffer.h"
#include "internal.h"

static int nfail;
#define CHECK(cond) do {						\
	if (!(cond)) {							\
		fprintf(stderr, "FAIL %d: %s\n", __LINE__, #cond);	\
		nfail++;						\
	}								\
} while (0)

/* ---- shim globals (same as wrappers.c) ---- */
int kshim_cpu;
const unsigned long kshim_cpu_mask;
unsigned long kshim_warn_hits;
bool kshim_panic;
bool debug_non_panic_cpus;
bool legacy_allow_panic_sync;
char kshim_pcpu_area[KSHIM_NCPU][KSHIM_PCPU_STRIDE] __aligned(64);

#define PCPU_SLOT_SHIFT 5
#define PCPU_NSLOTS (KSHIM_PCPU_STRIDE >> PCPU_SLOT_SHIFT)
static uint8_t pcpu_slot_used[PCPU_NSLOTS];
static int pcpu_alloc_fail;	/* -1 never, else fail the Nth alloc */

void *__alloc_percpu_gfp(size_t size, size_t align, gfp_t gfp)
{
	unsigned int i, cpu;
	(void)align; (void)gfp;
	if (pcpu_alloc_fail == 0)
		return NULL;
	if (pcpu_alloc_fail > 0)
		pcpu_alloc_fail--;
	for (i = 0; i < PCPU_NSLOTS; i++) {
		if (!pcpu_slot_used[i] && size <= (1U << PCPU_SLOT_SHIFT)) {
			pcpu_slot_used[i] = 1;
			for (cpu = 0; cpu < KSHIM_NCPU; cpu++)
				memset(&kshim_pcpu_area[cpu][i << PCPU_SLOT_SHIFT],
				       0, 1U << PCPU_SLOT_SHIFT);
			return &kshim_pcpu_area[0][i << PCPU_SLOT_SHIFT];
		}
	}
	return NULL;
}

void *__alloc_percpu(size_t size, size_t align)
{
	return __alloc_percpu_gfp(size, align, GFP_KERNEL);
}

void free_percpu(void __percpu *ptr)
{
	char *p = ptr;
	unsigned int slot;
	if (!p)
		return;
	slot = (unsigned int)(p - &kshim_pcpu_area[0][0]) >> PCPU_SLOT_SHIFT;
	pcpu_slot_used[slot] = 0;
}

static uint64_t rng_state = 0x243f6a8885a308d3ULL;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static int slots_busy(void)
{
	int i, n = 0;
	for (i = 0; i < PCPU_NSLOTS; i++)
		n += pcpu_slot_used[i];
	return n;
}

/* ================= percpu_counter invariants ================= */

static void pcpu_hand(void)
{
	struct percpu_counter fbc, g, many[4];

	percpu_counter_batch = 8;
	memset(&fbc, 0, sizeof fbc);
	CHECK(!percpu_counter_initialized(&fbc));
	CHECK(percpu_counter_init(&fbc, 100, GFP_KERNEL) == 0);
	CHECK(percpu_counter_initialized(&fbc));
	CHECK(fbc.count == 100);
	CHECK(percpu_counter_read(&fbc) == 100);
	CHECK(percpu_counter_sum(&fbc) == 100);

	/* below-batch add stays local: read() sees count only */
	kshim_cpu = 0;
	percpu_counter_add(&fbc, 5);		/* local 5 < 8 */
	CHECK(percpu_counter_read(&fbc) == 100);
	CHECK(percpu_counter_sum(&fbc) == 105);

	/* crossing the batch folds into count */
	percpu_counter_add(&fbc, 4);		/* 5+4 = 9 >= 8 */
	CHECK(percpu_counter_read(&fbc) == 109);
	CHECK(percpu_counter_sum(&fbc) == 109);
	CHECK(*per_cpu_ptr(fbc.counters, 0) == 0);

	/* another CPU's bucket is independent */
	kshim_cpu = 1;
	percpu_counter_add(&fbc, 7);		/* local 7 < 8 */
	CHECK(percpu_counter_read(&fbc) == 109);
	CHECK(percpu_counter_sum(&fbc) == 116);
	kshim_cpu = 2;
	percpu_counter_sub(&fbc, 3);		/* local -3 */
	CHECK(percpu_counter_sum(&fbc) == 113);
	CHECK(*per_cpu_ptr(fbc.counters, 2) == -3);

	/* explicit set resets count and zeroes buckets */
	kshim_cpu = 0;
	percpu_counter_set(&fbc, -20);
	CHECK(percpu_counter_read(&fbc) == -20);
	CHECK(percpu_counter_read_positive(&fbc) == 0);
	CHECK(percpu_counter_sum(&fbc) == -20);

	/* positive variants clamp at zero from count and sum */
	percpu_counter_add(&fbc, 30);		/* fold: -20+30 = 10 */
	CHECK(percpu_counter_sum(&fbc) == 10);
	CHECK(percpu_counter_read_positive(&fbc) == 10);
	kshim_cpu = 3;
	percpu_counter_sub(&fbc, 50);		/* local -50 */
	CHECK(percpu_counter_sum(&fbc) == -40);
	CHECK(percpu_counter_sum_positive(&fbc) == 0);

	/* compare is a three-way */
	CHECK(percpu_counter_compare(&fbc, -40) == 0);
	CHECK(percpu_counter_compare(&fbc, -41) > 0);
	CHECK(percpu_counter_compare(&fbc, -39) < 0);

	/* limited_add adds only when it fits under the limit */
	percpu_counter_set(&fbc, 10);
	CHECK(percpu_counter_limited_add(&fbc, 20, 5));
	CHECK(percpu_counter_sum(&fbc) == 15);
	CHECK(!percpu_counter_limited_add(&fbc, 20, 10));
	CHECK(percpu_counter_sum(&fbc) == 15);

	percpu_counter_destroy(&fbc);
	CHECK(!percpu_counter_initialized(&fbc));

	/* init_many initializes a whole group; destroy_many frees it */
	memset(many, 0, sizeof many);
	CHECK(__percpu_counter_init_many(many, 7, GFP_KERNEL, 4,
					 &(struct lock_class_key){ 0 }) == 0);
	CHECK(percpu_counter_sum(&many[0]) == 7);
	CHECK(percpu_counter_sum(&many[3]) == 7);
	kshim_cpu = 1;
	percpu_counter_add(&many[2], 9);	/* local: sum sees it */
	CHECK(percpu_counter_sum(&many[2]) == 16);
	percpu_counter_destroy_many(many, 4);
	CHECK(!percpu_counter_initialized(&many[1]));

	/* alloc failure: init returns -ENOMEM and leaves it dead */
	memset(&g, 0, sizeof g);
	pcpu_alloc_fail = 0;			/* fail the next alloc */
	CHECK(percpu_counter_init(&g, 5, GFP_KERNEL) == -ENOMEM);
	CHECK(!percpu_counter_initialized(&g));
	pcpu_alloc_fail = -1;

	/* batch changes reroute the fold threshold */
	percpu_counter_batch = 2;
	CHECK(percpu_counter_init(&g, 0, GFP_KERNEL) == 0);
	kshim_cpu = 0;
	percpu_counter_inc(&g);			/* local 1 < 2 */
	CHECK(percpu_counter_read(&g) == 0);
	percpu_counter_inc(&g);			/* local 2 >= 2: fold */
	CHECK(percpu_counter_read(&g) == 2);
	percpu_counter_destroy(&g);
}

/* shadow model: count + 4 buckets walked over a seeded mix */
static void pcpu_shadow(void)
{
	struct percpu_counter fbc;
	s64 sh_count, sh_loc[KSHIM_NCPU];
	s64 batch = 8;
	int i, cpu;

	percpu_counter_batch = (s32)batch;
	memset(&fbc, 0, sizeof fbc);
	CHECK(percpu_counter_init(&fbc, 0, GFP_KERNEL) == 0);
	sh_count = 0;
	memset(sh_loc, 0, sizeof sh_loc);

	for (i = 0; i < 4000; i++) {
		uint64_t r = xs64();
		s64 amt = (s64)(r % 41) - 20;
		cpu = (int)((r >> 8) % KSHIM_NCPU);
		kshim_cpu = cpu;
		switch (r % 8) {
		case 0: case 1: case 2:
			percpu_counter_add(&fbc, amt);
			sh_loc[cpu] += amt;
			if (sh_loc[cpu] >= batch || sh_loc[cpu] <= -batch) {
				sh_count += sh_loc[cpu];
				sh_loc[cpu] = 0;
			}
			break;
		case 3:
			percpu_counter_set(&fbc, amt);
			sh_count = amt;
			memset(sh_loc, 0, sizeof sh_loc);
			break;
		case 4:
			percpu_counter_add_local(&fbc, amt);
			sh_loc[cpu] += amt;
			/* local batch is INT_MAX: never folds for us */
			break;
		case 5:
			percpu_counter_sub_local(&fbc, amt);
			sh_loc[cpu] -= amt;
			break;
		case 6:
			percpu_counter_sync(&fbc);
			sh_count += sh_loc[cpu];
			sh_loc[cpu] = 0;
			break;
		default:
			break;
		}
		if (i % 7 == 0) {
			/* __percpu_counter_sum() is read-only: the locals
			 * stay put and fbc->count is unchanged. */
			CHECK(percpu_counter_sum(&fbc) ==
			      sh_count + sh_loc[0] + sh_loc[1] +
			      sh_loc[2] + sh_loc[3]);
			CHECK(fbc.count == sh_count);
		}
		CHECK(percpu_counter_read(&fbc) == sh_count);
	}
	percpu_counter_destroy(&fbc);
	CHECK(slots_busy() == 0);
}

/* ================= printk_ringbuffer invariants ================= */

#define ST_TEXTBITS 6			/* 64B data ring */
#define ST_DESCBITS 2			/* 4 descriptors */

static struct prb_desc s_descs[1 << ST_DESCBITS];
static struct printk_info s_infos[1 << ST_DESCBITS];
static char s_text[1 << ST_TEXTBITS] __aligned(8);
static struct printk_ringbuffer rb;
struct printk_ringbuffer *kshim_rb = &rb;

static struct prb_reserved_entry e;
static struct printk_record rw;
static struct printk_info ri;
static char rt[64] __aligned(8);
static struct printk_record rr;

static void rb_setup(unsigned int textbits)
{
	memset(s_descs, 0xaa, sizeof s_descs);
	memset(s_infos, 0xaa, sizeof s_infos);
	memset(s_text, 0xaa, sizeof s_text);
	prb_init(&rb, s_text, textbits, s_descs, ST_DESCBITS, s_infos);
	kshim_panic = false;
	debug_non_panic_cpus = false;
	legacy_allow_panic_sync = false;
}

static u64 st_reserve_fill(const char *txt)
{
	unsigned int n = (unsigned int)strlen(txt);
	prb_rec_init_wr(&rw, n);
	if (!prb_reserve(&e, &rb, &rw))
		return ~(u64)0;
	memcpy(rw.text_buf, txt, n);
	rw.info->text_len = (u16)n;
	prb_final_commit(&e);
	return rw.info->seq;
}

static bool st_read(u64 seq, char *out)
{
	memset(&ri, 0, sizeof ri);
	memset(rt, 0, sizeof rt);
	prb_rec_init_rd(&rr, &ri, rt, sizeof rt);
	if (!prb_read_valid(&rb, seq, &rr))
		return false;
	memcpy(out, rt, ri.text_len);
	out[ri.text_len] = 0;
	return true;
}

static void prb_hand(void)
{
	char got[64];
	u64 s0, s1, first, nxt;

	rb_setup(ST_TEXTBITS);
	CHECK(prb_first_seq(&rb) == 0);
	CHECK(prb_next_seq(&rb) == 0);
	CHECK(!prb_read_valid(&rb, 0, &rr));

	/* one record, read back by seq */
	s0 = st_reserve_fill("hello");
	CHECK(s0 == 0);
	CHECK(prb_next_seq(&rb) == 1);
	CHECK(prb_first_valid_seq(&rb) == s0);
	CHECK(st_read(s0, got));
	CHECK(!strcmp(got, "hello"));
	CHECK(ri.seq == s0);
	CHECK(ri.text_len == 5);
	CHECK(!st_read(1, got));		/* beyond last record */

	/* reserve_in_last reopens only a committed (non-final) record
	 * whose caller_id matches */
	prb_rec_init_wr(&rw, 4);
	CHECK(prb_reserve(&e, &rb, &rw));
	memcpy(rw.text_buf, "ABCD", 4);
	rw.info->text_len = 4;
	rw.info->caller_id = 0xfeed;
	prb_commit(&e);				/* committed, not final */
	s1 = rw.info->seq;
	CHECK(s1 == s0 + 1);

	memset(&rw, 0, sizeof rw);
	prb_rec_init_wr(&rw, 3);
	CHECK(prb_reserve_in_last(&e, &rb, &rw, 0xfeed,
				  1U << ST_TEXTBITS));
	CHECK(rw.info == &s_infos[s1 % (1U << ST_DESCBITS)]);
	CHECK(rw.info->seq == s1);
	rw.info->text_len = (u16)rw.text_buf_size;
	prb_final_commit(&e);
	CHECK(prb_next_seq(&rb) == s1 + 1);	/* no new record */
	CHECK(st_read(s1, got));
	CHECK(ri.text_len == rw.text_buf_size);
	/* text_space is the whole aligned block: >= the request */
	CHECK(prb_record_text_space(&e) >= rw.text_buf_size);

	/* wrong caller_id does not extend */
	prb_rec_init_wr(&rw, 2);
	CHECK(!prb_reserve_in_last(&e, &rb, &rw, 0xbeef,
				   1U << ST_TEXTBITS));

	/* oversized request fails cleanly */
	prb_rec_init_wr(&rw, 1U << (ST_TEXTBITS + 1));
	CHECK(!prb_reserve(&e, &rb, &rw));

	/* a committed-but-not-finalized record is NOT yet visible;
	 * the next reserve finalizes it lazily (desc_make_final) */
	prb_rec_init_wr(&rw, 3);
	CHECK(prb_reserve(&e, &rb, &rw));
	memcpy(rw.text_buf, "mid", 3);
	rw.info->text_len = 3;
	prb_commit(&e);				/* committed, not final */
	{
		u64 mid_seq = rw.info->seq;
		CHECK(!st_read(mid_seq, got));	/* not finalized yet */
		/* reserving the next record finalizes the previous one */
		CHECK(st_reserve_fill("post") == mid_seq + 1);
		CHECK(st_read(mid_seq, got));
		CHECK(!strcmp(got, "mid"));
	}
}

/* eviction walk on a tiny data ring: every record's text is
 * deterministic, so reads can verify byte-for-byte; once the data is
 * gone the record must be unreachable */
static void prb_wrap(void)
{
	u64 first, next, s;
	char got[64], want[32];
	int i;

	rb_setup(5);			/* 32B data ring, 4 descs */

	/* 32B ring: a block is the 8-byte id header + text, and must be
	 * <= half the ring (16B), so text must be <= 8 bytes. */
	for (i = 0; i < 40; i++) {
		snprintf(want, sizeof want, "r%02d", i);
		CHECK(st_reserve_fill(want) != ~(u64)0);
	}

	first = prb_first_valid_seq(&rb);
	next = prb_next_seq(&rb);
	CHECK(next > 0);
	CHECK(first <= next);
	CHECK(next - first <= (1 << ST_DESCBITS));

	for (s = first; s < next; s++) {
		memset(&ri, 0, sizeof ri);
		memset(rt, 0, sizeof rt);
		prb_rec_init_rd(&rr, &ri, rt, sizeof rt);
		if (!prb_read_valid(&rb, s, &rr))
			continue;	/* seq lost entirely is fine */
		CHECK(ri.seq == s);
		snprintf(want, sizeof want, "r%02d", (int)(s % 100));
		if (ri.text_len > 0) {
			CHECK(ri.text_len == strlen(want));
			CHECK(!memcmp(rt, want, ri.text_len));
		}
	}

	/* far-future seq is never readable */
	CHECK(!st_read(next + 100, got));
	CHECK(!st_read(~(u64)0, got));
}

/* invalid-descriptor path: reads starting from seq 0 across a fresh
 * ring skip the bootstrap reusable descriptor correctly */
static void prb_edges(void)
{
	unsigned int lc;
	char got[64];

	rb_setup(5);
	CHECK(prb_first_seq(&rb) == 0);
	CHECK(!st_read(0, got));
	st_reserve_fill("x");
	CHECK(prb_first_valid_seq(&rb) == 0);
	lc = 999;
	memset(&ri, 0, sizeof ri);
	CHECK(prb_read_valid_info(&rb, 0, &ri, &lc));
	CHECK(ri.text_len == 1);
	CHECK(lc == 1);
	/* info read past the end fails */
	CHECK(!prb_read_valid_info(&rb, 500, &ri, &lc));
	/* next_reserve_seq tracks the would-be-next record */
	CHECK(prb_next_reserve_seq(&rb) == 1);
}

int main(void)
{
	pcpu_alloc_fail = -1;
	pcpu_hand();
	pcpu_shadow();
	prb_hand();
	prb_wrap();
	prb_edges();
	if (nfail) {
		fprintf(stderr, "selftest: %d FAILURES\n", nfail);
		return 1;
	}
	printf("selftest OK\n");
	return 0;
}
