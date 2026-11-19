//! Safe Rust port of the lib-percpu-printk differential harness.
//!
//! Two layers are ported, both faithfully to `wrappers.c`:
//!   1. upstream `lib/percpu_counter.c` semantics — the shared s64
//!      `count` plus four per-CPU `s32` buckets, the batch fold rule,
//!      the read-only `__percpu_counter_sum`, `compare`, `limited_add`
//!      and `init_many`/`destroy_many` — modelled on the deterministic
//!      8-slot per-cpu allocator whose "pointers" are byte offsets into
//!      `kshim_pcpu_area` exactly like `canon_pcpu()` produces in C; and
//!   2. upstream `kernel/printk/printk_ringbuffer.c` semantics — the
//!      descriptor ring (62-bit IDs + top-2-bit state), the data ring
//!      (logical positions, wrapping blocks, data-less sentinels),
//!      reservation/commit/finalize/reopen state machine, the reader
//!      skip logic and the panic path — modelled with canonical
//!      indices instead of pointers.
//!
//! `#![forbid(unsafe_code)]`: every C pointer is an index/offset u64.
//!
//! UB curations mirrored from the C harness (see internal.h):
//!   - `panic_on_this_cpu()` also requires `!kshim_nrs_livelock()`: the
//!     state where upstream's panic skip / `prb_next_reserve_seq()`
//!     could never terminate is reported as "not panicking" so every
//!     terminating case replays identically.
//!   - direct `prb_next_reserve_seq()` calls in the drivers fold the
//!     sentinel 0xdeaddead when that livelock predicate holds.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

const NILX: u64 = 0xffff;
const KSHIM_NCPU: usize = 4;
const PCPU_STRIDE: usize = 256;
const PCPU_SLOT_SHIFT: usize = 5;
const PCPU_NSLOTS: usize = PCPU_STRIDE >> PCPU_SLOT_SHIFT; /* 8 */

const NCTR: usize = 8;
const NMANY: usize = 4;

const NW: usize = 3;
const RTBUF: usize = 64;
const PRB_MAX_DESCS: usize = 8;
const PRB_MAX_TEXT: usize = 256;

const PERCPU_COUNTER_LOCAL_BATCH: i32 = i32::MAX;

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

const EINVAL: i32 = -22;
const ENOENT: i32 = -2;
const ENOMEM: i32 = -12;

/* WARN_ON_ONCE call sites (each `static int __seen` in C).  Order must
 * not change: the flags persist for the process just like the C
 * statics do. */
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
const W_DESTROY_MANY_NULL: usize = 0;
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

fn kabs(v: i64) -> i64 {
    if v < 0 { v.wrapping_neg() } else { v }
}

/* ------------------------------------------------------------------ */
/* fold plumbing (identical to wrappers.c)                            */
/* ------------------------------------------------------------------ */

struct Fold {
    h: u64,
}
impl Fold {
    fn add(&mut self, v: u64) {
        self.h = (self.h ^ v).wrapping_mul(0x100000001b3);
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.add(x as u64);
        }
    }
}

fn xs64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(2685821657736338717)
}

fn iabs(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}

/* ================================================================== */
/* percpu_counter unit                                                */
/* ================================================================== */

#[derive(Clone, Copy, Default)]
struct Fbc {
    count: i64,
    counters: Option<usize>, /* byte offset inside each cpu chunk */
}

struct S {
    /* harness globals */
    kshim_cpu: usize,
    warn_hits: u64,
    panic: bool,
    debug_non_panic_cpus: bool,
    legacy_allow_panic_sync: bool,

    /* kshim_pcpu_area as flat s32s: byte offset o -> word o/4 + cpu*64 */
    pcpu: [i32; KSHIM_NCPU * (PCPU_STRIDE / 4)],
    slot_used: [bool; PCPU_NSLOTS],
    alloc_ctr: u64,
    fail_mask: u64,
    batch: i32,

    ctrs: [Fbc; NCTR],
    many: [Fbc; NMANY],

    /* ringbuffer arrays */
    descs: [PrbDesc; PRB_MAX_DESCS],
    infos: [PrintkInfo; PRB_MAX_DESCS],
    text: [u8; PRB_MAX_TEXT],
    rb: Rb,

    es: [REntry; NW],
    rws: [WRec; NW],
    eopen: [bool; NW],
    rw2: WRec,
    last_cid: u32,

    rinfo: PrintkInfo,
    rtext: [u8; RTBUF],

    rng: u64,
}

fn pcpu_off(off: usize, cpu: usize) -> usize {
    /* byte offset -> flat s32 index */
    (off + cpu * PCPU_STRIDE) / 4
}

impl S {
    fn new() -> Self {
        S {
            kshim_cpu: 0,
            warn_hits: 0,
            panic: false,
            debug_non_panic_cpus: false,
            legacy_allow_panic_sync: false,
            pcpu: [0; KSHIM_NCPU * (PCPU_STRIDE / 4)],
            slot_used: [false; PCPU_NSLOTS],
            alloc_ctr: 0,
            fail_mask: 0,
            batch: 32,
            ctrs: [Fbc::default(); NCTR],
            many: [Fbc::default(); NMANY],
            descs: [PrbDesc::default(); PRB_MAX_DESCS],
            infos: [PrintkInfo::default(); PRB_MAX_DESCS],
            text: [0; PRB_MAX_TEXT],
            rb: Rb::default(),
            es: [REntry::default(); NW],
            rws: [WRec::default(); NW],
            eopen: [false; NW],
            rw2: WRec::default(),
            last_cid: 0,
            rinfo: PrintkInfo::default(),
            rtext: [0; RTBUF],
            rng: 0,
        }
    }

    /* ---- deterministic percpu allocator ---- */

