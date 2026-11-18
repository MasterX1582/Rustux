//! Resizable chained hash table — port of `lib/rhashtable.c` +
//! `include/linux/rhashtable.h` (+ `lib/bucket_locks.c`).
//!
//! Verified port of the record/diff corpus (1111/1111 cases). The model
//! reproduces the C canonical address space verbatim over a caller-owned
//! `&mut [u64]` arena: objects are 48-byte `tobj` slots at [`obj_addr`],
//! `bucket_table` storage is bump-allocated above `ARENA`, and NULLS
//! markers (`p & 1`) are encoded *values* — never memory addresses.
//! Locks, RCU, workqueues and uaccess are sequential models like the C
//! harness; `fail_nth` injects alloc failures for `-ENOMEM` testing.

use crate::hash::{jhash, jhash2};

/// Canonical address of the caller's object space. Objects passed to
/// `rhashtable_insert_fast` & co are u64 "addresses" in
/// `[POOLV, POOLV + obj_cells)`; `Self::obj(i)` gives the conventional
/// 48-byte-slot layout used by the tests.
pub const POOLV: u64 = 0x1000;
/// Canonical base of the internal `bucket_table` arena.
pub const ARENAV: u64 = 0x1_000_000;
const RHNULLV: u64 = 0x70_0000_0000;
const ITERV: u64 = 0x68_0000_0000; /* canonical base for iter walker list nodes */

/// Number of `rhashtable_iter` walker slots (matches the C harness).
pub const NI: usize = 2;
/// Dense cells reserved for the per-iter walker list nodes
/// (`rhashtable_walker{list(16B),tbl(8B)}` at `ITERV + k*32`).
const ITER_CELLS: usize = NI * 32;
pub const NILX: u64 = 0xffff;
const DRAIN_MAX: i32 = 8;
/// Conventional object slot size used by `Self::obj`/`Self::tidx`.
pub const TOBJ_SZ: u64 = 48;

/* struct bucket_table offsets (header is 56 bytes) */
const T_SIZE: u64 = 0;
const T_NEST: u64 = 4;
const T_HASH_RND: u64 = 8;
const T_WNEXT: u64 = 16; /* walkers.next */
const T_RCU_FUNC: u64 = 32;
const T_RCU_CALLED: u64 = 40;
const T_FUTURE: u64 = 48;
const T_BUCKETS: u64 = 56;

const FREE_RCU_TAG: u64 = 1;

/* gfp bits (mirror linux/gfp.h shim) */
const GFP_BLOCKING: u64 = 0x0001;
const GFP_ZERO: u64 = 0x0100;
const GFP_NOWARN: u64 = 0x0200;
const GFP_NOFAIL: u64 = 0x0400;
const GFP_ATOMIC: u64 = 0;
const GFP_KERNEL: u64 = GFP_BLOCKING;

const PAGE_SHIFT: u64 = 12;
const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;
const NESTED_SHIFT: u64 = PAGE_SHIFT - 3; /* ilog2(sizeof(void*)) == 3 */

const HASH_DEFAULT_SIZE: u64 = 64;
const HASH_MIN_SIZE: u64 = 4;
const RHT_ELASTICITY: i64 = 16;

/* errno */
const EAGAIN: i64 = 11;
const ENOMEM: i64 = 12;
const EEXIST: i64 = 17;
const EINVAL: i64 = 22;
const EBUSY: i64 = 16;
const ENOENT: i64 = 2;
const E2BIG: i64 = 7;
const EOPNOTSUPP: i64 = 95;

fn err_ptr(e: i64) -> u64 {
    e as u64
}
fn ptr_err(p: u64) -> i64 {
    p as i64
}
fn is_err(p: u64) -> bool {
    p != 0 && (p as i64) >= -(MAX_ERRNO as i64) && (p as i64) < 0
}
const MAX_ERRNO: u64 = 4095;
fn is_err_or_null(p: u64) -> bool {
    p == 0 || is_err(p)
}
fn is_a_nulls(p: u64) -> bool {
    p & 1 != 0
}

/* ================= params / hash dispatch ================= */

#[derive(Clone, Copy, PartialEq)]
pub enum HashFn {
    Jhash,
    Jhash2,
    Custom, /* jhash(data,len,seed) ^ 0x9e3779b9 */
}

#[derive(Clone, Copy, PartialEq)]
pub enum ObjHash {
    Good, /* jhash(&o->key, 8, seed) */
    Bad,  /* key & 0xf */
}

#[derive(Clone, Copy, PartialEq)]
pub enum ObjCmp {
    Eq,   /* key != *arg */
    Lo32, /* (u32)key != (u32)*arg */
}

/// `struct rhashtable_params` — caller configuration for
/// [`Rhashtable::rhashtable_init`]. `u16`/`u32` widths mirror the C
/// bitfields (`key_len:16`, `nelem_hint:16`, `max_size`, `min_size:16`).
#[derive(Clone, Copy, Default)]
pub struct Params {
    pub nelem_hint: u16,
    pub key_len: u16,
    pub key_offset: u16,
    pub head_offset: u16,
    pub max_size: u32,
    pub min_size: u16,
    pub insecure_elasticity: bool,
    pub automatic_shrinking: bool,
    pub hashfn: Option<HashFn>, /* None until init assigns default */
    pub obj_hashfn: Option<ObjHash>,
    pub obj_cmpfn: Option<ObjCmp>,
}

pub struct Rht {
    tbl: u64,
    key_len: u32, /* in u32 words when hashfn == Jhash2 */
    max_elems: u32,
    p: Params,
    rhlist: bool,
    run_work_queued: bool,
    run_irq_queued: bool,
    nelems: i64,
}

impl Rht {
    fn new() -> Self {
        Rht {
            tbl: 0,
            key_len: 0,
            max_elems: 0,
            p: Params::default(),
            rhlist: false,
            run_work_queued: false,
            run_irq_queued: false,
            nelems: 0,
        }
    }
}

#[derive(Clone)]
pub struct Iter {
    p: u64,
    list: u64,
    slot: u32,
    skip: u32,
    end_of_table: bool,
}

impl Iter {
    fn new() -> Self {
        Iter {
            p: 0,
            list: 0,
            slot: 0,
            skip: 0,
            end_of_table: false,
        }
    }
}

/// `Rhashtable` — `struct rhashtable` plus the canonical-address
/// memory it operates on. `mem` is dense caller-owned storage split
/// into three regions:
///
/// | cells                        | canonical range            |
/// |------------------------------|----------------------------|
/// | `[0, obj_cells)`             | `[POOLV, ARENAV)` objects  |
/// | `[obj_cells, obj+arena)`     | `[ARENAV, ..)` tables      |
/// | tail [`ITER_CELLS`] cells    | `[ITERV, ITERV + NI*32)`   |
///
/// Canonical addresses are translated by [`Rhashtable::idx`]; NULLS
/// markers (`p & 1`) are encoded *values*, never memory addresses.
/// Locks, RCU, workqueues and uaccess are sequential models like the
/// C harness; `fail_nth` injects alloc failures for `-ENOMEM` testing.
pub struct Rhashtable<'a> {
    mem: &'a mut [u64],
    obj_cells: usize,
    arena_cells: usize,
    arena_off: u64,
    alloc_ord: u64,
    /// test hook: allocation #fail_nth fails (0 = off), like injected
    /// -ENOMEM; arena exhaustion also fails naturally.
    pub fail_nth: u64,
    /// seed for hash_rnd (kernel get_random_bytes).
    pub rnd: u64,
    ht: Rht,
    /// iterator cursor state, one per caller's rhashtable_iter slot.
    iters: [Iter; NI],
}

impl<'a> Rhashtable<'a> {
    /// `Rhashtable::new(mem, obj_cells)` — `mem` is dense storage for
    /// the whole canonical space: the first `obj_cells` cells hold the
    /// caller's objects (canonical `[POOLV, POOLV + obj_cells)`), the
    /// last [`ITER_CELLS`] cells are reserved for walker list nodes,
    /// and everything between is the `bucket_table` arena. Zero `mem`
    /// first (HashMap-miss = 0 semantics upstream).
    ///
    /// Panics if `mem` is too small to hold `obj_cells + ITER_CELLS`.
    pub fn new(mem: &'a mut [u64], obj_cells: usize) -> Self {
        assert!(mem.len() >= obj_cells + ITER_CELLS);
        let arena_cells = mem.len() - obj_cells - ITER_CELLS;
        Rhashtable {
            mem,
            obj_cells,
            arena_cells,
            arena_off: 0,
            alloc_ord: 0,
            fail_nth: 0,
            rnd: 0x9e3779b97f4a7c15,
            ht: Rht::new(),
            iters: [Iter::new(), Iter::new()],
        }
    }

