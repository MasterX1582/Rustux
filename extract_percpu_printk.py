#!/usr/bin/env python3
"""Transform ports/lib-percpu-printk/rust/wrappers-rs/src/port.rs
-> klib/src/percpu_counter.rs + klib/src/prb.rs

Caller-owned-arena adoption:
  * the per-cpu area becomes a caller &'a mut [i64] lane slice holding
    s32-domain values (the C `s32 *pcount`), one lane per cpu;
  * the printk rings become caller slices (prb_desc_ring/prb_data_ring);
  * record pointers are indices (infos index / data byte offset);
  * the 0xdeaddead livelock sentinel becomes Option::None.
Harness-only items (Fold/xs64/gen_*/dump_*/op_*/wrap_*/aa_fill/
writer_fill/es/rws/eopen/rinfo/rtext drivers) are dropped.
"""
import re

SRC = "ports/lib-percpu-printk/rust/wrappers-rs/src/port.rs"
lines = open(SRC, encoding="utf-8").read().split("\n")


def region(a, b):
    """1-based inclusive line range."""
    return "\n".join(lines[a - 1 : b])


def prb_rename(t):
    """S-state field accesses -> Prb/PrbDescRing/PrbDataRing fields."""
    t = t.replace("self.rb.count_bits", "self.desc_ring.count_bits")
    t = t.replace("self.rb.head_id", "self.desc_ring.head_id")
    t = t.replace("self.rb.tail_id", "self.desc_ring.tail_id")
    t = t.replace(
        "self.rb.last_finalized_seq", "self.desc_ring.last_finalized_seq"
    )
    t = t.replace("self.rb.size_bits", "self.text_ring.size_bits")
    t = t.replace("self.rb.head_lpos", "self.text_ring.head_lpos")
    t = t.replace("self.rb.tail_lpos", "self.text_ring.tail_lpos")
    t = t.replace("self.rb.fail", "self.fail")
    t = t.replace("self.descs", "self.desc_ring.descs")
    t = t.replace("self.infos", "self.desc_ring.infos")
    t = re.sub(r"self\.text\b", "self.text_ring.data", t)
    t = t.replace("self.warn_hits", "self.warn")
    t = t.replace("self.es[w]", "e")
    t = t.replace("WRec::default()", "PrintkRecord::default()")
    t = t.replace("self.prb_commit(", "self.prb_commit_id(")
    t = t.replace("self.prb_first_seq()", "self.first_seq()")
    # F9 — pub ring bits fields are caller-mutable; shift-overflow panic
    # becomes C-wrap semantics
    t = t.replace("1u64 << self.desc_ring.count_bits",
                  "1u64.wrapping_shl(self.desc_ring.count_bits)")
    t = t.replace("1u64 << self.text_ring.size_bits",
                  "1u64.wrapping_shl(self.text_ring.size_bits)")
    # critic — desc_idx bounds: a crafted count_bits makes `n & mask`
    # exceed the caller arrays; mod the physical ring size (identity
    # in-contract since descs/infos >= 1<<count_bits).
    t = t.replace(
        "(n & self.mask()) as usize",
        "(n & self.mask()) as usize % "
        "self.desc_ring.descs.len().min(self.desc_ring.infos.len()).max(1)",
    )
    # blk_read_id/blk_write_id — crafted lpos/begin can put the 8-byte
    # window outside the caller's data ring
    t = t.replace(
        """        let o = data_index(self.text_ring.size_bits, lpos);
        u64::from_le_bytes(self.text_ring.data[o..o + 8].try_into().unwrap())""",
        """        let o = data_index(self.text_ring.size_bits, lpos);
        let Some(bytes) = o
            .checked_add(8)
            .and_then(|e| self.text_ring.data.get(o..e))
        else {
            return 0;
        };
        u64::from_le_bytes(bytes.try_into().unwrap())""",
    )
    t = t.replace(
        "        self.text_ring.data[off..off + 8].copy_from_slice(&id.to_le_bytes());",
        """        if let Some(dst) = off
            .checked_add(8)
            .and_then(|e| self.text_ring.data.get_mut(off..e))
        {
            dst.copy_from_slice(&id.to_le_bytes());
        } else {
            self.warn += 1;
        }""",
    )
    # get_data align check: `x != (x + 7) & !7` overflows at u64::MAX;
    # `x & 7 != 0` is the identical non-overflowing form
    t = t.replace("let align_bad_b = blk.begin != (blk.begin + 7) & !7;",
                  "let align_bad_b = blk.begin & 7 != 0;")
    t = t.replace("let align_bad_n = blk.next != (blk.next + 7) & !7;",
                  "let align_bad_n = blk.next & 7 != 0;")
    # get_data wrapped-copy path — bound both arithmetic and loop extent
    t = t.replace("(blk.next.wrapping_sub(blk.begin) - 8) as usize",
                  "(blk.next.wrapping_sub(blk.begin).wrapping_sub(8)) as usize")
    t = t.replace(
        """                for i in 0..n {
                    self.text_ring.data[8 + i] = self.text_ring.data[old_off + 8 + i];
                }""",
        """                let n = n.min(self.text_ring.data.len().saturating_sub(8));
                for i in 0..n {
                    let src = old_off.wrapping_add(8).wrapping_add(i);
                    if src < self.text_ring.data.len() {
                        self.text_ring.data[8 + i] = self.text_ring.data[src];
                    }
                }""",
    )
    t = t.replace("Some(old_off + 8)", "Some(old_off.wrapping_add(8))")
    # byte-offset `+ 8` on ring positions: u64::MAX-ish lpos values wrap
    t = t.replace("Some((off + 8, data_size as u32))",
                  "Some((off.wrapping_add(8), data_size as u32))")
    t = t.replace("Some(if wrapped { 8 } else { off + 8 })",
                  "Some(if wrapped { 8 } else { off.wrapping_add(8) })")
    t = t.replace("data_index(self.text_ring.size_bits, blk.begin) + 8",
                  "data_index(self.text_ring.size_bits, blk.begin).wrapping_add(8)")
    # seq arithmetic: pub-field/crafted values can sit at u64::MAX — C
    # wraps, match it (order matters: `*seq` before bare `seq`)
    t = t.replace("*seq += 1;", "*seq = seq.wrapping_add(1);")
    t = t.replace("let mut try_seq = finalized + 1;",
                  "let mut try_seq = finalized.wrapping_add(1);")
    t = t.replace("try_seq += 1;", "try_seq = try_seq.wrapping_add(1);")
    t = re.sub(r"\bseq \+= 1;", "seq = seq.wrapping_add(1);", t)
    t = t.replace(
        "r.text_buf_size += self.desc_ring.infos[idx].text_len as u32;",
        "r.text_buf_size = r\n                        .text_buf_size\n                        .wrapping_add(self.desc_ring.infos[idx].text_len as u32);",
    )
    # count_lines — clamp the window to the physical ring and guarantee
    # forward progress
    t = t.replace("        let end = off + len;",
                  "        let end = off.saturating_add(len).min(self.text_ring.data.len());\n        if off >= end {\n            return 1;\n        }")
    t = t.replace("            next_size = end - pos;",
                  "            next_size = end.saturating_sub(pos);")
    return t