    fn pcpu_alloc_raw(&mut self, size: usize) -> Option<usize> {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            return None;
        }
        for i in 0..PCPU_NSLOTS {
            if !self.slot_used[i] && size <= (1 << PCPU_SLOT_SHIFT) {
                self.slot_used[i] = true;
                for cpu in 0..KSHIM_NCPU {
                    for w in 0..(1 << (PCPU_SLOT_SHIFT - 2)) {
                        self.pcpu[pcpu_off(i << PCPU_SLOT_SHIFT, cpu) + w] = 0;
                    }
                }
                return Some(i << PCPU_SLOT_SHIFT);
            }
        }
        None
    }

    fn free_percpu(&mut self, off: Option<usize>) {
        if let Some(o) = off {
            self.slot_used[o >> PCPU_SLOT_SHIFT] = false;
        }
    }

    fn canon_pcpu(p: Option<usize>) -> u64 {
        match p {
            Some(o) => o as u64,
            None => NILX,
        }
    }

    /* ---- upstream percpu_counter semantics ---- */

    fn init_many(&mut self, arr: &mut [Fbc], amount: i64, nr: usize) -> i32 {
        let counter_size = 4usize; /* ALIGN(sizeof(s32), alignof) */
        match self.pcpu_alloc_raw(nr * counter_size) {
            None => {
                arr[0].counters = None;
                ENOMEM
            }
            Some(base) => {
                for (i, a) in arr.iter_mut().enumerate() {
                    a.count = amount;
                    a.counters = Some(base + i * counter_size);
                }
                0
            }
        }
    }

    fn destroy_many(&mut self, arr: &mut [Fbc], _nr: usize) {
        let mut hits = self.warn_hits;
        if warn_once(W_DESTROY_MANY_NULL, false, &mut hits) {
            self.warn_hits = hits;
            return;
        }
        self.warn_hits = hits;
        if arr[0].counters.is_none() {
            return;
        }
        let base = arr[0].counters;
        self.free_percpu(base);
        for a in arr.iter_mut() {
            a.counters = None;
        }
    }

    fn set(fbc: &mut Fbc, pcpu: &mut [i32], amount: i64) {
        if let Some(o) = fbc.counters {
            for cpu in 0..KSHIM_NCPU {
                pcpu[pcpu_off(o, cpu)] = 0;
            }
        }
        fbc.count = amount;
    }

    fn add_batch(&mut self, fbc: &mut Fbc, amount: i64, batch: i32) {
        let cpu = self.kshim_cpu;
        let off = fbc.counters.unwrap();
        let idx = pcpu_off(off, cpu);
        let count = self.pcpu[idx] as i64 + amount;
        if kabs(count) >= batch as i64 {
            fbc.count += count;
            self.pcpu[idx] = (self.pcpu[idx] as i64 - (count - amount)) as i32;
        } else {
            self.pcpu[idx] = (self.pcpu[idx] as i64).wrapping_add(amount) as i32;
        }
    }

    fn sync(&mut self, fbc: &mut Fbc) {
        let idx = pcpu_off(fbc.counters.unwrap(), self.kshim_cpu);
        let count = self.pcpu[idx];
        fbc.count += count as i64;
        self.pcpu[idx] = 0;
    }

    fn sum(&self, fbc: &Fbc) -> i64 {
        let mut ret = fbc.count;
        if let Some(o) = fbc.counters {
            for cpu in 0..KSHIM_NCPU {
                ret += self.pcpu[pcpu_off(o, cpu)] as i64;
            }
        }
        ret
    }

    fn compare(&self, fbc: &Fbc, rhs: i64, batch: i32) -> i32 {
        let unknown = batch.wrapping_mul(KSHIM_NCPU as i32) as i64;
        let count = fbc.count;
        if kabs(count - rhs) > unknown {
            return if count > rhs { 1 } else { -1 };
        }
        let count = self.sum(fbc);
        if count > rhs {
            1
        } else if count < rhs {
            -1
        } else {
            0
        }
    }

    fn limited_add(&mut self, fbc: &mut Fbc, limit: i64, amount: i64, batch: i32) -> bool {
        if amount == 0 {
            return true;
        }
        let unknown = batch.wrapping_mul(KSHIM_NCPU as i32) as i64;
        let cpu = self.kshim_cpu;
        let idx = pcpu_off(fbc.counters.unwrap(), cpu);
        let count = self.pcpu[idx] as i64;
        let gcount = fbc.count;

        /* skip taking the lock when safe */
        if kabs(count + amount) <= batch as i64
            && ((amount > 0 && gcount + unknown <= limit)
                || (amount < 0 && gcount - unknown >= limit))
        {
            self.pcpu[idx] = (self.pcpu[idx] as i64).wrapping_add(amount) as i32;
            return true;
        }

        let mut count2 = fbc.count + amount;
        let mut good = false;
        if amount > 0 {
            if count2 - unknown > limit {
                return false;
            }
            if count2 + unknown <= limit {
                good = true;
            }
        } else {
            if count2 + unknown < limit {
                return false;
            }
            if count2 - unknown >= limit {
                good = true;
            }
        }

        if !good {
            for c in 0..KSHIM_NCPU {
                count2 += self.pcpu[pcpu_off(fbc.counters.unwrap(), c)] as i64;
            }
            if amount > 0 {
                if count2 > limit {
                    return false;
                }
            } else if count2 < limit {
                return false;
            }
        }

        let count3 = self.pcpu[idx] as i64;
        fbc.count += count3 + amount;
        self.pcpu[idx] = 0;
        true
    }

    /* ---- dumps ---- */

    fn dump_fbc(&self, f: &mut Fold, fbc: &Fbc) {
        f.add(fbc.count as u64);
        f.add(S::canon_pcpu(fbc.counters));
        match fbc.counters {
            None => f.add(NILX),
            Some(o) => {
                for cpu in 0..KSHIM_NCPU {
                    f.add(self.pcpu[pcpu_off(o, cpu)] as u32 as u64);
                }
            }
        }
    }

    fn dump_pcpu(&self, f: &mut Fold) {
        f.add(0xbc50);
        for i in 0..NCTR {
            self.dump_fbc(f, &self.ctrs[i]);
        }
        for i in 0..NMANY {
            self.dump_fbc(f, &self.many[i]);
        }
        for &u in self.slot_used.iter() {
            f.add(u as u64);
        }
        f.add(self.batch as u64);
        f.add(self.kshim_cpu as u64);
        f.add(self.warn_hits);
    }

    fn pcpu_reset(&mut self, seed2: u64) {
        self.pcpu = [0; KSHIM_NCPU * (PCPU_STRIDE / 4)];
        self.slot_used = [false; PCPU_NSLOTS];
        self.ctrs = [Fbc::default(); NCTR];
        self.many = [Fbc::default(); NMANY];
        self.alloc_ctr = 0;
        self.kshim_cpu = 0;
        self.warn_hits = 0;
        self.batch = 8i32 << (iabs(seed2 as i64) % 5) as u32; /* 8..128 */
    }
}

fn gen_amt(r: u64) -> i64 {
    match r % 10 {
        0..=2 => ((r >> 8) % 13) as i64 - 6,
        3 => 29 + ((r >> 8) % 7) as i64,
        4 => -(29 + ((r >> 8) % 7) as i64),
        5 => ((r >> 8) % 250) as i64 - 125,
        6 => ((r >> 8) % 1024) as i64,
        7 => -(((r >> 8) % 1024) as i64),
        8 => ((r >> 8) % (1 << 20)) as i64,
        _ => -(((r >> 8) % (1 << 20)) as i64),
    }
}

fn gen_batch(r: u64) -> i32 {
    match r % 8 {
        0 => 1,
        1 => 8,
        2 => 32,
        3 => 33,
        4 => 64,
        5 => 128,
        6 => 256,
        _ => i32::MAX,
    }
}

