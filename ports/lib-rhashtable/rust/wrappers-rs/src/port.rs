//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! The port reproduces lib/rhashtable.c + lib/bucket_locks.c semantics over a
//! canonical address space identical to the C harness's `canon()`:
//!   0                       -> NULL
//!   POOLV + i*48 + off      -> &pool[i] byte `off`   (tobj slots)
//!   ARENAV + off            -> bump-arena allocation at byte `off`
//!   RHNULLV                 -> static objects (e.g. &rhnull)
//!   p | 1                   -> NULLS marker of bucket address p / lock bit
//! All "memory" (bucket_table headers, bucket arrays, nested pages, rhash_head
//! links, tobj fields) lives in a sparse word map keyed by canonical address,
//! so pointer arithmetic and NULLS-marker encoding are reproduced exactly.

#![forbid(unsafe_code)]

use std::collections::HashMap;

/* ================= canonical address space ================= */

const POOLV: u64 = 0x1000;
const ARENAV: u64 = 0x1_000_000;
const RHNULLV: u64 = 0x70_0000_0000;
const ITERV: u64 = 0x68_0000_0000; /* canonical base for iter walker list nodes */

const N: usize = 1024;
const NI: usize = 2;
const NILX: u64 = 0xffff;
const KEYSPACE: u64 = 4 * N as u64;
const DRAIN_MAX: i32 = 8;
const TOBJ_SZ: u64 = 48;

/* struct tobj offsets */
const O_META: u64 = 0;
const O_RHEAD: u64 = 8; /* rhlist_head.rhead.next */
const O_KEY: u64 = 24;
const O_TAG: u64 = 32;
const O_PRESENT: u64 = 40;
const O_FREED: u64 = 44;

const HEAD_OFF: u64 = O_RHEAD; /* head_offset = offsetof(tobj, rhl) */
const KEY_OFF: u64 = O_KEY;

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

/* ================= jhash ================= */

const JHASH_INITVAL: u32 = 0xdeadbeef;

macro_rules! jmix {
    ($a:expr, $b:expr, $c:expr) => {{
        $a = $a.wrapping_sub($c);
        $a ^= $c.rotate_left(4);
        $c = $c.wrapping_add($b);
        $b = $b.wrapping_sub($a);
        $b ^= $a.rotate_left(6);
        $a = $a.wrapping_add($c);
        $c = $c.wrapping_sub($b);
        $c ^= $b.rotate_left(8);
        $b = $b.wrapping_add($a);
        $a = $a.wrapping_sub($c);
        $a ^= $c.rotate_left(16);
        $c = $c.wrapping_add($b);
        $b = $b.wrapping_sub($a);
        $b ^= $a.rotate_left(19);
        $a = $a.wrapping_add($c);
        $c = $c.wrapping_sub($b);
        $c ^= $b.rotate_left(4);
        $b = $b.wrapping_add($a);
    }};
}

macro_rules! jfinal {
    ($a:expr, $b:expr, $c:expr) => {{
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(14));
        $a ^= $c;
        $a = $a.wrapping_sub($c.rotate_left(11));
        $b ^= $a;
        $b = $b.wrapping_sub($a.rotate_left(25));
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(16));
        $a ^= $c;
        $a = $a.wrapping_sub($c.rotate_left(4));
        $b ^= $a;
        $b = $b.wrapping_sub($a.rotate_left(14));
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(24));
    }};
}

fn le32(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]])
}

/// jhash() over arbitrary bytes (lookup3), matching include/linux/jhash.h.
fn jhash(key: &[u8], initval: u32) -> u32 {
    let length = key.len() as u32;
    let mut a = JHASH_INITVAL.wrapping_add(length).wrapping_add(initval);
    let mut b = a;
    let mut c = a;
    let mut i = 0usize;
    let mut len = key.len();

    while len > 12 {
        a = a.wrapping_add(le32(key, i));
        b = b.wrapping_add(le32(key, i + 1));
        c = c.wrapping_add(le32(key, i + 2));
        jmix!(a, b, c);
        i += 3;
        len -= 12;
    }

    /* fallthrough byte tail, exactly as lookup3 */
    let k = &key[i * 4..];
    if len >= 12 {
        c = c.wrapping_add((k[11] as u32) << 24);
    }
    if len >= 11 {
        c = c.wrapping_add((k[10] as u32) << 16);
    }
    if len >= 10 {
        c = c.wrapping_add((k[9] as u32) << 8);
    }
    if len >= 9 {
        c = c.wrapping_add(k[8] as u32);
    }
    if len >= 8 {
        b = b.wrapping_add((k[7] as u32) << 24);
    }
    if len >= 7 {
        b = b.wrapping_add((k[6] as u32) << 16);
    }
    if len >= 6 {
        b = b.wrapping_add((k[5] as u32) << 8);
    }
    if len >= 5 {
        b = b.wrapping_add(k[4] as u32);
    }
    if len >= 4 {
        a = a.wrapping_add((k[3] as u32) << 24);
    }
    if len >= 3 {
        a = a.wrapping_add((k[2] as u32) << 16);
    }
    if len >= 2 {
        a = a.wrapping_add((k[1] as u32) << 8);
    }
    if len >= 1 {
        a = a.wrapping_add(k[0] as u32);
        jfinal!(a, b, c);
    }
    c
}

/// jhash2() over a u32 word array (k = le32 words, length = count).
fn jhash2(words: &[u32], initval: u32) -> u32 {
    let mut a = JHASH_INITVAL
        .wrapping_add((words.len() as u32) << 2)
        .wrapping_add(initval);
    let mut b = a;
    let mut c = a;
    let mut i = 0usize;
    let mut n = words.len();

    while n > 3 {
        a = a.wrapping_add(words[i]);
        b = b.wrapping_add(words[i + 1]);
        c = c.wrapping_add(words[i + 2]);
        jmix!(a, b, c);
        i += 3;
        n -= 3;
    }
    match n {
        3 => {
            c = c.wrapping_add(words[i + 2]);
            b = b.wrapping_add(words[i + 1]);
            a = a.wrapping_add(words[i]);
            jfinal!(a, b, c);
        }
        2 => {
            b = b.wrapping_add(words[i + 1]);
            a = a.wrapping_add(words[i]);
            jfinal!(a, b, c);
        }
        1 => {
            a = a.wrapping_add(words[i]);
            jfinal!(a, b, c);
        }
        _ => {}
    }
    c
}

/* ================= params / hash dispatch ================= */

#[derive(Clone, Copy, PartialEq)]
enum HashFn {
    Jhash,
    Jhash2,
    Custom, /* jhash(data,len,seed) ^ 0x9e3779b9 */
}

#[derive(Clone, Copy, PartialEq)]
enum ObjHash {
    Good, /* jhash(&o->key, 8, seed) */
    Bad,  /* key & 0xf */
}

#[derive(Clone, Copy, PartialEq)]
enum ObjCmp {
    Eq,   /* key != *arg */
    Lo32, /* (u32)key != (u32)*arg */
}

#[derive(Clone, Copy, Default)]
struct Params {
    nelem_hint: u16,
    key_len: u16,
    key_offset: u16,
    head_offset: u16,
    max_size: u32,
    min_size: u16,
    insecure_elasticity: bool,
    automatic_shrinking: bool,
    hashfn: Option<HashFn>, /* None until init assigns default */
    obj_hashfn: Option<ObjHash>,
    obj_cmpfn: Option<ObjCmp>,
}