    /// Dense `mem` index for canonical address `a`, or `None` outside
    /// the mapped regions (HashMap-miss semantics: reads see 0, writes
    /// are dropped).
    pub fn idx(&self, a: u64) -> Option<usize> {
        if a >= POOLV && a < POOLV + self.obj_cells as u64 {
            Some((a - POOLV) as usize)
        } else if a >= ARENAV && a < ARENAV + self.arena_cells as u64 {
            Some(self.obj_cells + (a - ARENAV) as usize)
        } else if a >= ITERV && a < ITERV + ITER_CELLS as u64 {
            Some(self.obj_cells + self.arena_cells + (a - ITERV) as usize)
        } else {
            None
        }
    }

    /* ---- memory ---- */
    /// Sparse-read like the HashMap model: out-of-range addresses
    /// (RHNULLV, odd NULLS markers, never-written cells) read as 0.
    fn r(&self, a: u64) -> u64 {
        self.idx(a).map(|i| self.mem[i]).unwrap_or(0)
    }
    /// Writes outside the mapped space are dropped — every real write
    /// targets pool/iter/arena cells (verified by the corpus).
    fn w(&mut self, a: u64, v: u64) {
        if let Some(i) = self.idx(a) {
            self.mem[i] = v;
        }
    }

    /// Read a `u64` cell at canonical address `a` (object fields, key
    /// cells, …). Out-of-range reads return 0.
    pub fn mem_r(&self, a: u64) -> u64 {
        self.r(a)
    }
    /// Write a `u64` cell at canonical address `a`. Writes outside the
    /// mapped regions are dropped.
    pub fn mem_w(&mut self, a: u64, v: u64) {
        self.w(a, v);
    }

    /* ---- introspection (model state; not part of the C API) ---- */
    /// `ht->nelems` — objects currently stored.
    pub fn nelems(&self) -> i64 {
        self.ht.nelems
    }
    /// Number of buckets in the current table.
    pub fn table_size(&self) -> u64 {
        self.tbl_size(self.ht.tbl)
    }
    /// True while a rehash is attached but not yet drained
    /// (`tbl->future_tbl != NULL`).
    pub fn rehash_in_progress(&self) -> bool {
        self.tbl_future(self.ht.tbl) != 0
    }
    /// True while deferred work is pending (`run_work` /
    /// `run_irq_work` queued); callers drive it with [`drain_work`].
    ///
    /// [`drain_work`]: Rhashtable::drain_work
    pub fn work_pending(&self) -> bool {
        self.ht.run_work_queued || self.ht.run_irq_queued
    }

    pub fn random_u32(&mut self) -> u32 {
        self.rnd ^= self.rnd >> 12;
        self.rnd ^= self.rnd << 25;
        self.rnd ^= self.rnd >> 27;
        self.rnd = self.rnd.wrapping_mul(2685821657736338717);
        (self.rnd >> 32) as u32
    }

    /* ---- allocator ---- */
    fn kalloc(&mut self, size: u64, gfp: u64) -> u64 {
        self.alloc_ord += 1;
        if self.alloc_ord == self.fail_nth && (gfp & GFP_NOFAIL) == 0 {
            return 0;
        }
        let off = (self.arena_off + 7) & !7;
        let Some(end) = off.checked_add(size) else {
            return 0;
        };
        if end > self.arena_cells as u64 {
            return 0; /* arena exhaustion -> -ENOMEM, like kmalloc */
        }
        self.arena_off = end;
        if gfp & GFP_ZERO != 0 {
            for a in (off..off + size).step_by(8) {
                self.w(ARENAV + a, 0);
            }
        }
        ARENAV + off
    }
    fn kfree(&mut self, _p: u64) {}

    /* A valid chain never holds more nodes than the table holds
     * objects, and a tbl holds at most one future_tbl — exceeding
     * either bound means corrupted links. C would spin; panic
     * deterministically instead of hanging the kernel. */
    fn chain_limit(&self) -> u64 {
        self.ht.nelems.max(0) as u64 + 1
    }
    fn cycle_check(&self, guard: &mut u64) {
        Self::cycle_check_at(guard, self.chain_limit());
    }
    fn cycle_check_at(guard: &mut u64, cap: u64) {
        *guard += 1;
        assert!(*guard <= cap, "rhashtable cyclic chain");
    }
    fn future_check(guard: &mut u64) {
        *guard += 1;
        /* Doubling growth attaches at most ~log2(max_elems) links even
         * if the caller never drains work; 128 is far above that yet
         * still catches corrupted cyclic links. */
        assert!(*guard <= 128, "rhashtable cyclic future_tbl chain");
    }

    /* iter->walker list node canon: rhashtable_walker{list(16B),tbl(8B)}
     * at ITERV + k*32, so walker.tbl is at node+16 (stride keeps the
     * two walkers' list_heads out of each other's tbl cell). */
    fn wnode(k: usize) -> u64 {
        ITERV + k as u64 * 32
    }
    fn wtbl(&self, k: usize) -> u64 {
        self.r(Self::wnode(k) + 16)
    }
    fn set_wtbl(&mut self, k: usize, v: u64) {
        self.w(Self::wnode(k) + 16, v);
    }

    /* ---- list_head on mem cells ---- */
    fn list_init(&mut self, head: u64) {
        self.w(head, head);
        self.w(head + 8, head);
    }
    fn list_add(&mut self, new: u64, head: u64) {
        let next = self.r(head);
        let prev = head;
        self.w(next + 8, new);
        self.w(new, next);
        self.w(new + 8, prev);
        self.w(prev, new);
    }
    fn list_del(&mut self, entry: u64) {
        let prev = self.r(entry + 8);
        let next = self.r(entry);
        self.w(next + 8, prev);
        self.w(prev, next);
        self.w(entry, entry);
        self.w(entry + 8, entry);
    }
    /* ---- tobj pool ---- */
    /// Canonical address of the `i`-th conventional 48-byte object
    /// slot. Callers with other layouts may use any address in
    /// `[POOLV, POOLV + obj_cells)` — only the `head_offset`/`key_offset`
    /// fields inside an object are touched by the table code.
    pub fn obj(i: usize) -> u64 {
        POOLV + (i as u64) * TOBJ_SZ
    }
    /// Inverse of [`Rhashtable::obj`]: the slot index for `o`, or
    /// [`NILX`] if `o` is not a slot start inside the object region.
    pub fn tidx(&self, o: u64) -> u64 {
        if o == 0 || o < POOLV || !(o - POOLV).is_multiple_of(TOBJ_SZ) {
            return NILX;
        }
        let i = (o - POOLV) / TOBJ_SZ;
        if i * TOBJ_SZ + TOBJ_SZ > self.obj_cells as u64 {
            return NILX;
        }
        i
    }

    /* ================= bucket_table fields ================= */
    fn ts(&self, t: u64, off: u64) -> u64 {
        self.r(t + off)
    }
    fn tw(&mut self, t: u64, off: u64, v: u64) {
        self.w(t + off, v);
    }
    fn tbl_size(&self, t: u64) -> u64 {
        self.ts(t, T_SIZE)
    }
    fn tbl_nest(&self, t: u64) -> u64 {
        self.ts(t, T_NEST)
    }
    fn tbl_future(&self, t: u64) -> u64 {
        self.ts(t, T_FUTURE)
    }

    /* rht_bucket cell address for a flat table */
    fn bkt_addr(tbl: u64, i: u64) -> u64 {
        tbl + T_BUCKETS + i * 8
    }

