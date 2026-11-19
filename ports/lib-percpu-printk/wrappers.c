/* Scalar wrappers around lib/percpu_counter.c (+ include/linux/
 * percpu_counter.h) and kernel/printk/printk_ringbuffer.c, so the
 * standard scalar bridge can drive them.
 *
 * Concurrency model: sequential.  Atomics are plain long ops
 * (linux/atomic.h), spinlocks/irq flags are no-ops, "the current CPU"
 * is the harness global kshim_cpu set per-op by the driver, and
 * kshim_panic + the two internal.h bools toggle the reader's panic
 * skip path.  All NCPU=4 per-cpu variables live as byte offsets in
 * kshim_pcpu_area (NCPU x 256B chunks); __alloc_percpu_gfp is a
 * deterministic 8-slot allocator with seeded fail injection, matching
 * the kmem_cache boundary pattern of ports/lib-xarray.
 *
 * Fold model: every wrapper returns one order-sensitive FNV-1a-style
 * checksum over each op's results plus a periodic FULL STATE DUMP.
 * Pointer-shaped values fold canonically: per-cpu offsets are byte
 * offsets into cpu chunk 0, text_buf/info fold as ring/array offsets,
 * NULL folds as NILX.  Raw addresses never fold.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/kernel.h>
#include <linux/percpu.h>
#include <linux/percpu_counter.h>
#include <linux/errno.h>
#include <linux/gfp.h>
#include <linux/bug.h>

#include "printk_ringbuffer.h"
#include "internal.h"

#define NILX 0xffffu	/* "no such object" fold sentinel */

/* ---- harness globals consumed by the shims ---- */

int kshim_cpu;
const unsigned long kshim_cpu_mask;
unsigned long kshim_warn_hits;
bool kshim_panic;
bool debug_non_panic_cpus;
bool legacy_allow_panic_sync;

char kshim_pcpu_area[KSHIM_NCPU][KSHIM_PCPU_STRIDE] __aligned(64);

/* ---- deterministic percpu allocator (linux/percpu.h boundary) ---- */

/* 8 slots of 32 bytes: chunk offset = slot * 32.  alloc_ctr/fail_mask
 * nominate which alloc calls return NULL — this drives the -ENOMEM
 * path in __percpu_counter_init_many deterministically. */
#define PCPU_SLOT_SHIFT 5	/* 32 bytes */
#define PCPU_NSLOTS (KSHIM_PCPU_STRIDE >> PCPU_SLOT_SHIFT)

static unsigned long alloc_ctr;
static uint64_t fail_mask;
static uint8_t pcpu_slot_used[PCPU_NSLOTS];

static void *pcpu_alloc_raw(size_t size, size_t align, gfp_t gfp)
{
	unsigned int i;
	(void)align; (void)gfp;
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return NULL;
	for (i = 0; i < PCPU_NSLOTS; i++) {
		if (!pcpu_slot_used[i] && size <= (1U << PCPU_SLOT_SHIFT)) {
			int cpu;
			pcpu_slot_used[i] = 1;
			for (cpu = 0; cpu < KSHIM_NCPU; cpu++)
				memset(&kshim_pcpu_area[cpu]
						       [i << PCPU_SLOT_SHIFT],
				       0, 1U << PCPU_SLOT_SHIFT);
			return &kshim_pcpu_area[0][i << PCPU_SLOT_SHIFT];
		}
	}
	return NULL;
}

void *__alloc_percpu_gfp(size_t size, size_t align, gfp_t gfp)
{
	return pcpu_alloc_raw(size, align, gfp);
}