def meth(a, b, old_sig, new_sig, extra=None, pub=False):
    """Extract an impl-S method, transform it, swap the signature."""
    t = prb_rename(region(a, b))
    assert old_sig in t, old_sig
    t = t.replace(old_sig, new_sig, 1)
    if extra:
        for old, new in extra:
            assert old in t, old
            t = t.replace(old, new)
    return t


# =====================================================================
# percpu_counter.rs — hand-shaped: the pcpu byte-offset model becomes a
# caller lane slice; bodies mirror the verified port verbatim.
# =====================================================================

PC_HEADER = '''//! `klib::percpu_counter` — Linux `lib/percpu_counter.c` +
//! `include/linux/percpu_counter.h` ported to safe Rust, verified
//! against the C implementation (966/966 recorded differential cases
//! shared with the printk ringbuffer, `ports/lib-percpu-printk`).
//!
//! Storage model: a caller-owned `&mut [i64]` lane slice replaces the
//! per-cpu allocator — lane `c` is `per_cpu_ptr(fbc->counters, c)` for
//! "cpu" c, and [`PercpuCounter::cpu`] is the ambient
//! `smp_processor_id()` of the sequential model (`this_cpu_*` ops touch
//! that lane). Each lane cell holds an s32-domain value: writes wrap at
//! 32 bits exactly like the C `s32 *pcount` (`s32w` below).
//!
//! Notes:
//! * Locks, irq-save sections and cmpxchg retry loops collapse in a
//!   sequential model — every cmpxchg succeeds on the first pass.
//! * Alloc-failure injection: set `fail_mask` bit `k` to fail the
//!   (k+1)-th `init`/`init_many` attempt with `-ENOMEM` (`alloc_ctr`
//!   counts attempts). A failed init releases the caller's slice.
//! * `init` on a live counter overwrites `counters` — like C
//!   re-init'ing a live counter leaks the old alloc; `destroy` first.
//!   `destroy` returns the lane slice, the `free_percpu` counterpart.
//!
//! Caller invariants (same contract as the C API):
//! * mutating ops on a non-`initialized` counter panic — the safe-Rust
//!   equivalent of the C NULL-pointer deref;
//! * `cpu` must be `< counters.len()` before `this_cpu` ops;
//! * the lane slice must not be modified while the counter is live.

#![forbid(unsafe_code)]
#![allow(clippy::all)]

use core::sync::atomic::{AtomicBool, Ordering};

/// `PERCPU_COUNTER_LOCAL_BATCH` — batch for add_local/sub_local.
pub const PERCPU_COUNTER_LOCAL_BATCH: i32 = i32::MAX;

/// `-ENOMEM` — a per-cpu allocation failed (or `fail_mask` bit was set).
pub const ENOMEM: i32 = -12;

/* WARN_ON_ONCE call sites — `static int seen` in C, shared process-wide
 * exactly like the C statics. */
static WARN_SEEN: [AtomicBool; 1] = [AtomicBool::new(false)];
const W_DESTROY_MANY_NULL: usize = 0;

fn warn_once(site: usize, cond: bool, hits: &mut u64) -> bool {
    if cond && !WARN_SEEN[site].swap(true, Ordering::Relaxed) {
        *hits += 1;
    }
    cond
}

fn kabs(v: i64) -> i64 {
    if v < 0 {
        v.wrapping_neg()
    } else {
        v
    }
}

/// Wrap a lane value into the C `s32` domain — the lanes are i64 cells
/// holding `s32` semantics, so a write truncates like `s32 *pcount`.
fn s32w(v: i64) -> i64 {
    (v as i32) as i64
}

/// `struct percpu_counter` — a batched approximate counter over a
/// caller-owned lane slice.
pub struct PercpuCounter<'a> {
    /// `fbc->count`: the folded global value (what `read` returns).
    pub count: i64,
    /// `fbc->counters`: the per-cpu `s32` buckets — one lane per cpu.
    /// `None` mirrors `counters == NULL` (never inited or destroyed).
    pub counters: Option<&'a mut [i64]>,
    /// Ambient `smp_processor_id()` of the sequential model — the lane
    /// `this_cpu_*` ops touch.
    pub cpu: usize,
    /// `percpu_counter_batch` (upstream default 32).
    pub batch: i32,
    /// Number of init attempts (for `fail_mask` injection).
    pub alloc_ctr: u64,
    /// Bit `k` set => the (k+1)-th `init`/`init_many` alloc fails.
    pub fail_mask: u64,
    /// WARN_ONCE hit counter (modelled; observable only).
    pub warn: u64,
}

impl<'a> PercpuCounter<'a> {
    /// A zeroed `struct percpu_counter` (`counters == NULL`).
    pub fn new() -> Self {
        PercpuCounter {
            count: 0,
            counters: None,
            cpu: 0,
            batch: 32,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
        }
    }

    /// `percpu_counter_init` — `__alloc_percpu_gfp` for one counter:
    /// takes the caller's lane slice (one cell per cpu), zeroes it like
    /// the alloc's memset, sets `count`. Returns 0 or `-ENOMEM` on
    /// `fail_mask` injection; on failure the counter stays
    /// uninitialized and the slice borrow is released.
    pub fn init(&mut self, lanes: &'a mut [i64], amount: i64) -> i32 {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            self.counters = None;
            return ENOMEM;
        }
        lanes.fill(0);
        self.count = amount;
        self.counters = Some(lanes);
        0
    }

    /// `percpu_counter_init_many` — one shared per-cpu allocation
    /// serving `fbcs.len()` counters, modelled as
    /// `fbcs.len() * ncpu` lane cells: `fbcs[i]` owns chunk `i`, so
    /// counter i's lane on cpu c is `lanes[i*ncpu + c]` (the per-cpu
    /// stride layout of the C block is unobservable in a sequential
    /// model). Single attempt against `fbcs[0]`'s `alloc_ctr`/
    /// `fail_mask`, all-or-nothing like the C alloc.
    ///
    /// `lanes.len()` must equal `fbcs.len() * ncpu` and `fbcs` must be
    /// non-empty.
    pub fn init_many(
        fbcs: &mut [PercpuCounter<'a>],
        lanes: &'a mut [i64],
        ncpu: usize,
        amount: i64,
    ) -> i32 {
        assert!(!fbcs.is_empty(), "percpu_counter: init_many of 0 counters");
        assert_eq!(
            lanes.len(),
            fbcs.len() * ncpu,
            "percpu_counter: init_many lanes != nr_counters * ncpu"
        );
        let f0 = &mut fbcs[0];
        f0.alloc_ctr += 1;
        if f0.alloc_ctr <= 64 && (f0.fail_mask >> (f0.alloc_ctr - 1)) & 1 == 1 {
            f0.counters = None;
            return ENOMEM;
        }
        lanes.fill(0);
        for (f, chunk) in fbcs.iter_mut().zip(lanes.chunks_exact_mut(ncpu)) {
            f.count = amount;
            f.counters = Some(chunk);
        }
        0
    }

    /// `percpu_counter_destroy_many` — frees the shared alloc; every
    /// counter's `counters` becomes NULL (None). `WARN_ON_ONCE(!fbc)`
    /// is unrepresentable: a slice is never null.
    pub fn destroy_many(fbcs: &mut [PercpuCounter<'a>]) {
        assert!(!fbcs.is_empty(), "percpu_counter: destroy_many of 0 counters");
        let mut hits = fbcs[0].warn;
        if warn_once(W_DESTROY_MANY_NULL, false, &mut hits) {
            fbcs[0].warn = hits;
            return;
        }
        fbcs[0].warn = hits;
        if fbcs[0].counters.is_none() {
            return;
        }
        for f in fbcs.iter_mut() {
            f.counters = None;
        }
    }

    /// `percpu_counter_destroy` — `free_percpu` counterpart: returns
    /// the lane slice so the caller gets its arena back.
    pub fn destroy(&mut self) -> Option<&'a mut [i64]> {
        self.counters.take()
    }

    /// `percpu_counter_initialized`.
    pub fn initialized(&self) -> bool {
        self.counters.is_some()
    }

    fn ncpu(&self) -> usize {
        self.counters.as_deref().map(|l| l.len()).unwrap_or(0)
    }

    /// Live lane slice; the panic is the safe equivalent of C's
    /// `*fbc->counters` NULL deref.
    fn lanes(&self) -> &[i64] {
        self.counters
            .as_deref()
            .expect("percpu_counter: op on uninitialized counter")
    }

    fn lanes_mut(&mut self) -> &mut [i64] {
        self.counters
            .as_deref_mut()
            .expect("percpu_counter: op on uninitialized counter")
    }

    /// `percpu_counter_set` — zero every lane, then store `amount`.
    /// (`fbc->count` is written even when `counters == NULL`, like C.)
    pub fn set(&mut self, amount: i64) {
        if let Some(lanes) = self.counters.as_deref_mut() {
            lanes.fill(0);
        }
        self.count = amount;
    }

    /// `self.cpu` bounds-checked — a caller-set cpu outside the lane
    /// arena is a model violation: warn and treat the op as a no-op
    /// (the `nidx`-scratch convention; C would corrupt memory).
    /// Uninitialized counters still panic like `lanes()` does.
    fn cpu_idx(&mut self) -> Option<usize> {
        let n = self.lanes().len();
        if self.cpu >= n {
            self.warn += 1;
            return None;
        }
        Some(self.cpu)
    }

    /// `percpu_counter_add_batch` — the `!CONFIG_HAVE_CMPXCHG_LOCAL`
    /// build the port models; the cmpxchg variant is equivalent in a
    /// sequential model.
    pub fn add_batch(&mut self, amount: i64, batch: i32) {
        let Some(cpu) = self.cpu_idx() else { return };
        let count = self.lanes()[cpu].wrapping_add(amount);
        if kabs(count) >= batch as i64 {
            self.count = self.count.wrapping_add(count);
            let folded = s32w(self.lanes()[cpu].wrapping_sub(count.wrapping_sub(amount)));
            self.lanes_mut()[cpu] = folded;
        } else {
            let next = s32w(self.lanes()[cpu].wrapping_add(amount));
            self.lanes_mut()[cpu] = next;
        }
    }

    /// `percpu_counter_sync` — fold this cpu's lane into `count`.
    pub fn sync(&mut self) {
        let Some(cpu) = self.cpu_idx() else { return };
        let lane = self.lanes()[cpu];
        self.count = self.count.wrapping_add(lane);
        self.lanes_mut()[cpu] = 0;
    }

    /// `percpu_counter_add` (`batch = self.batch`).
    pub fn add(&mut self, amount: i64) {
        self.add_batch(amount, self.batch);
    }

    /// `percpu_counter_sub`.
    pub fn sub(&mut self, amount: i64) {
        self.add(amount.wrapping_neg());
    }

    /// `percpu_counter_inc`.
    pub fn inc(&mut self) {
        self.add(1);
    }

    /// `percpu_counter_dec`.
    pub fn dec(&mut self) {
        self.add(-1);
    }

    /// `percpu_counter_add_local` (batch = PERCPU_COUNTER_LOCAL_BATCH).
    pub fn add_local(&mut self, amount: i64) {
        self.add_batch(amount, PERCPU_COUNTER_LOCAL_BATCH);
    }

    /// `percpu_counter_sub_local`.
    pub fn sub_local(&mut self, amount: i64) {
        self.add_local(amount.wrapping_neg());
    }

    /// `percpu_counter_add_return` — add, then report the precise sum.
    pub fn add_return(&mut self, amount: i64) -> i64 {
        self.add(amount);
        self.sum()
    }

    /// `percpu_counter_sub_return` — sub, then report the precise sum.
    pub fn sub_return(&mut self, amount: i64) -> i64 {
        self.sub(amount);
        self.sum()
    }

    /// `__percpu_counter_sum` / `percpu_counter_sum` — `count` plus all
    /// lanes. Sums zero lanes on a non-initialized counter (the port's
    /// model; the fast `compare` path uses `count` alone anyway).
    pub fn sum(&self) -> i64 {
        let mut ret = self.count;
        if let Some(lanes) = self.counters.as_deref() {
            for &v in lanes.iter() {
                ret = ret.wrapping_add(v);
            }
        }
        ret
    }

    /// `percpu_counter_sum_positive`.
    pub fn sum_positive(&self) -> i64 {
        let ret = self.sum();
        if ret < 0 {
            0
        } else {
            ret
        }
    }

    /// `percpu_counter_read` — the rough `fbc->count`.
    pub fn read(&self) -> i64 {
        self.count
    }

    /// `percpu_counter_read_positive`.
    pub fn read_positive(&self) -> i64 {
        if self.count >= 0 {
            self.count
        } else {
            0
        }
    }

    /// `__percpu_counter_compare`.
    pub fn compare_batch(&self, rhs: i64, batch: i32) -> i32 {
        let unknown = (batch as i64).wrapping_mul(self.ncpu() as i64);
        let count = self.count;
        if kabs(count.wrapping_sub(rhs)) > unknown {
            return if count > rhs { 1 } else { -1 };
        }
        let count = self.sum();
        if count > rhs {
            1
        } else if count < rhs {
            -1
        } else {
            0
        }
    }

    /// `percpu_counter_compare` (`batch = self.batch`).
    pub fn compare(&self, rhs: i64) -> i32 {
        self.compare_batch(rhs, self.batch)
    }

    /// `__percpu_counter_limited_add`.
    pub fn limited_add_batch(&mut self, limit: i64, amount: i64, batch: i32) -> bool {
        if amount == 0 {
            return true;
        }
        let unknown = (batch as i64).wrapping_mul(self.ncpu() as i64);
        let Some(cpu) = self.cpu_idx() else { return false };
        let count = self.lanes()[cpu];
        let gcount = self.count;

        /* skip taking the lock when safe */
        if kabs(count.wrapping_add(amount)) <= batch as i64
            && ((amount > 0 && gcount.wrapping_add(unknown) <= limit)
                || (amount < 0 && gcount.wrapping_sub(unknown) >= limit))
        {
            let next = s32w(self.lanes()[cpu].wrapping_add(amount));
            self.lanes_mut()[cpu] = next;
            return true;
        }

        let mut count2 = self.count.wrapping_add(amount);
        let mut good = false;
        if amount > 0 {
            if count2.wrapping_sub(unknown) > limit {
                return false;
            }
            if count2.wrapping_add(unknown) <= limit {
                good = true;
            }
        } else {
            if count2.wrapping_add(unknown) < limit {
                return false;
            }
            if count2.wrapping_sub(unknown) >= limit {
                good = true;
            }
        }

        if !good {
            for c in 0..self.ncpu() {
                count2 = count2.wrapping_add(self.lanes()[c]);
            }
            if amount > 0 {
                if count2 > limit {
                    return false;
                }
            } else if count2 < limit {
                return false;
            }
        }

        let count3 = self.lanes()[cpu];
        self.count = self.count.wrapping_add(count3).wrapping_add(amount);
        self.lanes_mut()[cpu] = 0;
        true
    }

    /// `percpu_counter_limited_add` (`batch = self.batch`).
    pub fn limited_add(&mut self, limit: i64, amount: i64) -> bool {
        self.limited_add_batch(limit, amount, self.batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("percpu_counter_tests.inc");
}
'''