pub fn wrap_pcpu_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let nops = (iabs(nops_i) % 140) as usize;
    let dump_every = 5 + (iabs(seed2) % 6) as usize;

    let mut s = S::new();
    let mut f = Fold {
        h: 0x9e3779b97f4a7c15,
    };

    s.pcpu_reset(seed2 as u64);
    s.rng = seed | 1;
    s.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        s.fail_mask = !0;
    }

    for i in 0..nops {
        let r = xs64(&mut s.rng);
        let cpu = ((r >> 4) % KSHIM_NCPU as u64) as usize;
        let fi = ((r >> 9) % NCTR as u64) as usize;
        let amt = gen_amt(r >> 20);

        s.kshim_cpu = cpu;
        match (r % 24) as usize {
            0 | 1 => {
                if s.ctrs[fi].counters.is_none() {
                    let mut tmp = [s.ctrs[fi]];
                    let v = s.init_many(&mut tmp, amt, 1);
                    s.ctrs[fi] = tmp[0];
                    f.add(v as i64 as u64);
                } else {
                    f.add(NILX);
                }
            }
            2 => {
                if s.many[0].counters.is_none() {
                    let v = {
                        let mut m = s.many;
                        let r2 = s.init_many(&mut m, amt, NMANY);
                        s.many = m;
                        r2
                    };
                    f.add(v as i64 as u64);
                } else {
                    f.add(NILX);
                }
            }
            3 => {
                let mut tmp = [s.ctrs[fi]];
                s.destroy_many(&mut tmp, 1);
                s.ctrs[fi] = tmp[0];
            }
            4 => {
                let mut m = s.many;
                s.destroy_many(&mut m, NMANY);
                s.many = m;
            }
            5 => {
                if s.ctrs[fi].counters.is_some() {
                    S::set(&mut s.ctrs[fi], &mut s.pcpu, amt);
                } else {
                    f.add(NILX);
                }
            }
            6..=8 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, amt, s.batch);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            9 | 10 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, amt, gen_batch(r >> 50));
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            11 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, -amt, s.batch);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            12 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, 1, s.batch);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            13 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, -1, s.batch);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            14 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, amt, PERCPU_COUNTER_LOCAL_BATCH);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            15 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.add_batch(&mut fc, -amt, PERCPU_COUNTER_LOCAL_BATCH);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            16 => {
                if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    s.sync(&mut fc);
                    s.ctrs[fi] = fc;
                } else {
                    f.add(NILX);
                }
            }
            17 => {
                f.add(if s.ctrs[fi].counters.is_some() {
                    s.sum(&s.ctrs[fi]) as u64
                } else {
                    NILX
                });
            }
            18 => {
                f.add(if s.ctrs[fi].counters.is_some() {
                    let v = s.sum(&s.ctrs[fi]);
                    (if v < 0 { 0 } else { v }) as u64
                } else {
                    NILX
                });
            }
            19 => {
                f.add(s.ctrs[fi].count as u64);
            }
            20 => {
                f.add(
                    (if s.ctrs[fi].count >= 0 {
                        s.ctrs[fi].count
                    } else {
                        0
                    }) as u64,
                );
            }
            21 => {
                f.add(if s.ctrs[fi].counters.is_some() {
                    s.compare(&s.ctrs[fi], amt, s.batch) as i64 as u64
                } else {
                    NILX
                });
            }
            22 => {
                f.add(if s.ctrs[fi].counters.is_some() {
                    let mut fc = s.ctrs[fi];
                    let v = s.limited_add(&mut fc, gen_amt(r >> 40), amt, s.batch);
                    s.ctrs[fi] = fc;
                    v as u64
                } else {
                    NILX
                });
            }
            _ => {
                f.add(s.ctrs[fi].counters.is_some() as u64);
            }
        }
        f.add(fi as u64);
        s.dump_fbc(&mut f, &s.ctrs[fi]);
        if i % dump_every == dump_every - 1 {
            s.dump_pcpu(&mut f);
        }
    }
    s.dump_pcpu(&mut f);
    f.add(s.warn_hits);
    f.h
}

/* ================================================================== */
/* printk_ringbuffer unit                                             */
/* ================================================================== */

#[derive(Clone, Copy, Default)]
struct BlkLpos {
    begin: u64,
    next: u64,
}

#[derive(Clone, Copy, Default)]
struct PrbDesc {
    sv: u64, /* state_var: id | state<<62 */
    blk: BlkLpos,
}

#[derive(Clone, Copy)]
struct PrintkInfo {
    seq: u64,
    ts_nsec: u64,
    text_len: u16,
    caller_id: u32,
    facility: u8,
    level: u8,
    flags: u8,
    subsystem: [u8; 16],
    device: [u8; 48],
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

#[derive(Default)]
struct Rb {
    count_bits: u32,
    head_id: u64,
    tail_id: u64,
    last_finalized_seq: u64,
    size_bits: u32,
    head_lpos: u64,
    tail_lpos: u64,
    fail: u64,
}

#[derive(Clone, Copy, Default)]
struct REntry {
    id: u64,
    text_space: u32,
}

#[derive(Clone, Copy, Default)]
struct WRec {
    info: Option<usize>,     /* index into infos[] */
    text_buf: Option<usize>, /* byte offset into text[] (at blk data) */
    text_buf_size: u32,
}

fn desc_id(sv: u64) -> u64 {
    sv & DESC_ID_MASK
}
fn desc_sv(id: u64, state: u64) -> u64 {
    (state << DESC_FLAGS_SHIFT) | id
}
fn desc_state_bits(sv: u64) -> u64 {
    3 & (sv >> DESC_FLAGS_SHIFT)
}
fn data_index(size_bits: u32, lpos: u64) -> usize {
    (lpos & ((1u64 << size_bits) - 1)) as usize
}
fn data_wraps(size_bits: u32, lpos: u64) -> u64 {
    lpos >> size_bits
}
fn blk0_lpos(bits: u32) -> u64 {
    (1u64 << bits).wrapping_neg()
}
fn desc0_id(bits: u32) -> u64 {
    desc_id(((1u64 << bits) + 1).wrapping_neg())
}
fn to_blk_size(size: u32) -> u32 {
    /* + sizeof(id) then align to id size (8) */
    (size + 8).wrapping_add(7) & !7
}

impl S {
    fn count(&self) -> u64 {
        1u64 << self.rb.count_bits
    }
    fn mask(&self) -> u64 {
        self.count() - 1
    }
    fn desc_idx(&self, n: u64) -> usize {
        (n & self.mask()) as usize
    }
    fn data_size(&self) -> u64 {
        1u64 << self.rb.size_bits
    }

    fn blk_read_id(&self, lpos: u64) -> u64 {
        let o = data_index(self.rb.size_bits, lpos);
        u64::from_le_bytes(self.text[o..o + 8].try_into().unwrap())
    }
    fn blk_write_id(&mut self, off: usize, id: u64) {
        self.text[off..off + 8].copy_from_slice(&id.to_le_bytes());
    }

    fn is_blk_wrapped(&self, begin: u64, next: u64) -> bool {
        data_wraps(self.rb.size_bits, begin) != data_wraps(self.rb.size_bits, next.wrapping_sub(1))
    }

    fn need_more_space(&self, cur: u64, target: u64) -> bool {
        target.wrapping_sub(cur).wrapping_sub(1) < self.data_size()
    }

    fn get_next_lpos(&self, lpos: u64, size: u32) -> u64 {
        let next = lpos.wrapping_add(size as u64);
        if !self.is_blk_wrapped(lpos, next) {
            return next;
        }
        (next & !(self.data_size() - 1)).wrapping_add(size as u64)
    }

    fn get_desc_state(&self, id: u64, sv: u64) -> i32 {
        if id != desc_id(sv) {
            return DESC_MISS;
        }
        desc_state_bits(sv) as i32
    }

    /* desc_read(): returns (state, sv_last, blk, seq, cid).  The copy
     * fields are only meaningful for consistent states, mirroring C. */
    fn desc_read(&self, id: u64) -> (i32, u64, BlkLpos, u64, u32) {
        let idx = self.desc_idx(id);
        let mut sv = self.descs[idx].sv;
        let mut st = self.get_desc_state(id, sv);
        if st == DESC_MISS || st == DESC_RESERVED as i32 {
            return (st, sv, BlkLpos::default(), 0, 0);
        }
        let blk = self.descs[idx].blk;
        let seq = self.infos[idx].seq;
        let cid = self.infos[idx].caller_id;
        sv = self.descs[idx].sv;
        st = self.get_desc_state(id, sv);
        (st, sv, blk, seq, cid)
    }

    fn desc_make_reusable(&mut self, id: u64) {
        let idx = self.desc_idx(id);
        if self.descs[idx].sv == desc_sv(id, DESC_FINALIZED) {
            self.descs[idx].sv = desc_sv(id, DESC_REUSABLE);
        }
    }

    fn data_make_reusable(&mut self, mut begin: u64, end: u64, out: &mut u64) -> bool {
        while self.need_more_space(begin, end) {
            let id = self.blk_read_id(begin);
            let (st, _sv, blk, _seq, _cid) = self.desc_read(id);
            match st {
                s if s == DESC_MISS || s == DESC_RESERVED as i32 || s == DESC_COMMITTED as i32 => {
                    return false;
                }
                s if s == DESC_FINALIZED as i32 => {
                    if blk.begin != begin {
                        return false;
                    }
                    self.desc_make_reusable(id);
                }
                _ => {
                    /* desc_reusable */
                    if blk.begin != begin {
                        return false;
                    }
                }
            }
            begin = blk.next;
        }
        *out = begin;
        true
    }