void *__alloc_percpu(size_t size, size_t align)
{
	return pcpu_alloc_raw(size, align, GFP_KERNEL);
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

static uint64_t g_h;
static void fold(uint64_t v)
{
	g_h = (g_h ^ v) * 0x100000001b3ULL;
}

static void fold_bytes(const void *p, unsigned int n)
{
	const unsigned char *b = p;
	unsigned int i;
	for (i = 0; i < n; i++)
		fold(b[i]);
}

static uint64_t canon_pcpu(const void __percpu *p)
{
	return p ? (uint64_t)((const char *)p - &kshim_pcpu_area[0][0]) : NILX;
}

/* ==================================================================
 * percpu_counter unit
 * ================================================================== */

#define NCTR 8		/* independent single counters */
#define NMANY 4		/* one init_many group */

static struct percpu_counter ctrs[NCTR];
static struct percpu_counter many[NMANY];

/* canonical per-fbc dump: count, alloc state, every cpu's bucket */
static void dump_fbc(const struct percpu_counter *fbc)
{
	int cpu;
	fold((uint64_t)fbc->count);
	fold(canon_pcpu(fbc->counters));
	if (!fbc->counters) {
		fold(NILX);
		return;
	}
	for (cpu = 0; cpu < KSHIM_NCPU; cpu++)
		fold((uint64_t)(uint32_t)*per_cpu_ptr(fbc->counters, cpu));
}

static void dump_pcpu(void)
{
	int i;
	fold(0xbc50);
	for (i = 0; i < NCTR; i++)
		dump_fbc(&ctrs[i]);
	for (i = 0; i < NMANY; i++)
		dump_fbc(&many[i]);
	for (i = 0; i < PCPU_NSLOTS; i++)
		fold(pcpu_slot_used[i]);
	fold((uint64_t)percpu_counter_batch);
	fold((uint64_t)kshim_cpu);
	fold(kshim_warn_hits);
}

static s64 gen_amt(uint64_t r)
{
	switch (r % 10) {
	case 0: case 1: case 2:
		return (s64)((r >> 8) % 13) - 6;		/* -6..6 */
	case 3:
		return 29 + (s64)((r >> 8) % 7);		/* 29..35 batch edge */
	case 4:
		return -(29 + (s64)((r >> 8) % 7));
	case 5:
		return (s64)((r >> 8) % 250) - 125;	/* -125..124 */
	case 6:
		return (s64)((r >> 8) % 1024);		/* big positive */
	case 7:
		return -(s64)((r >> 8) % 1024);		/* big negative */
	case 8:
		return (s64)((r >> 8) % (1 << 20));	/* huge positive */
	default:
		return -(s64)((r >> 8) % (1 << 20));	/* huge negative */
	}
}

static s32 gen_batch(uint64_t r)
{
	switch (r % 8) {
	case 0: return 1;
	case 1: return 8;
	case 2: return 32;
	case 3: return 33;
	case 4: return 64;
	case 5: return 128;
	case 6: return 256;
	default: return INT_MAX;	/* the add_local batch */
	}
}

static struct percpu_counter *pick_fbc(uint64_t r)
{
	return &ctrs[r % NCTR];
}

static void pcpu_reset(uint64_t seed2)
{
	memset(&kshim_pcpu_area, 0, sizeof kshim_pcpu_area);
	memset(pcpu_slot_used, 0, sizeof pcpu_slot_used);
	memset(ctrs, 0, sizeof ctrs);
	memset(many, 0, sizeof many);
	alloc_ctr = 0;
	kshim_cpu = 0;
	kshim_warn_hits = 0;
	percpu_counter_batch = 8 << (int)(iabs(seed2) % 5); /* 8..128 */
}

uint64_t wrap_pcpu_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 140);
	int dump_every = 5 + (int)(iabs(seed2) % 6);
	int i;

	pcpu_reset((uint64_t)seed2);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;	/* every early alloc fails */
	g_h = 0x9e3779b97f4a7c15ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int cpu = (int)((r >> 4) % KSHIM_NCPU);
		struct percpu_counter *fbc = pick_fbc(r >> 9);
		s64 amt = gen_amt(r >> 20);

		kshim_cpu = cpu;
		switch ((int)(r % 24)) {
		case 0: case 1:
			if (!percpu_counter_initialized(fbc))
				fold((uint64_t)(int64_t)percpu_counter_init(
					fbc, amt, GFP_KERNEL));
			else
				fold(NILX);
			break;
		case 2:
			if (!many[0].counters)
				fold((uint64_t)(int64_t)
				     __percpu_counter_init_many(many, amt,
					GFP_KERNEL, NMANY,
					&(struct lock_class_key){ 0 }));
			else
				fold(NILX);
			break;
		case 3:
			percpu_counter_destroy(fbc);
			break;
		case 4:
			percpu_counter_destroy_many(many, NMANY);
			break;
		case 5:
			if (fbc->counters)
				percpu_counter_set(fbc, amt);
			else
				fold(NILX);
			break;
		case 6: case 7: case 8:
			if (fbc->counters)
				percpu_counter_add(fbc, amt);
			else
				fold(NILX);
			break;
		case 9: case 10:
			if (fbc->counters)
				percpu_counter_add_batch(fbc, amt,
							 gen_batch(r >> 50));
			else
				fold(NILX);
			break;
		case 11:
			if (fbc->counters)
				percpu_counter_sub(fbc, amt);
			else
				fold(NILX);
			break;
		case 12:
			if (fbc->counters)
				percpu_counter_inc(fbc);
			else
				fold(NILX);
			break;
		case 13:
			if (fbc->counters)
				percpu_counter_dec(fbc);
			else
				fold(NILX);
			break;
		case 14:
			if (fbc->counters)
				percpu_counter_add_local(fbc, amt);
			else
				fold(NILX);
			break;
		case 15:
			if (fbc->counters)
				percpu_counter_sub_local(fbc, amt);
			else
				fold(NILX);
			break;
		case 16:
			if (fbc->counters)
				percpu_counter_sync(fbc);
			else
				fold(NILX);
			break;
		case 17:
			fold(fbc->counters ?
			     (uint64_t)percpu_counter_sum(fbc) : NILX);
			break;
		case 18:
			fold(fbc->counters ?
			     (uint64_t)percpu_counter_sum_positive(fbc) : NILX);
			break;
		case 19:
			fold((uint64_t)percpu_counter_read(fbc));
			break;
		case 20:
			fold((uint64_t)percpu_counter_read_positive(fbc));
			break;
		case 21:
			fold(fbc->counters ?
			     (uint64_t)(int64_t)percpu_counter_compare(fbc, amt)
			     : NILX);
			break;
		case 22:
			fold(fbc->counters ?
			     (uint64_t)percpu_counter_limited_add(fbc,
					gen_amt(r >> 40), amt) : NILX);
			break;
		default:
			fold((uint64_t)percpu_counter_initialized(fbc));
			break;
		}
		/* light per-op fold of the touched counter */
		fold((uint64_t)(fbc - ctrs));
		dump_fbc(fbc);
		if (i % dump_every == dump_every - 1)
			dump_pcpu();
	}
	dump_pcpu();
	fold(kshim_warn_hits);
	return g_h;
}