    /* __rht_bucket_nested: address of bucket cell in nested pages, or 0 */
    fn __rht_bucket_nested(&self, tbl: u64, hash: u64) -> u64 {
        let shift = NESTED_SHIFT;
        let nest = self.tbl_nest(tbl);
        let mut index = hash & ((1 << nest) - 1);
        let mut size = self.tbl_size(tbl) >> nest;
        let mut subhash = hash;
        let mut ntbl = self.r(Self::bkt_addr(tbl, 0)); /* nested_table_top */
        ntbl = self.r(ntbl + index * 8); /* ntbl[index].table */
        subhash >>= nest;

        while ntbl != 0 && size > (1 << shift) {
            index = subhash & ((1 << shift) - 1);
            ntbl = self.r(ntbl + index * 8);
            size >>= shift;
            subhash >>= shift;
        }
        if ntbl == 0 {
            return 0;
        }
        ntbl + subhash * 8 /* &ntbl[subhash].bucket */
    }

    fn rht_bucket(&self, tbl: u64, hash: u64) -> u64 {
        if self.tbl_nest(tbl) != 0 {
            let b = self.__rht_bucket_nested(tbl, hash);
            if b == 0 {
                RHNULLV
            } else {
                b
            }
        } else {
            Self::bkt_addr(tbl, hash)
        }
    }
    fn rht_bucket_var(&self, tbl: u64, hash: u64) -> u64 {
        if self.tbl_nest(tbl) != 0 {
            self.__rht_bucket_nested(tbl, hash)
        } else {
            Self::bkt_addr(tbl, hash)
        }
    }

    /* lazy nested page alloc for inserts (GFP_ATOMIC|__GFP_ZERO) */
    fn nested_table_alloc(&mut self, prev: u64, leaf: bool) -> u64 {
        let cur = self.r(prev);
        if cur != 0 {
            return cur;
        }
        let ntbl = self.kalloc(PAGE_SIZE, GFP_ATOMIC | GFP_ZERO);
        if ntbl != 0 && leaf {
            for i in 0..(PAGE_SIZE / 8) {
                self.w(ntbl + i * 8, 0); /* INIT_RHT_NULLS_HEAD == NULL */
            }
        }
        /* sequential cmpxchg(prev, NULL, ntbl): prev was NULL */
        self.w(prev, ntbl);
        ntbl
    }

    fn rht_bucket_insert(&mut self, tbl: u64, hash: u64) -> u64 {
        if self.tbl_nest(tbl) == 0 {
            return Self::bkt_addr(tbl, hash);
        }
        /* rht_bucket_nested_insert */
        let shift = NESTED_SHIFT;
        let nest = self.tbl_nest(tbl);
        let mut index = hash & ((1 << nest) - 1);
        let mut size = self.tbl_size(tbl) >> nest;
        let mut hash_ = hash;
        let top = self.r(Self::bkt_addr(tbl, 0));
        hash_ >>= nest;
        let mut ntbl = self.nested_table_alloc(top + index * 8, size <= (1 << shift));

        while ntbl != 0 && size > (1 << shift) {
            index = hash_ & ((1 << shift) - 1);
            size >>= shift;
            hash_ >>= shift;
            ntbl = self.nested_table_alloc(ntbl + index * 8, size <= (1 << shift));
        }
        if ntbl == 0 {
            return 0;
        }
        ntbl + hash_ * 8
    }

    /* ---- bucket cell / lock helpers ---- */
    fn rht_lock(&mut self, bkt: u64) {
        let v = self.r(bkt);
        self.w(bkt, v | 1);
    }
    fn rht_unlock(&mut self, bkt: u64) {
        let v = self.r(bkt);
        self.w(bkt, v & !1);
    }
    fn rht_assign_locked(&mut self, bkt: u64, obj: u64) {
        let obj = if is_a_nulls(obj) { 0 } else { obj };
        self.w(bkt, obj | 1);
    }
    fn rht_assign_unlock(&mut self, bkt: u64, obj: u64) {
        let obj = if is_a_nulls(obj) { 0 } else { obj };
        self.w(bkt, obj);
    }

    /* __rht_ptr: read bucket head, mask lock bit, NULL -> marker */
    fn rht_ptr(&self, bkt: u64) -> u64 {
        let p = self.r(bkt) & !1;
        if p == 0 {
            bkt | 1 /* RHT_NULLS_MARKER(bkt) in canon space */
        } else {
            p
        }
    }

    fn rht_obj(&self, ht: &Rht, he: u64) -> u64 {
        he.wrapping_sub(ht.p.head_offset as u64)
    }

    /* ================= hashing ================= */

    fn key_bytes(&self, a: u64) -> u64 {
        /* keys are stored as u64; "bytes at &key" == LE bytes of value */
        self.r(a)
    }

    /* ht->p.hashfn(key_ptr_canon, ht->key_len, rnd) */
    fn apply_hashfn(&self, ht: &Rht, key_addr: u64, rnd: u32) -> u32 {
        let v = self.key_bytes(key_addr);
        match ht.p.hashfn.unwrap_or(HashFn::Jhash) {
            HashFn::Custom => {
                jhash(&v.to_le_bytes()[..ht.key_len.min(8) as usize], rnd) ^ 0x9e3779b9
            }
            HashFn::Jhash => jhash(&v.to_le_bytes()[..(ht.key_len as usize).min(8)], rnd),
            HashFn::Jhash2 => {
                /* key_len in u32 words (<= 2 for an 8-byte key) */
                let nw = ht.key_len.min(2) as usize;
                let mut words = [0u32; 2];
                for (i, w) in words.iter_mut().enumerate().take(nw) {
                    let b = v.to_le_bytes();
                    *w = u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
                }
                jhash2(&words[..nw], rnd)
            }
        }
    }

    fn rht_key_hashfn(&self, ht: &Rht, tbl: u64, key_addr: u64) -> u64 {
        let h = self.apply_hashfn(ht, key_addr, self.ts(tbl, T_HASH_RND) as u32);
        let size = self.tbl_size(tbl);
        if size == 0 {
            return 0; /* no table yet: C would NULL-deref here */
        }
        (h as u64) & (size - 1)
    }

    fn rht_head_hashfn(&self, ht: &Rht, tbl: u64, he: u64) -> u64 {
        let obj = self.rht_obj(ht, he);
        let size = self.tbl_size(tbl);
        let h = if let Some(oh) = ht.p.obj_hashfn {
            match oh {
                ObjHash::Good => {
                    let kv = self.r(obj + ht.p.key_offset as u64);
                    jhash(&kv.to_le_bytes(), self.ts(tbl, T_HASH_RND) as u32)
                }
                ObjHash::Bad => (self.r(obj + ht.p.key_offset as u64) & 0xf) as u32,
            }
        } else {
            return self.rht_key_hashfn(ht, tbl, obj + ht.p.key_offset as u64);
        };
        if size == 0 {
            return 0;
        }
        (h as u64) & (size - 1)
    }

    /* compare arg->key (canon addr of u64) vs obj key */
    fn compare(&self, ht: &Rht, key_addr: u64, obj: u64) -> bool {
        /* returns true if keys DIFFER (like memcmp/obj_cmpfn != 0) */
        let ov = self.r(obj + ht.p.key_offset as u64);
        let kv = self.r(key_addr);
        if let Some(oc) = ht.p.obj_cmpfn {
            return match oc {
                ObjCmp::Eq => ov != kv,
                ObjCmp::Lo32 => (ov as u32) != (kv as u32),
            };
        }
        let len = ht.p.key_len.min(8) as u64;
        let mask: u64 = if len >= 8 {
            u64::MAX
        } else {
            (1u64 << (len * 8)) - 1
        };
        (ov ^ kv) & mask != 0
    }

    /* grow/shrink predicates */
    fn grow_above_75(&self, ht: &Rht, tbl: u64) -> bool {
        ht.nelems > (self.tbl_size(tbl) / 4 * 3) as i64
            && (ht.p.max_size == 0 || self.tbl_size(tbl) < ht.p.max_size as u64)
    }
    fn shrink_below_30(&self, ht: &Rht, tbl: u64) -> bool {
        ht.nelems < (self.tbl_size(tbl) * 3 / 10) as i64
            && self.tbl_size(tbl) > ht.p.min_size as u64
    }
    fn grow_above_100(&self, ht: &Rht, tbl: u64) -> bool {
        ht.nelems > self.tbl_size(tbl) as i64
            && (ht.p.max_size == 0 || self.tbl_size(tbl) < ht.p.max_size as u64)
    }
    fn grow_above_max(&self, ht: &Rht) -> bool {
        ht.nelems >= ht.max_elems as i64
    }