# =====================================================================
# prb.rs — extracted verbatim methods + hand-shaped API surface.
# =====================================================================

PRB_HEADER = '''//! `klib::prb` — Linux `kernel/printk/printk_ringbuffer.c` ported to
//! safe Rust, verified against the C implementation (966/966 recorded
//! differential cases shared with `percpu_counter`,
//! `ports/lib-percpu-printk`).
//!
//! Storage model: caller-owned rings replace the C pointers —
//! [`PrbDescRing`]'s `descs`/`infos` are the `struct prb_desc`/
//! `struct printk_info` arrays (`1 << count_bits` entries in use) and
//! [`PrbDataRing`]'s `data` is the text byte ring (`1 << size_bits`
//! bytes in use), exactly like upstream's caller-allocated
//! `prb_desc_ring`/`prb_data_ring`. Descriptor state_var packs a 62-bit
//! ID plus a top-2-bit state; record "pointers" ([`PrintkRecord::info`]/
//! `text_buf`) are an `infos` index and a byte offset into `data`.
//!
//! Sequential model: every `atomic_long` cmpxchg succeeds on the first
//! pass; the verified model makes no claims about interleaved writers.
//!
//! Livelock curation (from `internal.h`, unchanged):
//! * `panic_on_this_cpu()` (driven by the `panic` knob) reports panic
//!   only while the panic skip provably terminates — i.e. while
//!   [`Prb::nrs_livelock`] is false;
//! * [`Prb::next_reserve_seq`] returns `None` in exactly the state
//!   where upstream's `prb_next_reserve_seq()` could loop forever —
//!   the verified harness folds the `0xdeaddead` sentinel there
//!   instead of calling the function. Every terminating case returns
//!   `Some` with upstream's value.
//!
//! Caller invariants (same contract as the C API):
//! * `init` must run before any other op (like `prb_init`);
//! * ring storage must not be modified while the `Prb` is live;
//! * `PrintkRecord`/`PrbReservedEntry` are opaque handles — start each
//!   op with fresh ones (`PrintkRecord::wr`, `Default`).

#![forbid(unsafe_code)]
#![allow(clippy::all)]

use core::sync::atomic::{AtomicBool, Ordering};

const FAILED_LPOS: u64 = 0x1;
const EMPTY_LINE_LPOS: u64 = 0x3;

const DESC_FLAGS_SHIFT: u32 = 62;
const DESC_ID_MASK: u64 = !(3u64 << DESC_FLAGS_SHIFT);

/* descriptor states (enum desc_state) */
const DESC_MISS: i32 = -1;
const DESC_RESERVED: u64 = 0x0;
const DESC_COMMITTED: u64 = 0x1;
const DESC_FINALIZED: u64 = 0x2;
const DESC_REUSABLE: u64 = 0x3;

/// `-EINVAL` — invalid id/seq or not-yet-finalized record.
pub const EINVAL: i32 = -22;
/// `-ENOENT` — record is reusable/data-less or its data is gone.
pub const ENOENT: i32 = -2;

/* WARN_ON_ONCE call sites — `static int seen` in C, shared process-wide
 * exactly like the C statics. Site numbering matches the verified port;
 * site 0 (destroy_many NULL) belongs to the percpu_counter unit. */
static WARN_SEEN: [AtomicBool; 13] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];
const W_DESC_RESERVE_STALE: usize = 1;
const W_DESC_RESERVE_CMPXCHG: usize = 2;
const W_DATA_ALLOC_OVERSIZE: usize = 3;
const W_DATA_REALLOC_OVERSIZE: usize = 4;
const W_GET_DATA_ILLEGAL: usize = 5;
const W_GET_DATA_BEGIN_ALIGN: usize = 6;
const W_GET_DATA_NEXT_ALIGN: usize = 7;
const W_GET_DATA_TOO_SMALL: usize = 8;
const W_GET_DATA_CHECK: usize = 9;
const W_RIL_DATALESS_LEN: usize = 10;
const W_RIL_LEN_OVERRUN: usize = 11;
const W_COMMIT_CMPXCHG: usize = 12;

fn warn_once(site: usize, cond: bool, hits: &mut u64) -> bool {
    if cond && !WARN_SEEN[site].swap(true, Ordering::Relaxed) {
        *hits += 1;
    }
    cond
}

/* ------------------------------------------------------------------ */
/* types (struct prb_desc / printk_info / data_blk_lpos + the rings)    */
/* ------------------------------------------------------------------ */

/// `struct prb_data_blk_lpos` — logical begin/next of a data block.
#[derive(Clone, Copy, Default)]
pub struct BlkLpos {
    pub begin: u64,
    pub next: u64,
}

/// `struct prb_desc` — state_var (id | state<<62) plus block lpos.
#[derive(Clone, Copy, Default)]
pub struct PrbDesc {
    pub sv: u64,
    pub blk: BlkLpos,
}

/// `struct printk_info` — record metadata. `subsystem`/`device` are the
/// flattened `dev_printk_info` fields (dev_info.subsystem/dev_info.device).
#[derive(Clone, Copy)]
pub struct PrintkInfo {
    pub seq: u64,
    pub ts_nsec: u64,
    pub text_len: u16,
    pub caller_id: u32,
    pub facility: u8,
    pub level: u8,
    pub flags: u8,
    pub subsystem: [u8; 16],
    pub device: [u8; 48],
}
impl Default for PrintkInfo {
    fn default() -> Self {
        PrintkInfo {
            seq: 0,
            ts_nsec: 0,
            text_len: 0,
            caller_id: 0,
            facility: 0,
            level: 0,
            flags: 0,
            subsystem: [0; 16],
            device: [0; 48],
        }
    }
}

/// `struct prb_desc_ring` — caller-owned desc + info arrays.
pub struct PrbDescRing<'a> {
    pub count_bits: u32,
    pub descs: &'a mut [PrbDesc],
    pub infos: &'a mut [PrintkInfo],
    pub head_id: u64,
    pub tail_id: u64,
    pub last_finalized_seq: u64,
}

/// `struct prb_data_ring` — the caller-owned text byte ring.
pub struct PrbDataRing<'a> {
    pub size_bits: u32,
    pub data: &'a mut [u8],
    pub head_lpos: u64,
    pub tail_lpos: u64,
}

/// `struct printk_ringbuffer` over caller-owned rings.
pub struct Prb<'a> {
    /// `rb->desc_ring`.
    pub desc_ring: PrbDescRing<'a>,
    /// `rb->text_data_ring`.
    pub text_ring: PrbDataRing<'a>,
    /// `rb->fail`: count of reserves that couldn't even create a
    /// data-less record.
    pub fail: u64,
    /// `kshim_panic` model knob — gates the panic skip path in
    /// `read_valid` (see `panic_on_this_cpu`).
    pub panic: bool,
    /// `debug_non_panic_cpus` model knob.
    pub debug_non_panic_cpus: bool,
    /// `legacy_allow_panic_sync` model knob.
    pub legacy_allow_panic_sync: bool,
    /// WARN_ONCE hit counter (modelled; observable only).
    pub warn: u64,
}

/// `struct prb_reserved_entry` — the writer's reserve/commit handle.
/// (The C `rb`/`irqflags` fields are implicit in the sequential model.)
#[derive(Clone, Copy, Default)]
pub struct PrbReservedEntry {
    /// ID of the reserved descriptor.
    pub id: u64,
    /// Total occupied text-ring space (id + padding + wrapping).
    pub text_space: u32,
}

/// `struct printk_record` — pointers modelled as ring indices.
///
/// `info` is an index into `desc_ring.infos` (`r->info`), `text_buf` a
/// byte offset into `text_ring.data` at the record's data past the
/// 8-byte block id (`r->text_buf`). `None` is the C NULL.
#[derive(Clone, Copy, Default)]
pub struct PrintkRecord {
    pub info: Option<usize>,
    pub text_buf: Option<usize>,
    pub text_buf_size: u32,
}

impl PrintkRecord {
    /// `prb_rec_init_wr` — a fresh writer record requesting
    /// `text_buf_size` bytes.
    pub fn wr(text_buf_size: u32) -> Self {
        PrintkRecord {
            info: None,
            text_buf: None,
            text_buf_size,
        }
    }
}

/* reader-destination descriptor — `struct printk_record` for reads:
 * `info` is `r->info`, `buf` is `r->text_buf`/`r->text_buf_size`. */
struct ReadDst<'x> {
    info: Option<&'x mut PrintkInfo>,
    buf: Option<&'x mut [u8]>,
}

/* ------------------------------------------------------------------ */
/* free helpers (verbatim from the port)                               */
/* ------------------------------------------------------------------ */
'''