    fn data_push_tail(&mut self, lpos: u64) -> bool {
        if lpos & 1 != 0 {
            return true;
        }
        let mut tail = self.rb.tail_lpos;
        while self.need_more_space(tail, lpos) {
            let mut next = 0u64;
            if !self.data_make_reusable(tail, lpos, &mut next) {
                let t2 = self.rb.tail_lpos;
                if t2 == tail {
                    return false;
                }
                tail = t2;
                continue;
            }
            /* cmpxchg succeeds sequentially */
            self.rb.tail_lpos = next;
            break;
        }
        true
    }

    fn desc_push_tail(&mut self, tail_id: u64) -> bool {
        let (st, sv, blk, _seq, _cid) = self.desc_read(tail_id);
        match st {
            s if s == DESC_MISS => {
                if desc_id(sv) == desc_id(tail_id.wrapping_sub(self.count())) {
                    return false;
                }
                return true;
            }
            s if s == DESC_RESERVED as i32 || s == DESC_COMMITTED as i32 => {
                return false;
            }
            s if s == DESC_FINALIZED as i32 => {
                self.desc_make_reusable(tail_id);
            }
            _ => {}
        }

        if !self.data_push_tail(blk.next) {
            return false;
        }

        let next_id = desc_id(tail_id.wrapping_add(1));
        let (st2, _sv2, _b2, _s2, _c2) = self.desc_read(next_id);
        if st2 == DESC_FINALIZED as i32 || st2 == DESC_REUSABLE as i32 {
            self.rb.tail_id = next_id;
        } else if self.rb.tail_id == tail_id {
            return false;
        }
        true
    }

    fn desc_reserve(&mut self) -> Option<u64> {
        let head_id = self.rb.head_id;
        let id = desc_id(head_id.wrapping_add(1));
        let id_prev_wrap = desc_id(id.wrapping_sub(self.count()));
        if id_prev_wrap == self.rb.tail_id && !self.desc_push_tail(id_prev_wrap) {
            return None;
        }
        /* try_cmpxchg(head_id -> id): sequential, always succeeds */
        self.rb.head_id = id;

        let idx = self.desc_idx(id);
        let prev = self.descs[idx].sv;
        let id_prev_wrap = desc_id(id.wrapping_sub(self.count()));
        if prev != 0 && self.get_desc_state(id_prev_wrap, prev) != DESC_REUSABLE as i32 {
            let mut h = self.warn_hits;
            warn_once(W_DESC_RESERVE_STALE, true, &mut h);
            self.warn_hits = h;
            return None;
        }
        /* try_cmpxchg prev -> SV(id, reserved): expected == current */
        if self.descs[idx].sv != prev {
            let mut h = self.warn_hits;
            warn_once(W_DESC_RESERVE_CMPXCHG, true, &mut h);
            self.warn_hits = h;
            return None;
        }
        self.descs[idx].sv = desc_sv(id, DESC_RESERVED);
        Some(id)
    }

    fn desc_make_final(&mut self, id: u64) {
        let idx = self.desc_idx(id);
        if self.descs[idx].sv == desc_sv(id, DESC_COMMITTED) {
            self.descs[idx].sv = desc_sv(id, DESC_FINALIZED);
            self.desc_update_last_finalized();
        }
    }

    fn desc_last_finalized_seq(&self) -> u64 {
        self.rb.last_finalized_seq /* 64-bit: identity mapping */
    }

    fn desc_update_last_finalized(&mut self) {
        let old_seq = self.desc_last_finalized_seq();
        let mut finalized = old_seq;
        let mut try_seq = finalized + 1;
        while self.prb_read_valid_raw(&mut try_seq, None, None) {
            finalized = try_seq;
            try_seq += 1;
        }
        if finalized == old_seq {
            return;
        }
        self.rb.last_finalized_seq = finalized;
    }

    fn desc_read_finalized_seq(&self, id: u64, seq: u64, desc_out: &mut BlkLpos) -> i32 {
        let (st, _sv, blk, s, _cid) = self.desc_read(id);
        *desc_out = blk;
        if st == DESC_MISS || st == DESC_RESERVED as i32 || st == DESC_COMMITTED as i32 || s != seq
        {
            return EINVAL;
        }
        if st == DESC_REUSABLE as i32 || (blk.begin == FAILED_LPOS && blk.next == FAILED_LPOS) {
            return ENOENT;
        }
        0
    }

    /* get_data(): validate blk_lpos, return (data_off, data_size) */
    fn get_data(&mut self, blk: &BlkLpos) -> Option<(usize, u32)> {
        let mut data_size: u64;
        let off: usize;
        if (blk.begin & 1) != 0 && (blk.next & 1) != 0 {
            if blk.begin == EMPTY_LINE_LPOS && blk.next == EMPTY_LINE_LPOS {
                return Some((0, 0));
            }
            return None;
        }
        if !self.is_blk_wrapped(blk.begin, blk.next) {
            off = data_index(self.rb.size_bits, blk.begin);
            data_size = blk.next.wrapping_sub(blk.begin);
        } else if !self.is_blk_wrapped(blk.begin.wrapping_add(self.data_size()), blk.next) {
            off = 0;
            data_size = data_index(self.rb.size_bits, blk.next) as u64;
        } else {
            let mut h = self.warn_hits;
            warn_once(W_GET_DATA_ILLEGAL, true, &mut h);
            self.warn_hits = h;
            return None;
        }

        let align_bad_b = blk.begin != (blk.begin + 7) & !7;
        let align_bad_n = blk.next != (blk.next + 7) & !7;
        let mut h = self.warn_hits;
        if warn_once(W_GET_DATA_BEGIN_ALIGN, align_bad_b, &mut h)
            || warn_once(W_GET_DATA_NEXT_ALIGN, align_bad_n, &mut h)
        {
            self.warn_hits = h;
            return None;
        }
        self.warn_hits = h;

        if data_size <= 8 {
            let mut h2 = self.warn_hits;
            warn_once(W_GET_DATA_TOO_SMALL, true, &mut h2);
            self.warn_hits = h2;
            return None;
        }
        data_size -= 8;
        if !self.data_check_size(data_size as u32) {
            let mut h2 = self.warn_hits;
            warn_once(W_GET_DATA_CHECK, true, &mut h2);
            self.warn_hits = h2;
            return None;
        }
        Some((off + 8, data_size as u32))
    }

    fn data_check_size(&self, size: u32) -> bool {
        if size == 0 {
            return true;
        }
        to_blk_size(size) as u64 <= self.data_size() / 2
    }

    fn space_used(&self, blk: &BlkLpos) -> u32 {
        if (blk.begin & 1) != 0 && (blk.next & 1) != 0 {
            return 0;
        }
        if !self.is_blk_wrapped(blk.begin, blk.next) {
            return blk.next.wrapping_sub(blk.begin) as u32;
        }
        (data_index(self.rb.size_bits, blk.next) as u64 + self.data_size()
            - data_index(self.rb.size_bits, blk.begin) as u64) as u32
    }