    /* ================= allocation machinery ================= */

    fn nested_bucket_table_alloc(&mut self, nbuckets: u64, gfp: u64) -> u64 {
        let shift = NESTED_SHIFT;
        if nbuckets < (1 << (shift + 1)) {
            return 0;
        }
        let size = 56 + 8; /* sizeof(*tbl) + sizeof(tbl->buckets[0]) */
        let tbl = self.kalloc(size, gfp | GFP_ZERO);
        if tbl == 0 {
            return 0;
        }
        if self.nested_table_alloc(Self::bkt_addr(tbl, 0), false) == 0 {
            self.kfree(tbl);
            return 0;
        }
        let nest = (ilog2(nbuckets.min(u32::MAX as u64) as u32) - 1) % shift + 1;
        self.tw(tbl, T_NEST, nest);
        tbl
    }

    fn bucket_table_alloc(&mut self, nbuckets: u64, gfp: u64) -> u64 {
        let mut nb = nbuckets;
        let Some(bytes) = nb.checked_mul(8).and_then(|x| x.checked_add(56)) else {
            return 0;
        };
        let mut tbl = self.kalloc(bytes, gfp | GFP_ZERO);

        if tbl == 0 && (gfp & GFP_BLOCKING) == 0 {
            tbl = self.nested_bucket_table_alloc(nb, gfp);
            nb = 0;
        }
        if tbl == 0 {
            return 0;
        }

        self.tw(tbl, T_SIZE, nbuckets);
        /* rcu_head_init */
        self.tw(tbl, T_RCU_FUNC, 0);
        self.tw(tbl, T_RCU_CALLED, 0);
        self.list_init(tbl + T_WNEXT);
        let rnd = self.random_u32();
        self.tw(tbl, T_HASH_RND, rnd as u64);
        for i in 0..nb {
            self.w(Self::bkt_addr(tbl, i), 0); /* INIT_RHT_NULLS_HEAD */
        }
        tbl
    }

    /* elem is the ADDRESS of a union nested_table slot; reads elem->table */
    fn nested_table_free(&mut self, elem: u64, size: u64) {
        let shift = NESTED_SHIFT;
        let len = 1u64 << shift;
        let ntbl = self.r(elem);
        if ntbl == 0 {
            return;
        }
        let mut size = size;
        if size > len {
            size >>= shift;
            for i in 0..len {
                self.nested_table_free(ntbl + i * 8, size);
            }
        }
        self.kfree(ntbl);
    }

    fn nested_bucket_table_free(&mut self, tbl: u64) {
        let nest = self.tbl_nest(tbl);
        let size = self.tbl_size(tbl) >> nest;
        let len = 1u64 << nest;
        let ntbl = self.r(Self::bkt_addr(tbl, 0)); /* top page */
        for i in 0..len {
            self.nested_table_free(ntbl + i * 8, size);
        }
        self.kfree(ntbl);
    }

    fn bucket_table_free(&mut self, tbl: u64) {
        if self.tbl_nest(tbl) != 0 {
            self.nested_bucket_table_free(tbl);
        }
        self.kfree(tbl);
    }

    fn last_table(&self, mut tbl: u64) -> u64 {
        let mut guard = 0u64;
        loop {
            Self::future_check(&mut guard);
            let f = self.tbl_future(tbl);
            if f == 0 {
                return tbl;
            }
            tbl = f;
        }
    }

    /* ================= rehash ================= */

    fn rehash_one(&mut self, bkt: u64, _old_hash: u64) -> i64 {
        let old_tbl = self.ht.tbl;
        let new_tbl = self.last_table(old_tbl);
        let mut err = -EAGAIN;

        if self.tbl_nest(new_tbl) != 0 {
            return err;
        }
        err = -ENOENT;

        let mut entry = self.rht_ptr(bkt);
        let mut next = 0u64;
        let mut pprev = 0u64; /* canon addr of prev->next cell, or 0 */
        let mut cguard = 0u64;
        while entry != 0 && !is_a_nulls(entry) {
            self.cycle_check(&mut cguard);
            err = 0;
            next = self.r(entry); /* entry->next */
            if is_a_nulls(next) {
                break;
            }
            pprev = entry; /* &entry->next == entry addr */
            entry = next;
        }
        if err != 0 {
            return err;
        }
        if entry == 0 {
            return -ENOENT; /* corrupt chain (dropped write) — C would NULL-deref */
        }

        let new_hash = self.rht_head_hashfn(&self.ht, new_tbl, entry);
        let nbkt = Self::bkt_addr(new_tbl, new_hash);
        self.rht_lock(nbkt);
        let head = self.rht_ptr(nbkt);
        self.w(entry, head); /* RCU_INIT_POINTER(entry->next, head) */
        self.rht_assign_unlock(nbkt, entry);
        if pprev != 0 {
            self.w(pprev, next);
        } else {
            self.rht_assign_locked(bkt, next);
        }
        err
    }

    fn rehash_chain(&mut self, old_hash: u64) -> i64 {
        let old_tbl = self.ht.tbl;
        let bkt = self.rht_bucket_var(old_tbl, old_hash);
        if bkt == 0 {
            return 0;
        }
        self.rht_lock(bkt);
        let mut err;
        loop {
            err = self.rehash_one(bkt, old_hash);
            if err != 0 {
                break;
            }
        }
        if err == -ENOENT {
            err = 0;
        }
        self.rht_unlock(bkt);
        err
    }

    fn rehash_attach(&mut self, old_tbl: u64, new_tbl: u64) -> i64 {
        if self.tbl_future(old_tbl) != 0 {
            return -EEXIST;
        }
        self.tw(old_tbl, T_FUTURE, new_tbl);
        0
    }

    pub fn rehash_table(&mut self) -> i64 {
        let old_tbl = self.ht.tbl;
        let new_tbl = self.tbl_future(old_tbl);
        if new_tbl == 0 {
            return 0;
        }
        let size = self.tbl_size(old_tbl);
        for old_hash in 0..size {
            let e = self.rehash_chain(old_hash);
            if e != 0 {
                return e;
            }
            /* cond_resched: no-op */
        }
        self.ht.tbl = new_tbl;

        /* walkers of old_tbl get tbl=NULL; call_rcu runs free immediately */
        {
            let head = old_tbl + T_WNEXT;
            let mut p = self.r(head);
            let mut guard = 0u64;
            while p != head && guard < 64 {
                /* walker->tbl lives at walker list node + 16 */
                self.w(p + 16, 0);
                let np = self.r(p);
                p = np;
                guard += 1;
            }
        }
        self.tw(old_tbl, T_RCU_FUNC, FREE_RCU_TAG);
        self.tw(old_tbl, T_RCU_CALLED, 1);
        self.bucket_table_free(old_tbl);

        if self.tbl_future(new_tbl) != 0 {
            -EAGAIN
        } else {
            0
        }
    }

    pub fn rehash_alloc(&mut self, old_tbl: u64, size: u64) -> i64 {
        if old_tbl == 0 || size == 0 {
            /* a size-0 future table wedges the insert slow path
             * (grow_above_100 always fires -> -EAGAIN loop). */
            return -EINVAL;
        }
        let new_tbl = self.bucket_table_alloc(size, GFP_KERNEL);
        if new_tbl == 0 {
            return -ENOMEM;
        }
        let err = self.rehash_attach(old_tbl, new_tbl);
        if err != 0 {
            self.bucket_table_free(new_tbl);
        }
        err
    }

    pub fn rhashtable_shrink(&mut self) -> i64 {
        let old_tbl = self.ht.tbl;
        let nelems = self.ht.nelems.max(0) as u64;
        let mut size = 0u64;
        if nelems != 0 {
            size = roundup_pow_of_two(nelems * 3 / 2);
        }
        if size < self.ht.p.min_size as u64 {
            size = self.ht.p.min_size as u64;
        }
        if self.tbl_size(old_tbl) <= size {
            return 0;
        }
        if self.tbl_future(old_tbl) != 0 {
            return -EEXIST;
        }
        self.rehash_alloc(old_tbl, size)
    }