/* ==================================================================
 * printk_ringbuffer unit
 * ================================================================== */

#define PRB_MAX_DESCBITS 3			/* up to 8 descriptors */
#define PRB_MAX_DESCS (1 << PRB_MAX_DESCBITS)
#define PRB_MAX_TEXTBITS 8			/* up to 256B data ring */
#define PRB_MAX_TEXT (1 << PRB_MAX_TEXTBITS)
#define NW 3					/* simulated writer slots */
#define RTBUF 64				/* reader text buffer */

static struct prb_desc prb_descs[PRB_MAX_DESCS];
static struct printk_info prb_infos[PRB_MAX_DESCS];
static char prb_text[PRB_MAX_TEXT] __aligned(8);
static struct printk_ringbuffer rb;
struct printk_ringbuffer *kshim_rb = &rb;	/* panic-gate in internal.h */

static struct prb_reserved_entry es[NW];
static struct printk_record rws[NW];	/* per-writer reserve records */
static uint8_t eopen[NW];
static u32 last_cid;			/* caller_id of newest commit */

static struct printk_info rinfo;
static char rtext[RTBUF] __aligned(8);
static struct printk_record rr;
static struct printk_record rw2;		/* reserve_in_last record */

static uint64_t canon_text(const char *p)
{
	return p ? (uint64_t)(p - &prb_text[0]) : NILX;
}