# region 741-765: desc_id..to_blk_size free helpers
PRB_HELPERS = region(741, 765)
PRB_HELPERS = re.sub(r"(?m)^fn ", "pub fn ", PRB_HELPERS)
# critic — `bits` is derived from pub caller-mutable fields; C shifts
# wrap/UB, model with wrapping shifts, and to_blk_size must not panic
# on u32::MAX requests
for _old, _new in [
    ("(lpos & ((1u64 << size_bits) - 1))",
     "(lpos & 1u64.wrapping_shl(size_bits).wrapping_sub(1))"),
    ("lpos >> size_bits", "lpos.wrapping_shr(size_bits)"),
    ("(1u64 << bits).wrapping_neg()", "1u64.wrapping_shl(bits).wrapping_neg()"),
    ("((1u64 << bits) + 1).wrapping_neg()",
     "1u64.wrapping_shl(bits).wrapping_add(1).wrapping_neg()"),
    ("(size + 8).wrapping_add(7)", "size.wrapping_add(8).wrapping_add(7)"),
]:
    assert _old in PRB_HELPERS, _old
    PRB_HELPERS = PRB_HELPERS.replace(_old, _new)

IMPL_HEAD = '''
impl<'a> Prb<'a> {
    /// Construct + `prb_init` over caller-owned rings:
    /// `prb_init(rb, text, textbits, descs, descbits, infos)`.
    pub fn new(
        text: &'a mut [u8],
        textbits: u32,
        descs: &'a mut [PrbDesc],
        descbits: u32,
        infos: &'a mut [PrintkInfo],
    ) -> Self {
        let mut rb = Prb {
            desc_ring: PrbDescRing {
                count_bits: 0,
                descs,
                infos,
                head_id: 0,
                tail_id: 0,
                last_finalized_seq: 0,
            },
            text_ring: PrbDataRing {
                size_bits: 0,
                data: text,
                head_lpos: 0,
                tail_lpos: 0,
            },
            fail: 0,
            panic: false,
            debug_non_panic_cpus: false,
            legacy_allow_panic_sync: false,
            warn: 0,
        };
        rb.init(textbits, descbits);
        rb
    }

    /// Re-initialize the ring at its current geometry — the harness's
    /// "clear" op (`prb_init` on the live arrays; upstream has no
    /// separate clear). Equivalent to a fresh ring.
    pub fn clear_ringbuffer(&mut self) {
        self.init(self.text_ring.size_bits, self.desc_ring.count_bits);
    }
'''