    /* rht_deferred_worker (run_work.func) */
    pub fn deferred_worker(&mut self) {
        if self.ht.tbl == 0 {
            return;
        }
        let mut err;
        {
            let ht = &self.ht;
            let tbl = self.last_table(ht.tbl);
            if self.grow_above_75(ht, tbl) {
                let s = self.tbl_size(tbl);
                err = self.rehash_alloc(tbl, s.saturating_mul(2));
            } else if ht.p.automatic_shrinking && self.shrink_below_30(ht, tbl) {
                err = self.rhashtable_shrink();
            } else if self.tbl_nest(tbl) != 0 {
                let s = self.tbl_size(tbl);
                err = self.rehash_alloc(tbl, s);
            } else {
                err = 0;
            }
        }
        if err == 0 || err == -EEXIST {
            let nerr = self.rehash_table();
            err = if err != 0 { err } else { nerr };
        }
        if err != 0 {
            self.ht.run_work_queued = true;
        }
    }

    /* rht_deferred_irq_work (run_irq_work.func) */
    pub fn deferred_irq_work(&mut self) {
        self.ht.run_work_queued = true;
    }

    pub fn drain_work(&mut self) {
        let mut n = 0;
        while n < DRAIN_MAX {
            if self.ht.run_irq_queued {
                self.ht.run_irq_queued = false;
                self.deferred_irq_work();
                n += 1;
                continue;
            }
            if self.ht.run_work_queued {
                self.ht.run_work_queued = false;
                self.deferred_worker();
                n += 1;
                continue;
            }
            break;
        }
    }

    /* ================= insert path (rhashtable.c) ================= */

    fn lookup_one(&mut self, bkt: u64, _tbl: u64, _hash: u64, key_addr: u64, obj: u64) -> u64 {
        let mut elasticity = RHT_ELASTICITY;
        let mut pprev = 0u64;
        let mut head = self.rht_ptr(bkt);
        let mut guard = 0u64;
        let chain_cap = self.chain_limit();

        while head != 0 && !is_a_nulls(head) {
            /* A corrupted (cyclic) chain would spin forever like the C
             * model — panic instead of hanging the kernel. A chain can
             * never exceed nelems nodes, so the bound is exact. */
            guard += 1;
            if guard > chain_cap {
                panic!("lookup_one cyclic chain at bkt={:#x}", bkt);
            }
            elasticity -= 1;
            let key_differs =
                key_addr == 0 || self.compare(&self.ht, key_addr, self.rht_obj(&self.ht, head));
            if key_differs {
                pprev = head; /* &head->next */
                head = self.r(head);
                continue;
            }
            let ht = &self.ht;
            if !ht.rhlist {
                return self.rht_obj(ht, head);
            }
            /* rhlist: splice obj in front of the matched list head */
            let list = obj; /* container_of(obj, rhlist_head, rhead) == obj */
            let plist = head;
            self.w(list + 8, plist); /* list->next = plist */
            let nxt = self.r(head);
            self.w(list, nxt); /* list->rhead.next = head->next */
            if pprev != 0 {
                self.w(pprev, obj);
            } else {
                self.rht_assign_locked(bkt, obj);
            }
            return 0;
        }

        if elasticity <= 0 && !self.ht.p.insecure_elasticity {
            return err_ptr(-EAGAIN);
        }
        err_ptr(-ENOENT)
    }

    fn insert_one(&mut self, bkt: u64, tbl: u64, _hash: u64, obj: u64, data: u64) -> u64 {
        if !is_err_or_null(data) {
            return err_ptr(-EEXIST);
        }
        if ptr_err(data) != -EAGAIN && ptr_err(data) != -ENOENT {
            return data;
        }
        let new_tbl = self.tbl_future(tbl);
        if new_tbl != 0 {
            return new_tbl;
        }
        if ptr_err(data) != -ENOENT {
            return data;
        }
        if self.grow_above_max(&self.ht) {
            return err_ptr(-E2BIG);
        }
        if self.grow_above_100(&self.ht, tbl) && !self.ht.p.insecure_elasticity {
            return err_ptr(-EAGAIN);
        }
        let head = self.rht_ptr(bkt);
        self.w(obj, head); /* obj->next = head */
        if self.ht.rhlist {
            self.w(obj + 8, 0); /* list->next = NULL */
        }
        self.rht_assign_locked(bkt, obj);
        0
    }

    fn try_insert(&mut self, key_addr: u64, obj: u64) -> u64 {
        let mut new_tbl = self.ht.tbl;
        let mut tbl;
        let mut data;
        let mut last_tbl;
        let mut fguard = 0u64;

        loop {
            Self::future_check(&mut fguard);
            tbl = new_tbl;
            last_tbl = tbl;
            let hash = self.rht_head_hashfn(&self.ht, tbl, obj);
            let bkt = if self.tbl_future(tbl) != 0 {
                self.rht_bucket_var(tbl, hash)
            } else {
                self.rht_bucket_insert(tbl, hash)
            };
            if bkt == 0 {
                new_tbl = self.tbl_future(tbl);
                data = err_ptr(-EAGAIN);
            } else {
                self.rht_lock(bkt);
                data = self.lookup_one(bkt, tbl, hash, key_addr, obj);
                new_tbl = self.insert_one(bkt, tbl, hash, obj, data);
                let inserted = data != 0 && new_tbl == 0;
                if inserted {
                    self.ht.nelems += 1;
                }
                if ptr_err(new_tbl) != -EEXIST {
                    data = new_tbl; /* ERR_CAST */
                }
                self.rht_unlock(bkt);
                if inserted && {
                    let ht = &self.ht;
                    self.grow_above_75(ht, tbl)
                } {
                    self.ht.run_irq_queued = true;
                }
            }
            if is_err_or_null(new_tbl) {
                break;
            }
        }

        if ptr_err(data) == -EAGAIN {
            let r = self.insert_rehash(last_tbl);
            data = err_ptr(if r != 0 { r } else { -EAGAIN });
        }
        data
    }

    fn insert_rehash(&mut self, tbl: u64) -> i64 {
        let old_tbl = self.ht.tbl;
        let mut size = self.tbl_size(tbl);
        let mut err = -EBUSY;
        {
            let ht = &self.ht;
            if self.grow_above_75(ht, tbl) {
                size *= 2;
            } else if old_tbl != tbl {
                /* fail */
                if self.tbl_future(tbl) != 0 {
                    return 0;
                }
                if err == -ENOMEM {
                    self.ht.run_irq_queued = true;
                }
                return err;
            }
        }
        err = -ENOMEM;
        let new_tbl = self.bucket_table_alloc(size, GFP_ATOMIC | GFP_NOWARN);
        if new_tbl == 0 {
            if self.tbl_future(tbl) != 0 {
                return 0;
            }
            if err == -ENOMEM {
                self.ht.run_irq_queued = true;
            }
            return err;
        }
        err = self.rehash_attach(tbl, new_tbl);
        if err != 0 {
            self.bucket_table_free(new_tbl);
            if err == -EEXIST {
                err = 0;
            }
        } else {
            self.ht.run_irq_queued = true;
        }
        err
    }

    fn insert_slow(&mut self, key_addr: u64, obj: u64) -> u64 {
        loop {
            let data = self.try_insert(key_addr, obj);
            if ptr_err(data) != -EAGAIN {
                return data;
            }
        }
    }

    /* ================= header-inline insert path ================= */