static uint64_t canon_info(const struct printk_info *p)
{
	return p ? (uint64_t)(p - &prb_infos[0]) : NILX;
}

static void fold_info(const struct printk_info *info)
{
	fold(info->seq);
	fold(info->ts_nsec);
	fold(info->text_len);
	fold(info->facility);
	fold(info->flags);
	fold(info->level);
	fold(info->caller_id);
	fold_bytes(info->dev_info.subsystem, 8);
	fold_bytes(info->dev_info.device, 8);
}

/* full rb dump: ring scalars, every desc + info, data bytes */
static void dump_rb(void)
{
	unsigned int i, n = 1U << rb.desc_ring.count_bits;

	fold(0xdb1);
	fold(rb.desc_ring.count_bits);
	fold(rb.text_data_ring.size_bits);
	fold((uint64_t)atomic_long_read(&rb.desc_ring.head_id));
	fold((uint64_t)atomic_long_read(&rb.desc_ring.tail_id));
	fold((uint64_t)atomic_long_read(&rb.desc_ring.last_finalized_seq));
	fold((uint64_t)atomic_long_read(&rb.text_data_ring.head_lpos));
	fold((uint64_t)atomic_long_read(&rb.text_data_ring.tail_lpos));
	fold((uint64_t)atomic_long_read(&rb.fail));
	for (i = 0; i < n; i++) {
		fold((uint64_t)atomic_long_read(&prb_descs[i].state_var));
		fold(prb_descs[i].text_blk_lpos.begin);
		fold(prb_descs[i].text_blk_lpos.next);
	}
	for (i = 0; i < n; i++)
		fold_info(&prb_infos[i]);
	for (i = 0; i < n; i++)
		fold(eopen[i % NW]);
	fold_bytes(prb_text, 1U << rb.text_data_ring.size_bits);
	fold(kshim_panic);
	fold(kshim_warn_hits);
}

/* deterministic record text: f(seq-ish nonce, writer, index) */
static char gen_text_byte(uint64_t nonce, int w, int i)
{
	return (char)('A' + (int)((nonce + w * 7 + i * 13) % 26));
}

/* writer fills the freshly reserved record */
static void writer_fill(int w, uint64_t r)
{
	struct printk_info *info = rws[w].info;
	unsigned int size = rws[w].text_buf_size;
	unsigned int i, len;

	if (!info)
		return;
	len = (unsigned int)((r >> 33) % (size + 1));
	for (i = 0; i < len && i < size; i++)
		rws[w].text_buf[i] = gen_text_byte(r >> 40, w, (int)i);
	info->text_len = (u16)len;
	info->ts_nsec = (r >> 8) & 0xffffff;
	info->caller_id = (r & 0x4000) ? 0x77 : (u32)(0x1000 + w);
	info->facility = (u8)((r >> 12) % 4);
	info->level = (u8)((r >> 16) % 8);
	info->flags = (u8)((r >> 19) % 32);
	if (r & 0x8000) {
		memset(info->dev_info.subsystem, (int)('a' + w), 6);
		memset(info->dev_info.device, (int)('0' + w), 6);
	}
	last_cid = info->caller_id;
}