struct Rht {
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
struct Iter {
    ht: usize, /* 0 = ht, 1 = hlt */
    p: u64,
    list: u64,
    slot: u32,
    skip: u32,
    end_of_table: bool,
}

impl Iter {
    fn new() -> Self {
        Iter {
            ht: 0,
            p: 0,
            list: 0,
            slot: 0,
            skip: 0,
            end_of_table: false,
        }
    }
}

/* ================= simulator ================= */

struct Sim {
    mem: HashMap<u64, u64>,
    arena_off: u64,
    alloc_ord: u64,
    fail_nth: u64,
    rnd: u64, /* kshim_random_u32 state */
    rng: u64, /* wrapper xs64 state */
    g_h: u64,
    ht: Rht,
    hlt: Rht,
    g_par: Params,
    ht_live: bool,
    hlt_live: bool,
    iters: Vec<Iter>,
    iter_entered: [i32; NI],
    walk_on: [i32; NI],
}

impl Sim {
    fn new() -> Self {
        Sim {
            mem: HashMap::new(),
            arena_off: 0,
            alloc_ord: 0,
            fail_nth: 0,
            rnd: 0,
            rng: 0,
            g_h: 0,
            ht: Rht::new(),
            hlt: Rht::new(),
            g_par: Params::default(),
            ht_live: false,
            hlt_live: false,
            iters: vec![Iter::new(), Iter::new()],
            iter_entered: [0; NI],
            walk_on: [0; NI],
        }
    }

    /* ---- memory ---- */
    fn r(&self, a: u64) -> u64 {
        *self.mem.get(&a).unwrap_or(&0)
    }
    fn w(&mut self, a: u64, v: u64) {
        if v == 0 {
            self.mem.remove(&a);
        } else {
            self.mem.insert(a, v);
        }
    }

    /* ---- rng ---- */
    fn xs64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(2685821657736338717)
    }
    fn random_u32(&mut self) -> u32 {
        self.rnd ^= self.rnd >> 12;
        self.rnd ^= self.rnd << 25;
        self.rnd ^= self.rnd >> 27;
        self.rnd = self.rnd.wrapping_mul(2685821657736338717);
        (self.rnd >> 32) as u32
    }

    fn fold(&mut self, v: u64) {
        if std::env::var_os("RFOLD").is_some() {
            eprintln!("{:016x}", v);
        }
        self.g_h = (self.g_h ^ v).wrapping_mul(0x100000001b3);
    }

    /* ---- allocator ---- */
    fn kalloc(&mut self, size: u64, gfp: u64) -> u64 {
        self.alloc_ord += 1;
        if self.alloc_ord == self.fail_nth && (gfp & GFP_NOFAIL) == 0 {
            return 0;
        }
        let off = (self.arena_off + 7) & !7;
        self.arena_off = off + size;
        if gfp & GFP_ZERO != 0 {
            for a in (off..off + size).step_by(8) {
                self.mem.remove(&(ARENAV + a));
            }
        }
        ARENAV + off
    }
    fn kfree(&mut self, _p: u64) {}

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
    fn list_count(&self, head: u64) -> u64 {
        let mut c = 0u64;
        let mut p = self.r(head);
        let mut guard = 0u64;
        while p != head && guard < 64 {
            c += 1;
            p = self.r(p);
            guard += 1;
        }
        c
    }

    /* ---- tobj pool ---- */
    fn obj(i: usize) -> u64 {
        POOLV + (i as u64) * TOBJ_SZ
    }
    fn tidx(o: u64) -> u64 {
        if o == 0 || o < POOLV || !(o - POOLV).is_multiple_of(TOBJ_SZ) {
            return NILX;
        }
        let i = (o - POOLV) / TOBJ_SZ;
        if i >= N as u64 {
            return NILX;
        }
        i
    }
    fn pool_present(&self, i: usize) -> bool {
        self.r(Self::obj(i) + O_PRESENT) != 0
    }
    fn pool_freed(&self, i: usize) -> bool {
        self.r(Self::obj(i) + O_FREED) != 0
    }
    fn pool_key(&self, i: usize) -> u64 {
        self.r(Self::obj(i) + O_KEY)
    }
    fn free_slot(&self) -> i64 {
        for i in 0..N {
            if !self.pool_present(i) && !self.pool_freed(i) {
                return i as i64;
            }
        }
        -1
    }
    fn nth_present(&self, n: u64) -> i64 {
        let mut n = n;
        for i in 0..N {
            if self.pool_present(i) && !self.pool_freed(i) {
                if n == 0 {
                    return i as i64;
                }
                n -= 1;
            }
        }
        -1
    }
    fn count_present(&self) -> u64 {
        let mut c = 0;
        for i in 0..N {
            if self.pool_present(i) && !self.pool_freed(i) {
                c += 1;
            }
        }
        c
    }

