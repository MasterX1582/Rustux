//! `klib::percpu_counter` — Linux `lib/percpu_counter.c` +
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