static unsigned int gen_size(uint64_t r, unsigned int textbits)
{
	unsigned int half = (1U << textbits) / 2;
	switch (r % 8) {
	case 0: return 0;			/* empty line (data-less) */
	case 1: return 1;
	case 2: return 2 + (unsigned int)((r >> 8) % 6);
	case 3: return half > 8 ? half - 8 : 1;	/* near max legal */
	case 4: return (unsigned int)((r >> 8) % (half + 2));
	case 5: return half;			/* too big -> reserve fails */
	case 6: return 2 * half;		/* way too big */
	default: return (unsigned int)((r >> 8) % 4);
	}
}

static u64 gen_seq(uint64_t r)
{
	switch (r % 8) {
	case 0: return 0;
	case 1: return (r >> 8) % 8;
	case 2: return (r >> 8) % 32;
	case 3: return 1 + (r >> 8) % 3;	/* low seqs */
	case 4: return (r >> 8) % 64;
	case 5: return 0x100 + (r >> 8) % 16;	/* beyond any valid */
	case 6: return (r >> 8) % 4;
	default: return (r >> 8) % 1024;
	}
}

static void op_prb_read(uint64_t r)
{
	u64 seq = gen_seq(r);
	memset(&rinfo, 0, sizeof rinfo);
	memset(rtext, 0, sizeof rtext);
	prb_rec_init_rd(&rr, &rinfo, rtext, (r & 0x2000) ? RTBUF :
			(unsigned int)(1 + (r >> 20) % RTBUF));
	fold((uint64_t)prb_read_valid(&rb, seq, &rr));
	fold_info(&rinfo);
	fold_bytes(rtext, RTBUF);
}

static void op_prb_read_info(uint64_t r)
{
	u64 seq = gen_seq(r);
	unsigned int lc = 0xdead;
	memset(&rinfo, 0, sizeof rinfo);
	fold((uint64_t)prb_read_valid_info(&rb, seq, &rinfo, &lc));
	fold_info(&rinfo);
	fold(lc);
}

static void prb_reset(uint64_t seed2, unsigned int *descbits,
		      unsigned int *textbits)
{
	*descbits = 2 + (unsigned int)(iabs(seed2) % 2);		/* 2|3 */
	*textbits = 5 + (unsigned int)((iabs(seed2) >> 3) % 4);	/* 5..8 */
	memset(prb_descs, 0xaa, sizeof prb_descs);
	memset(prb_infos, 0xaa, sizeof prb_infos);
	memset(prb_text, 0xaa, sizeof prb_text);
	prb_init(&rb, prb_text, *textbits, prb_descs, *descbits,
		 prb_infos);
	memset(es, 0, sizeof es);
	memset(rws, 0, sizeof rws);
	memset(eopen, 0, sizeof eopen);
	last_cid = 0;
	kshim_warn_hits = 0;
	kshim_cpu = 0;
}