    /* __rhashtable_insert_fast(ht, key_addr, obj, params, rhlist) -> void* */
    fn __insert_fast(&mut self, key_addr: u64, obj: u64, rhlist: bool) -> u64 {
        let tbl = self.ht.tbl;
        if tbl == 0 {
            return err_ptr(-EINVAL); /* init failed or not called */
        }
        let hash = self.rht_head_hashfn(&self.ht, tbl, obj);
        let mut elasticity = RHT_ELASTICITY;
        let bkt = self.rht_bucket_insert(tbl, hash);
        let mut data = err_ptr(-ENOMEM);
        if bkt == 0 {
            return data;
        }
        let mut pprev = 0u64;
        self.rht_lock(bkt);

        if self.tbl_future(tbl) != 0 {
            self.rht_unlock(bkt);
            return self.insert_slow(key_addr, obj);
        }

        let mut head = self.rht_ptr(bkt);
        let mut cguard = 0u64;
        while head != 0 && !is_a_nulls(head) {
            self.cycle_check(&mut cguard);
            elasticity -= 1;
            let key_differs =
                key_addr == 0 || self.compare(&self.ht, key_addr, self.rht_obj(&self.ht, head));
            if key_differs {
                pprev = head;
                head = self.r(head);
                continue;
            }
            /* key match */
            data = self.rht_obj(&self.ht, head);
            if !rhlist {
                self.rht_unlock(bkt);
                return data;
            }
            let list = obj;
            let plist = head;
            self.w(list + 8, plist);
            let nxt = self.r(head);
            self.w(list, nxt);
            if pprev != 0 {
                self.w(pprev, obj);
                self.rht_unlock(bkt);
            } else {
                self.rht_assign_unlock(bkt, obj);
            }
            return 0;
        }

        let insecure = self.ht.p.insecure_elasticity;
        if elasticity <= 0 && !insecure {
            self.rht_unlock(bkt);
            return self.insert_slow(key_addr, obj);
        }

        data = err_ptr(-E2BIG);
        if self.grow_above_max(&self.ht) {
            self.rht_unlock(bkt);
            return data;
        }
        if self.grow_above_100(&self.ht, tbl) && !insecure {
            self.rht_unlock(bkt);
            return self.insert_slow(key_addr, obj);
        }

        let head2 = self.rht_ptr(bkt);
        self.w(obj, head2);
        if rhlist {
            self.w(obj + 8, 0);
        }
        self.ht.nelems += 1;
        self.rht_assign_unlock(bkt, obj);
        if self.grow_above_75(&self.ht, tbl) {
            self.ht.run_irq_queued = true;
        }
        0
    }

    fn insert_fast_ret(&mut self, key_addr: u64, obj: u64, rhlist: bool) -> i64 {
        let ret = self.__insert_fast(key_addr, obj, rhlist);
        if is_err(ret) {
            return ptr_err(ret);
        }
        if ret == 0 {
            0
        } else {
            -EEXIST
        }
    }

    pub fn rhashtable_insert_fast(&mut self, obj: u64) -> i64 {
        self.insert_fast_ret(0, obj, false)
    }
    pub fn rhashtable_lookup_insert_fast(&mut self, obj: u64) -> i64 {
        let key = self.rht_obj(&self.ht, obj) + self.ht.p.key_offset as u64;
        self.insert_fast_ret(key, obj, false)
    }
    pub fn rhashtable_lookup_get_insert_fast(&mut self, obj: u64) -> u64 {
        let key = self.rht_obj(&self.ht, obj) + self.ht.p.key_offset as u64;
        self.__insert_fast(key, obj, false)
    }
    pub fn rhashtable_lookup_insert_key(&mut self, key_addr: u64, obj: u64) -> i64 {
        self.insert_fast_ret(key_addr, obj, false)
    }
    pub fn rhashtable_lookup_get_insert_key(&mut self, key_addr: u64, obj: u64) -> u64 {
        self.__insert_fast(key_addr, obj, false)
    }
    pub fn rhltable_insert(&mut self, obj: u64) -> i64 {
        /* key = rht_obj(list->rhead) + key_offset */
        let key = self.rht_obj(&self.ht, obj) + self.ht.p.key_offset as u64;
        let ret = self.__insert_fast(key, obj, true);
        ptr_err(ret)
    }
    pub fn rhltable_insert_key(&mut self, key_addr: u64, obj: u64) -> i64 {
        let ret = self.__insert_fast(key_addr, obj, true);
        ptr_err(ret)
    }

    /* ================= lookup ================= */

    /* __rhashtable_lookup: returns rhash_head canon (or 0) */
    fn __rhashtable_lookup(&self, key_addr: u64) -> u64 {
        let mut tbl = self.ht.tbl;
        if tbl == 0 {
            return 0;
        }
        let mut fguard = 0u64;
        loop {
            Self::future_check(&mut fguard);
            let hash = self.rht_key_hashfn(&self.ht, tbl, key_addr);
            let bkt = self.rht_bucket(tbl, hash);
            let mut cguard = 0u64;
            loop {
                self.cycle_check(&mut cguard);
                let mut he = self.rht_ptr(bkt);
                while he != 0 && !is_a_nulls(he) {
                    self.cycle_check(&mut cguard);
                    let differs = self.compare(&self.ht, key_addr, self.rht_obj(&self.ht, he));
                    if !differs {
                        return he;
                    }
                    he = self.r(he);
                }
                /* do..while (he != RHT_NULLS_MARKER(bkt)): retry chain walk
                 * when the terminating marker belongs to another bucket.
                 * A NULL next (dropped write, out-of-pool obj) terminates:
                 * C would NULL-deref; a miss is the graceful answer. */
                if he == 0 || he == (bkt | 1) {
                    break;
                }
            }
            let f = self.tbl_future(tbl);
            if f == 0 {
                return 0;
            }
            tbl = f;
        }
    }

    pub fn rhashtable_lookup(&self, key_addr: u64) -> u64 {
        let he = self.__rhashtable_lookup(key_addr);
        if he == 0 {
            0
        } else {
            self.rht_obj(&self.ht, he)
        }
    }
    pub fn rhashtable_lookup_likely(&self, key_addr: u64) -> u64 {
        self.rhashtable_lookup(key_addr)
    }
    pub fn rhashtable_lookup_fast_(&self, key_addr: u64) -> u64 {
        self.rhashtable_lookup(key_addr)
    }
    pub fn rhltable_lookup(&self, key_addr: u64) -> u64 {
        self.__rhashtable_lookup(key_addr) /* he == rhlist_head canon */
    }
    pub fn rhltable_lookup_likely(&self, key_addr: u64) -> u64 {
        self.__rhashtable_lookup(key_addr)
    }

    /* ================= remove ================= */

    fn __remove_fast_one(&mut self, tbl: u64, obj: u64, rhlist: bool) -> i64 {
        let hash = self.rht_head_hashfn(&self.ht, tbl, obj);
        let bkt = self.rht_bucket_var(tbl, hash);
        if bkt == 0 {
            return -ENOENT;
        }
        let mut err = -ENOENT;
        let mut pprev = 0u64;
        self.rht_lock(bkt);
        let mut he = self.rht_ptr(bkt);
        let mut unlocked = false;
        let mut cguard = 0u64;
        while he != 0 && !is_a_nulls(he) {
            self.cycle_check(&mut cguard);
            /* list = container_of(he, rhlist_head, rhead) == he */
            let mut list = he;
            if he != obj {
                pprev = he; /* &he->next */
                if !rhlist {
                    he = self.r(he);
                    continue;
                }
                /* walk the rhlist for obj */
                let mut lpprev;
                loop {
                    self.cycle_check(&mut cguard);
                    lpprev = list + 8; /* &list->next */
                    list = self.r(list + 8);
                    if list == 0 || obj == list {
                        break;
                    }
                }
                if list == 0 {
                    he = self.r(he);
                    continue;
                }
                let lnext = self.r(list + 8);
                self.w(lpprev, lnext);
                err = 0;
                break;
            }

            let mut objn = self.r(obj);
            err = 1;

            if rhlist {
                let lnext = self.r(list + 8); /* list->next */
                if lnext != 0 {
                    self.w(lnext, objn); /* list->next->rhead.next = objn */
                    objn = lnext; /* obj = &list->next->rhead == lnext */
                    err = 0;
                }
            }

            if pprev != 0 {
                self.w(pprev, objn);
                self.rht_unlock(bkt);
            } else {
                self.rht_assign_unlock(bkt, objn);
            }
            unlocked = true;
            break;
        }
        if !unlocked {
            self.rht_unlock(bkt);
        }
        if err > 0 {
            self.ht.nelems -= 1;
            if self.ht.p.automatic_shrinking && self.shrink_below_30(&self.ht, tbl) {
                self.ht.run_irq_queued = true;
            }
            err = 0;
        }
        err
    }

    fn __remove_fast(&mut self, obj: u64, rhlist: bool) -> i64 {
        let mut tbl = self.ht.tbl;
        if tbl == 0 {
            return -ENOENT;
        }
        let mut fguard = 0u64;
        loop {
            Self::future_check(&mut fguard);
            let err = self.__remove_fast_one(tbl, obj, rhlist);
            if err == 0 {
                return 0;
            }
            let f = self.tbl_future(tbl);
            if f == 0 {
                return err;
            }
            tbl = f;
        }
    }