# prb_init — extracted, signature swapped, capacity asserts injected.
PRB_INIT = meth(
    1453, 1472,
    "    fn prb_init(&mut self, textbits: u32, descbits: u32) {",
    """    /// `prb_init` — (re)initialize over the caller-owned rings.
    /// Requires `descs.len() >= 1<<descbits`,
    /// `infos.len() >= 1<<descbits`, `data.len() >= 1<<textbits`
    /// (C takes exactly-sized arrays).
    pub fn init(&mut self, textbits: u32, descbits: u32) {""",
    extra=[
        (
            "        let n = 1usize << descbits;",
            """        assert!(
            descbits < usize::BITS && textbits < usize::BITS,
            "prb: ring geometry bits must be < usize::BITS"
        );
        let n = 1usize << descbits;
        assert!(
            self.desc_ring.descs.len() >= n && self.desc_ring.infos.len() >= n,
            "prb: desc/info rings smaller than 1<<descbits"
        );
        assert!(
            self.text_ring.data.len() >= 1usize << textbits,
            "prb: text ring smaller than 1<<textbits"
        );""",
        )
    ],
)

# verbatim-extracted internal methods (port line ranges)
EXTRACTED = [
    region(768, 779),   # count, mask, desc_idx, data_size
    region(781, 787),   # blk_read_id, blk_write_id
    region(789, 791),   # is_blk_wrapped
    region(793, 795),   # need_more_space
    region(797, 803),   # get_next_lpos
    region(805, 810),   # get_desc_state
    region(812, 827),   # desc_read
    region(829, 834),   # desc_make_reusable
    region(836, 861),   # data_make_reusable
    region(863, 883),   # data_push_tail
    region(885, 915),   # desc_push_tail
    region(917, 945),   # desc_reserve
    region(947, 953),   # desc_make_final
    region(955, 957),   # desc_last_finalized_seq
    region(959, 971),   # desc_update_last_finalized
    region(973, 984),   # desc_read_finalized_seq
    region(986, 1034),  # get_data
    region(1036, 1041), # data_check_size
    region(1043, 1052), # space_used
    region(1054, 1087), # data_alloc
    region(1089, 1133), # data_realloc
    region(1135, 1151), # count_lines
]