uint64_t wrap_prb_ops(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	unsigned int descbits, textbits;
	int nops = (int)(iabs(nops_i) % 160);
	int dump_every = 6 + (int)(iabs(seed2) % 6);
	int i;

	prb_reset((uint64_t)seed2, &descbits, &textbits);
	kshim_panic = !!((iabs(seed2) >> 6) & 1);
	debug_non_panic_cpus = !!((iabs(seed2) >> 7) & 1);
	legacy_allow_panic_sync = !!((iabs(seed2) >> 8) & 1);
	rng_state = seed | 1;
	g_h = 0x9e3779b97f4a7c15ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int w = (int)((r >> 3) % NW);
		unsigned int sz;

		/* per-op panic/console flags exercise the reader skip */
		kshim_panic = !!((r >> 13) & 1);
		debug_non_panic_cpus = !!((r >> 14) & 1);
		legacy_allow_panic_sync = !!((r >> 15) & 1);

		switch ((int)(r % 20)) {
		case 0: case 1: case 2: case 3: case 4:
			/* reserve + fill (+ maybe commit) */
			sz = gen_size(r >> 8, textbits);
			prb_rec_init_wr(&rws[w], sz);
			if (prb_reserve(&es[w], &rb, &rws[w])) {
				eopen[w] = 1;
				fold(1);
				fold((uint64_t)es[w].id);
				fold(canon_text(rws[w].text_buf));
				fold(canon_info(rws[w].info));
				fold(es[w].text_space);
				writer_fill(w, r);
				if ((r >> 30) % 3 == 0) {
					prb_commit(&es[w]);
					eopen[w] = 0;
					fold(0xcc01);
				} else if ((r >> 30) % 3 == 1) {
					prb_final_commit(&es[w]);
					eopen[w] = 0;
					fold(0xfc01);
				}
			} else {
				fold(0);
				fold(canon_text(rws[w].text_buf));
				fold(canon_info(rws[w].info));
			}
			break;
		case 5: case 6:
			if (eopen[w]) {
				prb_commit(&es[w]);
				eopen[w] = 0;
				fold(0xcc02);
			} else {
				fold(NILX);
			}
			break;
		case 7: case 8:
			if (eopen[w]) {
				prb_final_commit(&es[w]);
				eopen[w] = 0;
				fold(0xfc02);
			} else {
				fold(NILX);
			}
			break;
		case 9: case 10: {
			/* extend newest record: match the stored caller_id
			 * half the time, random otherwise */
			u32 cid = (r & 0x100) ? last_cid : (u32)((r >> 32) % 8);
			unsigned int maxsz = 1 + (unsigned int)((r >> 40) % (2U << textbits));
			sz = (unsigned int)((r >> 48) % (maxsz + 2));
			prb_rec_init_wr(&rw2, sz);
			fold((uint64_t)prb_reserve_in_last(&es[w], &rb, &rw2,
							 cid, maxsz));
			if (rw2.info) {
				/* fill appended text */
				unsigned int i2, base = rw2.info->text_len;
				for (i2 = base; i2 < rw2.text_buf_size; i2++)
					rw2.text_buf[i2] =
						gen_text_byte(r >> 40, w,
							      (int)i2);
				rw2.info->text_len = (u16)rw2.text_buf_size;
				eopen[w] = 1;
				fold(canon_text(rw2.text_buf));
				fold(canon_info(rw2.info));
				fold(es[w].text_space);
				if ((r >> 30) % 3 == 0) {
					prb_commit(&es[w]);
					eopen[w] = 0;
					fold(0xcc03);
				} else if ((r >> 30) % 3 == 1) {
					prb_final_commit(&es[w]);
					eopen[w] = 0;
					fold(0xfc03);
				}
			} else {
				eopen[w] = 0;
				fold(NILX);
			}
			break;
		}
		case 11: case 12: case 13:
			op_prb_read(r);
			break;
		case 14: case 15:
			op_prb_read_info(r);
			break;
		case 16:
			fold(prb_first_seq(&rb));
			fold(prb_first_valid_seq(&rb));
			break;
		case 17:
			fold(prb_next_seq(&rb));
			/* UB curation: a stale/reserved last-finalized slot
			 * makes upstream's try_again loop livelock; the
			 * recorded value is a sentinel instead (the Rust
			 * port mirrors the predicate). */
			fold(kshim_nrs_livelock(&rb) ? 0xdeaddeadULL
					 : prb_next_reserve_seq(&rb));
			break;
		case 18:
			fold(eopen[w] ? (uint64_t)prb_record_text_space(&es[w])
				      : NILX);
			break;
		default:
			/* re-init ("clear the ringbuffer") */
			prb_init(&rb, prb_text, textbits, prb_descs,
				 descbits, prb_infos);
			memset(eopen, 0, sizeof eopen);
			fold(0x1e17);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_rb();
	}
	dump_rb();
	fold(kshim_warn_hits);
	return g_h;
}

