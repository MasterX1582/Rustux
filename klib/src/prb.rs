//! `klib::prb` — Linux `kernel/printk/printk_ringbuffer.c` ported to
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

pub fn desc_id(sv: u64) -> u64 {
    sv & DESC_ID_MASK
}
pub fn desc_sv(id: u64, state: u64) -> u64 {
    (state << DESC_FLAGS_SHIFT) | id
}
pub fn desc_state_bits(sv: u64) -> u64 {
    3 & (sv >> DESC_FLAGS_SHIFT)
}
pub fn data_index(size_bits: u32, lpos: u64) -> usize {
    (lpos & 1u64.wrapping_shl(size_bits).wrapping_sub(1)) as usize
}
pub fn data_wraps(size_bits: u32, lpos: u64) -> u64 {
    lpos.wrapping_shr(size_bits)
}
pub fn blk0_lpos(bits: u32) -> u64 {
    1u64.wrapping_shl(bits).wrapping_neg()
}
pub fn desc0_id(bits: u32) -> u64 {
    desc_id(1u64.wrapping_shl(bits).wrapping_add(1).wrapping_neg())
}
pub fn to_blk_size(size: u32) -> u32 {
    /* + sizeof(id) then align to id size (8) */
    size.wrapping_add(8).wrapping_add(7) & !7
}

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

    /// `prb_init` — (re)initialize over the caller-owned rings.
    /// Requires `descs.len() >= 1<<descbits`,
    /// `infos.len() >= 1<<descbits`, `data.len() >= 1<<textbits`
    /// (C takes exactly-sized arrays).
    pub fn init(&mut self, textbits: u32, descbits: u32) {
        assert!(
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
        );
        for (d, inf) in self.desc_ring.descs.iter_mut().zip(self.desc_ring.infos.iter_mut()).take(n) {
            *d = PrbDesc::default();
            *inf = PrintkInfo::default();
        }
        self.desc_ring.count_bits = descbits;
        self.desc_ring.head_id = desc0_id(descbits);
        self.desc_ring.tail_id = desc0_id(descbits);
        self.desc_ring.last_finalized_seq = 0;
        self.text_ring.size_bits = textbits;
        self.text_ring.head_lpos = blk0_lpos(textbits);
        self.text_ring.tail_lpos = blk0_lpos(textbits);
        self.fail = 0;
        self.desc_ring.descs[n - 1].sv = desc_sv(desc0_id(descbits), DESC_REUSABLE);
        self.desc_ring.descs[n - 1].blk.begin = FAILED_LPOS;
        self.desc_ring.descs[n - 1].blk.next = FAILED_LPOS;
        self.desc_ring.infos[0].seq = (n as u64).wrapping_neg();
        self.desc_ring.infos[n - 1].seq = 0;
    }
    fn count(&self) -> u64 {
        1u64.wrapping_shl(self.desc_ring.count_bits)
    }
    fn mask(&self) -> u64 {
        self.count() - 1
    }
    fn desc_idx(&self, n: u64) -> usize {
        (n & self.mask()) as usize % self.desc_ring.descs.len().min(self.desc_ring.infos.len()).max(1)
    }
    fn data_size(&self) -> u64 {
        1u64.wrapping_shl(self.text_ring.size_bits)
    }
    fn blk_read_id(&self, lpos: u64) -> u64 {
        let o = data_index(self.text_ring.size_bits, lpos);
        let Some(bytes) = o
            .checked_add(8)
            .and_then(|e| self.text_ring.data.get(o..e))
        else {
            return 0;
        };
        u64::from_le_bytes(bytes.try_into().unwrap())
    }
    fn blk_write_id(&mut self, off: usize, id: u64) {
        if let Some(dst) = off
            .checked_add(8)
            .and_then(|e| self.text_ring.data.get_mut(off..e))
        {
            dst.copy_from_slice(&id.to_le_bytes());
        } else {
            self.warn += 1;
        }
    }
    fn is_blk_wrapped(&self, begin: u64, next: u64) -> bool {
        data_wraps(self.text_ring.size_bits, begin) != data_wraps(self.text_ring.size_bits, next.wrapping_sub(1))
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
        let mut sv = self.desc_ring.descs[idx].sv;
        let mut st = self.get_desc_state(id, sv);
        if st == DESC_MISS || st == DESC_RESERVED as i32 {
            return (st, sv, BlkLpos::default(), 0, 0);
        }
        let blk = self.desc_ring.descs[idx].blk;
        let seq = self.desc_ring.infos[idx].seq;
        let cid = self.desc_ring.infos[idx].caller_id;
        sv = self.desc_ring.descs[idx].sv;
        st = self.get_desc_state(id, sv);
        (st, sv, blk, seq, cid)
    }
    fn desc_make_reusable(&mut self, id: u64) {
        let idx = self.desc_idx(id);
        if self.desc_ring.descs[idx].sv == desc_sv(id, DESC_FINALIZED) {
            self.desc_ring.descs[idx].sv = desc_sv(id, DESC_REUSABLE);
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
        let mut tail = self.text_ring.tail_lpos;
        while self.need_more_space(tail, lpos) {
            let mut next = 0u64;
            if !self.data_make_reusable(tail, lpos, &mut next) {
                let t2 = self.text_ring.tail_lpos;
                if t2 == tail {
                    return false;
                }
                tail = t2;
                continue;
            }
            /* cmpxchg succeeds sequentially */
            self.text_ring.tail_lpos = next;
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
            self.desc_ring.tail_id = next_id;
        } else if self.desc_ring.tail_id == tail_id {
            return false;
        }
        true
    }
    fn desc_reserve(&mut self) -> Option<u64> {
        let head_id = self.desc_ring.head_id;
        let id = desc_id(head_id.wrapping_add(1));
        let id_prev_wrap = desc_id(id.wrapping_sub(self.count()));
        if id_prev_wrap == self.desc_ring.tail_id && !self.desc_push_tail(id_prev_wrap) {
            return None;
        }
        /* try_cmpxchg(head_id -> id): sequential, always succeeds */
        self.desc_ring.head_id = id;

        let idx = self.desc_idx(id);
        let prev = self.desc_ring.descs[idx].sv;
        let id_prev_wrap = desc_id(id.wrapping_sub(self.count()));
        if prev != 0 && self.get_desc_state(id_prev_wrap, prev) != DESC_REUSABLE as i32 {
            let mut h = self.warn;
            warn_once(W_DESC_RESERVE_STALE, true, &mut h);
            self.warn = h;
            return None;
        }
        /* try_cmpxchg prev -> SV(id, reserved): expected == current */
        if self.desc_ring.descs[idx].sv != prev {
            let mut h = self.warn;
            warn_once(W_DESC_RESERVE_CMPXCHG, true, &mut h);
            self.warn = h;
            return None;
        }
        self.desc_ring.descs[idx].sv = desc_sv(id, DESC_RESERVED);
        Some(id)
    }
    fn desc_make_final(&mut self, id: u64) {
        let idx = self.desc_idx(id);
        if self.desc_ring.descs[idx].sv == desc_sv(id, DESC_COMMITTED) {
            self.desc_ring.descs[idx].sv = desc_sv(id, DESC_FINALIZED);
            self.desc_update_last_finalized();
        }
    }
    fn desc_last_finalized_seq(&self) -> u64 {
        self.desc_ring.last_finalized_seq /* 64-bit: identity mapping */
    }
    fn desc_update_last_finalized(&mut self) {
        let old_seq = self.desc_last_finalized_seq();
        let mut finalized = old_seq;
        let mut try_seq = finalized.wrapping_add(1);
        while self.prb_read_valid_raw(&mut try_seq, None, None) {
            finalized = try_seq;
            try_seq = try_seq.wrapping_add(1);
        }
        if finalized == old_seq {
            return;
        }
        self.desc_ring.last_finalized_seq = finalized;
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
            off = data_index(self.text_ring.size_bits, blk.begin);
            data_size = blk.next.wrapping_sub(blk.begin);
        } else if !self.is_blk_wrapped(blk.begin.wrapping_add(self.data_size()), blk.next) {
            off = 0;
            data_size = data_index(self.text_ring.size_bits, blk.next) as u64;
        } else {
            let mut h = self.warn;
            warn_once(W_GET_DATA_ILLEGAL, true, &mut h);
            self.warn = h;
            return None;
        }

        let align_bad_b = blk.begin & 7 != 0;
        let align_bad_n = blk.next & 7 != 0;
        let mut h = self.warn;
        if warn_once(W_GET_DATA_BEGIN_ALIGN, align_bad_b, &mut h)
            || warn_once(W_GET_DATA_NEXT_ALIGN, align_bad_n, &mut h)
        {
            self.warn = h;
            return None;
        }
        self.warn = h;

        if data_size <= 8 {
            let mut h2 = self.warn;
            warn_once(W_GET_DATA_TOO_SMALL, true, &mut h2);
            self.warn = h2;
            return None;
        }
        data_size -= 8;
        if !self.data_check_size(data_size as u32) {
            let mut h2 = self.warn;
            warn_once(W_GET_DATA_CHECK, true, &mut h2);
            self.warn = h2;
            return None;
        }
        Some((off.wrapping_add(8), data_size as u32))
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
        (data_index(self.text_ring.size_bits, blk.next) as u64 + self.data_size()
            - data_index(self.text_ring.size_bits, blk.begin) as u64) as u32
    }
    fn data_alloc(&mut self, mut size: u32, blk_out: &mut BlkLpos, id: u64) -> Option<usize> {
        if size == 0 {
            blk_out.begin = EMPTY_LINE_LPOS;
            blk_out.next = EMPTY_LINE_LPOS;
            return None;
        }
        size = to_blk_size(size);
        let begin_lpos = self.text_ring.head_lpos;
        let next_lpos = self.get_next_lpos(begin_lpos, size);
        let mut h = self.warn;
        let bad = warn_once(
            W_DATA_ALLOC_OVERSIZE,
            next_lpos.wrapping_sub(begin_lpos) > self.data_size(),
            &mut h,
        );
        self.warn = h;
        if bad || !self.data_push_tail(next_lpos.wrapping_sub(self.data_size())) {
            blk_out.begin = FAILED_LPOS;
            blk_out.next = FAILED_LPOS;
            return None;
        }
        /* cmpxchg head begin->next: sequential success */
        self.text_ring.head_lpos = next_lpos;
        let off = data_index(self.text_ring.size_bits, begin_lpos);
        self.blk_write_id(off, id);
        let wrapped = self.is_blk_wrapped(begin_lpos, next_lpos);
        if wrapped {
            /* wrapping data blocks store their data at the beginning */
            self.blk_write_id(0, id);
        }
        blk_out.begin = begin_lpos;
        blk_out.next = next_lpos;
        Some(if wrapped { 8 } else { off.wrapping_add(8) })
    }
    fn data_realloc(&mut self, mut size: u32, blk: &mut BlkLpos, id: u64) -> Option<usize> {
        let head_lpos = self.text_ring.head_lpos;
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
                data_index(self.text_ring.size_bits, blk.begin).wrapping_add(8)
            });
        }

        let mut h = self.warn;
        let bad = warn_once(
            W_DATA_REALLOC_OVERSIZE,
            next_lpos.wrapping_sub(blk.begin) > self.data_size(),
            &mut h,
        );
        self.warn = h;
        if bad || !self.data_push_tail(next_lpos.wrapping_sub(self.data_size())) {
            return None;
        }
        self.text_ring.head_lpos = next_lpos;

        let old_off = data_index(self.text_ring.size_bits, blk.begin);
        if self.is_blk_wrapped(blk.begin, next_lpos) {
            self.blk_write_id(0, id);
            if !wrapped {
                let n = (blk.next.wrapping_sub(blk.begin).wrapping_sub(8)) as usize;
                let n = n.min(self.text_ring.data.len().saturating_sub(8));
                for i in 0..n {
                    let src = old_off.wrapping_add(8).wrapping_add(i);
                    if src < self.text_ring.data.len() {
                        self.text_ring.data[8 + i] = self.text_ring.data[src];
                    }
                }
            }
            blk.next = next_lpos;
            return Some(8);
        }
        blk.next = next_lpos;
        Some(old_off.wrapping_add(8))
    }
    fn count_lines(&self, off: usize, len: usize) -> u32 {
        let mut next_size = len;
        let mut line_count = 1u32;
        let mut pos = off;
        let end = off.saturating_add(len).min(self.text_ring.data.len());
        if off >= end {
            return 1;
        }
        while next_size > 0 {
            match self.text_ring.data[pos..end].iter().position(|&c| c == b'\n') {
                None => break,
                Some(p) => {
                    line_count += 1;
                    pos += p + 1;
                    next_size = end.saturating_sub(pos);
                }
            }
        }
        line_count
    }

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

    /// `prb_first_seq` — sequence number of the tail (oldest)
    /// descriptor, finalized or not.
    pub fn first_seq(&mut self) -> u64 {
        // sequential-model bound: the upstream cmpxchg-retry loop can
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
    }
    /// `kshim_nrs_livelock` — true exactly when
    /// `prb_next_reserve_seq()` (and therefore the panic skip) could
    /// never terminate: a `try_again` loop or the un-masked head/tail
    /// id diff climbing toward ~2^62 (see `internal.h`).
    pub fn nrs_livelock(&self) -> bool {
        let dr_mask = self.mask();
        let lfs = self.desc_ring.last_finalized_seq;
        let head = self.desc_ring.head_id;
        let d = &self.desc_ring.descs[self.desc_idx(lfs)];
        let st = desc_state_bits(d.sv);
        let einv = st == DESC_RESERVED
            || st == DESC_COMMITTED
            || self.desc_ring.infos[(lfs & dr_mask) as usize].seq != lfs;
        let lf_id;
        if einv {
            if lfs == 0 {
                if head == desc0_id(self.desc_ring.count_bits) {
                    return false;
                }
                lf_id = desc0_id(self.desc_ring.count_bits).wrapping_add(1);
            } else {
                return true;
            }
        } else {
            lf_id = desc_id(d.sv);
        }
        head.wrapping_sub(lf_id) > self.count()
    }

    /// `panic_on_this_cpu` — the `kshim_panic` gate: panic only
    /// while the panic skip provably terminates.
    pub fn panic_on_this_cpu(&self) -> bool {
        self.panic && !self.nrs_livelock()
    }
    /* `prb_next_reserve_seq` raw — may loop forever when
     * nrs_livelock(); call it only via next_reserve_seq(). */
    fn prb_next_reserve_seq(&mut self) -> u64 {
        loop {
            let lfs = self.desc_last_finalized_seq();
            let head_id = self.desc_ring.head_id;
            let d_idx = self.desc_idx(lfs);
            let lf_id = desc_id(self.desc_ring.descs[d_idx].sv);
            let mut blk = BlkLpos::default();
            let err = self.desc_read_finalized_seq(lf_id, lfs, &mut blk);
            let lf_id2;
            if err == EINVAL {
                if lfs == 0 {
                    if head_id == desc0_id(self.desc_ring.count_bits) {
                        return 0;
                    }
                    lf_id2 = desc0_id(self.desc_ring.count_bits).wrapping_add(1);
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
    /// `prb_next_reserve_seq` with the verified harness's livelock
    /// curation: `None` is the `0xdeaddead` sentinel — the state where
    /// upstream's loop cannot terminate (`nrs_livelock()`).
    pub fn next_reserve_seq(&mut self) -> Option<u64> {
        if self.nrs_livelock() {
            return None;
        }
        Some(self.prb_next_reserve_seq())
    }

    fn prb_read_valid_raw(
        &mut self,
        seq: &mut u64,
        r: Option<&mut ReadDst<'_>>,
        line_count: Option<&mut u32>,
    ) -> bool {
        let mut r = r;
        let mut lc = line_count;
        loop {
            let err = self.prb_read(*seq, r.as_deref_mut(), lc.as_deref_mut());
            if err == 0 {
                return true;
            }
            let tail_seq = self.first_seq();
            if *seq < tail_seq {
                *seq = tail_seq;
            } else if err == ENOENT {
                *seq = seq.wrapping_add(1);
            } else {
                if self.panic_on_this_cpu()
                    && (!self.debug_non_panic_cpus || self.legacy_allow_panic_sync)
                    && seq.wrapping_add(1) < self.prb_next_reserve_seq()
                {
                    *seq = seq.wrapping_add(1);
                } else {
                    return false;
                }
            }
        }
    }
    fn _prb_commit(&mut self, id: u64, state: u64) {
        let idx = self.desc_idx(id);
        let prev = desc_sv(id, DESC_RESERVED);
        if self.desc_ring.descs[idx].sv == prev {
            self.desc_ring.descs[idx].sv = desc_sv(id, state);
        } else {
            let mut h = self.warn;
            warn_once(W_COMMIT_CMPXCHG, true, &mut h);
            self.warn = h;
        }
    }

    /* `prb_commit` by id — used internally by reserve paths. */
    fn prb_commit_id(&mut self, id: u64) {
        self._prb_commit(id, DESC_COMMITTED);
        if self.desc_ring.head_id != id {
            self.desc_make_final(id);
        }
    }

    /* `prb_final_commit` by id. */
    fn prb_final_commit_id(&mut self, id: u64) {
        self._prb_commit(id, DESC_FINALIZED);
        self.desc_update_last_finalized();
    }
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

    /// `prb_reserve` — on success `e` is the commit handle and `r`
    /// carries `info`/`text_buf` for the writer to fill.
    pub fn reserve(&mut self, e: &mut PrbReservedEntry, r: &mut PrintkRecord) -> bool {
        if !self.data_check_size(r.text_buf_size) {
            *r = PrintkRecord::default();
            return false;
        }
        let Some(id) = self.desc_reserve() else {
            self.fail += 1;
            *r = PrintkRecord::default();
            return false;
        };
        let idx = self.desc_idx(id);
        let seq = self.desc_ring.infos[idx].seq;
        self.desc_ring.infos[idx] = PrintkInfo::default();

        e.id = id;

        if seq == 0 && idx != 0 {
            self.desc_ring.infos[idx].seq = idx as u64;
        } else {
            self.desc_ring.infos[idx].seq = seq.wrapping_add(self.count());
        }

        if self.desc_ring.infos[idx].seq > 0 {
            self.desc_make_final(desc_id(id.wrapping_sub(1)));
        }

        let mut blk = self.desc_ring.descs[idx].blk;
        let buf = self.data_alloc(r.text_buf_size, &mut blk, id);
        self.desc_ring.descs[idx].blk = blk;

        if r.text_buf_size != 0 && buf.is_none() {
            /* data-less record is committed */
            self.prb_commit_id(id);
            *r = PrintkRecord::default();
            return false;
        }

        r.text_buf = buf;
        r.info = Some(idx);
        e.text_space = self.space_used(&self.desc_ring.descs[idx].blk);
        true
    }
    /// `prb_reserve_in_last` — reopen the head record (must be
    /// `commit`ted, not finalized, by `caller_id`) and extend it up to
    /// `max_size` total bytes.
    pub fn reserve_in_last(
        &mut self,
        e: &mut PrbReservedEntry,
        r: &mut PrintkRecord,
        caller_id: u32,
        max_size: u32,
    ) -> bool {
        /* desc_reopen_last */
        let head = self.desc_ring.head_id;
        let (st, _sv, _blk, _seq, cid) = self.desc_read(head);
        if st != DESC_COMMITTED as i32 || cid != caller_id {
            *r = PrintkRecord::default();
            return false;
        }
        let idx = self.desc_idx(head);
        self.desc_ring.descs[idx].sv = desc_sv(head, DESC_RESERVED);

        e.id = head;

        if caller_id != self.desc_ring.infos[idx].caller_id {
            self.prb_commit_id(head);
            *r = PrintkRecord::default();
            return false;
        }

        let mut ok = true;
        if (self.desc_ring.descs[idx].blk.begin & 1) != 0 && (self.desc_ring.descs[idx].blk.next & 1) != 0 {
            /* data-less last record */
            let mut h = self.warn;
            if warn_once(W_RIL_DATALESS_LEN, self.desc_ring.infos[idx].text_len != 0, &mut h) {
                self.desc_ring.infos[idx].text_len = 0;
            }
            self.warn = h;
            if !self.data_check_size(r.text_buf_size) || r.text_buf_size > max_size {
                ok = false;
            } else {
                let mut blk = self.desc_ring.descs[idx].blk;
                r.text_buf = self.data_alloc(r.text_buf_size, &mut blk, head);
                self.desc_ring.descs[idx].blk = blk;
            }
        } else {
            let blk0 = self.desc_ring.descs[idx].blk;
            match self.get_data(&blk0) {
                None => ok = false,
                Some((_off, data_size)) => {
                    let mut h = self.warn;
                    if warn_once(
                        W_RIL_LEN_OVERRUN,
                        self.desc_ring.infos[idx].text_len as u32 > data_size,
                        &mut h,
                    ) {
                        self.desc_ring.infos[idx].text_len = data_size as u16;
                    }
                    self.warn = h;
                    r.text_buf_size = r
                        .text_buf_size
                        .wrapping_add(self.desc_ring.infos[idx].text_len as u32);
                    if !self.data_check_size(r.text_buf_size) || r.text_buf_size > max_size {
                        ok = false;
                    } else {
                        let mut blk = self.desc_ring.descs[idx].blk;
                        r.text_buf = self.data_realloc(r.text_buf_size, &mut blk, head);
                        self.desc_ring.descs[idx].blk = blk;
                    }
                }
            }
        }

        if ok && r.text_buf_size != 0 && r.text_buf.is_none() {
            ok = false;
        }
        if !ok {
            self.prb_commit_id(head);
            *r = PrintkRecord::default();
            return false;
        }
        r.info = Some(idx);
        e.text_space = self.space_used(&self.desc_ring.descs[idx].blk);
        true
    }

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

    /// `prb_first_valid_seq` — oldest readable (finalized) seq.
    pub fn first_valid_seq(&mut self) -> u64 {
        let mut seq = 0u64;
        if !self.prb_read_valid_raw(&mut seq, None, None) {
            return 0;
        }
        seq
    }
    /// `prb_next_seq` — the next seq a writer will be assigned.
    pub fn next_seq(&mut self) -> u64 {
        let mut seq = self.desc_last_finalized_seq();
        if seq != 0 {
            seq = seq.wrapping_add(1);
        }
        while self.prb_read_valid_raw(&mut seq, None, None) {
            seq = seq.wrapping_add(1);
        }
        seq
    }

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