    fn data_alloc(&mut self, mut size: u32, blk_out: &mut BlkLpos, id: u64) -> Option<usize> {
        if size == 0 {
            blk_out.begin = EMPTY_LINE_LPOS;
            blk_out.next = EMPTY_LINE_LPOS;
            return None;
        }
        size = to_blk_size(size);
        let begin_lpos = self.rb.head_lpos;
        let next_lpos = self.get_next_lpos(begin_lpos, size);
        let mut h = self.warn_hits;
        let bad = warn_once(
            W_DATA_ALLOC_OVERSIZE,
            next_lpos.wrapping_sub(begin_lpos) > self.data_size(),
            &mut h,
        );
        self.warn_hits = h;
        if bad || !self.data_push_tail(next_lpos.wrapping_sub(self.data_size())) {
            blk_out.begin = FAILED_LPOS;
            blk_out.next = FAILED_LPOS;
            return None;
        }
        /* cmpxchg head begin->next: sequential success */
        self.rb.head_lpos = next_lpos;
        let off = data_index(self.rb.size_bits, begin_lpos);
        self.blk_write_id(off, id);
        let wrapped = self.is_blk_wrapped(begin_lpos, next_lpos);
        if wrapped {
            /* wrapping data blocks store their data at the beginning */
            self.blk_write_id(0, id);
        }
        blk_out.begin = begin_lpos;
        blk_out.next = next_lpos;
        Some(if wrapped { 8 } else { off + 8 })
    }

    fn data_realloc(&mut self, mut size: u32, blk: &mut BlkLpos, id: u64) -> Option<usize> {
        let head_lpos = self.rb.head_lpos;
        if head_lpos != blk.next {
            return None;
        }
        let wrapped = self.is_blk_wrapped(blk.begin, blk.next);
        size = to_blk_size(size);
        let next_lpos = self.get_next_lpos(blk.begin, size);

        if !self.need_more_space(head_lpos, next_lpos) {
            /* reuse current block */
            return Some(if wrapped {
                8
            } else {
                data_index(self.rb.size_bits, blk.begin) + 8
            });
        }

        let mut h = self.warn_hits;
        let bad = warn_once(
            W_DATA_REALLOC_OVERSIZE,
            next_lpos.wrapping_sub(blk.begin) > self.data_size(),
            &mut h,
        );
        self.warn_hits = h;
        if bad || !self.data_push_tail(next_lpos.wrapping_sub(self.data_size())) {
            return None;
        }
        self.rb.head_lpos = next_lpos;

        let old_off = data_index(self.rb.size_bits, blk.begin);
        if self.is_blk_wrapped(blk.begin, next_lpos) {
            self.blk_write_id(0, id);
            if !wrapped {
                let n = (blk.next.wrapping_sub(blk.begin) - 8) as usize;
                for i in 0..n {
                    self.text[8 + i] = self.text[old_off + 8 + i];
                }
            }
            blk.next = next_lpos;
            return Some(8);
        }
        blk.next = next_lpos;
        Some(old_off + 8)
    }

    fn count_lines(&self, off: usize, len: usize) -> u32 {
        let mut next_size = len;
        let mut line_count = 1u32;
        let mut pos = off;
        let end = off + len;
        while next_size > 0 {
            match self.text[pos..end].iter().position(|&c| c == b'\n') {
                None => break,
                Some(p) => {
                    line_count += 1;
                    pos += p + 1;
                    next_size = end - pos;
                }
            }
        }
        line_count
    }