    pub fn rhashtable_remove_fast(&mut self, obj: u64) -> i64 {
        self.__remove_fast(obj, false)
    }
    pub fn rhltable_remove(&mut self, obj: u64) -> i64 {
        self.__remove_fast(obj, true)
    }

    /* ================= replace ================= */

    fn __replace_fast(&mut self, tbl: u64, obj_old: u64, obj_new: u64) -> i64 {
        let hash = self.rht_head_hashfn(&self.ht, tbl, obj_old);
        if hash != self.rht_head_hashfn(&self.ht, tbl, obj_new) {
            return -EINVAL;
        }
        let bkt = self.rht_bucket_var(tbl, hash);
        if bkt == 0 {
            return -ENOENT;
        }
        let mut pprev = 0u64;
        self.rht_lock(bkt);
        let mut he = self.rht_ptr(bkt);
        let mut unlocked = false;
        let mut err = -ENOENT;
        let mut cguard = 0u64;
        while he != 0 && !is_a_nulls(he) {
            self.cycle_check(&mut cguard);
            if he != obj_old {
                pprev = he;
                he = self.r(he);
                continue;
            }
            let nxt = self.r(obj_old);
            self.w(obj_new, nxt); /* obj_new->next = obj_old->next */
            if pprev != 0 {
                self.w(pprev, obj_new);
                self.rht_unlock(bkt);
            } else {
                self.rht_assign_unlock(bkt, obj_new);
            }
            unlocked = true;
            err = 0;
            break;
        }
        if !unlocked {
            self.rht_unlock(bkt);
        }
        err
    }

    pub fn rhashtable_replace_fast(&mut self, obj_old: u64, obj_new: u64) -> i64 {
        let mut tbl = self.ht.tbl;
        if tbl == 0 {
            return -ENOENT;
        }
        let mut fguard = 0u64;
        loop {
            Self::future_check(&mut fguard);
            let err = self.__replace_fast(tbl, obj_old, obj_new);
            if err == 0 {
                return 0;
            }
            let f = self.tbl_future(tbl);
            if f == 0 {
                return err;
            }
            tbl = f;
        }
    }

    /* ================= next_key ================= */

    fn __next_in_table(&self, tbl: u64, prev_key: u64) -> u64 {
        let mut b = 0u64;
        if tbl == 0 {
            return 0;
        }
        let mut found = false;
        if prev_key != 0 {
            b = self.rht_key_hashfn(&self.ht, tbl, prev_key);
            let mut he = self.rht_ptr(self.rht_bucket(tbl, b));
            let mut cguard = 0u64;
            while he != 0 && !is_a_nulls(he) {
                self.cycle_check(&mut cguard);
                let matches = !self.compare(&self.ht, prev_key, self.rht_obj(&self.ht, he));
                if found {
                    if matches {
                        he = self.r(he);
                        continue;
                    }
                    return he;
                }
                if matches {
                    found = true;
                }
                he = self.r(he);
            }
            if !found {
                return err_ptr(-ENOENT);
            }
            b += 1;
        }
        while b < self.tbl_size(tbl) {
            let mut he = self.rht_ptr(self.rht_bucket(tbl, b));
            if !is_a_nulls(he) {
                return he;
            }
            let _ = &mut he;
            b += 1;
        }
        0
    }

    pub fn rhashtable_next_key(&mut self, prev_key: u64) -> u64 {
        if self.ht.rhlist {
            return err_ptr(-EOPNOTSUPP);
        }
        let mut tbl = self.ht.tbl;
        let mut prev = prev_key;
        let mut fguard = 0u64;
        loop {
            Self::future_check(&mut fguard);
            let he = self.__next_in_table(tbl, prev);
            if !is_err_or_null(he) {
                return self.rht_obj(&self.ht, he);
            }
            if he == 0 {
                prev = 0;
            }
            let f = self.tbl_future(tbl);
            if f == 0 {
                return he;
            }
            tbl = f;
        }
    }

    /* ================= walk ================= */

    pub fn walk_enter(&mut self, k: usize) {
        assert!(k < NI, "walk slot out of range");
        let node = Self::wnode(k);
        self.iters[k].p = 0;
        self.iters[k].slot = 0;
        self.iters[k].skip = 0;
        self.iters[k].end_of_table = false;
        let tbl = self.ht.tbl;
        self.set_wtbl(k, tbl);
        self.list_add(node, tbl + T_WNEXT);
    }

    pub fn walk_exit(&mut self, k: usize) {
        assert!(k < NI, "walk slot out of range");
        let node = Self::wnode(k);
        if self.wtbl(k) != 0 {
            self.list_del(node);
        }
    }

