/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * internal.h - printk internal definitions: HARNESS STUB.
 *
 * This is NOT a byte-identical copy of kernel/printk/internal.h (the
 * real one drags in the whole console/sysctl surface).  It provides
 * exactly the symbols printk_ringbuffer.c consumes:
 *
 *   - panic_on_this_cpu(): gates the panic skip path in
 *     _prb_read_valid(); backed by kshim_panic plus a livelock
 *     curation described below.
 *   - debug_non_panic_cpus / legacy_allow_panic_sync: gate the same
 *     path; backed by harness globals.
 *   - kshim_nrs_livelock(): the livelock predicate, exported so
 *     wrappers.c can also gate the direct prb_next_reserve_seq()
 *     calls it makes (with a sentinel fold).
 *
 * UB curation (same convention as ports/lib-xarray): the panic skip in
 * _prb_read_valid() increments *seq once per iteration while
 * (*seq + 1) < prb_next_reserve_seq().  Two sequential-model states
 * make prb_next_reserve_seq() unusable for that purpose:
 *
 *   1. try_again livelock inside prb_next_reserve_seq(): when
 *      last_finalized_seq != 0 and the descriptor slot it names does
 *      not read back as finalized/reusable with a matching seq
 *      (reserved/committed, or recycled to a different seq), upstream
 *      loops forever.
 *
 *   2. Garbage diff: prb_next_reserve_seq() computes
 *      head_id - last_finalized_id WITHOUT the DESC_ID mask.  The ID
 *      space overflows quickly by design (DESC0_ID sits at the top of
 *      the 62-bit space), so once head_id has wrapped past the stored
 *      slot's ID the difference is ~2^62 and the panic skip climbs
 *      one seq at a time toward it — a genuine upstream livelock in
 *      this interleaving, but unrepresentable in a finite recording.
 *      (In a sane state diff is a count of live descriptors and is
 *      always <= DESCS_COUNT.)
 *
 * kshim_nrs_livelock() replays prb_next_reserve_seq()'s control flow
 * to detect exactly those states: panic_on_this_cpu() reports panic
 * only while the panic skip provably terminates, and the wrappers
 * fold a sentinel instead of calling prb_next_reserve_seq() directly.
 * Every case that terminates upstream produces the identical result.
 */
#include <linux/types.h>
#include <linux/atomic.h>

extern bool kshim_panic;
extern bool debug_non_panic_cpus;
extern bool legacy_allow_panic_sync;
extern struct printk_ringbuffer *kshim_rb;

/* true when the panic skip / direct call could never terminate */
static inline bool kshim_nrs_livelock(struct printk_ringbuffer *rb)
{
	struct prb_desc_ring *dr = &rb->desc_ring;
	unsigned long mask = (1UL << dr->count_bits) - 1;
	unsigned long count = 1UL << dr->count_bits;
	unsigned long head, sv, st, lf_id;
	struct prb_desc *d;
	u64 lfs;
	bool einv;

	lfs = (u64)atomic_long_read(&dr->last_finalized_seq);
	head = (unsigned long)atomic_long_read(&dr->head_id);

	d = &dr->descs[lfs & mask];
	sv = (unsigned long)atomic_long_read(&d->state_var);
	st = 3UL & (sv >> DESC_FLAGS_SHIFT);

	/* desc_read_finalized_seq() == -EINVAL: the slot's ID always
	 * matches (it is read from the slot itself), so only the state
	 * and the stored seq decide. */
	einv = (st == (unsigned long)desc_reserved ||
		st == (unsigned long)desc_committed ||
		dr->infos[lfs & mask].seq != lfs);

	if (einv) {
		if (lfs == 0) {
			/* upstream returns 0 early here: no livelock */
			if (head == DESC0_ID(dr->count_bits))
				return false;
			lf_id = DESC0_ID(dr->count_bits) + 1;
		} else {
			/* upstream: goto try_again forever */
			return true;
		}
	} else {
		lf_id = DESC_ID(sv);
	}

	/* garbage head-lf_id difference: ~2^62 instead of a small count */
	return head - lf_id > count;
}

static inline bool panic_on_this_cpu(void)
{
	if (!kshim_panic || !kshim_rb)
		return false;
	return !kshim_nrs_livelock(kshim_rb);
}