    fn reset_all(&mut self, seed: u64, fail_i: i64) {
        self.mem.clear();
        self.arena_off = 0;
        self.alloc_ord = 0;
        self.fail_nth = fail_i.unsigned_abs() % 97;
        self.rnd = (seed ^ 0x517cc1b727220a95) | 1;
        self.rng = seed | 1;
        self.g_h = 0x9e3779b97f4a7c15;
        self.ht = Rht::new();
        self.hlt = Rht::new();
        self.g_par = Params::default();
        self.ht_live = false;
        self.hlt_live = false;
        self.iters = vec![Iter::new(), Iter::new()];
        self.iter_entered = [0; NI];
        self.walk_on = [0; NI];
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
            if b == 0 { RHNULLV } else { b }
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
                /* key_len in u32 words */
                let words: Vec<u32> = (0..ht.key_len.min(2))
                    .map(|i| {
                        u32::from_le_bytes([
                            v.to_le_bytes()[(i * 4) as usize],
                            v.to_le_bytes()[(i * 4) as usize + 1],
                            v.to_le_bytes()[(i * 4) as usize + 2],
                            v.to_le_bytes()[(i * 4) as usize + 3],
                        ])
                    })
                    .collect();
                jhash2(&words, rnd)
            }
        }
    }

    fn rht_key_hashfn(&self, ht: &Rht, tbl: u64, key_addr: u64) -> u64 {
        let h = self.apply_hashfn(ht, key_addr, self.ts(tbl, T_HASH_RND) as u32);
        (h as u64) & (self.tbl_size(tbl) - 1)
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
        let nest = (ilog2(nbuckets as u32) - 1) % shift + 1;
        self.tw(tbl, T_NEST, nest);
        tbl
    }

    fn bucket_table_alloc(&mut self, nbuckets: u64, gfp: u64) -> u64 {
        let mut nb = nbuckets;
        let mut tbl = self.kalloc(56 + nb * 8, gfp | GFP_ZERO);

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
        loop {
            let f = self.tbl_future(tbl);
            if f == 0 {
                return tbl;
            }
            tbl = f;
        }
    }

    /* ================= rehash ================= */

    fn rehash_one(&mut self, which: usize, bkt: u64, _old_hash: u64) -> i64 {
        let old_tbl = self.ht_ref(which).tbl;
        let new_tbl = self.last_table(old_tbl);
        let mut err = -EAGAIN;

        if self.tbl_nest(new_tbl) != 0 {
            return err;
        }
        err = -ENOENT;

        let mut entry = self.rht_ptr(bkt);
        let mut next = 0u64;
        let mut pprev = 0u64; /* canon addr of prev->next cell, or 0 */
        while !is_a_nulls(entry) {
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

        let new_hash = self.rht_head_hashfn(self.ht_ref(which), new_tbl, entry);
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

    fn rehash_chain(&mut self, which: usize, old_hash: u64) -> i64 {
        let old_tbl = self.ht_ref(which).tbl;
        let bkt = self.rht_bucket_var(old_tbl, old_hash);
        if bkt == 0 {
            return 0;
        }
        self.rht_lock(bkt);
        let mut err;
        loop {
            err = self.rehash_one(which, bkt, old_hash);
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

    fn rehash_table(&mut self, which: usize) -> i64 {
        let old_tbl = self.ht_ref(which).tbl;
        let new_tbl = self.tbl_future(old_tbl);
        if new_tbl == 0 {
            return 0;
        }
        let size = self.tbl_size(old_tbl);
        for old_hash in 0..size {
            let e = self.rehash_chain(which, old_hash);
            if e != 0 {
                return e;
            }
            /* cond_resched: no-op */
        }
        self.ht_mut(which).tbl = new_tbl;

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

    fn rehash_alloc(&mut self, _which: usize, old_tbl: u64, size: u64) -> i64 {
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

    fn rhashtable_shrink(&mut self, which: usize) -> i64 {
        let old_tbl = self.ht_ref(which).tbl;
        let nelems = self.ht_ref(which).nelems.max(0) as u64;
        let mut size = 0u64;
        if nelems != 0 {
            size = roundup_pow_of_two(nelems * 3 / 2);
        }
        if size < self.ht_ref(which).p.min_size as u64 {
            size = self.ht_ref(which).p.min_size as u64;
        }
        if self.tbl_size(old_tbl) <= size {
            return 0;
        }
        if self.tbl_future(old_tbl) != 0 {
            return -EEXIST;
        }
        self.rehash_alloc(which, old_tbl, size)
    }

    /* rht_deferred_worker (run_work.func) */
    fn deferred_worker(&mut self, which: usize) {
        let mut err;
        {
            let ht = self.ht_ref(which);
            let tbl = self.last_table(ht.tbl);
            if self.grow_above_75(ht, tbl) {
                let s = self.tbl_size(tbl);
                err = self.rehash_alloc(which, tbl, s * 2);
            } else if ht.p.automatic_shrinking && self.shrink_below_30(ht, tbl) {
                err = self.rhashtable_shrink(which);
            } else if self.tbl_nest(tbl) != 0 {
                let s = self.tbl_size(tbl);
                err = self.rehash_alloc(which, tbl, s);
            } else {
                err = 0;
            }
        }
        if err == 0 || err == -EEXIST {
            let nerr = self.rehash_table(which);
            err = if err != 0 { err } else { nerr };
        }
        if err != 0 {
            self.ht_mut(which).run_work_queued = true;
        }
    }

    /* rht_deferred_irq_work (run_irq_work.func) */
    fn deferred_irq_work(&mut self, which: usize) {
        self.ht_mut(which).run_work_queued = true;
    }

    fn ht_mut(&mut self, which: usize) -> &mut Rht {
        if which == 0 {
            &mut self.ht
        } else {
            &mut self.hlt
        }
    }
    fn ht_ref(&self, which: usize) -> &Rht {
        if which == 0 { &self.ht } else { &self.hlt }
    }

    fn drain_work(&mut self, which: usize) {
        let mut n = 0;
        while n < DRAIN_MAX {
            if self.ht_ref(which).run_irq_queued {
                self.ht_mut(which).run_irq_queued = false;
                self.deferred_irq_work(which);
                n += 1;
                continue;
            }
            if self.ht_ref(which).run_work_queued {
                self.ht_mut(which).run_work_queued = false;
                self.deferred_worker(which);
                n += 1;
                continue;
            }
            break;
        }
        self.fold(0xdeed0000 | n as u64);
    }

    /* ================= insert path (rhashtable.c) ================= */

    fn lookup_one(
        &mut self,
        which: usize,
        bkt: u64,
        _tbl: u64,
        _hash: u64,
        key_addr: u64,
        obj: u64,
    ) -> u64 {
        let mut elasticity = RHT_ELASTICITY;
        let mut pprev = 0u64;
        let mut head = self.rht_ptr(bkt);

        while !is_a_nulls(head) {
            elasticity -= 1;
            let key_differs = key_addr == 0
                || self.compare(
                    self.ht_ref(which),
                    key_addr,
                    self.rht_obj(self.ht_ref(which), head),
                );
            if key_differs {
                pprev = head; /* &head->next */
                head = self.r(head);
                continue;
            }
            let ht = self.ht_ref(which);
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

        if elasticity <= 0 && !self.ht_ref(which).p.insecure_elasticity {
            return err_ptr(-EAGAIN);
        }
        err_ptr(-ENOENT)
    }

    fn insert_one(
        &mut self,
        which: usize,
        bkt: u64,
        tbl: u64,
        _hash: u64,
        obj: u64,
        data: u64,
    ) -> u64 {
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
        if self.grow_above_max(self.ht_ref(which)) {
            return err_ptr(-E2BIG);
        }
        if self.grow_above_100(self.ht_ref(which), tbl) && !self.ht_ref(which).p.insecure_elasticity
        {
            return err_ptr(-EAGAIN);
        }
        let head = self.rht_ptr(bkt);
        self.w(obj, head); /* obj->next = head */
        if self.ht_ref(which).rhlist {
            self.w(obj + 8, 0); /* list->next = NULL */
        }
        self.rht_assign_locked(bkt, obj);
        0
    }

    fn try_insert(&mut self, which: usize, key_addr: u64, obj: u64) -> u64 {
        let mut new_tbl = self.ht_ref(which).tbl;
        let mut tbl;
        let mut data;
        let mut last_tbl;

        loop {
            tbl = new_tbl;
            last_tbl = tbl;
            let hash = self.rht_head_hashfn(self.ht_ref(which), tbl, obj);
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
                data = self.lookup_one(which, bkt, tbl, hash, key_addr, obj);
                new_tbl = self.insert_one(which, bkt, tbl, hash, obj, data);
                let inserted = data != 0 && new_tbl == 0;
                if inserted {
                    self.ht_mut(which).nelems += 1;
                }
                if ptr_err(new_tbl) != -EEXIST {
                    data = new_tbl; /* ERR_CAST */
                }
                self.rht_unlock(bkt);
                if inserted && {
                    let ht = self.ht_ref(which);
                    self.grow_above_75(ht, tbl)
                } {
                    self.ht_mut(which).run_irq_queued = true;
                }
            }
            if is_err_or_null(new_tbl) {
                break;
            }
        }

        if ptr_err(data) == -EAGAIN {
            let r = self.insert_rehash(which, last_tbl);
            data = err_ptr(if r != 0 { r } else { -EAGAIN });
        }
        data
    }

    fn insert_rehash(&mut self, which: usize, tbl: u64) -> i64 {
        let old_tbl = self.ht_ref(which).tbl;
        let mut size = self.tbl_size(tbl);
        let mut err = -EBUSY;
        {
            let ht = self.ht_ref(which);
            if self.grow_above_75(ht, tbl) {
                size *= 2;
            } else if old_tbl != tbl {
                /* fail */
                if self.tbl_future(tbl) != 0 {
                    return 0;
                }
                if err == -ENOMEM {
                    self.ht_mut(which).run_irq_queued = true;
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
                self.ht_mut(which).run_irq_queued = true;
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
            self.ht_mut(which).run_irq_queued = true;
        }
        err
    }

    fn insert_slow(&mut self, which: usize, key_addr: u64, obj: u64) -> u64 {
        loop {
            let data = self.try_insert(which, key_addr, obj);
            if ptr_err(data) != -EAGAIN {
                return data;
            }
        }
    }

    /* ================= header-inline insert path ================= */

    /* __rhashtable_insert_fast(ht, key_addr, obj, params, rhlist) -> void* */
    fn __insert_fast(&mut self, which: usize, key_addr: u64, obj: u64, rhlist: bool) -> u64 {
        let tbl = self.ht_ref(which).tbl;
        let hash = self.rht_head_hashfn(self.ht_ref(which), tbl, obj);
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
            return self.insert_slow(which, key_addr, obj);
        }

        let mut head = self.rht_ptr(bkt);
        while !is_a_nulls(head) {
            elasticity -= 1;
            let key_differs = key_addr == 0
                || self.compare(
                    self.ht_ref(which),
                    key_addr,
                    self.rht_obj(self.ht_ref(which), head),
                );
            if key_differs {
                pprev = head;
                head = self.r(head);
                continue;
            }
            /* key match */
            data = self.rht_obj(self.ht_ref(which), head);
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

        let insecure = self.ht_ref(which).p.insecure_elasticity;
        if elasticity <= 0 && !insecure {
            self.rht_unlock(bkt);
            return self.insert_slow(which, key_addr, obj);
        }

        data = err_ptr(-E2BIG);
        if self.grow_above_max(self.ht_ref(which)) {
            self.rht_unlock(bkt);
            return data;
        }
        if self.grow_above_100(self.ht_ref(which), tbl) && !insecure {
            self.rht_unlock(bkt);
            return self.insert_slow(which, key_addr, obj);
        }

        let head2 = self.rht_ptr(bkt);
        self.w(obj, head2);
        if rhlist {
            self.w(obj + 8, 0);
        }
        self.ht_mut(which).nelems += 1;
        self.rht_assign_unlock(bkt, obj);
        if self.grow_above_75(self.ht_ref(which), tbl) {
            self.ht_mut(which).run_irq_queued = true;
        }
        0
    }

    fn insert_fast_ret(&mut self, which: usize, key_addr: u64, obj: u64, rhlist: bool) -> i64 {
        let ret = self.__insert_fast(which, key_addr, obj, rhlist);
        if is_err(ret) {
            return ptr_err(ret);
        }
        if ret == 0 { 0 } else { -EEXIST }
    }

    fn rhashtable_insert_fast(&mut self, which: usize, obj: u64) -> i64 {
        self.insert_fast_ret(which, 0, obj, false)
    }
    fn rhashtable_lookup_insert_fast(&mut self, which: usize, obj: u64) -> i64 {
        let key = self.rht_obj(self.ht_ref(which), obj) + self.ht_ref(which).p.key_offset as u64;
        self.insert_fast_ret(which, key, obj, false)
    }
    fn rhashtable_lookup_get_insert_fast(&mut self, which: usize, obj: u64) -> u64 {
        let key = self.rht_obj(self.ht_ref(which), obj) + self.ht_ref(which).p.key_offset as u64;
        self.__insert_fast(which, key, obj, false)
    }
    fn rhashtable_lookup_insert_key(&mut self, which: usize, key_addr: u64, obj: u64) -> i64 {
        self.insert_fast_ret(which, key_addr, obj, false)
    }
    fn rhashtable_lookup_get_insert_key(&mut self, which: usize, key_addr: u64, obj: u64) -> u64 {
        self.__insert_fast(which, key_addr, obj, false)
    }
    fn rhltable_insert(&mut self, obj: u64) -> i64 {
        /* key = rht_obj(list->rhead) + key_offset */
        let key = self.rht_obj(self.ht_ref(1), obj) + self.ht_ref(1).p.key_offset as u64;
        let ret = self.__insert_fast(1, key, obj, true);
        ptr_err(ret)
    }
    fn rhltable_insert_key(&mut self, key_addr: u64, obj: u64) -> i64 {
        let ret = self.__insert_fast(1, key_addr, obj, true);
        ptr_err(ret)
    }

    /* ================= lookup ================= */

    /* __rhashtable_lookup: returns rhash_head canon (or 0) */
    fn __rhashtable_lookup(&self, which: usize, key_addr: u64) -> u64 {
        let mut tbl = self.ht_ref(which).tbl;
        loop {
            let hash = self.rht_key_hashfn(self.ht_ref(which), tbl, key_addr);
            let bkt = self.rht_bucket(tbl, hash);
            loop {
                let mut he = self.rht_ptr(bkt);
                while !is_a_nulls(he) {
                    let differs = self.compare(
                        self.ht_ref(which),
                        key_addr,
                        self.rht_obj(self.ht_ref(which), he),
                    );
                    if !differs {
                        return he;
                    }
                    he = self.r(he);
                }
                /* do..while (he != RHT_NULLS_MARKER(bkt)): retry chain walk
                 * when the terminating marker belongs to another bucket */
                if he == (bkt | 1) {
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

    fn rhashtable_lookup(&self, which: usize, key_addr: u64) -> u64 {
        let he = self.__rhashtable_lookup(which, key_addr);
        if he == 0 {
            0
        } else {
            self.rht_obj(self.ht_ref(which), he)
        }
    }
    fn rhashtable_lookup_likely(&self, which: usize, key_addr: u64) -> u64 {
        self.rhashtable_lookup(which, key_addr)
    }
    fn rhashtable_lookup_fast_(&self, which: usize, key_addr: u64) -> u64 {
        self.rhashtable_lookup(which, key_addr)
    }
    fn rhltable_lookup(&self, key_addr: u64) -> u64 {
        self.__rhashtable_lookup(1, key_addr) /* he == rhlist_head canon */
    }
    fn rhltable_lookup_likely(&self, key_addr: u64) -> u64 {
        self.__rhashtable_lookup(1, key_addr)
    }

    /* ================= remove ================= */

    fn __remove_fast_one(&mut self, which: usize, tbl: u64, obj: u64, rhlist: bool) -> i64 {
        let hash = self.rht_head_hashfn(self.ht_ref(which), tbl, obj);
        let bkt = self.rht_bucket_var(tbl, hash);
        if bkt == 0 {
            return -ENOENT;
        }
        let mut err = -ENOENT;
        let mut pprev = 0u64;
        self.rht_lock(bkt);
        let mut he = self.rht_ptr(bkt);
        let mut unlocked = false;
        while !is_a_nulls(he) {
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
            self.ht_mut(which).nelems -= 1;
            if self.ht_ref(which).p.automatic_shrinking
                && self.shrink_below_30(self.ht_ref(which), tbl)
            {
                self.ht_mut(which).run_irq_queued = true;
            }
            err = 0;
        }
        err
    }

    fn __remove_fast(&mut self, which: usize, obj: u64, rhlist: bool) -> i64 {
        let mut tbl = self.ht_ref(which).tbl;
        loop {
            let err = self.__remove_fast_one(which, tbl, obj, rhlist);
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

    fn rhashtable_remove_fast(&mut self, which: usize, obj: u64) -> i64 {
        self.__remove_fast(which, obj, false)
    }
    fn rhltable_remove(&mut self, obj: u64) -> i64 {
        self.__remove_fast(1, obj, true)
    }

    /* ================= replace ================= */

    fn __replace_fast(&mut self, which: usize, tbl: u64, obj_old: u64, obj_new: u64) -> i64 {
        let hash = self.rht_head_hashfn(self.ht_ref(which), tbl, obj_old);
        if hash != self.rht_head_hashfn(self.ht_ref(which), tbl, obj_new) {
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
        while !is_a_nulls(he) {
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

    fn rhashtable_replace_fast(&mut self, which: usize, obj_old: u64, obj_new: u64) -> i64 {
        let mut tbl = self.ht_ref(which).tbl;
        loop {
            let err = self.__replace_fast(which, tbl, obj_old, obj_new);
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

    fn __next_in_table(&self, which: usize, tbl: u64, prev_key: u64) -> u64 {
        let mut b = 0u64;
        let mut found = false;
        if prev_key != 0 {
            b = self.rht_key_hashfn(self.ht_ref(which), tbl, prev_key);
            let mut he = self.rht_ptr(self.rht_bucket(tbl, b));
            while !is_a_nulls(he) {
                let matches = !self.compare(
                    self.ht_ref(which),
                    prev_key,
                    self.rht_obj(self.ht_ref(which), he),
                );
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

    fn rhashtable_next_key(&mut self, which: usize, prev_key: u64) -> u64 {
        if self.ht_ref(which).rhlist {
            return err_ptr(-EOPNOTSUPP);
        }
        let mut tbl = self.ht_ref(which).tbl;
        let mut prev = prev_key;
        loop {
            let he = self.__next_in_table(which, tbl, prev);
            if !is_err_or_null(he) {
                return self.rht_obj(self.ht_ref(which), he);
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

    fn walk_enter(&mut self, which: usize, k: usize) {
        let node = Self::wnode(k);
        self.iters[k].ht = which;
        self.iters[k].p = 0;
        self.iters[k].slot = 0;
        self.iters[k].skip = 0;
        self.iters[k].end_of_table = false;
        let tbl = self.ht_ref(which).tbl;
        self.set_wtbl(k, tbl);
        self.list_add(node, tbl + T_WNEXT);
    }

    fn walk_exit(&mut self, k: usize) {
        let node = Self::wnode(k);
        if self.wtbl(k) != 0 {
            self.list_del(node);
        }
    }

    fn walk_start_check(&mut self, k: usize) -> i64 {
        let which = self.iters[k].ht;
        let node = Self::wnode(k);
        let rhlist = self.ht_ref(which).rhlist;

        if self.wtbl(k) != 0 {
            self.list_del(node);
        }
        if self.iters[k].end_of_table {
            return 0;
        }
        if self.wtbl(k) == 0 {
            let tbl = self.ht_ref(which).tbl;
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
            while !is_a_nulls(p) {
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
            'outer: while !is_a_nulls(p) {
                let mut list = p; /* container_of(p) == p */
                while list != 0 {
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

    fn walk_start(&mut self, k: usize) {
        let _ = self.walk_start_check(k);
    }

    fn __walk_find_next(&mut self, k: usize) -> u64 {
        let which = self.iters[k].ht;
        let tbl = self.wtbl(k);
        let rhlist = self.ht_ref(which).rhlist;
        if tbl == 0 {
            return 0;
        }
        while (self.iters[k].slot as u64) < self.tbl_size(tbl) {
            let mut skip = self.iters[k].skip as i64;
            let mut p = self.rht_ptr(self.rht_bucket(tbl, self.iters[k].slot as u64));
            let mut list = self.iters[k].list;
            /* rht_for_each_rcu over this slot's chain heads */
            while !is_a_nulls(p) {
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
            /* label 'next' */
            if !is_a_nulls(p) {
                self.iters[k].skip += 1;
                self.iters[k].p = p;
                self.iters[k].list = list;
                return self.rht_obj(self.ht_ref(which), if rhlist { list } else { p });
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

    fn walk_next(&mut self, k: usize) -> u64 {
        let which = self.iters[k].ht;
        let rhlist = self.ht_ref(which).rhlist;
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
            if !is_a_nulls(p) {
                self.iters[k].skip += 1;
                self.iters[k].p = p;
                self.iters[k].list = list;
                return self.rht_obj(self.ht_ref(which), if rhlist { list } else { p });
            }
            self.iters[k].skip = 0;
            self.iters[k].slot += 1;
        }
        self.__walk_find_next(k)
    }

    fn walk_peek(&mut self, k: usize) -> u64 {
        let which = self.iters[k].ht;
        let rhlist = self.ht_ref(which).rhlist;
        let p = self.iters[k].p;
        if p != 0 {
            return self.rht_obj(
                self.ht_ref(which),
                if rhlist { self.iters[k].list } else { p },
            );
        }
        if self.iters[k].skip != 0 {
            self.iters[k].skip -= 1;
        }
        self.__walk_find_next(k)
    }

    fn walk_stop(&mut self, k: usize) {
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

    fn rounded_hashtable_size(&self, p: &Params) -> u64 {
        if p.nelem_hint != 0 {
            roundup_pow_of_two((p.nelem_hint as u64) * 4 / 3).max(p.min_size as u64)
        } else {
            HASH_DEFAULT_SIZE.max(p.min_size as u64)
        }
    }

    fn rhashtable_init(&mut self, which: usize, params: &Params, rhlist: bool) -> i64 {
        if (params.key_len == 0 && params.obj_hashfn.is_none())
            || (params.obj_hashfn.is_some() && params.obj_cmpfn.is_none())
        {
            return -EINVAL;
        }
        {
            let ht = self.ht_mut(which);
            *ht = Rht::new();
            ht.p = *params;
            ht.rhlist = rhlist;
        }
        {
            let ht = self.ht_mut(which);
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
        let size = self.rounded_hashtable_size(&self.ht_ref(which).p);
        let mut tbl = self.bucket_table_alloc(size, GFP_KERNEL);
        if tbl == 0 {
            let min_sz = (self.ht_ref(which).p.min_size as u64).max(HASH_MIN_SIZE);
            tbl = self.bucket_table_alloc(min_sz, GFP_KERNEL | GFP_NOFAIL);
        }
        let ht = self.ht_mut(which);
        ht.nelems = 0;
        ht.tbl = tbl;
        ht.run_work_queued = false;
        ht.run_irq_queued = false;
        0
    }

    /* free_fn callback for pool objects: records release order */
    fn free_cb(&mut self, ptr: u64) {
        let i = Self::tidx(ptr);
        self.fold(i);
        if i != NILX {
            self.w(Self::obj(i as usize) + O_PRESENT, 0);
            self.w(Self::obj(i as usize) + O_FREED, 1);
        }
    }

    fn free_one(&mut self, which: usize, obj: u64) {
        if !self.ht_ref(which).rhlist {
            let o = self.rht_obj(self.ht_ref(which), obj);
            self.free_cb(o);
            return;
        }
        let mut list = obj; /* container_of */
        loop {
            let o = self.rht_obj(self.ht_ref(which), list);
            let nxt = self.r(list + 8);
            self.free_cb(o);
            if nxt == 0 {
                break;
            }
            list = nxt;
        }
    }

    fn free_and_destroy(&mut self, which: usize, free_fn: bool) {
        /* irq_work_sync: no-op; cancel_work_sync clears run_work */
        self.ht_mut(which).run_work_queued = false;
        let mut tbl = self.ht_ref(which).tbl;
        loop {
            if free_fn {
                let size = self.tbl_size(tbl);
                for i in 0..size {
                    let bkt = self.rht_bucket(tbl, i);
                    let mut pos = self.rht_ptr(bkt);
                    let mut next = if !is_a_nulls(pos) { self.r(pos) } else { 0 };
                    while !is_a_nulls(pos) {
                        self.free_one(which, pos);
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

    /* ================= dumps / op helpers ================= */

    fn fold_chain(&mut self, tbl: u64, b: u64, rhlist: bool) {
        let bkt = self.rht_bucket(tbl, b);
        let mut pos = self.rht_ptr(bkt);
        while !is_a_nulls(pos) {
            self.fold(pos); /* canon(pos) == pos in canon space */
            if rhlist {
                let mut l = pos;
                while l != 0 {
                    self.fold(0x1a);
                    self.fold(l);
                    l = self.r(l + 8);
                }
                self.fold(0x1b);
            }
            pos = self.r(pos);
        }
        self.fold(pos); /* terminating marker */
    }

    fn dump_ht(&mut self, which: usize) {
        self.fold(0xd09e0000);
        self.fold(self.ht_ref(which).nelems as u64);
        let mut tbl = self.ht_ref(which).tbl;
        let mut k = 0;
        while tbl != 0 && k < 8 {
            self.fold(tbl);
            self.fold(self.tbl_size(tbl));
            self.fold(self.tbl_nest(tbl));
            self.fold(self.ts(tbl, T_HASH_RND));
            self.fold(self.list_count(tbl + T_WNEXT));
            let size = self.tbl_size(tbl);
            for b in 0..size {
                self.fold_chain(tbl, b, self.ht_ref(which).rhlist);
            }
            tbl = self.tbl_future(tbl);
            k += 1;
        }
        self.fold(0xd09e1111);
    }

    fn dump_iters(&mut self) {
        self.fold(0x1e7e0000);
        for k in 0..NI {
            self.fold(self.iters[k].slot as u64);
            self.fold(self.iters[k].skip as u64);
            self.fold(self.iters[k].end_of_table as u64);
            self.fold(self.iters[k].p);
            self.fold(self.iters[k].list);
            self.fold(self.wtbl(k));
            self.fold(self.iter_entered[k] as u64);
            self.fold(self.walk_on[k] as u64);
        }
    }

    fn mk_params(&mut self, v: u64, rhlist: bool) -> Params {
        let hints = [0u16, 24, 48, 64];
        let keylens = [8u16, 5, 4, 8];
        let maxsz = [0u32, 64, 256, 4096];
        let minsz = [0u16, 4, 8, 16];
        let m = v % 64;
        let mut p = Params {
            nelem_hint: hints[(v % 4) as usize],
            key_len: keylens[((v >> 2) % 4) as usize],
            key_offset: KEY_OFF as u16,
            head_offset: HEAD_OFF as u16,
            max_size: maxsz[((v >> 4) % 4) as usize],
            min_size: minsz[((v >> 6) % 4) as usize],
            insecure_elasticity: (v & 0x200) != 0,
            automatic_shrinking: (v & 0x100) != 0,
            ..Default::default()
        };
        if v & 0x400 != 0 {
            p.hashfn = Some(HashFn::Custom);
        }
        if rhlist || m % 5 == 3 {
            p.obj_hashfn = Some(if v & 0x800 != 0 {
                ObjHash::Bad
            } else {
                ObjHash::Good
            });
            p.obj_cmpfn = Some(if v & 0x1000 != 0 {
                ObjCmp::Lo32
            } else {
                ObjCmp::Eq
            });
        }
        p
    }

    fn pick_key(&mut self) -> u64 {
        let r = self.xs64();
        if (r & 3) == 0 {
            let c = self.count_present();
            if c != 0 {
                let i = self.nth_present((r >> 8) % c);
                if i >= 0 {
                    return self.pool_key(i as usize);
                }
            }
        }
        self.xs64() % KEYSPACE
    }

    fn prep_obj(&mut self, key: u64) -> i64 {
        let i = self.free_slot();
        if i < 0 {
            return -1;
        }
        let i = i as usize;
        let meta = self.xs64();
        self.w(Self::obj(i) + O_META, meta);
        self.w(Self::obj(i) + O_KEY, key);
        let tag = self.xs64() % 1000;
        self.w(Self::obj(i) + O_TAG, tag);
        i as i64
    }

    fn set_present(&mut self, i: usize, v: bool) {
        self.w(Self::obj(i) + O_PRESENT, v as u64);
    }

    /* scratch cell for passing a u64 key by address */
    const KEYCELL: u64 = 0x66_0000_0000;
    fn store_key(&mut self, k: u64) -> u64 {
        self.w(Self::KEYCELL, k);
        Self::KEYCELL
    }
    fn key_addr_of(&mut self, i: usize) -> u64 {
        Self::obj(i) + O_KEY
    }

    /* ================= op_walk (shared) ================= */

    fn op_walk(&mut self, which: usize, r: u64) {
        let k = ((r >> 16) % NI as u64) as usize;
        let mut q = (r >> 8) % 8;

        if self.iter_entered[k] == 0 && q != 0 {
            q = 0;
        }
        match q {
            0 => {
                if self.iter_entered[k] != 0 {
                    self.fold(0xec);
                } else {
                    self.walk_enter(which, k);
                    self.iter_entered[k] = 1;
                    self.walk_on[k] = 0;
                    self.fold(0xec00);
                }
            }
            1 => {
                if self.walk_on[k] != 0 {
                    self.walk_stop(k);
                }
                self.walk_exit(k);
                self.iter_entered[k] = 0;
                self.walk_on[k] = 0;
                self.fold(0xec01);
            }
            2 => {
                if self.walk_on[k] != 0 {
                    self.fold(0xec02);
                } else {
                    self.fold(0xec03);
                    let r_ = self.walk_start_check(k);
                    self.fold(r_ as u64);
                    self.walk_on[k] = 1;
                }
            }
            3 => {
                if self.walk_on[k] != 0 {
                    self.fold(0xec04);
                } else {
                    self.walk_start(k);
                    self.walk_on[k] = 1;
                    self.fold(0xec05);
                }
            }
            4 => {
                if self.walk_on[k] == 0 {
                    self.fold(0xec06);
                } else {
                    let o = self.walk_next(k);
                    self.fold(0xec07);
                    if is_err(o) {
                        self.fold(ptr_err(o) as u64);
                    } else {
                        self.fold(o);
                    }
                }
            }
            5 => {
                if self.walk_on[k] == 0 {
                    self.fold(0xec08);
                } else {
                    let o = self.walk_peek(k);
                    self.fold(0xec09);
                    if is_err(o) {
                        self.fold(ptr_err(o) as u64);
                    } else {
                        self.fold(o);
                    }
                }
            }
            6 => {
                if self.walk_on[k] == 0 {
                    self.fold(0xec0a);
                } else {
                    self.walk_stop(k);
                    self.walk_on[k] = 0;
                    self.fold(0xec0b);
                }
            }
            _ => {
                if self.iter_entered[k] == 0 {
                    self.fold(0xec0c);
                } else {
                    if self.walk_on[k] != 0 {
                        self.walk_stop(k);
                    }
                    self.fold(0xec0d);
                    let r_ = self.walk_start_check(k);
                    self.fold(r_ as u64);
                    self.walk_on[k] = 1;
                }
            }
        }
        self.dump_iters();
    }
}

/* ================= ilog2 / pow2 helpers ================= */

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

fn iabs(v: i64) -> u64 {
    v.unsigned_abs()
}

/* ================= the five wrap entry points ================= */

pub fn num_possible_cpus() -> std::ffi::c_uint {
    8
}

pub fn wrap_rht_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    let nops = 40 + (iabs(nops_i) % 480) as usize;
    let mut s = Sim::new();
    s.reset_all(seed, fail_i);
    let pv = s.xs64();
    s.g_par = s.mk_params(pv, false);
    let par = s.g_par;
    let ret = s.rhashtable_init(0, &par, false);
    s.fold(ret as u64);
    s.ht_live = true;
    s.fold(s.ht.nelems as u64);
    {
        let t = s.ht.tbl;
        s.fold(if t != 0 { s.tbl_size(t) } else { 0 });
    }

    for i in 0..nops {
        let r = s.xs64();
        let t = (r % 100) as i64;

        if !s.ht_live {
            let pv = s.xs64();
            s.g_par = s.mk_params(pv, false);
            let par = s.g_par;
            let ret = s.rhashtable_init(0, &par, false);
            s.fold(ret as u64);
            s.ht_live = true;
            continue;
        }

        if t < 22 {
            let kv = s.xs64() % KEYSPACE;
            let idx = s.prep_obj(kv);
            if idx < 0 {
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = s.rhashtable_insert_fast(0, obj);
                s.set_present(idx as usize, ret == 0);
                s.fold(ret as u64);
                s.fold(idx as u64);
            }
        } else if t < 40 {
            let k = s.pick_key();
            let idx = s.prep_obj(k);
            if idx < 0 {
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = if s.g_par.obj_hashfn.is_some() {
                    let ka = s.key_addr_of(idx as usize);
                    s.rhashtable_lookup_insert_key(0, ka, obj)
                } else {
                    s.rhashtable_lookup_insert_fast(0, obj)
                };
                s.set_present(idx as usize, ret == 0);
                s.fold(ret as u64);
                s.fold(idx as u64);
            }
        } else if t < 50 {
            let k = s.pick_key();
            let idx = s.prep_obj(k);
            if idx < 0 {
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = if s.g_par.obj_hashfn.is_some() {
                    let ka = s.key_addr_of(idx as usize);
                    s.rhashtable_lookup_get_insert_key(0, ka, obj)
                } else {
                    s.rhashtable_lookup_get_insert_fast(0, obj)
                };
                s.set_present(idx as usize, !is_err(ret) && ret == 0);
                if is_err(ret) {
                    s.fold(ptr_err(ret) as u64);
                } else {
                    s.fold(ret);
                }
                s.fold(idx as u64);
            }
        } else if t < 60 {
            let c = s.count_present();
            let idx = if c != 0 && (r & 0x300) != 0 {
                s.nth_present((r >> 10) % c)
            } else {
                ((r >> 10) % N as u64) as i64
            };
            if idx < 0 || !s.pool_present(idx as usize) || s.pool_freed(idx as usize) {
                let dead = ((r >> 20) % N as u64) as usize;
                let obj = Sim::obj(dead) + HEAD_OFF;
                let ret = s.rhashtable_remove_fast(0, obj);
                s.fold(ret as u64);
                if ret == 0 {
                    s.set_present(dead, false);
                }
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = s.rhashtable_remove_fast(0, obj);
                s.fold(ret as u64);
                s.fold(idx as u64);
                s.set_present(idx as usize, false);
            }
        } else if t < 65 {
            let c = s.count_present();
            let ni = s.free_slot();
            if c == 0 || ni < 0 {
                s.fold(NILX);
            } else {
                let v = s.nth_present((r >> 10) % c);
                let ni = ni as usize;
                let v = v as usize;
                let meta = s.xs64();
                s.w(Sim::obj(ni) + O_META, meta);
                let key = if (r & 0x800) != 0 {
                    s.xs64() % KEYSPACE
                } else {
                    s.pool_key(v)
                };
                s.w(Sim::obj(ni) + O_KEY, key);
                let tag = s.xs64() % 1000;
                s.w(Sim::obj(ni) + O_TAG, tag);
                let ret =
                    s.rhashtable_replace_fast(0, Sim::obj(v) + HEAD_OFF, Sim::obj(ni) + HEAD_OFF);
                s.fold(ret as u64);
                s.fold(v as u64);
                s.fold(ni as u64);
                if ret == 0 {
                    s.set_present(v, false);
                    s.set_present(ni, true);
                }
            }
        } else if t < 72 {
            let k = s.xs64() % KEYSPACE;
            let ka = s.store_key(k);
            let o = match (r >> 8) % 3 {
                0 => s.rhashtable_lookup_fast_(0, ka),
                1 => s.rhashtable_lookup(0, ka),
                _ => s.rhashtable_lookup_likely(0, ka),
            };
            s.fold(o);
            s.fold(Sim::tidx(o));
        } else if t < 77 {
            let mut pk = 0u64;
            let mut steps = 0u64;
            let c = s.count_present();
            if c != 0 && (r & 0x400) != 0 {
                let si = s.nth_present((r >> 12) % c);
                pk = Sim::obj(si as usize) + O_KEY;
                s.fold(si as u64);
            } else {
                s.fold(NILX);
            }
            loop {
                let o = s.rhashtable_next_key(0, pk);
                if is_err(o) {
                    s.fold(ptr_err(o) as u64);
                    break;
                }
                s.fold(o);
                if o == 0 {
                    break;
                }
                pk = o + KEY_OFF;
                steps += 1;
                if steps >= 12 {
                    break;
                }
            }
            s.fold(steps);
        } else if t < 87 {
            s.op_walk(0, r);
        } else if t < 93 {
            s.drain_work(0);
        } else if t < 96 {
            for k in 0..NI {
                if s.walk_on[k] != 0 {
                    s.walk_stop(k);
                }
                if s.iter_entered[k] != 0 {
                    s.walk_exit(k);
                }
                s.iter_entered[k] = 0;
                s.walk_on[k] = 0;
            }
            s.fold(0xde5700);
            s.free_and_destroy(0, true);
            s.ht_live = false;
            s.fold(0xde5701);
        } else {
            let k = KEYSPACE + (s.xs64() % 64);
            let ka = s.store_key(k);
            let o = s.rhashtable_lookup_fast_(0, ka);
            s.fold(o);
            s.fold(s.ht.nelems as u64);
        }

        s.fold(s.ht.nelems as u64);
        if i % 33 == 32 {
            s.dump_ht(0);
        }
    }

    s.fold(0xe9d0000);
    s.dump_ht(0);
    s.fold(0xe9d1111);
    s.free_and_destroy(0, true);
    s.ht_live = false;
    s.g_h
}

pub fn wrap_rhlt_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    let nops = 40 + (iabs(nops_i) % 480) as usize;
    let mut s = Sim::new();
    s.reset_all(seed, fail_i);
    let pv = s.xs64();
    s.g_par = s.mk_params(pv, true);
    let par = s.g_par;
    let ret = s.rhashtable_init(1, &par, true);
    s.fold(ret as u64);
    s.hlt_live = true;
    {
        let t = s.hlt.tbl;
        s.fold(if t != 0 { s.tbl_size(t) } else { 0 });
    }

    for i in 0..nops {
        let r = s.xs64();
        let t = (r % 100) as i64;

        if !s.hlt_live {
            let pv = s.xs64();
            s.g_par = s.mk_params(pv, true);
            let par = s.g_par;
            let ret = s.rhashtable_init(1, &par, true);
            s.fold(ret as u64);
            s.hlt_live = true;
            continue;
        }

        if t < 40 {
            let k = s.pick_key();
            let idx = s.prep_obj(k);
            if idx < 0 {
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = if (r & 0x400) != 0 {
                    s.rhltable_insert(obj)
                } else {
                    let ka = s.key_addr_of(idx as usize);
                    s.rhltable_insert_key(ka, obj)
                };
                s.set_present(idx as usize, ret == 0);
                s.fold(ret as u64);
                s.fold(idx as u64);
            }
        } else if t < 55 {
            let c = s.count_present();
            let idx = if c != 0 && (r & 0x300) != 0 {
                s.nth_present((r >> 10) % c)
            } else {
                ((r >> 10) % N as u64) as i64
            };
            if idx < 0 || !s.pool_present(idx as usize) || s.pool_freed(idx as usize) {
                let dead = ((r >> 20) % N as u64) as usize;
                let obj = Sim::obj(dead) + HEAD_OFF;
                let ret = s.rhltable_remove(obj);
                s.fold(ret as u64);
                s.fold(NILX);
            } else {
                let obj = Sim::obj(idx as usize) + HEAD_OFF;
                let ret = s.rhltable_remove(obj);
                s.fold(ret as u64);
                s.fold(idx as u64);
                s.set_present(idx as usize, false);
            }
        } else if t < 70 {
            let k = s.xs64() % KEYSPACE;
            s.rhl_fold_lookup(k);
        } else if t < 77 {
            s.op_walk(1, r);
        } else if t < 83 {
            s.drain_work(1);
        } else if t < 88 {
            for k in 0..NI {
                if s.walk_on[k] != 0 {
                    s.walk_stop(k);
                }
                if s.iter_entered[k] != 0 {
                    s.walk_exit(k);
                }
                s.iter_entered[k] = 0;
                s.walk_on[k] = 0;
            }
            s.fold(0xde5702);
            s.free_and_destroy(1, true);
            s.hlt_live = false;
            s.fold(0xde5703);
        } else {
            let k = if (r & 0x800) != 0 {
                s.pick_key()
            } else {
                s.xs64() % KEYSPACE
            };
            s.rhl_fold_lookup(k);
            {
                let ka = s.store_key(k);
                let l2 = s.rhltable_lookup_likely(ka);
                s.fold(l2);
            }
        }

        s.fold(s.hlt.nelems as u64);
        if i % 33 == 32 {
            s.dump_ht(1);
        }
    }

    s.fold(0xe9d2222);
    s.dump_ht(1);
    s.free_and_destroy(1, true);
    s.hlt_live = false;
    s.g_h
}

impl Sim {
    fn rhl_fold_lookup(&mut self, k: u64) {
        let ka = self.store_key(k);
        let l = self.rhltable_lookup(ka);
        self.fold(l);
        let mut c = 0u64;
        if l != 0 {
            let mut pos = l;
            while pos != 0 {
                self.fold(pos);
                let key = self.r(pos.wrapping_sub(8) + O_KEY); /* to = pos-8; to->key */
                self.fold(key);
                c += 1;
                if c == 8 {
                    break;
                }
                pos = self.r(pos + 8);
            }
        }
        self.fold(c);
    }
}

pub fn wrap_rht_walk(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    let n = 4 + (iabs(n_i) % 60) as usize;
    let mut s = Sim::new();
    s.reset_all(seed, fail_i);
    let pv = s.xs64();
    s.g_par = s.mk_params(pv, false);
    s.g_par.obj_hashfn = None;
    s.g_par.obj_cmpfn = None;
    let par = s.g_par;
    let ret = s.rhashtable_init(0, &par, false);
    s.fold(ret as u64);
    s.ht_live = true;

    for _ in 0..n {
        let kv = s.xs64() % KEYSPACE;
        let idx = s.prep_obj(kv);
        if idx < 0 {
            break;
        }
        let obj = Sim::obj(idx as usize) + HEAD_OFF;
        let ret = s.rhashtable_lookup_insert_fast(0, obj);
        s.set_present(idx as usize, ret == 0);
        s.fold(ret as u64);
    }
    s.drain_work(0);
    s.dump_ht(0);

    s.walk_enter(0, 0);
    s.iter_entered[0] = 1;
    for _ in 0..48 {
        let r = s.xs64();
        match r % 6 {
            0 => {
                if s.walk_on[0] == 0 {
                    let r_ = s.walk_start_check(0);
                    s.fold(r_ as u64);
                    s.walk_on[0] = 1;
                } else {
                    let o = s.walk_next(0);
                    s.fold(if is_err(o) { ptr_err(o) as u64 } else { o });
                }
            }
            1 => {
                if s.walk_on[0] != 0 {
                    let o = s.walk_peek(0);
                    s.fold(if is_err(o) { ptr_err(o) as u64 } else { o });
                } else {
                    s.fold(NILX);
                }
            }
            2 => {
                if s.walk_on[0] != 0 {
                    s.walk_stop(0);
                    s.walk_on[0] = 0;
                }
                s.fold(0x57);
            }
            3 => {
                let c = s.count_present();
                if (r & 0x400) != 0 || c == 0 {
                    let kv = s.xs64() % KEYSPACE;
                    let idx = s.prep_obj(kv);
                    if idx >= 0 {
                        let obj = Sim::obj(idx as usize) + HEAD_OFF;
                        let ret = s.rhashtable_lookup_insert_fast(0, obj);
                        s.set_present(idx as usize, ret == 0);
                        s.fold(ret as u64);
                    }
                } else {
                    let v = s.nth_present((r >> 12) % c);
                    let obj = Sim::obj(v as usize) + HEAD_OFF;
                    let ret = s.rhashtable_remove_fast(0, obj);
                    s.fold(ret as u64);
                    s.set_present(v as usize, false);
                    s.fold(v as u64);
                }
            }
            4 => {
                s.drain_work(0);
            }
            _ => {
                s.dump_iters();
            }
        }
    }
    if s.walk_on[0] != 0 {
        s.walk_stop(0);
    }
    s.walk_exit(0);
    s.iter_entered[0] = 0;
    s.dump_ht(0);
    s.free_and_destroy(0, true);
    s.g_h
}

pub fn wrap_rht_elastic(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    let n = 4 + (iabs(n_i) % 920) as usize;
    let mut s = Sim::new();
    s.reset_all(seed, fail_i);
    let mode = (s.xs64() % 3) as i64;
    let mut p = Params {
        key_len: 8,
        key_offset: KEY_OFF as u16,
        head_offset: HEAD_OFF as u16,
        min_size: 4,
        ..Default::default()
    };
    match mode {
        0 => {
            p.obj_hashfn = Some(ObjHash::Bad);
            p.obj_cmpfn = Some(ObjCmp::Eq);
            p.max_size = 2048;
        }
        1 => {
            p.obj_hashfn = Some(ObjHash::Bad);
            p.obj_cmpfn = Some(ObjCmp::Eq);
            p.insecure_elasticity = true;
        }
        _ => {
            p.max_size = 16;
        }
    }
    s.g_par = p;
    s.fold(mode as u64);
    let ret = s.rhashtable_init(0, &p, false);
    s.fold(ret as u64);
    s.ht_live = true;
    {
        let t = s.ht.tbl;
        s.fold(if t != 0 { s.tbl_size(t) } else { 0 });
    }

    for i in 0..n {
        let r = s.xs64();
        let kv = r % KEYSPACE;
        let idx = s.prep_obj(kv);
        if idx < 0 {
            s.fold(NILX);
            break;
        }
        let obj = Sim::obj(idx as usize) + HEAD_OFF;
        let ret = if s.g_par.obj_hashfn.is_some() {
            let ka = s.key_addr_of(idx as usize);
            s.rhashtable_lookup_insert_key(0, ka, obj)
        } else {
            s.rhashtable_lookup_insert_fast(0, obj)
        };
        s.set_present(idx as usize, ret == 0);
        s.fold(ret as u64);
        s.fold(s.ht.nelems as u64);
        if i % 9 == 8 {
            s.drain_work(0);
        }
        if i % 17 == 16 {
            let c = s.count_present();
            if c != 0 {
                let v = s.nth_present(r % c);
                let obj = Sim::obj(v as usize) + HEAD_OFF;
                let ret = s.rhashtable_remove_fast(0, obj);
                s.fold(ret as u64);
                s.set_present(v as usize, false);
            }
        }
    }
    s.drain_work(0);
    s.dump_ht(0);
    s.fold(0xe9d3333);
    s.free_and_destroy(0, true);
    s.g_h
}

pub fn wrap_bkl(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    let n = 1 + (iabs(n_i) % 8) as usize;
    let mut s = Sim::new();
    s.reset_all(seed, fail_i);

    for _ in 0..n {
        let r = s.xs64();
        let max_size = (r >> 8) % 1025;
        let cpu_mult = ((r >> 24) % 5) as u32;
        let gfp = if (r & 1) != 0 { GFP_KERNEL } else { GFP_ATOMIC };
        /* __alloc_bucket_spinlocks */
        let size: u64 = if cpu_mult != 0 {
            let nr = 8u64;
            (nr * cpu_mult as u64).min(max_size)
        } else {
            max_size
        };
        let tlocks = s.kalloc(4 * size, gfp); /* sizeof(spinlock_t)==4 */
        let ret: i64;
        let mut mask: u32 = 0;
        let mut locks = 0u64;
        if tlocks == 0 {
            ret = -ENOMEM;
        } else {
            locks = tlocks;
            mask = (size as u32).wrapping_sub(1);
            ret = 0;
        }
        s.fold(ret as u64);
        s.fold(mask as u64);
        if ret == 0 {
            let sz = mask.wrapping_add(1);
            let mut k = ((r >> 32) % 3) as i64 + 1;
            while k > 0 && sz != 0 {
                k -= 1;
                let b = s.xs64() % sz as u64;
                let _ = b; /* spin_lock/unlock: no-ops */
            }
            s.fold(locks);
            s.kfree(locks);
        }
        s.fold(s.alloc_ord);
    }
    s.g_h
}