    pub fn walk_start_check(&mut self, k: usize) -> i64 {
        assert!(k < NI, "walk slot out of range");
        let node = Self::wnode(k);
        let rhlist = self.ht.rhlist;

        if self.wtbl(k) != 0 {
            self.list_del(node);
        }
        if self.iters[k].end_of_table {
            return 0;
        }
        if self.wtbl(k) == 0 {
            let tbl = self.ht.tbl;
            self.set_wtbl(k, tbl);
            self.iters[k].slot = 0;
            self.iters[k].skip = 0;
            self.iters[k].p = 0;
            return -EAGAIN;
        }

        let tbl = self.wtbl(k);
        if self.iters[k].p != 0 && !rhlist {
            /* validate p still in table; update skip */
            let mut p = self.rht_ptr(self.rht_bucket(tbl, self.iters[k].slot as u64));
            let mut skip = 0u64;
            let mut found = false;
            let mut cguard = 0u64;
            while p != 0 && !is_a_nulls(p) {
                self.cycle_check(&mut cguard);
                skip += 1;
                if p == self.iters[k].p {
                    self.iters[k].skip = skip as u32;
                    found = true;
                    break;
                }
                p = self.r(p);
            }
            if !found {
                self.iters[k].p = 0;
            }
        } else if self.iters[k].p != 0 && rhlist {
            let mut p = self.rht_ptr(self.rht_bucket(tbl, self.iters[k].slot as u64));
            let mut skip = 0u64;
            let mut found = false;
            let mut cguard = 0u64;
            'outer: while p != 0 && !is_a_nulls(p) {
                self.cycle_check(&mut cguard);
                let mut list = p; /* container_of(p) == p */
                while list != 0 {
                    self.cycle_check(&mut cguard);
                    skip += 1;
                    if list == self.iters[k].list {
                        self.iters[k].p = p;
                        self.iters[k].skip = skip as u32;
                        found = true;
                        break 'outer;
                    }
                    list = self.r(list + 8);
                }
                p = self.r(p);
            }
            if !found {
                self.iters[k].p = 0;
            }
        }
        0
    }

    pub fn walk_start(&mut self, k: usize) {
        let _ = self.walk_start_check(k);
    }

    fn __walk_find_next(&mut self, k: usize) -> u64 {
        let tbl = self.wtbl(k);
        let rhlist = self.ht.rhlist;
        if tbl == 0 {
            return 0;
        }
        while (self.iters[k].slot as u64) < self.tbl_size(tbl) {
            let mut skip = self.iters[k].skip as i64;
            let mut p = self.rht_ptr(self.rht_bucket(tbl, self.iters[k].slot as u64));
            let mut list = self.iters[k].list;
            /* rht_for_each_rcu over this slot's chain heads */
            let mut cguard = 0u64;
            while p != 0 && !is_a_nulls(p) {
                self.cycle_check(&mut cguard);
                if rhlist {
                    list = p; /* container_of(p, rhlist_head, rhead) == p */
                    let mut found = false;
                    loop {
                        if skip == 0 {
                            found = true;
                            break;
                        }
                        skip -= 1;
                        list = self.r(list + 8);
                        if list == 0 {
                            break;
                        }
                    }
                    if found {
                        break; /* goto next with p=head, list=member */
                    }
                    p = self.r(p);
                    continue;
                }
                if skip == 0 {
                    break;
                }
                skip -= 1;
                p = self.r(p);
            }
            /* label 'next' — p == 0 means the chain ended on a dropped
             * write; treat it like the NULLS marker (advance slot). */
            if p != 0 && !is_a_nulls(p) {
                self.iters[k].skip += 1;
                self.iters[k].p = p;
                self.iters[k].list = list;
                return self.rht_obj(&self.ht, if rhlist { list } else { p });
            }
            self.iters[k].skip = 0;
            self.iters[k].slot += 1;
        }

        self.iters[k].p = 0;
        let fut = self.tbl_future(tbl);
        self.set_wtbl(k, fut);
        if fut != 0 {
            self.iters[k].slot = 0;
            self.iters[k].skip = 0;
            return err_ptr(-EAGAIN);
        }
        self.iters[k].end_of_table = true;
        0
    }

    pub fn walk_next(&mut self, k: usize) -> u64 {
        assert!(k < NI, "walk slot out of range");
        let rhlist = self.ht.rhlist;
        let mut list = self.iters[k].list;
        let mut p = self.iters[k].p;
        if p != 0 {
            let mut advance = false;
            if !rhlist || {
                list = self.r(list + 8);
                list == 0
            } {
                p = self.r(p);
                list = p;
                advance = true;
            }
            let _ = advance;
            if p != 0 && !is_a_nulls(p) {
                self.iters[k].skip += 1;
                self.iters[k].p = p;
                self.iters[k].list = list;
                return self.rht_obj(&self.ht, if rhlist { list } else { p });
            }
            self.iters[k].skip = 0;
            self.iters[k].slot += 1;
        }
        self.__walk_find_next(k)
    }

    pub fn walk_peek(&mut self, k: usize) -> u64 {
        assert!(k < NI, "walk slot out of range");
        let rhlist = self.ht.rhlist;
        let p = self.iters[k].p;
        if p != 0 {
            return self.rht_obj(&self.ht, if rhlist { self.iters[k].list } else { p });
        }
        if self.iters[k].skip != 0 {
            self.iters[k].skip -= 1;
        }
        self.__walk_find_next(k)
    }

    pub fn walk_stop(&mut self, k: usize) {
        assert!(k < NI, "walk slot out of range");
        let node = Self::wnode(k);
        let tbl = self.wtbl(k);
        if tbl == 0 {
            return;
        }
        /* rcu_head_after_call_rcu(&tbl->rcu, bucket_table_free_rcu) */
        if self.ts(tbl, T_RCU_CALLED) != 0 && self.ts(tbl, T_RCU_FUNC) == FREE_RCU_TAG {
            self.set_wtbl(k, 0);
        } else {
            self.list_add(node, tbl + T_WNEXT);
        }
    }

    /* ================= init / destroy ================= */

    pub fn rounded_hashtable_size(&self, p: &Params) -> u64 {
        if p.nelem_hint != 0 {
            roundup_pow_of_two((p.nelem_hint as u64) * 4 / 3).max(p.min_size as u64)
        } else {
            HASH_DEFAULT_SIZE.max(p.min_size as u64)
        }
    }

    pub fn rhashtable_init(&mut self, params: &Params, rhlist: bool) -> i64 {
        if (params.key_len == 0 && params.obj_hashfn.is_none())
            || (params.obj_hashfn.is_some() && params.obj_cmpfn.is_none())
        {
            return -EINVAL;
        }
        {
            let ht = &mut self.ht;
            *ht = Rht::new();
            ht.p = *params;
            ht.rhlist = rhlist;
        }
        {
            let ht = &mut self.ht;
            if ht.p.min_size != 0 {
                ht.p.min_size = roundup_pow_of_two(ht.p.min_size as u64) as u16;
            }
            ht.max_elems = 1u32 << 31;
            if ht.p.max_size != 0 {
                ht.p.max_size = rounddown_pow_of_two(ht.p.max_size as u64) as u32;
                if (ht.p.max_size as u64) < (ht.max_elems as u64) / 2 {
                    ht.max_elems = ht.p.max_size * 2;
                }
            }
            ht.p.min_size = ht.p.min_size.max(HASH_MIN_SIZE as u16);
            ht.key_len = ht.p.key_len as u32;
            if ht.p.hashfn.is_none() {
                ht.p.hashfn = Some(HashFn::Jhash);
                if (ht.key_len & 3) == 0 {
                    ht.key_len /= 4;
                    ht.p.hashfn = Some(HashFn::Jhash2);
                }
            }
        }
        let size = self.rounded_hashtable_size(&self.ht.p);
        let mut tbl = self.bucket_table_alloc(size, GFP_KERNEL);
        if tbl == 0 {
            let min_sz = (self.ht.p.min_size as u64).max(HASH_MIN_SIZE);
            tbl = self.bucket_table_alloc(min_sz, GFP_KERNEL | GFP_NOFAIL);
        }
        if tbl == 0 {
            /* __GFP_NOFAIL retries forever on real kmalloc; the finite
             * arena can still run out — report it instead of succeeding
             * with a null table (C would NULL-deref on first use). */
            return -ENOMEM;
        }
        let ht = &mut self.ht;
        ht.nelems = 0;
        ht.tbl = tbl;
        ht.run_work_queued = false;
        ht.run_irq_queued = false;
        0
    }

    fn free_one<F: FnMut(&mut Self, u64)>(
        &mut self,
        obj: u64,
        free_fn: &mut F,
        cap: u64,
    ) {
        if !self.ht.rhlist {
            let o = self.rht_obj(&self.ht, obj);
            free_fn(self, o);
            return;
        }
        let mut list = obj; /* container_of */
        let mut cguard = 0u64;
        loop {
            Self::cycle_check_at(&mut cguard, cap);
            let o = self.rht_obj(&self.ht, list);
            let nxt = self.r(list + 8);
            free_fn(self, o);
            if nxt == 0 {
                break;
            }
            list = nxt;
        }
    }

    /// `rhashtable_free_and_destroy(ht, free_fn)` — walks every chain
    /// calling `free_fn(obj)` per stored object, then frees the tables.
    pub fn free_and_destroy<F: FnMut(&mut Self, u64)>(&mut self, mut free_fn: F) {
        self.inner_destroy(Some(&mut free_fn));
    }

    /// `rhashtable_free_and_destroy(ht, NULL)` — free the tables only;
    /// the bucket walk is skipped entirely like C's `if (free_fn)` arm.
    pub fn destroy(&mut self) {
        self.inner_destroy::<fn(&mut Self, u64)>(None);
    }

    fn inner_destroy<F: FnMut(&mut Self, u64)>(&mut self, mut free_fn: Option<&mut F>) {
        /* irq_work_sync: no-op; cancel_work_sync clears run_work */
        self.ht.run_work_queued = false;
        let mut tbl = self.ht.tbl;
        let mut fguard = 0u64;
        /* snapshot the cap: free_fn callbacks get &mut Self and may
         * shrink nelems mid-walk — a live bound could false-positive. */
        let cap = self.chain_limit();
        loop {
            Self::future_check(&mut fguard);
            if let Some(f) = free_fn.as_mut() {
                let size = self.tbl_size(tbl);
                for i in 0..size {
                    let bkt = self.rht_bucket(tbl, i);
                    let mut pos = self.rht_ptr(bkt);
                    let mut next = if !is_a_nulls(pos) { self.r(pos) } else { 0 };
                    let mut cguard = 0u64;
                    while pos != 0 && !is_a_nulls(pos) {
                        Self::cycle_check_at(&mut cguard, cap);
                        self.free_one(pos, *f, cap);
                        pos = next;
                        next = if !is_a_nulls(pos) { self.r(pos) } else { 0 };
                    }
                }
            }
            let next_tbl = self.tbl_future(tbl);
            self.bucket_table_free(tbl);
            if next_tbl == 0 {
                break;
            }
            tbl = next_tbl;
        }
    }
}

fn ilog2(n: u32) -> u64 {
    (31 - n.leading_zeros()) as u64
}
fn roundup_pow_of_two(n: u64) -> u64 {
    if n == 1 {
        return 1;
    }
    let nm1 = (n - 1) as u32;
    1u64 << (32 - nm1.leading_zeros())
}
fn rounddown_pow_of_two(n: u64) -> u64 {
    1u64 << (31 - ((n | 1) as u32).leading_zeros())
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("rhashtable_tests.inc");
}