/* Tiny-ring driver: descbits=2 (4 descs), textbits=5..6 (32/64B data)
 * forces constant desc recycling, data-ring wraparound and record loss.
 * Everything is final-committed so readers see the full eviction story. */
uint64_t wrap_prb_wrap(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	unsigned int textbits = 5 + (unsigned int)(iabs(seed2) % 2);
	int nops = (int)(iabs(nops_i) % 140);
	int dump_every = 5 + (int)(iabs(seed2) % 5);
	int i;

	memset(prb_descs, 0xaa, sizeof prb_descs);
	memset(prb_infos, 0xaa, sizeof prb_infos);
	memset(prb_text, 0xaa, sizeof prb_text);
	prb_init(&rb, prb_text, textbits, prb_descs, 2, prb_infos);
	memset(es, 0, sizeof es);
	memset(rws, 0, sizeof rws);
	memset(eopen, 0, sizeof eopen);
	last_cid = 0;
	kshim_warn_hits = 0;
	kshim_panic = !!((iabs(seed2) >> 6) & 1);
	debug_non_panic_cpus = !!((iabs(seed2) >> 7) & 1);
	legacy_allow_panic_sync = !!((iabs(seed2) >> 8) & 1);
	rng_state = seed | 1;
	g_h = 0x9e3779b97f4a7c15ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int w = (int)((r >> 3) % NW);
		unsigned int sz;

		kshim_panic = !!((r >> 13) & 1);
		debug_non_panic_cpus = !!((r >> 14) & 1);
		legacy_allow_panic_sync = !!((r >> 15) & 1);

		switch ((int)(r % 16)) {
		case 0: case 1: case 2: case 3: case 4: case 5: case 6:
			sz = gen_size(r >> 8, textbits);
			prb_rec_init_wr(&rws[w], sz);
			if (prb_reserve(&es[w], &rb, &rws[w])) {
				eopen[w] = 1;
				fold(1);
				fold((uint64_t)es[w].id);
				fold(es[w].text_space);
				writer_fill(w, r);
				/* mostly finalize immediately: keeps the
				 * eviction story observable to readers */
				if ((r >> 30) % 5 != 0) {
					prb_final_commit(&es[w]);
					eopen[w] = 0;
					fold(0xfc04);
				}
			} else {
				fold(0);
			}
			break;
		case 7:
			if (eopen[w]) {
				if (r & 0x100)
					prb_commit(&es[w]);
				else
					prb_final_commit(&es[w]);
				eopen[w] = 0;
				fold(0xc07);
			} else {
				fold(NILX);
			}
			break;
		case 8: {
			u32 cid = (r & 0x100) ? last_cid : (u32)((r >> 32) % 8);
			unsigned int maxsz = 1 + (unsigned int)((r >> 40) % (2U << textbits));
			sz = (unsigned int)((r >> 48) % (maxsz + 2));
			prb_rec_init_wr(&rw2, sz);
			fold((uint64_t)prb_reserve_in_last(&es[w], &rb, &rw2,
							 cid, maxsz));
			if (rw2.info) {
				rw2.info->text_len = (u16)rw2.text_buf_size;
				eopen[w] = 1;
				fold(es[w].text_space);
				if ((r >> 30) & 1) {
					prb_final_commit(&es[w]);
					eopen[w] = 0;
				}
			} else {
				eopen[w] = 0;
				fold(NILX);
			}
			break;
		}
		case 9: case 10: case 11:
			op_prb_read(r);
			break;
		case 12:
			op_prb_read_info(r);
			break;
		case 13:
			fold(prb_first_seq(&rb));
			fold(prb_first_valid_seq(&rb));
			break;
		case 14:
			fold(prb_next_seq(&rb));
			break;
		default:
			fold(kshim_nrs_livelock(&rb) ? 0xdeaddeadULL
					 : prb_next_reserve_seq(&rb));
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_rb();
	}
	dump_rb();
	fold(kshim_warn_hits);
	return g_h;
}