HAND_MIDDLE = '''
    /* copy_data(): copy expected @len of data from @blk into the
     * reader's buf. buf presence is `r->text_buf != NULL` in C;
     * buf.len() is `r->text_buf_size`. */
    fn copy_data(
        &mut self,
        blk: &BlkLpos,
        len: u16,
        buf: Option<&mut [u8]>,
        line_count: Option<&mut u32>,
    ) -> bool {
        if buf.is_none() && line_count.is_none() {
            return true;
        }
        let Some((off, data_size)) = self.get_data(blk) else {
            return false;
        };
        if (data_size as u64) < len as u64 {
            return false;
        }
        if let Some(lc) = line_count {
            *lc = self.count_lines(off, len as usize);
        }
        let Some(b) = buf else {
            return true;
        };
        let n = (b.len() as u64).min(len as u64) as usize;
        let Some(src) = off
            .checked_add(n)
            .and_then(|e| self.text_ring.data.get(off..e))
        else {
            return false;
        };
        b[..n].copy_from_slice(src);
        true
    }

    /* prb_read(): core read. dst.info copies the found info out
     * (to_info(seq)); dst.buf copies text with buf.len() as the cap. */
    fn prb_read(
        &mut self,
        seq: u64,
        r: Option<&mut ReadDst<'_>>,
        line_count: Option<&mut u32>,
    ) -> i32 {
        let idx = self.desc_idx(seq);
        let id = desc_id(self.desc_ring.descs[idx].sv);
        let mut blk = BlkLpos::default();
        let err = self.desc_read_finalized_seq(id, seq, &mut blk);
        if err != 0 || r.is_none() {
            return err;
        }
        let r = r.unwrap();
        if let Some(out) = r.info.as_deref_mut() {
            *out = self.desc_ring.infos[idx];
        }
        let len = self.desc_ring.infos[idx].text_len;
        let ok = self.copy_data(&blk, len, r.buf.as_deref_mut(), line_count);
        if !ok {
            return ENOENT;
        }
        self.desc_read_finalized_seq(id, seq, &mut blk)
    }
'''