    /* copy_data(): copy expected @len of data from @blk into the
     * reader's rtext (the only text buffer in the harness).
     * has_buf mirrors `r->text_buf != NULL` in C; buf_size is
     * r->text_buf_size. */
    fn copy_data(
        &mut self,
        blk: &BlkLpos,
        len: u16,
        has_buf: bool,
        buf_size: u32,
        line_count: Option<&mut u32>,
    ) -> bool {
        if !has_buf && line_count.is_none() {
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
        if !has_buf {
            return true;
        }
        let n = (buf_size as u64).min(len as u64) as usize;
        self.rtext[..n].copy_from_slice(&self.text[off..off + n]);
        true
    }

    /* prb_read(): core read.  dst.info_slot==Some(_) copies the found
     * info into s.rinfo (memcpy source: to_info(seq)); dst.buf ==
     * Some((_, cap)) copies text into s.rtext with rtext_buf_size=cap. */
    fn prb_read(&mut self, seq: u64, r: Option<&mut ReadDst>, line_count: Option<&mut u32>) -> i32 {
        let idx = self.desc_idx(seq);
        let id = desc_id(self.descs[idx].sv);
        let mut blk = BlkLpos::default();
        let err = self.desc_read_finalized_seq(id, seq, &mut blk);
        if err != 0 || r.is_none() {
            return err;
        }
        let r = r.unwrap();
        if r.info_slot.is_some() {
            self.rinfo = self.infos[idx];
        }
        let len = self.infos[idx].text_len;
        let ok = match &r.buf {
            Some((_, cap)) => self.copy_data(&blk, len, true, *cap, line_count),
            None => self.copy_data(&blk, len, false, 0, line_count),
        };
        if !ok {
            return ENOENT;
        }
        self.desc_read_finalized_seq(id, seq, &mut blk)
    }

    fn prb_first_seq(&mut self) -> u64 {
        loop {
            let id = self.rb.tail_id;
            let (st, _sv, _blk, seq, _cid) = self.desc_read(id);
            if st == DESC_FINALIZED as i32 || st == DESC_REUSABLE as i32 {
                return seq;
            }
        }
    }

    fn nrs_livelock(&self) -> bool {
        let dr_mask = self.mask();
        let lfs = self.rb.last_finalized_seq;
        let head = self.rb.head_id;
        let d = &self.descs[self.desc_idx(lfs)];
        let st = desc_state_bits(d.sv);
        let einv = st == DESC_RESERVED
            || st == DESC_COMMITTED
            || self.infos[(lfs & dr_mask) as usize].seq != lfs;
        let lf_id;
        if einv {
            if lfs == 0 {
                if head == desc0_id(self.rb.count_bits) {
                    return false;
                }
                lf_id = desc0_id(self.rb.count_bits).wrapping_add(1);
            } else {
                return true;
            }
        } else {
            lf_id = desc_id(d.sv);
        }
        head.wrapping_sub(lf_id) > self.count()
    }

    fn panic_on_this_cpu(&self) -> bool {
        self.panic && !self.nrs_livelock()
    }

    fn prb_next_reserve_seq(&mut self) -> u64 {
        loop {
            let lfs = self.desc_last_finalized_seq();
            let head_id = self.rb.head_id;
            let d_idx = self.desc_idx(lfs);
            let lf_id = desc_id(self.descs[d_idx].sv);
            let mut blk = BlkLpos::default();
            let err = self.desc_read_finalized_seq(lf_id, lfs, &mut blk);
            let lf_id2;
            if err == EINVAL {
                if lfs == 0 {
                    if head_id == desc0_id(self.rb.count_bits) {
                        return 0;
                    }
                    lf_id2 = desc0_id(self.rb.count_bits).wrapping_add(1);
                } else {
                    continue;
                }
            } else {
                lf_id2 = lf_id;
            }
            let diff = head_id.wrapping_sub(lf_id2);
            return lfs.wrapping_add(diff).wrapping_add(1);
        }
    }

    fn prb_read_valid_raw(
        &mut self,
        seq: &mut u64,
        r: Option<&mut ReadDst>,
        line_count: Option<&mut u32>,
    ) -> bool {
        let mut r = r;
        let mut lc = line_count;
        loop {
            let err = self.prb_read(*seq, r.as_deref_mut(), lc.as_deref_mut());
            if err == 0 {
                return true;
            }
            let tail_seq = self.prb_first_seq();
            if *seq < tail_seq {
                *seq = tail_seq;
            } else if err == ENOENT {
                *seq += 1;
            } else {
                if self.panic_on_this_cpu()
                    && (!self.debug_non_panic_cpus || self.legacy_allow_panic_sync)
                    && seq.wrapping_add(1) < self.prb_next_reserve_seq()
                {
                    *seq += 1;
                } else {
                    return false;
                }
            }
        }
    }

    fn _prb_commit(&mut self, id: u64, state: u64) {
        let idx = self.desc_idx(id);
        let prev = desc_sv(id, DESC_RESERVED);
        if self.descs[idx].sv == prev {
            self.descs[idx].sv = desc_sv(id, state);
        } else {
            let mut h = self.warn_hits;
            warn_once(W_COMMIT_CMPXCHG, true, &mut h);
            self.warn_hits = h;
        }
    }

    fn prb_commit(&mut self, id: u64) {
        self._prb_commit(id, DESC_COMMITTED);
        if self.rb.head_id != id {
            self.desc_make_final(id);
        }
    }

    fn prb_final_commit(&mut self, id: u64) {
        self._prb_commit(id, DESC_FINALIZED);
        self.desc_update_last_finalized();
    }

    fn prb_reserve(&mut self, w: usize, r: &mut WRec) -> bool {
        if !self.data_check_size(r.text_buf_size) {
            *r = WRec::default();
            return false;
        }
        let Some(id) = self.desc_reserve() else {
            self.rb.fail += 1;
            *r = WRec::default();
            return false;
        };
        let idx = self.desc_idx(id);
        let seq = self.infos[idx].seq;
        self.infos[idx] = PrintkInfo::default();

        self.es[w].id = id;

        if seq == 0 && idx != 0 {
            self.infos[idx].seq = idx as u64;
        } else {
            self.infos[idx].seq = seq.wrapping_add(self.count());
        }

        if self.infos[idx].seq > 0 {
            self.desc_make_final(desc_id(id.wrapping_sub(1)));
        }

        let mut blk = self.descs[idx].blk;
        let buf = self.data_alloc(r.text_buf_size, &mut blk, id);
        self.descs[idx].blk = blk;

        if r.text_buf_size != 0 && buf.is_none() {
            /* data-less record is committed */
            self.prb_commit(id);
            *r = WRec::default();
            return false;
        }

        r.text_buf = buf;
        r.info = Some(idx);
        self.es[w].text_space = self.space_used(&self.descs[idx].blk);
        true
    }

    fn prb_reserve_in_last(
        &mut self,
        w: usize,
        r: &mut WRec,
        caller_id: u32,
        max_size: u32,
    ) -> bool {
        /* desc_reopen_last */
        let head = self.rb.head_id;
        let (st, _sv, _blk, _seq, cid) = self.desc_read(head);
        if st != DESC_COMMITTED as i32 || cid != caller_id {
            *r = WRec::default();
            return false;
        }
        let idx = self.desc_idx(head);
        self.descs[idx].sv = desc_sv(head, DESC_RESERVED);

        self.es[w].id = head;

        if caller_id != self.infos[idx].caller_id {
            self.prb_commit(head);
            *r = WRec::default();
            return false;
        }

        let mut ok = true;
        if (self.descs[idx].blk.begin & 1) != 0 && (self.descs[idx].blk.next & 1) != 0 {
            /* data-less last record */
            let mut h = self.warn_hits;
            if warn_once(W_RIL_DATALESS_LEN, self.infos[idx].text_len != 0, &mut h) {
                self.infos[idx].text_len = 0;
            }
            self.warn_hits = h;
            if !self.data_check_size(r.text_buf_size) || r.text_buf_size > max_size {
                ok = false;
            } else {
                let mut blk = self.descs[idx].blk;
                r.text_buf = self.data_alloc(r.text_buf_size, &mut blk, head);
                self.descs[idx].blk = blk;
            }
        } else {
            let blk0 = self.descs[idx].blk;
            match self.get_data(&blk0) {
                None => ok = false,
                Some((_off, data_size)) => {
                    let mut h = self.warn_hits;
                    if warn_once(
                        W_RIL_LEN_OVERRUN,
                        self.infos[idx].text_len as u32 > data_size,
                        &mut h,
                    ) {
                        self.infos[idx].text_len = data_size as u16;
                    }
                    self.warn_hits = h;
                    r.text_buf_size += self.infos[idx].text_len as u32;
                    if !self.data_check_size(r.text_buf_size) || r.text_buf_size > max_size {
                        ok = false;
                    } else {
                        let mut blk = self.descs[idx].blk;
                        r.text_buf = self.data_realloc(r.text_buf_size, &mut blk, head);
                        self.descs[idx].blk = blk;
                    }
                }
            }
        }

        if ok && r.text_buf_size != 0 && r.text_buf.is_none() {
            ok = false;
        }
        if !ok {
            self.prb_commit(head);
            *r = WRec::default();
            return false;
        }
        r.info = Some(idx);
        self.es[w].text_space = self.space_used(&self.descs[idx].blk);
        true
    }

    fn prb_init(&mut self, textbits: u32, descbits: u32) {
        let n = 1usize << descbits;
        for (d, inf) in self.descs.iter_mut().zip(self.infos.iter_mut()).take(n) {
            *d = PrbDesc::default();
            *inf = PrintkInfo::default();
        }
        self.rb.count_bits = descbits;
        self.rb.head_id = desc0_id(descbits);
        self.rb.tail_id = desc0_id(descbits);
        self.rb.last_finalized_seq = 0;
        self.rb.size_bits = textbits;
        self.rb.head_lpos = blk0_lpos(textbits);
        self.rb.tail_lpos = blk0_lpos(textbits);
        self.rb.fail = 0;
        self.descs[n - 1].sv = desc_sv(desc0_id(descbits), DESC_REUSABLE);
        self.descs[n - 1].blk.begin = FAILED_LPOS;
        self.descs[n - 1].blk.next = FAILED_LPOS;
        self.infos[0].seq = (n as u64).wrapping_neg();
        self.infos[n - 1].seq = 0;
    }

    fn prb_read_valid(&mut self, seq: u64, with_text: bool, buf_size: u32) -> bool {
        let mut q = seq;
        let mut dst = ReadDst {
            info_slot: Some(0),
            buf: if with_text {
                Some((0usize, buf_size))
            } else {
                None
            },
        };
        self.prb_read_valid_raw(&mut q, Some(&mut dst), None)
    }

    fn prb_read_valid_info(&mut self, seq: u64, lc: &mut u32) -> bool {
        let mut q = seq;
        let mut dst = ReadDst {
            info_slot: Some(0),
            buf: None,
        };
        self.prb_read_valid_raw(&mut q, Some(&mut dst), Some(lc))
    }

    fn prb_first_valid_seq(&mut self) -> u64 {
        let mut seq = 0u64;
        if !self.prb_read_valid_raw(&mut seq, None, None) {
            return 0;
        }
        seq
    }

    fn prb_next_seq(&mut self) -> u64 {
        let mut seq = self.desc_last_finalized_seq();
        if seq != 0 {
            seq += 1;
        }
        while self.prb_read_valid_raw(&mut seq, None, None) {
            seq += 1;
        }
        seq
    }

    /* ---- harness-side helpers ---- */

    fn canon_text(p: Option<usize>) -> u64 {
        match p {
            Some(o) => o as u64,
            None => NILX,
        }
    }
    fn canon_info(p: Option<usize>) -> u64 {
        match p {
            Some(i) => i as u64,
            None => NILX,
        }
    }

    fn fold_info(&self, f: &mut Fold, info: &PrintkInfo) {
        f.add(info.seq);
        f.add(info.ts_nsec);
        f.add(info.text_len as u64);
        f.add(info.facility as u64);
        f.add(info.flags as u64);
        f.add(info.level as u64);
        f.add(info.caller_id as u64);
        f.bytes(&info.subsystem[0..8]);
        f.bytes(&info.device[0..8]);
    }

    /* the C harness memsets all ring storage to 0xaa before prb_init */
    fn aa_fill(&mut self) {
        for d in self.descs.iter_mut() {
            d.sv = 0xaaaaaaaaaaaaaaaa;
            d.blk.begin = 0xaaaaaaaaaaaaaaaa;
            d.blk.next = 0xaaaaaaaaaaaaaaaa;
        }
        for i in self.infos.iter_mut() {
            i.seq = 0xaaaaaaaaaaaaaaaa;
            i.ts_nsec = 0xaaaaaaaaaaaaaaaa;
            i.text_len = 0xaaaa;
            i.caller_id = 0xaaaaaaaa;
            i.facility = 0xaa;
            i.level = 0xaa;
            i.flags = 0xaa;
            i.subsystem = [0xaa; 16];
            i.device = [0xaa; 48];
        }
        self.text = [0xaa; PRB_MAX_TEXT];
    }

    fn dump_rb(&self, f: &mut Fold) {
        let n = 1usize << self.rb.count_bits;
        f.add(0xdb1);
        f.add(self.rb.count_bits as u64);
        f.add(self.rb.size_bits as u64);
        f.add(self.rb.head_id);
        f.add(self.rb.tail_id);
        f.add(self.rb.last_finalized_seq);
        f.add(self.rb.head_lpos);
        f.add(self.rb.tail_lpos);
        f.add(self.rb.fail);
        for d in self.descs.iter().take(n) {
            f.add(d.sv);
            f.add(d.blk.begin);
            f.add(d.blk.next);
        }
        for inf in self.infos.iter().take(n) {
            self.fold_info(f, inf);
        }
        for i in 0..n {
            f.add(self.eopen[i % NW] as u64);
        }
        f.bytes(&self.text[0..(1usize << self.rb.size_bits)]);
        f.add(self.panic as u64);
        f.add(self.warn_hits);
    }

    fn prb_reset(&mut self, seed2: u64) -> (u32, u32) {
        let descbits = 2 + (iabs(seed2 as i64) % 2) as u32; /* 2|3 */
        let textbits = 5 + ((iabs(seed2 as i64) >> 3) % 4) as u32; /* 5..8 */
        self.aa_fill();
        self.prb_init(textbits, descbits);
        self.es = [REntry::default(); NW];
        self.rws = [WRec::default(); NW];
        self.eopen = [false; NW];
        self.rw2 = WRec::default();
        self.last_cid = 0;
        self.warn_hits = 0;
        self.kshim_cpu = 0;
        (descbits, textbits)
    }

    fn writer_fill(&mut self, w: usize, r: u64) {
        let Some(ii) = self.rws[w].info else { return };
        let size = self.rws[w].text_buf_size as usize;
        let len = ((r >> 33) as usize) % (size + 1);
        if let Some(bo) = self.rws[w].text_buf {
            for i in 0..len.min(size) {
                self.text[bo + i] = b'A' + (((r >> 40) + w as u64 * 7 + i as u64 * 13) % 26) as u8;
            }
        }
        let info = &mut self.infos[ii];
        info.text_len = len as u16;
        info.ts_nsec = (r >> 8) & 0xffffff;
        info.caller_id = if r & 0x4000 != 0 {
            0x77
        } else {
            0x1000 + w as u32
        };
        info.facility = ((r >> 12) % 4) as u8;
        info.level = ((r >> 16) % 8) as u8;
        info.flags = ((r >> 19) % 32) as u8;
        if r & 0x8000 != 0 {
            for i in 0..6 {
                info.subsystem[i] = b'a' + w as u8;
                info.device[i] = b'0' + w as u8;
            }
        }
        self.last_cid = info.caller_id;
    }

    fn op_prb_read(&mut self, f: &mut Fold, r: u64) {
        let seq = gen_seq(r);
        self.rinfo = PrintkInfo::default();
        self.rtext = [0; RTBUF];
        let buf_size = if r & 0x2000 != 0 {
            RTBUF as u32
        } else {
            1 + ((r >> 20) % RTBUF as u64) as u32
        };
        let ok = self.prb_read_valid(seq, true, buf_size);
        f.add(ok as u64);
        let ri = self.rinfo;
        self.fold_info(f, &ri);
        f.bytes(&self.rtext);
    }

    fn op_prb_read_info(&mut self, f: &mut Fold, r: u64) {
        let seq = gen_seq(r);
        let mut lc = 0xdeadu32;
        self.rinfo = PrintkInfo::default();
        let ok = self.prb_read_valid_info(seq, &mut lc);
        f.add(ok as u64);
        let ri = self.rinfo;
        self.fold_info(f, &ri);
        f.add(lc as u64);
    }
}

/* reader-destination descriptor: the C reader always uses the static
 * rinfo/rtext pair; info_slot 0 == &rinfo, buf (0,cap) == rtext[0..cap) */
struct ReadDst {
    info_slot: Option<usize>,
    buf: Option<(usize, u32)>,
}

fn gen_size(r: u64, textbits: u32) -> u32 {
    let half = (1u32 << textbits) / 2;
    match r % 8 {
        0 => 0,
        1 => 1,
        2 => 2 + ((r >> 8) % 6) as u32,
        3 => {
            if half > 8 {
                half - 8
            } else {
                1
            }
        }
        4 => ((r >> 8) % (half as u64 + 2)) as u32,
        5 => half,
        6 => 2 * half,
        _ => ((r >> 8) % 4) as u32,
    }
}

fn gen_seq(r: u64) -> u64 {
    match r % 8 {
        0 => 0,
        1 => (r >> 8) % 8,
        2 => (r >> 8) % 32,
        3 => 1 + (r >> 8) % 3,
        4 => (r >> 8) % 64,
        5 => 0x100 + (r >> 8) % 16,
        6 => (r >> 8) % 4,
        _ => (r >> 8) % 1024,
    }
}

pub fn wrap_prb_ops(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let nops = (iabs(nops_i) % 160) as usize;
    let dump_every = 6 + (iabs(seed2) % 6) as usize;

    let mut s = S::new();
    let mut f = Fold {
        h: 0x9e3779b97f4a7c15,
    };

    let (_descbits, textbits) = s.prb_reset(seed2 as u64);
    s.panic = ((iabs(seed2) >> 6) & 1) != 0;
    s.debug_non_panic_cpus = ((iabs(seed2) >> 7) & 1) != 0;
    s.legacy_allow_panic_sync = ((iabs(seed2) >> 8) & 1) != 0;
    s.rng = seed | 1;

    for i in 0..nops {
        let r = xs64(&mut s.rng);
        let w = ((r >> 3) % NW as u64) as usize;

        s.panic = ((r >> 13) & 1) != 0;
        s.debug_non_panic_cpus = ((r >> 14) & 1) != 0;
        s.legacy_allow_panic_sync = ((r >> 15) & 1) != 0;

        match (r % 20) as usize {
            0..=4 => {
                let sz = gen_size(r >> 8, textbits);
                let mut rec = WRec {
                    info: None,
                    text_buf: None,
                    text_buf_size: sz,
                };
                if s.prb_reserve(w, &mut rec) {
                    s.rws[w] = rec;
                    s.eopen[w] = true;
                    f.add(1);
                    f.add(s.es[w].id);
                    f.add(S::canon_text(rec.text_buf));
                    f.add(S::canon_info(rec.info));
                    f.add(s.es[w].text_space as u64);
                    s.writer_fill(w, r);
                    match (r >> 30) % 3 {
                        0 => {
                            let id = s.es[w].id;
                            s.prb_commit(id);
                            s.eopen[w] = false;
                            f.add(0xcc01);
                        }
                        1 => {
                            let id = s.es[w].id;
                            s.prb_final_commit(id);
                            s.eopen[w] = false;
                            f.add(0xfc01);
                        }
                        _ => {}
                    }
                } else {
                    s.rws[w] = rec;
                    f.add(0);
                    f.add(S::canon_text(rec.text_buf));
                    f.add(S::canon_info(rec.info));
                }
            }
            5 | 6 => {
                if s.eopen[w] {
                    let id = s.es[w].id;
                    s.prb_commit(id);
                    s.eopen[w] = false;
                    f.add(0xcc02);
                } else {
                    f.add(NILX);
                }
            }
            7 | 8 => {
                if s.eopen[w] {
                    let id = s.es[w].id;
                    s.prb_final_commit(id);
                    s.eopen[w] = false;
                    f.add(0xfc02);
                } else {
                    f.add(NILX);
                }
            }
            9 | 10 => {
                let cid = if r & 0x100 != 0 {
                    s.last_cid
                } else {
                    ((r >> 32) % 8) as u32
                };
                let maxsz = 1 + ((r >> 40) % (2u64 << textbits)) as u32;
                let sz = ((r >> 48) % (maxsz as u64 + 2)) as u32;
                let mut rec = WRec {
                    info: None,
                    text_buf: None,
                    text_buf_size: sz,
                };
                let ok = s.prb_reserve_in_last(w, &mut rec, cid, maxsz);
                f.add(ok as u64);
                s.rw2 = rec;
                if rec.info.is_some() {
                    /* fill appended text */
                    if let Some(bo) = rec.text_buf {
                        let base = rec
                            .info
                            .map(|ii| s.infos[ii].text_len as usize)
                            .unwrap_or(0);
                        for i2 in base..rec.text_buf_size as usize {
                            s.text[bo + i2] =
                                b'A' + (((r >> 40) + w as u64 * 7 + i2 as u64 * 13) % 26) as u8;
                        }
                    }
                    if let Some(ii) = rec.info {
                        s.infos[ii].text_len = rec.text_buf_size as u16;
                    }
                    s.eopen[w] = true;
                    f.add(S::canon_text(rec.text_buf));
                    f.add(S::canon_info(rec.info));
                    f.add(s.es[w].text_space as u64);
                    match (r >> 30) % 3 {
                        0 => {
                            let id = s.es[w].id;
                            s.prb_commit(id);
                            s.eopen[w] = false;
                            f.add(0xcc03);
                        }
                        1 => {
                            let id = s.es[w].id;
                            s.prb_final_commit(id);
                            s.eopen[w] = false;
                            f.add(0xfc03);
                        }
                        _ => {}
                    }
                } else {
                    s.eopen[w] = false;
                    f.add(NILX);
                }
            }
            11..=13 => s.op_prb_read(&mut f, r),
            14 | 15 => s.op_prb_read_info(&mut f, r),
            16 => {
                let a = s.prb_first_seq();
                f.add(a);
                let b = s.prb_first_valid_seq();
                f.add(b);
            }
            17 => {
                let a = s.prb_next_seq();
                f.add(a);
                let b = if s.nrs_livelock() {
                    0xdeaddead
                } else {
                    s.prb_next_reserve_seq()
                };
                f.add(b);
            }
            18 => {
                f.add(if s.eopen[w] {
                    s.es[w].text_space as u64
                } else {
                    NILX
                });
            }
            _ => {
                s.prb_init(textbits, s.rb.count_bits);
                s.eopen = [false; NW];
                f.add(0x1e17);
            }
        }
        if i % dump_every == dump_every - 1 {
            s.dump_rb(&mut f);
        }
    }
    s.dump_rb(&mut f);
    f.add(s.warn_hits);
    f.h
}

pub fn wrap_prb_wrap(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let textbits = 5 + (iabs(seed2) % 2) as u32;
    let nops = (iabs(nops_i) % 140) as usize;
    let dump_every = 5 + (iabs(seed2) % 5) as usize;

    let mut s = S::new();
    let mut f = Fold {
        h: 0x9e3779b97f4a7c15,
    };

    s.aa_fill();
    s.prb_init(textbits, 2);
    s.es = [REntry::default(); NW];
    s.rws = [WRec::default(); NW];
    s.eopen = [false; NW];
    s.rw2 = WRec::default();
    s.last_cid = 0;
    s.warn_hits = 0;
    s.panic = ((iabs(seed2) >> 6) & 1) != 0;
    s.debug_non_panic_cpus = ((iabs(seed2) >> 7) & 1) != 0;
    s.legacy_allow_panic_sync = ((iabs(seed2) >> 8) & 1) != 0;
    s.rng = seed | 1;

    for i in 0..nops {
        let r = xs64(&mut s.rng);
        let w = ((r >> 3) % NW as u64) as usize;

        s.panic = ((r >> 13) & 1) != 0;
        s.debug_non_panic_cpus = ((r >> 14) & 1) != 0;
        s.legacy_allow_panic_sync = ((r >> 15) & 1) != 0;

        match (r % 16) as usize {
            0..=6 => {
                let sz = gen_size(r >> 8, textbits);
                let mut rec = WRec {
                    info: None,
                    text_buf: None,
                    text_buf_size: sz,
                };
                if s.prb_reserve(w, &mut rec) {
                    s.rws[w] = rec;
                    s.eopen[w] = true;
                    f.add(1);
                    f.add(s.es[w].id);
                    f.add(s.es[w].text_space as u64);
                    s.writer_fill(w, r);
                    if !(r >> 30).is_multiple_of(5) {
                        let id = s.es[w].id;
                        s.prb_final_commit(id);
                        s.eopen[w] = false;
                        f.add(0xfc04);
                    }
                } else {
                    s.rws[w] = rec;
                    f.add(0);
                }
            }
            7 => {
                if s.eopen[w] {
                    let id = s.es[w].id;
                    if r & 0x100 != 0 {
                        s.prb_commit(id);
                    } else {
                        s.prb_final_commit(id);
                    }
                    s.eopen[w] = false;
                    f.add(0xc07);
                } else {
                    f.add(NILX);
                }
            }
            8 => {
                let cid = if r & 0x100 != 0 {
                    s.last_cid
                } else {
                    ((r >> 32) % 8) as u32
                };
                let maxsz = 1 + ((r >> 40) % (2u64 << textbits)) as u32;
                let sz = ((r >> 48) % (maxsz as u64 + 2)) as u32;
                let mut rec = WRec {
                    info: None,
                    text_buf: None,
                    text_buf_size: sz,
                };
                let ok = s.prb_reserve_in_last(w, &mut rec, cid, maxsz);
                f.add(ok as u64);
                s.rw2 = rec;
                if rec.info.is_some() {
                    if let Some(ii) = rec.info {
                        s.infos[ii].text_len = rec.text_buf_size as u16;
                    }
                    s.eopen[w] = true;
                    f.add(s.es[w].text_space as u64);
                    if (r >> 30) & 1 != 0 {
                        let id = s.es[w].id;
                        s.prb_final_commit(id);
                        s.eopen[w] = false;
                    }
                } else {
                    s.eopen[w] = false;
                    f.add(NILX);
                }
            }
            9..=11 => s.op_prb_read(&mut f, r),
            12 => s.op_prb_read_info(&mut f, r),
            13 => {
                let a = s.prb_first_seq();
                f.add(a);
                let b = s.prb_first_valid_seq();
                f.add(b);
            }
            14 => {
                let a = s.prb_next_seq();
                f.add(a);
            }
            _ => {
                let b = if s.nrs_livelock() {
                    0xdeaddead
                } else {
                    s.prb_next_reserve_seq()
                };
                f.add(b);
            }
        }
        if i % dump_every == dump_every - 1 {
            s.dump_rb(&mut f);
        }
    }
    s.dump_rb(&mut f);
    f.add(s.warn_hits);
    f.h
}