PRB_FIRST_SEQ = meth(
    1211, 1219,
    "    fn prb_first_seq(&mut self) -> u64 {",
    """    /// `prb_first_seq` — sequence number of the tail (oldest)
    /// descriptor, finalized or not.
    pub fn first_seq(&mut self) -> u64 {""",
).replace(
    """        loop {
            let id = self.desc_ring.tail_id;
            let (st, _sv, _blk, seq, _cid) = self.desc_read(id);
            if st == DESC_FINALIZED as i32 || st == DESC_REUSABLE as i32 {
                return seq;
            }
        }
    }""",
    """        // sequential-model bound: the upstream cmpxchg-retry loop can
        // only spin when a (caller-corrupted) tail id never reaches
        // FINALIZED/REUSABLE — bound it like the other livelock sites.
        // Bound by the physical ring, not count(): a crafted count_bits
        // would give a de-facto-unbounded 2^63.
        for _ in 0..self.desc_ring.descs.len().max(1) {
            let id = self.desc_ring.tail_id;
            let (st, _sv, _blk, seq, _cid) = self.desc_read(id);
            if st == DESC_FINALIZED as i32 || st == DESC_REUSABLE as i32 {
                return seq;
            }
        }
        self.warn += 1;
        0
    }""",
)

NRS_PANIC = meth(1221, 1248, "    fn nrs_livelock(&self) -> bool {",
    """    /// `kshim_nrs_livelock` — true exactly when
    /// `prb_next_reserve_seq()` (and therefore the panic skip) could
    /// never terminate: a `try_again` loop or the un-masked head/tail
    /// id diff climbing toward ~2^62 (see `internal.h`).
    pub fn nrs_livelock(&self) -> bool {""").replace(
    "    fn panic_on_this_cpu(&self) -> bool {",
    """    /// `panic_on_this_cpu` — the `kshim_panic` gate: panic only
    /// while the panic skip provably terminates.
    pub fn panic_on_this_cpu(&self) -> bool {""",
)

NEXT_RESERVE = meth(
    1250, 1274,
    "    fn prb_next_reserve_seq(&mut self) -> u64 {",
    """    /* `prb_next_reserve_seq` raw — may loop forever when
     * nrs_livelock(); call it only via next_reserve_seq(). */
    fn prb_next_reserve_seq(&mut self) -> u64 {""",
) + """
    /// `prb_next_reserve_seq` with the verified harness's livelock
    /// curation: `None` is the `0xdeaddead` sentinel — the state where
    /// upstream's loop cannot terminate (`nrs_livelock()`).
    pub fn next_reserve_seq(&mut self) -> Option<u64> {
        if self.nrs_livelock() {
            return None;
        }
        Some(self.prb_next_reserve_seq())
    }
"""

READ_VALID_RAW = meth(
    1276, 1305,
    "        r: Option<&mut ReadDst>,",
    "        r: Option<&mut ReadDst<'_>>,",
)

COMMITS = meth(
    1307, 1329,
    "    fn prb_commit(&mut self, id: u64) {",
    """    /* `prb_commit` by id — used internally by reserve paths. */
    fn prb_commit_id(&mut self, id: u64) {""",
).replace(
    "    fn prb_final_commit(&mut self, id: u64) {",
    """    /* `prb_final_commit` by id. */
    fn prb_final_commit_id(&mut self, id: u64) {""",
) + """
    /// `prb_commit` — commit the reserved entry (reopenable by
    /// `reserve_in_last` while it is the head).
    pub fn commit(&mut self, e: &PrbReservedEntry) {
        self.prb_commit_id(e.id);
    }

    /// `prb_final_commit` — commit and finalize in one step.
    pub fn final_commit(&mut self, e: &PrbReservedEntry) {
        self.prb_final_commit_id(e.id);
    }

    /// `prb_record_text_space`.
    pub fn record_text_space(&self, e: &PrbReservedEntry) -> u32 {
        e.text_space
    }
"""

RESERVE = meth(
    1331, 1372,
    "    fn prb_reserve(&mut self, w: usize, r: &mut WRec) -> bool {",
    """    /// `prb_reserve` — on success `e` is the commit handle and `r`
    /// carries `info`/`text_buf` for the writer to fill.
    pub fn reserve(&mut self, e: &mut PrbReservedEntry, r: &mut PrintkRecord) -> bool {""",
)

RIL = meth(
    1374, 1451,
    """    fn prb_reserve_in_last(
        &mut self,
        w: usize,
        r: &mut WRec,
        caller_id: u32,
        max_size: u32,
    ) -> bool {""",
    """    /// `prb_reserve_in_last` — reopen the head record (must be
    /// `commit`ted, not finalized, by `caller_id`) and extend it up to
    /// `max_size` total bytes.
    pub fn reserve_in_last(
        &mut self,
        e: &mut PrbReservedEntry,
        r: &mut PrintkRecord,
        caller_id: u32,
        max_size: u32,
    ) -> bool {""",
)

TAIL_API = '''
    /// `prb_read_valid(rb, seq, r)` — `info` is `r->info`, `buf` the
    /// `r->text_buf`/`r->text_buf_size` pair (size = `buf.len()`).
    pub fn read_valid(
        &mut self,
        seq: u64,
        info: Option<&mut PrintkInfo>,
        buf: Option<&mut [u8]>,
    ) -> bool {
        let mut q = seq;
        let mut dst = ReadDst { info, buf };
        self.prb_read_valid_raw(&mut q, Some(&mut dst), None)
    }

    /// `prb_read_valid_info` — metadata only, plus the text line count.
    pub fn read_valid_info(
        &mut self,
        seq: u64,
        info: &mut PrintkInfo,
        line_count: &mut u32,
    ) -> bool {
        let mut q = seq;
        let mut dst = ReadDst {
            info: Some(info),
            buf: None,
        };
        self.prb_read_valid_raw(&mut q, Some(&mut dst), Some(line_count))
    }
'''

FIRST_VALID = meth(
    1496, 1502,
    "    fn prb_first_valid_seq(&mut self) -> u64 {",
    """    /// `prb_first_valid_seq` — oldest readable (finalized) seq.
    pub fn first_valid_seq(&mut self) -> u64 {""",
)

NEXT_SEQ = meth(
    1504, 1513,
    "    fn prb_next_seq(&mut self) -> u64 {",
    """    /// `prb_next_seq` — the next seq a writer will be assigned.
    pub fn next_seq(&mut self) -> u64 {""",
)

FOOT = '''
    /// `r->info` — borrow the reserved record's info (index into
    /// `desc_ring.infos`) to fill `text_len`, `caller_id`, `ts_nsec`,
    /// `level`, `facility`, `flags`, `subsystem`, `device`.
    pub fn info_mut(&mut self, r: &PrintkRecord) -> Option<&mut PrintkInfo> {
        let n = self.desc_ring.infos.len();
        r.info.and_then(move |i| {
            if i >= n {
                self.warn += 1;
                None
            } else {
                Some(&mut self.desc_ring.infos[i])
            }
        })
    }

    /// `r->text_buf` — the reserved text region
    /// `data[text_buf .. text_buf + text_buf_size]`. For a
    /// `reserve_in_last` record it covers the old plus appended text.
    pub fn text_buf_mut(&mut self, r: &PrintkRecord) -> Option<&mut [u8]> {
        let bo = r.text_buf?;
        let n = r.text_buf_size as usize;
        let end = match bo.checked_add(n) {
            Some(e) if e <= self.text_ring.data.len() => e,
            _ => {
                self.warn += 1;
                return None;
            }
        };
        Some(&mut self.text_ring.data[bo..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("prb_tests.inc");
}
'''


def write_prb():
    parts = [PRB_HEADER, PRB_HELPERS, IMPL_HEAD, PRB_INIT]
    parts += [prb_rename(t) for t in EXTRACTED]
    parts += [
        HAND_MIDDLE,
        PRB_FIRST_SEQ,
        NRS_PANIC,
        NEXT_RESERVE,
        READ_VALID_RAW,
        COMMITS,
        RESERVE,
        RIL,
        TAIL_API,
        FIRST_VALID,
        NEXT_SEQ,
        FOOT,
    ]
    out = "\n".join(parts)
    open("klib/src/prb.rs", "w", encoding="utf-8", newline="\n").write(out)
    print(f"wrote klib/src/prb.rs: {len(out.splitlines())} lines")


def write_pc():
    open("klib/src/percpu_counter.rs", "w", encoding="utf-8", newline="\n").write(
        PC_HEADER
    )
    print(f"wrote klib/src/percpu_counter.rs: {len(PC_HEADER.splitlines())} lines")


write_pc()
write_prb()
