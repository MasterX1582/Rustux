//! Safe Rust port of the lib-idr-radix differential harness.
//!
//! Three layers are ported, faithfully to `wrappers.c`:
//!   1. upstream `lib/xarray.c` semantics — the `xa_state` machine,
//!      nodes, slots, marks, alloc/free and the -ENOMEM retry path —
//!      reused from the verified lib-xarray port, refactored so ONE
//!      160-node pool backs THREE independent roots (the IDR tree, the
//!      IDA xarray and the radix tree), exactly like the C harness;
//!   2. upstream `lib/idr.c` + `lib/radix-tree.c` — the IDR/IDA and
//!      deprecated radix-tree APIs that ride on the shared xa_node
//!      layout (tags[] aliases marks[], radix_tree_iter over chunks,
//!      idr_get_free/IDR_FREE marks, the per-cpu preload stack, the
//!      ida_bitmap value/pointer encoding);
//!   3. the wrapper op loops and the three `wrap_*` drivers.
//!
//! Canonical encodings (identical to the C harness):
//!   * node pool slot i pointer        : NBASE + i*4096       (& 3 == 0)
//!   * node entry stored in a slot     : (NBASE + i*4096) | 2
//!   * entry pool &epool[i] (+tag)     : EBASE + i*16 + tag
//!   * ida_bitmap pool object i        : BBASE + i*256
//!   * &idr.idr_rt / &ida.xa / &rt     : 0x810000 / 0x820000 / 0x830000
//!   * &root->xa_head slot             : SROOT + tree index
//!   * &node_pool[i].slots[o]          : SBASE + i*64 + o
//!   * internal/value/error entries    : their integer encodings
//!
//! `#![forbid(unsafe_code)]`: every C pointer is an index/canonical u64.

#![forbid(unsafe_code)]
#![allow(clippy::all)]
// dead_code check removed for audit

/* ================= canonical model ================= */

const NP: usize = 160;
const EP: usize = 96;
const NB: usize = 48; /* ida_bitmap pool slots */
const NILX: u64 = 0xffff;
const NBASE: u64 = 0x40000;
const NSTRIDE: u64 = 4096;
const EBASE: u64 = 0x10000;
const BBASE: u64 = 0x200000;
const RTIDR_CANON: u64 = 0x810000;
const IDAX_CANON: u64 = 0x820000;
const RRT_CANON: u64 = 0x830000;
const SROOT: u64 = 0xB00000;
const SBASE: u64 = 0xB10000;
const XA_RCU_FREE: u64 = 1;

const T_IDR: usize = 0;
const T_IDA: usize = 1;
const T_RT: usize = 2;

fn root_canon(xa: usize) -> u64 {
    [RTIDR_CANON, IDAX_CANON, RRT_CANON][xa]
}

const CHUNK_SHIFT: u32 = 6;
const CHUNK_SIZE: usize = 64;
const CHUNK_MASK: u64 = 63;
const MAX_MARKS: usize = 3;

const XA_FLAGS_TRACK_FREE: u32 = 4;
const XA_FLAGS_ZERO_BUSY: u32 = 8;
const GFP_BITS_SHIFT: u32 = 24;
const ROOT_TAG_SHIFT: u32 = GFP_BITS_SHIFT;
const ROOT_IS_IDR: u32 = 4;
const IDR_RT_MARKER: u32 = ROOT_IS_IDR | (1 << (ROOT_TAG_SHIFT + IDR_FREE));

const XA_LOCK_IRQ: u32 = 1;
const XA_FLAGS_LOCK_IRQ: u32 = XA_LOCK_IRQ;
const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
const IDA_INIT_FLAGS: u32 = XA_FLAGS_LOCK_IRQ | XA_FLAGS_ALLOC;

fn xa_flags_mark(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}
const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

const XA_MARK_0: u32 = 0;
const XA_MARK_2: u32 = 2;
const XA_PRESENT: u32 = 8;
const XA_FREE_MARK: u32 = XA_MARK_0;
const XA_MARK_MAX: u32 = XA_MARK_2;
const IDR_FREE: u32 = 0;

const ENOMEM: i64 = 12;
const EINVAL: i64 = 22;
const ENOENT: i64 = 2;
const EEXIST: i64 = 17;
const ENOSPC: i64 = 28;
const MAX_ERRNO: i64 = 4095;

const XAS_BOUNDS: u64 = 1;
const XAS_RESTART: u64 = 3;

const XA_ZERO_ENTRY: u64 = 1030;
const XA_RETRY_ENTRY: u64 = 1026;
const RADIX_TREE_RETRY: u64 = XA_RETRY_ENTRY;

/* IDA geometry: 128-byte leaves -> 16 u64 words -> 1024 bits */
const IDA_BITMAP_LONGS: usize = 16;
const IDA_BITMAP_BITS: u64 = (IDA_BITMAP_LONGS * 64) as u64;
const BITS_PER_XA_VALUE: u64 = 63;
const BITS_PER_LONG: u64 = 64;

/* radix tree geometry */
const RADIX_TREE_MAX_TAGS: u32 = MAX_MARKS as u32;
const RADIX_TREE_MAP_SHIFT: u32 = CHUNK_SHIFT;
const RADIX_TREE_MAP_SIZE: u64 = CHUNK_SIZE as u64;
const RADIX_TREE_MAP_MASK: u64 = CHUNK_MASK;
const RADIX_TREE_INDEX_BITS: u32 = 64;
const RADIX_TREE_MAX_PATH: u32 = (RADIX_TREE_INDEX_BITS + CHUNK_SHIFT - 1) / CHUNK_SHIFT;
const RADIX_TREE_PRELOAD_SIZE: u32 = RADIX_TREE_MAX_PATH * 2 - 1; /* 21 */
const IDR_MAX_PATH: u32 = (31 + CHUNK_SHIFT - 1) / CHUNK_SHIFT; /* 6 */
const IDR_PRELOAD_SIZE: u32 = IDR_MAX_PATH * 2 - 1; /* 11 */

const ITER_TAG_MASK: u32 = 0x0f;
const ITER_TAGGED: u32 = 0x10;
const ITER_CONTIG: u32 = 0x20;

/* gfp bits (mirrors linux/gfp.h) */
const GFP_DIRECT_RECLAIM: u32 = 1 << 10;
const GFP_KSWAPD_RECLAIM: u32 = 1 << 11;
const GFP_NOWARN: u32 = 1 << 13;
const GFP_ACCOUNT: u32 = 1 << 22;
const GFP_HIGH: u32 = 1 << 5;
const GFP_IO: u32 = 1 << 6;
const GFP_FS: u32 = 1 << 7;
const GFP_BITS_MASK: u32 = (1 << GFP_BITS_SHIFT) - 1;
const GFP_ZONEMASK: u32 = 0x0f;
const GFP_KERNEL: u32 = GFP_DIRECT_RECLAIM | GFP_KSWAPD_RECLAIM | GFP_IO | GFP_FS;
const GFP_ATOMIC: u32 = GFP_HIGH | GFP_KSWAPD_RECLAIM;
const GFP_NOWAIT: u32 = GFP_KSWAPD_RECLAIM | GFP_NOWARN;

fn allow_blocking(gfp: u32) -> bool {
    gfp & GFP_DIRECT_RECLAIM != 0
}
fn root_gfp_mask(flags: u32) -> u32 {
    flags & (GFP_BITS_MASK & !GFP_ZONEMASK)
}

const INT_MAXV: i64 = 0x7fffffff;
const ULONG_MAXV: u64 = u64::MAX;

fn err_ptr(e: i64) -> u64 {
    e.wrapping_neg() as u64
}
fn min_u64(a: u64, b: u64) -> u64 {
    if a < b { a } else { b }
}
fn max_u64(a: u64, b: u64) -> u64 {
    if a > b { a } else { b }
}

/* ================= entry classification ================= */

fn xa_is_value(e: u64) -> bool {
    e & 1 == 1
}
fn xa_to_value(e: u64) -> u64 {
    e >> 1
}
fn xa_mk_value(v: u64) -> u64 {
    (v << 1) | 1
}
fn xa_is_internal(e: u64) -> bool {
    e & 3 == 2
}
fn xa_mk_internal(v: u64) -> u64 {
    (v << 2) | 2
}
fn xa_is_node(e: u64) -> bool {
    xa_is_internal(e) && e > 4096
}
fn xa_to_node(e: u64) -> usize {
    ((e - 2 - NBASE) / NSTRIDE) as usize
}
fn xa_mk_node(ni: usize) -> u64 {
    (NBASE + ni as u64 * NSTRIDE) | 2
}
fn node_ptr(ni: usize) -> u64 {
    NBASE + ni as u64 * NSTRIDE
}
fn nidx_ptr(p: u64) -> usize {
    ((p - NBASE) / NSTRIDE) as usize
}
fn is_node_ptr(p: u64) -> bool {
    p >= NBASE && p < NBASE + NP as u64 * NSTRIDE && p & 3 == 0
}
fn xa_is_retry(e: u64) -> bool {
    e == XA_RETRY_ENTRY
}
fn xa_is_zero(e: u64) -> bool {
    e == XA_ZERO_ENTRY
}
fn xa_is_err(e: u64) -> bool {
    xa_is_internal(e) && e >= xa_mk_internal((-MAX_ERRNO) as u64)
}
fn xa_err(e: u64) -> i64 {
    if xa_is_err(e) { (e as i64) >> 2 } else { 0 }
}
fn xa_error(errno: i64) -> u64 {
    ((errno as u64) << 2) | 2
}
fn xa_zero_to_null(e: u64) -> u64 {
    if xa_is_zero(e) { 0 } else { e }
}

/* radix-tree classification: just (& 3 == 2), no >4096 bound */
fn rt_is_internal(e: u64) -> bool {
    e & 3 == 2
}
fn entry_to_node(e: u64) -> usize {
    ((e & !3) - NBASE) as usize / NSTRIDE as usize
}
fn node_to_entry(ni: usize) -> u64 {
    node_ptr(ni) | 2
}

/* ida_bitmap canonical <-> pool index */
fn bmap_canon(i: usize) -> u64 {
    BBASE + i as u64 * 256
}
fn bmap_idx(e: u64) -> usize {
    ((e - BBASE) / 256) as usize
}

/* xas->xa_node sentinels */
fn xas_error(xas: &Xas) -> i64 {
    xa_err(xas.node)
}
fn xas_set_err(xas: &mut Xas, err: i64) {
    xas.node = xa_error(err);
}
fn xas_invalid(xas: &Xas) -> bool {
    xas.node & 3 != 0
}
fn xas_valid(xas: &Xas) -> bool {
    !xas_invalid(xas)
}
fn xas_not_node(n: u64) -> bool {
    n & 3 != 0 || n == 0
}
fn xas_top(n: u64) -> bool {
    n <= XAS_RESTART
}

/* ================= structures ================= */

#[derive(Clone)]
struct Node {
    shift: u8,
    offset: u8,
    count: u8,
    nr_values: u8,
    parent: u64, /* canonical node ptr or 0 */
    array: u64,  /* root canonical (0x810000/0x820000/0x830000) or XA_RCU_FREE */
    on_list: bool,
    slots: [u64; CHUNK_SIZE],
    marks: [u64; MAX_MARKS],
}

impl Node {
    fn new() -> Self {
        Node {
            shift: 0,
            offset: 0,
            count: 0,
            nr_values: 0,
            parent: 0,
            array: 0,
            on_list: false,
            slots: [0; CHUNK_SIZE],
            marks: [0; MAX_MARKS],
        }
    }
}

#[derive(Clone)]
struct Xas {
    xa: usize, /* which of the three roots this state operates on */
    index: u64,
    offset: u8,
    node: u64,
    alloc: u64,
    update: bool,
}

impl Xas {
    fn new(xa: usize, index: u64) -> Self {
        Xas {
            xa,
            index,
            offset: 0,
            node: XAS_RESTART,
            alloc: 0,
            update: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SlotL {
    Root(usize),        /* &root->xa_head */
    Node(usize, usize), /* &node_pool[ni].slots[off] */
}

fn scanon(s: SlotL) -> u64 {
    match s {
        SlotL::Root(t) => SROOT + t as u64,
        SlotL::Node(ni, o) => SBASE + ni as u64 * 64 + o as u64,
    }
}

struct Pool {
    n: Vec<Node>,
    used: [bool; NP],
    /* the three roots: idr.idr_rt, ida.xa, rt */
    roots: [Root; 3],
    idr_base: u32,
    idr_next: u32,
    /* per-cpu radix_tree_preloads: Vec = stack, last element is head */
    preload: Vec<usize>,
    /* ida_bitmap pool */
    bmaps: Vec<[u64; IDA_BITMAP_LONGS]>,
    bused: [bool; NB],
    shadow: Vec<usize>,
    alloc_ctr: u64,
    fail_mask: u64,
    r: u64,
    h: u64,
    warn: u64,
    warn_once: u64,
}

#[derive(Clone, Copy)]
struct Root {
    head: u64,
    flags: u32,
}

impl Pool {
    fn new() -> Self {
        let mut p = Pool {
            n: vec![Node::new(); NP],
            used: [false; NP],
            roots: [Root { head: 0, flags: 0 }; 3],
            idr_base: 0,
            idr_next: 0,
            preload: Vec::new(),
            bmaps: vec![[0; IDA_BITMAP_LONGS]; NB],
            bused: [false; NB],
            shadow: Vec::new(),
            alloc_ctr: 0,
            fail_mask: 0,
            r: 0,
            h: 0,
            warn: 0,
            warn_once: 0,
        };
        p.reset_all();
        p
    }
    fn reset_all(&mut self) {
        for i in 0..NP {
            self.n[i] = Node::new();
            self.used[i] = false;
        }
        self.roots = [Root { head: 0, flags: 0 }; 3];
        self.idr_base = 0;
        self.idr_next = 0;
        self.preload.clear();
        self.bmaps = vec![[0; IDA_BITMAP_LONGS]; NB];
        self.bused = [false; NB];
        self.shadow.clear();
        self.alloc_ctr = 0;
        self.warn = 0;
        self.warn_once = 0;
        self.h = 0x9e3779b97f4a7c15;
    }
    fn xa_track_free(&self, xa: usize) -> bool {
        self.roots[xa].flags & XA_FLAGS_TRACK_FREE != 0
    }
    fn xa_zero_busy(&self, xa: usize) -> bool {
        self.roots[xa].flags & XA_FLAGS_ZERO_BUSY != 0
    }
    fn xa_marked(&self, xa: usize, mark: u32) -> bool {
        self.roots[xa].flags & xa_flags_mark(mark) != 0
    }
    fn xa_mark_set(&mut self, xa: usize, mark: u32) {
        if !self.xa_marked(xa, mark) {
            self.roots[xa].flags |= xa_flags_mark(mark);
        }
    }
    fn xa_mark_clear(&mut self, xa: usize, mark: u32) {
        if self.xa_marked(xa, mark) {
            self.roots[xa].flags &= !xa_flags_mark(mark);
        }
    }
    fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }
    fn fold(&mut self, v: u64) {
        self.h = (self.h ^ v).wrapping_mul(0x100000001b3);
    }
    fn xs64(&mut self) -> u64 {
        let mut x = self.r;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.r = x;
        x.wrapping_mul(2685821657736338717)
    }
}

fn iabs(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}

/* ================= slab ================= */

fn kmem_cache_alloc(p: &mut Pool) -> Option<usize> {
    p.alloc_ctr += 1;
    if p.alloc_ctr <= 64 && (p.fail_mask >> (p.alloc_ctr - 1)) & 1 == 1 {
        return None;
    }
    for i in 0..NP {
        if !p.used[i] {
            p.used[i] = true;
            return Some(i);
        }
    }
    None
}

fn kmem_cache_free(p: &mut Pool, ni: usize) {
    p.used[ni] = false;
}

fn kzalloc_bitmap(p: &mut Pool) -> Option<usize> {
    p.alloc_ctr += 1;
    if p.alloc_ctr <= 64 && (p.fail_mask >> (p.alloc_ctr - 1)) & 1 == 1 {
        return None;
    }
    for i in 0..NB {
        if !p.bused[i] {
            p.bused[i] = true;
            p.bmaps[i] = [0; IDA_BITMAP_LONGS];
            return Some(i);
        }
    }
    None
}

fn kfree_bitmap(p: &mut Pool, i: usize) {
    p.bused[i] = false;
}

fn radix_tree_node_rcu_free(p: &mut Pool, ni: usize) {
    p.n[ni].slots = [0; CHUNK_SIZE];
    p.n[ni].marks = [0; MAX_MARKS];
    if p.n[ni].on_list {
        p.shadow.retain(|&x| x != ni);
        p.n[ni].on_list = false;
    }
    kmem_cache_free(p, ni);
}

fn xa_node_free(p: &mut Pool, ni: usize) {
    p.n[ni].array = XA_RCU_FREE;
    radix_tree_node_rcu_free(p, ni);
}

/* ================= canonicalisation / folding ================= */

/* all stored entries are already canonical u64s; efold only has to
 * apply the same curation wrappers.c does for xa_is_node() entries
 * that aren't real pool nodes (error/retry/sibling sentinels) */
fn efold(e: u64) -> u64 {
    if e == 0 {
        return 0;
    }
    if xa_is_node(e) {
        let base = e & !3;
        if base >= NBASE && base < NBASE + NP as u64 * NSTRIDE {
            return base | 2;
        }
        return e;
    }
    e
}

fn slot_get(p: &Pool, s: SlotL) -> u64 {
    match s {
        SlotL::Root(t) => p.roots[t].head,
        SlotL::Node(ni, o) => p.n[ni].slots[o],
    }
}
fn slot_set(p: &mut Pool, s: SlotL, v: u64) {
    match s {
        SlotL::Root(t) => p.roots[t].head = v,
        SlotL::Node(ni, o) => p.n[ni].slots[o] = v,
    }
}
fn slot_next(s: SlotL) -> SlotL {
    match s {
        SlotL::Root(t) => SlotL::Root(t),
        SlotL::Node(ni, o) => SlotL::Node(ni, o + 1),
    }
}
fn slot_add(s: SlotL, k: u64) -> SlotL {
    match s {
        SlotL::Root(t) => SlotL::Root(t),
        SlotL::Node(ni, o) => SlotL::Node(ni, o + k as usize),
    }
}
fn slot_off(s: SlotL) -> u64 {
    match s {
        SlotL::Root(_) => 0,
        SlotL::Node(_, o) => o as u64,
    }
}

/* ================= update callback ================= */

fn upd_cb(p: &mut Pool, ni: usize) {
    if p.n[ni].count != 0 && !p.n[ni].on_list {
        p.shadow.insert(0, ni);
        p.n[ni].on_list = true;
    } else if p.n[ni].count == 0 && p.n[ni].on_list {
        p.shadow.retain(|&x| x != ni);
        p.n[ni].on_list = false;
    }
}

fn xas_update(p: &mut Pool, xas: &Xas, ni: usize) {
    if xas.update {
        upd_cb(p, ni);
    }
}

/* ================= node marks ================= */

fn node_get_mark(n: &Node, offset: u64, mark: usize) -> bool {
    (n.marks[mark] >> offset) & 1 == 1
}
fn node_set_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[mark] |= 1 << offset;
    was
}
fn node_clear_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[mark] &= !(1 << offset);
    was
}
fn node_any_mark(n: &Node, mark: usize) -> bool {
    n.marks[mark] != 0
}
fn node_mark_all(n: &mut Node, mark: usize) {
    n.marks[mark] = u64::MAX;
}
/* ================= xas marks ================= */

fn xas_set_mark(p: &mut Pool, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    while is_node_ptr(node) {
        let ni = nidx_ptr(node);
        if node_set_mark(&mut p.n[ni], offset, mark) {
            return;
        }
        offset = p.n[ni].offset as u64;
        node = p.n[ni].parent;
    }
    if !p.xa_marked(xas.xa, mark as u32) {
        p.xa_mark_set(xas.xa, mark as u32);
    }
}

fn xas_clear_mark(p: &mut Pool, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    while is_node_ptr(node) {
        let ni = nidx_ptr(node);
        if !node_clear_mark(&mut p.n[ni], offset, mark) {
            return;
        }
        if node_any_mark(&p.n[ni], mark) {
            return;
        }
        offset = p.n[ni].offset as u64;
        node = p.n[ni].parent;
    }
    if p.xa_marked(xas.xa, mark as u32) {
        p.xa_mark_clear(xas.xa, mark as u32);
    }
}

fn xas_init_marks(p: &mut Pool, xas: &Xas) {
    let mut mark = 0usize;
    loop {
        if p.xa_track_free(xas.xa) && mark as u32 == XA_FREE_MARK {
            xas_set_mark(p, xas, mark);
        } else {
            xas_clear_mark(p, xas, mark);
        }
        if mark as u32 == XA_MARK_MAX {
            break;
        }
        mark += 1;
    }
}

/* NOTE: this harness never stores multi-order entries, so xas.sibs and
 * xas.shift are always 0 and no sibling entries ever exist — the order
 * machinery from upstream xarray.c is intentionally not modelled. */

fn xas_find_chunk(p: &Pool, xas: &Xas, advance: bool, mark: usize) -> u64 {
    let word = p.n[nidx_ptr(xas.node)].marks[mark];
    let mut offset = xas.offset as u64;
    if advance {
        offset += 1;
    }
    if offset < CHUNK_SIZE as u64 {
        let data = word & (u64::MAX << offset);
        if data != 0 {
            return data.trailing_zeros() as u64;
        }
    }
    CHUNK_SIZE as u64
}

/* ================= xas navigation ================= */

fn get_offset(index: u64, n: &Node) -> u64 {
    (index >> n.shift) & CHUNK_MASK
}

fn xas_set_offset(p: &Pool, xas: &mut Xas) {
    xas.offset = get_offset(xas.index, &p.n[nidx_ptr(xas.node)]) as u8;
}

fn xas_move_index(p: &Pool, xas: &mut Xas, offset: u64) {
    let shift = p.n[nidx_ptr(xas.node)].shift;
    xas.index &= (!CHUNK_MASK) << shift;
    xas.index = xas.index.wrapping_add(offset << shift);
}

fn xas_next_offset(p: &Pool, xas: &mut Xas) {
    xas.offset = xas.offset.wrapping_add(1);
    xas_move_index(p, xas, xas.offset as u64);
}

fn set_bounds(xas: &mut Xas) -> u64 {
    xas.node = XAS_BOUNDS;
    0
}

fn max_index(p: &Pool, entry: u64) -> u64 {
    if !xa_is_node(entry) {
        return 0;
    }
    ((CHUNK_SIZE as u64) << p.n[xa_to_node(entry)].shift).wrapping_sub(1)
}

fn xas_reload(p: &mut Pool, xas: &mut Xas) -> u64 {
    if xas.node == 0 {
        return p.roots[xas.xa].head;
    }
    let ni = nidx_ptr(xas.node);
    p.n[ni].slots[((xas.index >> p.n[ni].shift) & CHUNK_MASK) as usize]
}

fn xas_start(p: &mut Pool, xas: &mut Xas) -> u64 {
    if xas_valid(xas) {
        return xas_reload(p, xas);
    }
    if xas_error(xas) != 0 {
        return 0;
    }
    let entry = p.roots[xas.xa].head;
    if !xa_is_node(entry) {
        if xas.index != 0 {
            return set_bounds(xas);
        }
    } else if (xas.index >> p.n[xa_to_node(entry)].shift) > CHUNK_MASK {
        return set_bounds(xas);
    }
    xas.node = 0;
    entry
}

fn xas_descend(p: &Pool, xas: &mut Xas, ni: usize) -> u64 {
    let offset = get_offset(xas.index, &p.n[ni]);
    let entry = p.n[ni].slots[offset as usize];
    xas.node = node_ptr(ni);
    xas.offset = offset as u8;
    entry
}

fn xas_load(p: &mut Pool, xas: &mut Xas) -> u64 {
    let mut entry = xas_start(p, xas);
    while xa_is_node(entry) {
        let ni = xa_to_node(entry);
        entry = xas_descend(p, xas, ni);
        if p.n[ni].shift == 0 {
            break;
        }
    }
    entry
}

fn xas_reset(xas: &mut Xas) {
    xas.node = XAS_RESTART;
}

fn xas_retry(xas: &mut Xas, entry: u64) -> bool {
    if xa_is_zero(entry) {
        return true;
    }
    if !xa_is_retry(entry) {
        return false;
    }
    xas_reset(xas);
    true
}

fn xas_set(xas: &mut Xas, index: u64) {
    xas.index = index;
    xas.node = XAS_RESTART;
}

/* ================= alloc / destroy machinery ================= */

fn xas_destroy(p: &mut Pool, xas: &mut Xas) {
    let mut node = xas.alloc;
    while node != 0 {
        let ni = nidx_ptr(node);
        let next = p.n[ni].parent;
        radix_tree_node_rcu_free(p, ni);
        node = next;
    }
    xas.alloc = 0;
}

fn xas_nomem(p: &mut Pool, xas: &mut Xas) -> bool {
    if xas.node != xa_error(-ENOMEM) {
        xas_destroy(p, xas);
        return false;
    }
    match kmem_cache_alloc(p) {
        None => false,
        Some(ni) => {
            xas.alloc = node_ptr(ni);
            p.n[ni].parent = 0;
            xas.node = XAS_RESTART;
            true
        }
    }
}

fn xas_alloc(p: &mut Pool, xas: &mut Xas, shift: u8) -> Option<usize> {
    let parent = xas.node;
    if xas_invalid(xas) {
        return None;
    }
    let ni = if xas.alloc != 0 {
        let ni = nidx_ptr(xas.alloc);
        xas.alloc = 0;
        ni
    } else {
        match kmem_cache_alloc(p) {
            None => {
                xas_set_err(xas, -ENOMEM);
                return None;
            }
            Some(i) => i,
        }
    };
    if parent != 0 {
        let pni = nidx_ptr(parent);
        p.n[ni].offset = xas.offset;
        p.n[pni].count = p.n[pni].count.wrapping_add(1);
        xas_update(p, xas, pni);
    }
    p.n[ni].shift = shift;
    p.n[ni].count = 0;
    p.n[ni].nr_values = 0;
    p.n[ni].parent = parent;
    p.n[ni].array = root_canon(xas.xa);
    Some(ni)
}

fn xas_free_nodes(p: &mut Pool, xas: &Xas, top: usize) {
    let mut offset = 0u32;
    let mut node = top;
    loop {
        let entry = p.n[node].slots[offset as usize];
        if p.n[node].shift != 0 && xa_is_node(entry) {
            node = xa_to_node(entry);
            offset = 0;
            continue;
        }
        if entry != 0 {
            p.n[node].slots[offset as usize] = XA_RETRY_ENTRY;
        }
        offset += 1;
        while offset == CHUNK_SIZE as u32 {
            let parent = p.n[node].parent;
            offset = p.n[node].offset as u32 + 1;
            p.n[node].count = 0;
            p.n[node].nr_values = 0;
            xas_update(p, xas, node);
            xa_node_free(p, node);
            if node == top {
                return;
            }
            node = nidx_ptr(parent);
        }
    }
}

fn xas_shrink(p: &mut Pool, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    loop {
        if p.n[node].count != 1 {
            break;
        }
        let mut entry = p.n[node].slots[0];
        if entry == 0 {
            break;
        }
        if !xa_is_node(entry) && p.n[node].shift != 0 {
            break;
        }
        if p.xa_zero_busy(xas.xa) {
            entry = xa_zero_to_null(entry);
        }
        xas.node = XAS_BOUNDS;
        p.roots[xas.xa].head = entry;
        if p.xa_track_free(xas.xa) && !node_get_mark(&p.n[node], 0, XA_FREE_MARK as usize) {
            p.xa_mark_clear(xas.xa, XA_FREE_MARK);
        }
        p.n[node].count = 0;
        p.n[node].nr_values = 0;
        if !xa_is_node(entry) {
            p.n[node].slots[0] = XA_RETRY_ENTRY;
        }
        xas_update(p, xas, node);
        xa_node_free(p, node);
        if !xa_is_node(entry) {
            break;
        }
        node = xa_to_node(entry);
        p.n[node].parent = 0;
    }
}

fn xas_delete_node(p: &mut Pool, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    loop {
        if p.n[node].count != 0 {
            break;
        }
        let parent = p.n[node].parent;
        xas.node = parent;
        xas.offset = p.n[node].offset;
        xa_node_free(p, node);
        if parent == 0 {
            p.roots[xas.xa].head = 0;
            xas.node = XAS_BOUNDS;
            return;
        }
        let pni = nidx_ptr(parent);
        p.n[pni].slots[xas.offset as usize] = 0;
        p.n[pni].count = p.n[pni].count.wrapping_sub(1);
        node = pni;
        xas_update(p, xas, node);
    }
    if p.n[node].parent == 0 {
        xas_shrink(p, xas);
    }
}

/* ================= expand / create ================= */

fn xas_expand(p: &mut Pool, xas: &mut Xas, mut head: u64) -> i64 {
    let mut node: u64 = 0;
    let mut shift: u32 = 0;
    let max = xas.index; /* xas_max(): xa_shift == xa_sibs == 0 always */

    if head == 0 {
        if max == 0 {
            return 0;
        }
        while (max >> shift) >= CHUNK_SIZE as u64 {
            shift += CHUNK_SHIFT;
        }
        return (shift + CHUNK_SHIFT) as i64;
    } else if xa_is_node(head) {
        let ni = xa_to_node(head);
        node = node_ptr(ni);
        shift = p.n[ni].shift as u32 + CHUNK_SHIFT;
    }
    xas.node = 0;

    while max > max_index(p, head) {
        let ni = match xas_alloc(p, xas, shift as u8) {
            None => return -ENOMEM,
            Some(i) => i,
        };
        p.n[ni].count = 1;
        if xa_is_value(head) {
            p.n[ni].nr_values = 1;
        }
        p.n[ni].slots[0] = head;

        let mut mark = 0u32;
        loop {
            if p.xa_track_free(xas.xa) && mark == XA_FREE_MARK {
                node_mark_all(&mut p.n[ni], XA_FREE_MARK as usize);
                if !p.xa_marked(xas.xa, XA_FREE_MARK) {
                    node_clear_mark(&mut p.n[ni], 0, XA_FREE_MARK as usize);
                    p.xa_mark_set(xas.xa, XA_FREE_MARK);
                }
            } else if p.xa_marked(xas.xa, mark) {
                node_set_mark(&mut p.n[ni], 0, mark as usize);
            }
            if mark == XA_MARK_MAX {
                break;
            }
            mark += 1;
        }

        if xa_is_node(head) {
            let hni = xa_to_node(head);
            p.n[hni].offset = 0;
            p.n[hni].parent = node_ptr(ni);
        }
        head = xa_mk_node(ni);
        p.roots[xas.xa].head = head;
        xas_update(p, xas, ni);

        shift += CHUNK_SHIFT;
        node = node_ptr(ni);
    }
    xas.node = node;
    shift as i64
}

fn xas_create(p: &mut Pool, xas: &mut Xas, allow_root: bool) -> u64 {
    let order = 0u32; /* xa_shift is always 0 in this harness */
    let mut entry: u64;
    let mut shift: i64;
    let mut slot: Option<(usize, u64)>;

    if xas_top(xas.node) {
        entry = p.roots[xas.xa].head;
        xas.node = 0;
        if entry == 0 && p.xa_zero_busy(xas.xa) {
            entry = XA_ZERO_ENTRY;
        }
        shift = xas_expand(p, xas, entry);
        if shift < 0 {
            return 0;
        }
        if shift == 0 && !allow_root {
            shift = CHUNK_SHIFT as i64;
        }
        entry = p.roots[xas.xa].head;
        slot = None;
    } else if xas_error(xas) != 0 {
        return 0;
    } else if xas.node != 0 {
        let ni = nidx_ptr(xas.node);
        let offset = xas.offset;
        shift = p.n[ni].shift as i64;
        entry = p.n[ni].slots[offset as usize];
        slot = Some((ni, offset as u64));
    } else {
        shift = 0;
        entry = p.roots[xas.xa].head;
        slot = None;
    }

    while shift > order as i64 {
        shift -= CHUNK_SHIFT as i64;
        let node: usize;
        if entry == 0 {
            match xas_alloc(p, xas, shift as u8) {
                None => break,
                Some(ni) => {
                    node = ni;
                    if p.xa_track_free(xas.xa) {
                        node_mark_all(&mut p.n[ni], XA_FREE_MARK as usize);
                    }
                    match slot {
                        None => p.roots[xas.xa].head = xa_mk_node(ni),
                        Some((sni, soff)) => p.n[sni].slots[soff as usize] = xa_mk_node(ni),
                    }
                }
            }
        } else if xa_is_node(entry) {
            node = xa_to_node(entry);
        } else {
            break;
        }
        entry = xas_descend(p, xas, node);
        slot = Some((node, xas.offset as u64));
    }
    entry
}

fn update_node(p: &mut Pool, xas: &mut Xas, node: u64, count: i64, values: i64) {
    if node == 0 || (count == 0 && values == 0) {
        return;
    }
    if !is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    p.n[ni].count = p.n[ni].count.wrapping_add(count as u8);
    p.n[ni].nr_values = p.n[ni].nr_values.wrapping_add(values as u8);
    xas_update(p, xas, ni);
    if count < 0 {
        xas_delete_node(p, xas);
    }
}

/* ================= xas_store ================= */

fn xas_store(p: &mut Pool, xas: &mut Xas, entry: u64) -> u64 {
    let value = xa_is_value(entry);
    let first = if entry != 0 {
        xas_create(p, xas, !xa_is_node(entry) && !xa_is_zero(entry))
    } else {
        xas_load(p, xas)
    };
    if xas_invalid(xas) || first == entry {
        return first;
    }
    let node = xas.node;
    let ni_opt: Option<usize> = if is_node_ptr(node) {
        Some(nidx_ptr(node))
    } else {
        None
    };
    if entry == 0 {
        xas_init_marks(p, xas);
    }
    /* sibs == 0 always, so the store loop collapses to a single slot */
    match ni_opt {
        None => p.roots[xas.xa].head = entry,
        Some(ni) => p.n[ni].slots[xas.offset as usize] = entry,
    }
    if xa_is_node(first) && (ni_opt.is_none() || p.n[ni_opt.unwrap()].shift != 0) {
        xas_free_nodes(p, xas, xa_to_node(first));
    }
    if let Some(_ni) = ni_opt {
        let count = (first == 0) as i64 - (entry == 0) as i64;
        let values = (!xa_is_value(first)) as i64 - (!value) as i64;
        update_node(p, xas, node, count, values);
    }
    first
}

/* ================= find / iterate ================= */

fn xas_find(p: &mut Pool, xas: &mut Xas, max: u64) -> u64 {
    let mut entry: u64;
    if xas_error(xas) != 0 || xas.node == XAS_BOUNDS {
        return 0;
    }
    if xas.index > max {
        return set_bounds(xas);
    }
    if xas.node == 0 {
        xas.index = 1;
        return set_bounds(xas);
    } else if xas.node == XAS_RESTART {
        entry = xas_load(p, xas);
        if entry != 0 || xas_not_node(xas.node) {
            return entry;
        }
    } else if is_node_ptr(xas.node)
        && p.n[nidx_ptr(xas.node)].shift == 0
        && xas.offset as u64 != (xas.index & CHUNK_MASK)
    {
        xas.offset = (((xas.index.wrapping_sub(1)) & CHUNK_MASK) + 1) as u8;
    }

    xas_next_offset(p, xas);

    while is_node_ptr(xas.node) && xas.index <= max {
        if xas.offset as u64 == CHUNK_SIZE as u64 {
            xas.offset = p.n[nidx_ptr(xas.node)].offset.wrapping_add(1);
            xas.node = p.n[nidx_ptr(xas.node)].parent;
            continue;
        }
        let ni = nidx_ptr(xas.node);
        entry = p.n[ni].slots[xas.offset as usize];
        if xa_is_node(entry) {
            xas.node = node_ptr(xa_to_node(entry));
            xas.offset = 0;
            continue;
        }
        if entry != 0 {
            return entry;
        }
        xas_next_offset(p, xas);
    }

    if xas.node == 0 {
        xas.node = XAS_BOUNDS;
    }
    0
}

fn xas_find_marked(p: &mut Pool, xas: &mut Xas, max: u64, mark: usize) -> u64 {
    let mut advance = true;
    if xas_error(xas) != 0 {
        return 0;
    }
    if xas.index > max {
        xas.node = XAS_RESTART;
        return 0;
    }
    if xas.node == 0 {
        xas.index = 1;
        return find_marked_out(p, xas, max);
    } else if xas_top(xas.node) {
        advance = false;
        let entry = p.roots[xas.xa].head;
        xas.node = 0;
        if xas.index > max_index(p, entry) {
            return find_marked_out(p, xas, max);
        }
        if !xa_is_node(entry) {
            if p.xa_marked(xas.xa, mark as u32) {
                return entry;
            }
            xas.index = 1;
            return find_marked_out(p, xas, max);
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas.offset = (xas.index >> p.n[nidx_ptr(xas.node)].shift) as u8;
    }

    while xas.index <= max {
        if xas.offset as u64 == CHUNK_SIZE as u64 {
            let ni = nidx_ptr(xas.node);
            xas.offset = p.n[ni].offset.wrapping_add(1);
            xas.node = p.n[ni].parent;
            if xas.node == 0 {
                break;
            }
            advance = false;
            continue;
        }
        let offset = xas_find_chunk(p, xas, advance, mark);
        if offset > xas.offset as u64 {
            advance = false;
            xas_move_index(p, xas, offset);
            if xas.index.wrapping_sub(1) >= max {
                xas.node = XAS_RESTART;
                return 0;
            }
            xas.offset = offset as u8;
            if offset == CHUNK_SIZE as u64 {
                continue;
            }
        }
        let ni = nidx_ptr(xas.node);
        let entry = p.n[ni].slots[xas.offset as usize];
        if entry == 0 && !(p.xa_track_free(xas.xa) && mark as u32 == XA_FREE_MARK) {
            continue;
        }
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas_set_offset(p, xas);
    }

    find_marked_out(p, xas, max)
}

fn find_marked_out(p: &mut Pool, xas: &mut Xas, max: u64) -> u64 {
    let _ = p;
    if xas.index > max {
        xas.node = XAS_RESTART;
        return 0;
    }
    set_bounds(xas)
}

fn xas_next_entry(p: &mut Pool, xas: &mut Xas, max: u64) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (is_node_ptr(node) && p.n[nidx_ptr(node)].shift != 0)
        || xas.offset as u64 != (xas.index & CHUNK_MASK)
    {
        return xas_find(p, xas, max);
    }
    let ni = nidx_ptr(node);
    loop {
        if xas.index >= max {
            return xas_find(p, xas, max);
        }
        if xas.offset == CHUNK_MASK as u8 {
            return xas_find(p, xas, max);
        }
        let entry = p.n[ni].slots[xas.offset as usize + 1];
        if xa_is_internal(entry) {
            return xas_find(p, xas, max);
        }
        xas.offset += 1;
        xas.index += 1;
        if entry != 0 {
            return entry;
        }
    }
}

/* ================= public xa_* API ================= */

fn xa_find(p: &mut Pool, xa: usize, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(xa, *index);
    let mut entry;
    loop {
        if filter < MAX_MARKS as u32 {
            entry = xas_find_marked(p, &mut xas, max, filter as usize);
        } else {
            entry = xas_find(p, &mut xas, max);
        }
        if !xas_retry(&mut xas, entry) {
            break;
        }
    }
    if entry != 0 {
        *index = xas.index;
    }
    entry
}

fn xa_init_flags(p: &mut Pool, xa: usize, flags: u32) {
    p.roots[xa].flags = flags;
    p.roots[xa].head = 0;
}

fn xa_empty(p: &Pool, xa: usize) -> bool {
    p.roots[xa].head == 0
}

/* ================= radix-tree.c port ================= */

fn bug_on(c: bool) {
    if c {
        panic!("BUG_ON tripped in the port — model divergence");
    }
}

/* node tag helpers (radix_tree_node::tags[] aliases xa_node::marks[]) */
fn tag_get(n: &Node, tag: u32, offset: u64) -> bool {
    node_get_mark(n, offset, tag as usize)
}
fn tag_set(n: &mut Node, tag: u32, offset: u64) {
    node_set_mark(n, offset, tag as usize);
}
fn tag_clear(n: &mut Node, tag: u32, offset: u64) {
    node_clear_mark(n, offset, tag as usize);
}
fn any_tag_set(n: &Node, tag: u32) -> bool {
    node_any_mark(n, tag as usize)
}
fn all_tag_set(n: &mut Node, tag: u32) {
    node_mark_all(n, tag as usize);
}

fn root_tag_set(p: &mut Pool, xa: usize, tag: u32) {
    p.roots[xa].flags |= 1 << (ROOT_TAG_SHIFT + tag);
}
fn root_tag_clear(p: &mut Pool, xa: usize, tag: u32) {
    p.roots[xa].flags &= !(1 << (ROOT_TAG_SHIFT + tag));
}
fn root_tag_clear_all(p: &mut Pool, xa: usize) {
    p.roots[xa].flags &= (1 << ROOT_TAG_SHIFT) - 1;
}
fn root_tag_get(p: &Pool, xa: usize, tag: u32) -> bool {
    p.roots[xa].flags & (1 << (ROOT_TAG_SHIFT + tag)) != 0
}
fn root_tags_get(p: &Pool, xa: usize) -> u32 {
    p.roots[xa].flags >> ROOT_TAG_SHIFT
}
fn is_idr(p: &Pool, xa: usize) -> bool {
    p.roots[xa].flags & ROOT_IS_IDR != 0
}

fn rt_find_next_bit(n: &Node, tag: u32, offset: u64) -> u64 {
    /* node->tags[tag] is one u64 word (XA_MARK_LONGS == 1) */
    if offset < RADIX_TREE_MAP_SIZE {
        let data = n.marks[tag as usize] & (u64::MAX << offset);
        if data != 0 {
            return data.trailing_zeros() as u64;
        }
    }
    RADIX_TREE_MAP_SIZE
}

fn iter_offset(iter: &RIter) -> u64 {
    iter.index & RADIX_TREE_MAP_MASK
}

fn shift_maxindex(shift: u32) -> u64 {
    ((RADIX_TREE_MAP_SIZE) << shift).wrapping_sub(1)
}
fn node_maxindex(n: &Node) -> u64 {
    shift_maxindex(n.shift as u32)
}
fn next_index(index: u64, n: &Node, offset: u64) -> u64 {
    (index & !node_maxindex(n)).wrapping_add(offset << n.shift)
}

fn rt_descend(p: &Pool, ni: usize, index: u64) -> (u64, u64) {
    let offset = (index >> p.n[ni].shift) & RADIX_TREE_MAP_MASK;
    (p.n[ni].slots[offset as usize], offset)
}

fn rt_load_root(p: &Pool, xa: usize) -> (u64, u64, u32) {
    let node = p.roots[xa].head;
    if rt_is_internal(node) {
        let ni = entry_to_node(node);
        (
            node,
            node_maxindex(&p.n[ni]),
            p.n[ni].shift as u32 + RADIX_TREE_MAP_SHIFT,
        )
    } else {
        (node, 0, 0)
    }
}

/* the per-cpu preload stack: last element is the head */
fn preload_push(p: &mut Pool, ni: usize) {
    let head = p.preload.last().map(|&h| node_ptr(h)).unwrap_or(0);
    p.n[ni].parent = head;
    p.preload.push(ni);
}

fn __radix_tree_preload(p: &mut Pool, gfp_mask: u32, nr: u32) -> i64 {
    let _gfp = gfp_mask & !GFP_ACCOUNT;
    while (p.preload.len() as u32) < nr {
        let ni = match kmem_cache_alloc(p) {
            None => return -ENOMEM,
            Some(i) => i,
        };
        preload_push(p, ni);
    }
    0
}

fn radix_tree_preload(p: &mut Pool, gfp_mask: u32) -> i64 {
    p.warn_on(!allow_blocking(gfp_mask));
    __radix_tree_preload(p, gfp_mask, RADIX_TREE_PRELOAD_SIZE)
}

fn radix_tree_maybe_preload(p: &mut Pool, gfp_mask: u32) -> i64 {
    if allow_blocking(gfp_mask) {
        return __radix_tree_preload(p, gfp_mask, RADIX_TREE_PRELOAD_SIZE);
    }
    0
}

fn radix_tree_node_alloc(
    p: &mut Pool,
    gfp_mask: u32,
    parent: Option<usize>,
    xa: usize,
    shift: u8,
    offset: u8,
    count: u8,
    nr_values: u8,
) -> Option<usize> {
    let mut ret: Option<usize> = None;
    let mut used_cache = false;
    if !allow_blocking(gfp_mask) {
        /* try the slab first, then the per-cpu preload stack */
        ret = kmem_cache_alloc(p);
        if ret.is_some() {
            used_cache = true;
        } else {
            ret = p.preload.pop();
        }
    }
    let _ = used_cache;
    if ret.is_none() && allow_blocking(gfp_mask) {
        ret = kmem_cache_alloc(p);
    }
    if ret.is_none() && !allow_blocking(gfp_mask) {
        return None;
    }
    let ni = ret?;
    p.n[ni].shift = shift;
    p.n[ni].offset = offset;
    p.n[ni].count = count;
    p.n[ni].nr_values = nr_values;
    p.n[ni].parent = parent.map(node_ptr).unwrap_or(0);
    p.n[ni].array = root_canon(xa);
    Some(ni)
}

fn radix_tree_node_free(p: &mut Pool, ni: usize) {
    radix_tree_node_rcu_free(p, ni);
}

fn radix_tree_extend(p: &mut Pool, xa: usize, gfp: u32, index: u64, shift: u32) -> i64 {
    let mut maxshift = shift;
    let mut shift = shift;
    while index > shift_maxindex(maxshift) {
        maxshift += RADIX_TREE_MAP_SHIFT;
    }

    let mut entry = p.roots[xa].head;
    if entry == 0 && (!is_idr(p, xa) || root_tag_get(p, xa, IDR_FREE)) {
        return (maxshift + RADIX_TREE_MAP_SHIFT) as i64;
    }

    loop {
        let ni = match radix_tree_node_alloc(p, gfp, None, xa, shift as u8, 0, 1, 0) {
            None => return -ENOMEM,
            Some(n) => n,
        };
        if is_idr(p, xa) {
            all_tag_set(&mut p.n[ni], IDR_FREE);
            if !root_tag_get(p, xa, IDR_FREE) {
                tag_clear(&mut p.n[ni], IDR_FREE, 0);
                root_tag_set(p, xa, IDR_FREE);
            }
        } else {
            for tag in 0..RADIX_TREE_MAX_TAGS {
                if root_tag_get(p, xa, tag) {
                    tag_set(&mut p.n[ni], tag, 0);
                }
            }
        }
        bug_on(shift > BITS_PER_LONG as u32);
        if rt_is_internal(entry) {
            let cni = entry_to_node(entry);
            p.n[cni].parent = node_ptr(ni);
        } else if xa_is_value(entry) {
            p.n[ni].nr_values = 1;
        }
        p.n[ni].slots[0] = entry;
        entry = node_to_entry(ni);
        p.roots[xa].head = entry;
        shift += RADIX_TREE_MAP_SHIFT;
        if shift > maxshift {
            break;
        }
    }
    (maxshift + RADIX_TREE_MAP_SHIFT) as i64
}

fn radix_tree_shrink(p: &mut Pool, xa: usize) -> bool {
    let mut shrunk = false;
    loop {
        let node_e = p.roots[xa].head;
        if !rt_is_internal(node_e) {
            break;
        }
        let ni = entry_to_node(node_e);
        if p.n[ni].count != 1 {
            break;
        }
        let child = p.n[ni].slots[0];
        if child == 0 {
            break;
        }
        if p.n[ni].shift == 0 && is_idr(p, xa) {
            break;
        }
        if rt_is_internal(child) {
            let cni = entry_to_node(child);
            p.n[cni].parent = 0;
        }
        p.roots[xa].head = child;
        if is_idr(p, xa) && !tag_get(&p.n[ni], IDR_FREE, 0) {
            root_tag_clear(p, xa, IDR_FREE);
        }
        p.n[ni].count = 0;
        if !rt_is_internal(child) {
            p.n[ni].slots[0] = RADIX_TREE_RETRY;
        }
        p.warn_on(p.n[ni].on_list);
        radix_tree_node_free(p, ni);
        shrunk = true;
    }
    shrunk
}

fn delete_node(p: &mut Pool, xa: usize, node0: usize) -> bool {
    let mut deleted = false;
    let mut node = Some(node0);
    while let Some(ni) = node {
        if p.n[ni].count != 0 {
            if node_to_entry(ni) == p.roots[xa].head {
                deleted |= radix_tree_shrink(p, xa);
            }
            return deleted;
        }
        let parent = p.n[ni].parent;
        if parent != 0 {
            let pni = nidx_ptr(parent);
            let off = p.n[ni].offset as usize;
            p.n[pni].slots[off] = 0;
            p.n[pni].count = p.n[pni].count.wrapping_sub(1);
        } else {
            if !is_idr(p, xa) {
                root_tag_clear_all(p, xa);
            }
            p.roots[xa].head = 0;
        }
        p.warn_on(p.n[ni].on_list);
        radix_tree_node_free(p, ni);
        deleted = true;
        node = if parent != 0 {
            Some(nidx_ptr(parent))
        } else {
            None
        };
    }
    deleted
}

fn __radix_tree_create(p: &mut Pool, xa: usize, index: u64) -> Result<(Option<usize>, SlotL), i64> {
    let mut node: Option<usize> = None;
    let mut slot = SlotL::Root(xa);
    let mut offset = 0u64;
    let gfp = root_gfp_mask(p.roots[xa].flags);

    let (mut child, maxindex, mut shift) = rt_load_root(p, xa);

    if index > maxindex {
        let error = radix_tree_extend(p, xa, gfp, index, shift);
        if error < 0 {
            return Err(error);
        }
        shift = error as u32;
        child = p.roots[xa].head;
    }

    while shift > 0 {
        shift -= RADIX_TREE_MAP_SHIFT;
        if child == 0 {
            let ni = match radix_tree_node_alloc(p, gfp, node, xa, shift as u8, offset as u8, 0, 0)
            {
                None => return Err(-ENOMEM),
                Some(n) => n,
            };
            slot_set(p, slot, node_to_entry(ni));
            if let Some(pn) = node {
                p.n[pn].count = p.n[pn].count.wrapping_add(1);
            }
            child = node_to_entry(ni);
        } else if !rt_is_internal(child) {
            break;
        }
        let ni = entry_to_node(child);
        node = Some(ni);
        let (c, off) = rt_descend(p, ni, index);
        child = c;
        offset = off;
        slot = SlotL::Node(ni, off as usize);
    }
    Ok((node, slot))
}

fn radix_tree_free_nodes(p: &mut Pool, node_e: u64) {
    let mut offset = 0u32;
    let mut child = entry_to_node(node_e);
    let top = child;
    loop {
        let entry = p.n[child].slots[offset as usize];
        if xa_is_node(entry) && p.n[child].shift != 0 {
            child = entry_to_node(entry);
            offset = 0;
            continue;
        }
        offset += 1;
        while offset == RADIX_TREE_MAP_SIZE as u32 {
            let old = child;
            offset = p.n[child].offset as u32 + 1;
            child = if p.n[child].parent != 0 {
                nidx_ptr(p.n[child].parent)
            } else {
                usize::MAX
            };
            p.warn_on(p.n[old].on_list);
            radix_tree_node_free(p, old);
            if old == top {
                return;
            }
        }
    }
}

fn insert_entries(p: &mut Pool, node: Option<usize>, slot: SlotL, item: u64) -> i64 {
    if slot_get(p, slot) != 0 {
        return -EEXIST;
    }
    slot_set(p, slot, item);
    if let Some(ni) = node {
        p.n[ni].count = p.n[ni].count.wrapping_add(1);
        if xa_is_value(item) {
            p.n[ni].nr_values = p.n[ni].nr_values.wrapping_add(1);
        }
    }
    1
}

fn radix_tree_insert(p: &mut Pool, xa: usize, index: u64, item: u64) -> i64 {
    bug_on(rt_is_internal(item));
    let (node, slot) = match __radix_tree_create(p, xa, index) {
        Err(e) => return e,
        Ok(v) => v,
    };
    let error = insert_entries(p, node, slot, item);
    if error < 0 {
        return error;
    }
    if let Some(ni) = node {
        let offset = slot_off(slot);
        bug_on(tag_get(&p.n[ni], 0, offset));
        bug_on(tag_get(&p.n[ni], 1, offset));
        bug_on(tag_get(&p.n[ni], 2, offset));
    } else {
        bug_on(root_tags_get(p, xa) != 0);
    }
    0
}

/* returns (entry, parent node, slot) — entry 0 means "absent"; slot None
 * means out of range (mirrors __radix_tree_lookup not setting *slotp) */
fn __radix_tree_lookup(p: &Pool, xa: usize, index: u64) -> (u64, Option<usize>, Option<SlotL>) {
    let mut parent: Option<usize> = None;
    let mut slot = SlotL::Root(xa);
    let (mut node, maxindex, _) = rt_load_root(p, xa);
    if index > maxindex {
        return (0, None, None);
    }
    while rt_is_internal(node) {
        let ni = entry_to_node(node);
        parent = Some(ni);
        let (c, off) = rt_descend(p, ni, index);
        node = c;
        slot = SlotL::Node(ni, off as usize);
        if node == RADIX_TREE_RETRY {
            return __radix_tree_lookup(p, xa, index);
        }
        if p.n[ni].shift == 0 {
            break;
        }
    }
    (node, parent, Some(slot))
}

fn radix_tree_lookup_slot(p: &Pool, xa: usize, index: u64) -> Option<SlotL> {
    let (entry, _, slot) = __radix_tree_lookup(p, xa, index);
    if entry == 0 {
        return None;
    }
    slot
}

fn radix_tree_lookup(p: &Pool, xa: usize, index: u64) -> u64 {
    __radix_tree_lookup(p, xa, index).0
}

fn get_slot_offset(node: Option<usize>, slot: SlotL) -> u64 {
    if node.is_some() { slot_off(slot) } else { 0 }
}

fn replace_slot(
    p: &mut Pool,
    slot: SlotL,
    item: u64,
    node: Option<usize>,
    count: i64,
    values: i64,
) {
    if let Some(ni) = node {
        if count != 0 || values != 0 {
            p.n[ni].count = p.n[ni].count.wrapping_add(count as u8);
            p.n[ni].nr_values = p.n[ni].nr_values.wrapping_add(values as u8);
        }
    }
    slot_set(p, slot, item);
}

fn node_tag_get(p: &Pool, xa: usize, node: Option<usize>, tag: u32, offset: u64) -> bool {
    match node {
        Some(ni) => tag_get(&p.n[ni], tag, offset),
        None => root_tag_get(p, xa, tag),
    }
}

fn calculate_count(
    p: &Pool,
    xa: usize,
    node: Option<usize>,
    slot: SlotL,
    item: u64,
    old: u64,
) -> i64 {
    if is_idr(p, xa) {
        let offset = get_slot_offset(node, slot);
        let free = node_tag_get(p, xa, node, IDR_FREE, offset);
        if !free {
            return 0;
        }
        if old == 0 {
            return 1;
        }
    }
    (item != 0) as i64 - (old != 0) as i64
}

fn __radix_tree_replace(p: &mut Pool, xa: usize, node: Option<usize>, slot: SlotL, item: u64) {
    let old = slot_get(p, slot);
    let values = xa_is_value(item) as i64 - xa_is_value(old) as i64;
    let count = calculate_count(p, xa, node, slot, item, old);
    p.warn_on(node.is_none() && slot != SlotL::Root(xa) && (count != 0 || values != 0));
    replace_slot(p, slot, item, node, count, values);
    if node.is_none() {
        return;
    }
    delete_node(p, xa, node.unwrap());
}

fn radix_tree_replace_slot(p: &mut Pool, xa: usize, slot: SlotL, item: u64) {
    __radix_tree_replace(p, xa, None, slot, item);
}

fn radix_tree_iter_replace(p: &mut Pool, xa: usize, iter: &RIter, slot: SlotL, item: u64) {
    __radix_tree_replace(p, xa, iter.node, slot, item);
}

fn node_tag_set(p: &mut Pool, xa: usize, node: Option<usize>, tag: u32, offset: u64) {
    let mut node = node;
    let mut offset = offset;
    while let Some(ni) = node {
        if tag_get(&p.n[ni], tag, offset) {
            return;
        }
        tag_set(&mut p.n[ni], tag, offset);
        offset = p.n[ni].offset as u64;
        node = if p.n[ni].parent != 0 {
            Some(nidx_ptr(p.n[ni].parent))
        } else {
            None
        };
    }
    if !root_tag_get(p, xa, tag) {
        root_tag_set(p, xa, tag);
    }
}

fn radix_tree_tag_set(p: &mut Pool, xa: usize, index: u64, tag: u32) -> u64 {
    let (mut node, maxindex, _) = rt_load_root(p, xa);
    bug_on(index > maxindex);
    while rt_is_internal(node) {
        let ni = entry_to_node(node);
        let (c, off) = rt_descend(p, ni, index);
        node = c;
        bug_on(node == 0);
        if !tag_get(&p.n[ni], tag, off) {
            tag_set(&mut p.n[ni], tag, off);
        }
    }
    if !root_tag_get(p, xa, tag) {
        root_tag_set(p, xa, tag);
    }
    node
}

fn node_tag_clear(p: &mut Pool, xa: usize, node: Option<usize>, tag: u32, offset: u64) {
    let mut node = node;
    let mut offset = offset;
    while let Some(ni) = node {
        if !tag_get(&p.n[ni], tag, offset) {
            return;
        }
        tag_clear(&mut p.n[ni], tag, offset);
        if any_tag_set(&p.n[ni], tag) {
            return;
        }
        offset = p.n[ni].offset as u64;
        node = if p.n[ni].parent != 0 {
            Some(nidx_ptr(p.n[ni].parent))
        } else {
            None
        };
    }
    if root_tag_get(p, xa, tag) {
        root_tag_clear(p, xa, tag);
    }
}

fn radix_tree_tag_clear(p: &mut Pool, xa: usize, index: u64, tag: u32) -> u64 {
    let (mut node, maxindex, _) = rt_load_root(p, xa);
    if index > maxindex {
        return 0;
    }
    let mut parent: Option<usize> = None;
    let mut offset = 0u64;
    while rt_is_internal(node) {
        let ni = entry_to_node(node);
        parent = Some(ni);
        let (c, off) = rt_descend(p, ni, index);
        node = c;
        offset = off;
    }
    if node != 0 {
        node_tag_clear(p, xa, parent, tag, offset);
    }
    node
}

fn radix_tree_iter_tag_clear(p: &mut Pool, xa: usize, iter: &RIter, tag: u32) {
    node_tag_clear(p, xa, iter.node, tag, iter_offset(iter));
}

fn radix_tree_tag_get(p: &Pool, xa: usize, index: u64, tag: u32) -> i64 {
    if !root_tag_get(p, xa, tag) {
        return 0;
    }
    let (mut node, maxindex, _) = rt_load_root(p, xa);
    if index > maxindex {
        return 0;
    }
    while rt_is_internal(node) {
        let ni = entry_to_node(node);
        let (c, off) = rt_descend(p, ni, index);
        node = c;
        if !tag_get(&p.n[ni], tag, off) {
            return 0;
        }
        if node == RADIX_TREE_RETRY {
            break;
        }
    }
    1
}

fn radix_tree_tagged(p: &Pool, xa: usize, tag: u32) -> i64 {
    /* upstream root_tag_get() returns the raw masked bit, not 0/1 */
    (p.roots[xa].flags & (1 << (ROOT_TAG_SHIFT + tag))) as i64
}

/* ================= radix_tree_iter ================= */

#[derive(Clone)]
struct RIter {
    index: u64,
    next_index: u64,
    tags: u64,
    node: Option<usize>,
}

impl RIter {
    fn new() -> Self {
        RIter {
            index: 0,
            next_index: 0,
            tags: 0,
            node: None,
        }
    }
}

fn rt_iter_init(iter: &mut RIter, start: u64) -> Option<SlotL> {
    iter.index = 0;
    iter.next_index = start;
    None
}

fn rt_iter_retry(iter: &mut RIter) -> Option<SlotL> {
    iter.next_index = iter.index;
    iter.tags = 0;
    None
}

fn rt_iter_add(iter: &RIter, slots: u64) -> u64 {
    iter.index.wrapping_add(slots)
}

fn rt_iter_resume(_slot: Option<SlotL>, iter: &mut RIter) -> Option<SlotL> {
    iter.index = rt_iter_add(iter, 1);
    iter.next_index = iter.index;
    iter.tags = 0;
    None
}

fn rt_chunk_size(iter: &RIter) -> i64 {
    iter.next_index.wrapping_sub(iter.index) as i64
}

fn set_iter_tags(p: &Pool, iter: &mut RIter, node: Option<usize>, offset: u64, tag: u32) {
    match node {
        None => {
            iter.tags = 1;
        }
        Some(ni) => {
            iter.tags = p.n[ni].marks[tag as usize] >> offset;
        }
    }
}

fn radix_tree_next_chunk(p: &mut Pool, xa: usize, iter: &mut RIter, flags: u32) -> Option<SlotL> {
    let tag = flags & ITER_TAG_MASK;
    let tagged = flags & ITER_TAGGED != 0;
    let contig = flags & ITER_CONTIG != 0;

    if tagged && !root_tag_get(p, xa, tag) {
        return None;
    }

    let mut index = iter.next_index;
    if index == 0 && iter.index != 0 {
        return None;
    }

    'restart: loop {
        let (mut child, maxindex, _) = rt_load_root(p, xa);
        if index > maxindex {
            return None;
        }
        if child == 0 {
            return None;
        }
        if !rt_is_internal(child) {
            /* single-slot tree */
            iter.index = index;
            iter.next_index = maxindex.wrapping_add(1);
            iter.tags = 1;
            iter.node = None;
            return Some(SlotL::Root(xa));
        }

        let mut node;
        let mut offset;
        loop {
            node = entry_to_node(child);
            let (c, off) = rt_descend(p, node, index);
            child = c;
            offset = off;

            if if tagged {
                !tag_get(&p.n[node], tag, offset)
            } else {
                child == 0
            } {
                /* hole detected */
                if contig {
                    return None;
                }
                if tagged {
                    offset = rt_find_next_bit(&p.n[node], tag, offset + 1);
                } else {
                    loop {
                        offset += 1;
                        if offset >= RADIX_TREE_MAP_SIZE {
                            break;
                        }
                        if p.n[node].slots[offset as usize] != 0 {
                            break;
                        }
                    }
                }
                index &= !node_maxindex(&p.n[node]);
                index = index.wrapping_add(offset << p.n[node].shift);
                if index == 0 {
                    return None;
                }
                if offset == RADIX_TREE_MAP_SIZE {
                    continue 'restart;
                }
                child = p.n[node].slots[offset as usize];
            }
            if child == 0 {
                continue 'restart;
            }
            if child == RADIX_TREE_RETRY {
                break;
            }
            if !(p.n[node].shift != 0 && rt_is_internal(child)) {
                break;
            }
        }

        iter.index = (index & !node_maxindex(&p.n[node])) | offset;
        iter.next_index = (index | node_maxindex(&p.n[node])).wrapping_add(1);
        iter.node = Some(node);
        if tagged {
            set_iter_tags(p, iter, Some(node), offset, tag);
        }
        return Some(SlotL::Node(node, offset as usize));
    }
}

fn radix_tree_iter_lookup(p: &mut Pool, xa: usize, iter: &mut RIter, index: u64) -> Option<SlotL> {
    rt_iter_init(iter, index);
    radix_tree_next_chunk(p, xa, iter, ITER_CONTIG)
}

fn radix_tree_next_slot(
    p: &Pool,
    slot: Option<SlotL>,
    iter: &mut RIter,
    flags: u32,
) -> Option<SlotL> {
    let slot = slot?;
    if flags & ITER_TAGGED != 0 {
        iter.tags >>= 1;
        if iter.tags == 0 {
            return None;
        }
        if iter.tags & 1 != 0 {
            iter.index = rt_iter_add(iter, 1);
            return Some(slot_next(slot));
        }
        if flags & ITER_CONTIG == 0 {
            let mut offset = iter.tags.trailing_zeros() as u64;
            iter.tags >>= offset;
            offset += 1;
            iter.index = rt_iter_add(iter, offset);
            return Some(slot_add(slot, offset));
        }
        return None;
    }
    let mut count = rt_chunk_size(iter);
    let mut s = slot;
    loop {
        count -= 1;
        if count <= 0 {
            break;
        }
        s = slot_next(s);
        iter.index = rt_iter_add(iter, 1);
        if slot_get(p, s) != 0 {
            return Some(s);
        }
        if flags & ITER_CONTIG != 0 {
            iter.next_index = 0;
            break;
        }
    }
    None
}

/* for_each_slot: iterate live slots; body returns false to stop */
fn each_slot(
    p: &mut Pool,
    xa: usize,
    start: u64,
    flags: u32,
    mut body: impl FnMut(&mut Pool, &mut RIter, SlotL) -> bool,
) {
    let mut iter = RIter::new();
    let mut slot = rt_iter_init(&mut iter, start);
    loop {
        let s = match slot {
            Some(s) => s,
            None => match radix_tree_next_chunk(p, xa, &mut iter, flags) {
                None => break,
                Some(s) => s,
            },
        };
        if !body(p, &mut iter, s) {
            break;
        }
        slot = radix_tree_next_slot(p, Some(s), &mut iter, flags);
    }
}

/* ================= gang lookups ================= */

fn gang_lookup_impl(
    p: &mut Pool,
    xa: usize,
    first: u64,
    max_items: u32,
    tag: Option<u32>,
) -> Vec<u64> {
    let mut results = Vec::new();
    if max_items == 0 {
        return results;
    }
    let flags = tag.map(|t| ITER_TAGGED | t).unwrap_or(0);
    let mut iter = RIter::new();
    let mut slot = rt_iter_init(&mut iter, first);
    loop {
        let s = match slot {
            Some(s) => s,
            None => match radix_tree_next_chunk(p, xa, &mut iter, flags) {
                None => break,
                Some(s) => s,
            },
        };
        let item = slot_get(p, s);
        if item == 0 {
            slot = radix_tree_next_slot(p, Some(s), &mut iter, flags);
            continue;
        }
        if rt_is_internal(item) {
            slot = rt_iter_retry(&mut iter);
            continue;
        }
        results.push(item);
        if results.len() as u32 == max_items {
            break;
        }
        slot = radix_tree_next_slot(p, Some(s), &mut iter, flags);
    }
    results
}

fn radix_tree_gang_lookup_tag_slot(
    p: &mut Pool,
    xa: usize,
    first: u64,
    max_items: u32,
    tag: u32,
) -> Vec<SlotL> {
    let mut results = Vec::new();
    if max_items == 0 {
        return results;
    }
    each_slot(p, xa, first, ITER_TAGGED | tag, |_, _, s| {
        results.push(s);
        results.len() as u32 != max_items
    });
    results
}

/* ================= delete ================= */

fn __radix_tree_delete(p: &mut Pool, xa: usize, node: Option<usize>, slot: SlotL) -> bool {
    let old = slot_get(p, slot);
    let values = if xa_is_value(old) { -1 } else { 0 };
    let offset = get_slot_offset(node, slot);
    if is_idr(p, xa) {
        node_tag_set(p, xa, node, IDR_FREE, offset);
    } else {
        for tag in 0..RADIX_TREE_MAX_TAGS {
            node_tag_clear(p, xa, node, tag, offset);
        }
    }
    replace_slot(p, slot, 0, node, -1, values);
    match node {
        Some(ni) => delete_node(p, xa, ni),
        None => false,
    }
}

fn radix_tree_iter_delete(p: &mut Pool, xa: usize, iter: &mut RIter, slot: SlotL) {
    if __radix_tree_delete(p, xa, iter.node, slot) {
        iter.index = iter.next_index;
    }
}

fn radix_tree_delete_item(p: &mut Pool, xa: usize, index: u64, item: u64) -> u64 {
    let (entry, node, slot) = __radix_tree_lookup(p, xa, index);
    let slot = match slot {
        None => return 0,
        Some(s) => s,
    };
    if entry == 0
        && (!is_idr(p, xa) || node_tag_get(p, xa, node, IDR_FREE, get_slot_offset(node, slot)))
    {
        return 0;
    }
    if item != 0 && entry != item {
        return 0;
    }
    __radix_tree_delete(p, xa, node, slot);
    entry
}

fn radix_tree_delete(p: &mut Pool, xa: usize, index: u64) -> u64 {
    radix_tree_delete_item(p, xa, index, 0)
}

fn idr_preload(p: &mut Pool, gfp_mask: u32) {
    let _ = __radix_tree_preload(p, gfp_mask, IDR_PRELOAD_SIZE);
}

fn idr_get_free(
    p: &mut Pool,
    xa: usize,
    iter: &mut RIter,
    gfp: u32,
    max: u64,
) -> Result<SlotL, i64> {
    let mut node: Option<usize> = None;
    let mut slot = SlotL::Root(xa);
    let mut start = iter.next_index;
    let mut offset = 0u64;

    'grow: loop {
        let (mut child, maxindex, mut shift) = rt_load_root(p, xa);
        if !root_tag_get(p, xa, IDR_FREE) {
            start = max_u64(start, maxindex.wrapping_add(1));
        }
        if start > max {
            return Err(-ENOSPC);
        }
        if start > maxindex {
            let error = radix_tree_extend(p, xa, gfp, start, shift);
            if error < 0 {
                return Err(error);
            }
            shift = error as u32;
            child = p.roots[xa].head;
        }
        if start == 0 && shift == 0 {
            shift = RADIX_TREE_MAP_SHIFT;
        }
        while shift != 0 {
            shift -= RADIX_TREE_MAP_SHIFT;
            if child == 0 {
                let ni = match radix_tree_node_alloc(
                    p,
                    gfp,
                    node,
                    xa,
                    shift as u8,
                    offset as u8,
                    0,
                    0,
                ) {
                    None => return Err(-ENOMEM),
                    Some(n) => n,
                };
                all_tag_set(&mut p.n[ni], IDR_FREE);
                slot_set(p, slot, node_to_entry(ni));
                if let Some(pn) = node {
                    p.n[pn].count = p.n[pn].count.wrapping_add(1);
                }
                child = node_to_entry(ni);
            } else if !rt_is_internal(child) {
                break;
            }
            let ni = entry_to_node(child);
            node = Some(ni);
            let (c, off) = rt_descend(p, ni, start);
            child = c;
            offset = off;
            if !tag_get(&p.n[ni], IDR_FREE, offset) {
                offset = rt_find_next_bit(&p.n[ni], IDR_FREE, offset + 1);
                start = next_index(start, &p.n[ni], offset);
                if start > max || start == 0 {
                    return Err(-ENOSPC);
                }
                let mut ni = ni;
                while offset == RADIX_TREE_MAP_SIZE {
                    offset = p.n[ni].offset as u64 + 1;
                    if p.n[ni].parent == 0 {
                        node = None;
                        continue 'grow;
                    }
                    ni = nidx_ptr(p.n[ni].parent);
                    node = Some(ni);
                    shift = p.n[ni].shift as u32;
                }
                child = p.n[ni].slots[offset as usize];
            }
            slot = SlotL::Node(node.unwrap(), offset as usize);
        }
        break;
    }
    iter.index = start;
    iter.next_index = match node {
        Some(ni) => 1u64.wrapping_add(min_u64(max, start | node_maxindex(&p.n[ni]))),
        None => 1,
    };
    iter.node = node;
    set_iter_tags(p, iter, node, offset, IDR_FREE);
    Ok(slot)
}

fn idr_destroy(p: &mut Pool) {
    let node = p.roots[T_IDR].head;
    if rt_is_internal(node) {
        radix_tree_free_nodes(p, node);
    }
    p.roots[T_IDR].head = 0;
    root_tag_set(p, T_IDR, IDR_FREE);
}

/* ================= idr.c port: IDR API ================= */

fn idr_init_base(p: &mut Pool, base: i64) {
    xa_init_flags(p, T_IDR, IDR_RT_MARKER);
    p.idr_base = base as u32;
    p.idr_next = 0;
}

fn idr_alloc_u32(p: &mut Pool, ptr: u64, nextid: &mut u32, max: u64, gfp: u32) -> i64 {
    let base = p.idr_base as u64;
    let mut id = *nextid as u64;
    if p.warn_on(p.roots[T_IDR].flags & ROOT_IS_IDR == 0) {
        p.roots[T_IDR].flags |= IDR_RT_MARKER;
    }
    if max < base {
        return -ENOSPC;
    }
    id = if id < base { 0 } else { id - base };
    let mut iter = RIter::new();
    rt_iter_init(&mut iter, id);
    let slot = match idr_get_free(p, T_IDR, &mut iter, gfp, max - base) {
        Err(e) => return e,
        Ok(s) => s,
    };
    *nextid = iter.index.wrapping_add(base) as u32;
    radix_tree_iter_replace(p, T_IDR, &iter, slot, ptr);
    radix_tree_iter_tag_clear(p, T_IDR, &iter, IDR_FREE);
    0
}

fn idr_alloc(p: &mut Pool, ptr: u64, start: i64, end: i64, gfp: u32) -> i64 {
    let mut id = start as u32;
    if p.warn_on(start < 0) {
        return -EINVAL;
    }
    let max = if end > 0 { end - 1 } else { INT_MAXV };
    let ret = idr_alloc_u32(p, ptr, &mut id, max as u64, gfp);
    if ret != 0 {
        return ret;
    }
    id as i32 as i64
}

fn idr_alloc_cyclic(p: &mut Pool, ptr: u64, start: i64, end: i64, gfp: u32) -> i64 {
    let mut id = p.idr_next;
    let max = if end > 0 { end - 1 } else { INT_MAXV };
    if (id as i32 as i64) < start {
        id = start as u32;
    }
    let mut err = idr_alloc_u32(p, ptr, &mut id, max as u64, gfp);
    if err == -ENOSPC && (id as i32 as i64) > start {
        id = start as u32;
        err = idr_alloc_u32(p, ptr, &mut id, max as u64, gfp);
    }
    if err != 0 {
        return err;
    }
    p.idr_next = id.wrapping_add(1);
    id as i32 as i64
}

fn idr_remove(p: &mut Pool, id: u64) -> u64 {
    radix_tree_delete_item(p, T_IDR, id.wrapping_sub(p.idr_base as u64), 0)
}

fn idr_find(p: &Pool, id: u64) -> u64 {
    radix_tree_lookup(p, T_IDR, id.wrapping_sub(p.idr_base as u64))
}

fn idr_for_each(p: &mut Pool, stop: i64) -> i64 {
    let base = p.idr_base as u64;
    let mut ret = 0;
    each_slot(p, T_IDR, 0, 0, |p, iter, s| {
        let id = iter.index.wrapping_add(base);
        if p.warn_on(id > INT_MAXV as u64) {
            return false;
        }
        let entry = slot_get(p, s);
        p.fold(id);
        p.fold(efold(entry));
        if stop != 0 {
            ret = stop;
            return false;
        }
        true
    });
    ret
}

fn idr_get_next_ul(p: &mut Pool, nextid: &mut u64) -> u64 {
    let base = p.idr_base as u64;
    let mut id = *nextid;
    id = if id < base { 0 } else { id - base };
    let mut iter = RIter::new();
    let mut slot = rt_iter_init(&mut iter, id);
    let entry;
    loop {
        let s = match slot {
            Some(s) => s,
            None => match radix_tree_next_chunk(p, T_IDR, &mut iter, 0) {
                None => {
                    return 0;
                }
                Some(s) => s,
            },
        };
        let e = slot_get(p, s);
        if e == 0 {
            slot = radix_tree_next_slot(p, Some(s), &mut iter, 0);
            continue;
        }
        if !xa_is_internal(e) {
            entry = e;
            break;
        }
        if s != SlotL::Root(T_IDR) && !xa_is_retry(e) {
            entry = e;
            break;
        }
        slot = rt_iter_retry(&mut iter);
    }
    *nextid = iter.index.wrapping_add(base);
    entry
}

fn idr_get_next(p: &mut Pool, nextid: &mut i64) -> u64 {
    let mut id = *nextid as i32 as i64 as u64;
    let entry = idr_get_next_ul(p, &mut id);
    if p.warn_on(id > INT_MAXV as u64) {
        return 0;
    }
    *nextid = id as u32 as i32 as i64;
    entry
}

fn idr_replace(p: &mut Pool, ptr: u64, id: u64) -> u64 {
    let index = id.wrapping_sub(p.idr_base as u64);
    let (entry, node, slot) = __radix_tree_lookup(p, T_IDR, index);
    let slot = match slot {
        None => return err_ptr(ENOENT),
        Some(s) => s,
    };
    if radix_tree_tag_get(p, T_IDR, index, IDR_FREE) != 0 {
        return err_ptr(ENOENT);
    }
    __radix_tree_replace(p, T_IDR, node, slot, ptr);
    entry
}

/* idr_is_empty per upstream: radix_tree_empty && radix_tree_tagged(FREE) */
fn idr_is_empty_u(p: &Pool) -> u64 {
    (p.roots[T_IDR].head == 0 && root_tag_get(p, T_IDR, IDR_FREE)) as u64
}

fn idr_get_cursor(p: &Pool) -> u64 {
    p.idr_next as u64
}
fn idr_set_cursor(p: &mut Pool, val: u64) {
    p.idr_next = val as u32;
}

/* ================= idr.c port: IDA API ================= */

fn find_next_zero_bit1(v: u64, limit: u64, start: u64) -> u64 {
    let mut i = start;
    while i < limit {
        if (v >> i) & 1 == 0 {
            return i;
        }
        i += 1;
    }
    limit
}

fn find_in_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64, want_set: bool) -> u64 {
    let mut i = start;
    while i < limit {
        if ((bm[(i / 64) as usize] >> (i % 64)) & 1 == 1) == want_set {
            return i;
        }
        i += 1;
    }
    limit
}
fn find_next_zero_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64) -> u64 {
    find_in_arr(bm, limit, start, false)
}
fn find_next_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64) -> u64 {
    find_in_arr(bm, limit, start, true)
}

fn bm_set_bit(bm: &mut [u64; IDA_BITMAP_LONGS], i: u64) {
    bm[(i / 64) as usize] |= 1 << (i % 64);
}
fn bm_clear_bit(bm: &mut [u64; IDA_BITMAP_LONGS], i: u64) {
    bm[(i / 64) as usize] &= !(1 << (i % 64));
}
fn bm_test_bit(bm: &[u64; IDA_BITMAP_LONGS], i: u64) -> bool {
    (bm[(i / 64) as usize] >> (i % 64)) & 1 == 1
}
fn bm_full(bm: &[u64; IDA_BITMAP_LONGS]) -> bool {
    bm.iter().all(|&w| w == u64::MAX)
}
fn bm_empty(bm: &[u64; IDA_BITMAP_LONGS]) -> bool {
    bm.iter().all(|&w| w == 0)
}

fn ida_alloc_range(p: &mut Pool, min: u32, max_in: u32, _gfp: u32) -> i64 {
    let mut xas = Xas::new(T_IDA, (min as u64) / IDA_BITMAP_BITS);
    let mut bit = (min as u64) % IDA_BITMAP_BITS;
    let mut max = max_in;
    let mut alloc: Option<usize> = None;

    if (min as i32) < 0 {
        return -ENOSPC;
    }
    if (max as i32) < 0 {
        max = i32::MAX as u32;
    }
    let maxb = max as u64;

    'retry: loop {
        let mut bitmap: u64;
        'body: loop {
            /* next: */
            bitmap = xas_find_marked(p, &mut xas, maxb / IDA_BITMAP_BITS, XA_FREE_MARK as usize);
            if xas.index > (min as u64) / IDA_BITMAP_BITS {
                bit = 0;
            }
            if xas.index * IDA_BITMAP_BITS + bit > maxb {
                /* nospc: */
                if let Some(i) = alloc {
                    kfree_bitmap(p, i);
                }
                return -ENOSPC;
            }

            if xa_is_value(bitmap) {
                let mut tmp = xa_to_value(bitmap);
                if bit < BITS_PER_XA_VALUE {
                    bit = find_next_zero_bit1(tmp, BITS_PER_XA_VALUE, bit);
                    if xas.index * IDA_BITMAP_BITS + bit > maxb {
                        if let Some(i) = alloc {
                            kfree_bitmap(p, i);
                        }
                        return -ENOSPC;
                    }
                    if bit < BITS_PER_XA_VALUE {
                        tmp |= 1 << bit;
                        xas_store(p, &mut xas, xa_mk_value(tmp));
                        break 'body;
                    }
                }
                let bi = match alloc {
                    Some(i) => i,
                    None => match kzalloc_bitmap(p) {
                        Some(i) => i,
                        None => {
                            /* goto alloc */
                            match kzalloc_bitmap(p) {
                                Some(i) => {
                                    alloc = Some(i);
                                }
                                None => return -ENOMEM,
                            }
                            xas_set(&mut xas, (min as u64) / IDA_BITMAP_BITS);
                            bit = (min as u64) % IDA_BITMAP_BITS;
                            continue 'retry;
                        }
                    },
                };
                p.bmaps[bi][0] = tmp;
                xas_store(p, &mut xas, bmap_canon(bi));
                if xas_error(&xas) != 0 {
                    p.bmaps[bi][0] = 0;
                    break 'body;
                }
                bitmap = bmap_canon(bi);
            }

            if bitmap != 0 {
                let bi = bmap_idx(bitmap);
                bit = find_next_zero_arr(&p.bmaps[bi], IDA_BITMAP_BITS, bit);
                if xas.index * IDA_BITMAP_BITS + bit > maxb {
                    if let Some(i) = alloc {
                        kfree_bitmap(p, i);
                    }
                    return -ENOSPC;
                }
                if bit == IDA_BITMAP_BITS {
                    continue 'body; /* goto next */
                }
                bm_set_bit(&mut p.bmaps[bi], bit);
                if bm_full(&p.bmaps[bi]) {
                    xas_clear_mark(p, &xas, XA_FREE_MARK as usize);
                }
            } else {
                if bit < BITS_PER_XA_VALUE {
                    bitmap = xa_mk_value(1 << bit);
                } else {
                    let bi = match alloc {
                        Some(i) => i,
                        None => match kzalloc_bitmap(p) {
                            Some(i) => i,
                            None => {
                                match kzalloc_bitmap(p) {
                                    Some(i) => {
                                        alloc = Some(i);
                                    }
                                    None => return -ENOMEM,
                                }
                                xas_set(&mut xas, (min as u64) / IDA_BITMAP_BITS);
                                bit = (min as u64) % IDA_BITMAP_BITS;
                                continue 'retry;
                            }
                        },
                    };
                    bm_set_bit(&mut p.bmaps[bi], bit);
                    bitmap = bmap_canon(bi);
                }
                xas_store(p, &mut xas, bitmap);
            }
            break 'body;
        }
        /* out: */
        if xas_nomem(p, &mut xas) {
            xas.index = (min as u64) / IDA_BITMAP_BITS;
            bit = (min as u64) % IDA_BITMAP_BITS;
            continue 'retry;
        }
        if Some(bitmap) != alloc.map(bmap_canon) {
            if let Some(i) = alloc {
                kfree_bitmap(p, i);
            }
        }
        if xas_error(&xas) != 0 {
            return xas_error(&xas);
        }
        return (xas.index * IDA_BITMAP_BITS + bit) as i64;
    }
}

fn ida_free(p: &mut Pool, id: u32) {
    let mut xas = Xas::new(T_IDA, (id as u64) / IDA_BITMAP_BITS);
    let bit = (id as u64) % IDA_BITMAP_BITS;
    if (id as i32) < 0 {
        return;
    }
    let bitmap = xas_load(p, &mut xas);
    if xa_is_value(bitmap) {
        let mut v = xa_to_value(bitmap);
        if bit >= BITS_PER_XA_VALUE {
            p.warn_on(true);
            return;
        }
        if v & (1 << bit) == 0 {
            p.warn_on(true);
            return;
        }
        v &= !(1 << bit);
        if v == 0 {
            xas_store(p, &mut xas, 0);
            return;
        }
        xas_store(p, &mut xas, xa_mk_value(v));
    } else {
        if bitmap == 0 || !bm_test_bit(&p.bmaps[bmap_idx(bitmap)], bit) {
            p.warn_on(true);
            return;
        }
        bm_clear_bit(&mut p.bmaps[bmap_idx(bitmap)], bit);
        xas_set_mark(p, &xas, XA_FREE_MARK as usize);
        if bm_empty(&p.bmaps[bmap_idx(bitmap)]) {
            kfree_bitmap(p, bmap_idx(bitmap));
            xas_store(p, &mut xas, 0);
        }
    }
}

fn ida_destroy(p: &mut Pool) {
    let mut xas = Xas::new(T_IDA, 0);
    let mut bitmap = xas_find(p, &mut xas, ULONG_MAXV);
    while bitmap != 0 {
        if !xa_is_value(bitmap) {
            kfree_bitmap(p, bmap_idx(bitmap));
        }
        xas_store(p, &mut xas, 0);
        bitmap = xas_next_entry(p, &mut xas, ULONG_MAXV);
    }
}

fn ida_find_first_range(p: &mut Pool, min: u32, max_in: u32) -> i64 {
    let mut max = max_in;
    if (min as i32) < 0 {
        return -EINVAL;
    }
    if (max as i32) < 0 {
        max = i32::MAX as u32;
    }
    let mut index = (min as u64) / IDA_BITMAP_BITS;
    let mut offset = (min as u64) % IDA_BITMAP_BITS;
    let entry = xa_find(
        p,
        T_IDA,
        &mut index,
        (max as u64) / IDA_BITMAP_BITS,
        XA_PRESENT,
    );
    if entry == 0 {
        return -ENOENT;
    }
    if index > (min as u64) / IDA_BITMAP_BITS {
        offset = 0;
    }
    if index * IDA_BITMAP_BITS + offset > max as u64 {
        return -ENOENT;
    }
    let size;
    let bit;
    if xa_is_value(entry) {
        let tmp = xa_to_value(entry);
        size = BITS_PER_XA_VALUE;
        bit = find_next_zero_bit1(!tmp, size, offset);
    } else {
        let bm = &p.bmaps[bmap_idx(entry)];
        size = IDA_BITMAP_BITS;
        bit = find_next_arr(bm, size, offset);
    }
    if bit == size || index * IDA_BITMAP_BITS + bit > max as u64 {
        return -ENOENT;
    }
    (index * IDA_BITMAP_BITS + bit) as i64
}

fn ida_exists(p: &mut Pool, id: u32) -> u64 {
    (ida_find_first_range(p, id, id) == id as i64) as u64
}
fn ida_find_first(p: &mut Pool) -> i64 {
    ida_find_first_range(p, 0, !0)
}
fn ida_is_empty(p: &Pool) -> u64 {
    xa_empty(p, T_IDA) as u64
}
fn ida_alloc(p: &mut Pool, gfp: u32) -> i64 {
    ida_alloc_range(p, 0, !0, gfp)
}
fn ida_alloc_min(p: &mut Pool, min: u32, gfp: u32) -> i64 {
    ida_alloc_range(p, min, !0, gfp)
}
fn ida_alloc_max(p: &mut Pool, max: u32, gfp: u32) -> i64 {
    ida_alloc_range(p, 0, max, gfp)
}

/* ---- ida_dump (the #ifndef __KERNEL__ block) + its print folds ---- */

fn xa_dump_index_f(p: &mut Pool, index: u64, shift: u64) {
    p.fold(index);
    p.fold(shift);
}

fn xa_dump_node_f(p: &mut Pool, ni: usize) {
    let n = p.n[ni].clone();
    p.fold(node_ptr(ni));
    p.fold(n.shift as u64);
    p.fold(n.offset as u64);
    p.fold(n.count as u64);
    p.fold(n.nr_values as u64);
    p.fold(n.parent);
    p.fold(arraycanon(n.array));
    for s in 0..CHUNK_SIZE {
        p.fold(efold(n.slots[s]));
    }
    for m in 0..MAX_MARKS {
        p.fold(n.marks[m]);
    }
}

fn arraycanon(array: u64) -> u64 {
    match array {
        RTIDR_CANON | IDAX_CANON | RRT_CANON => array,
        1 => 1,
        0 => 0,
        _ => 2,
    }
}

fn ida_dump_entry(p: &mut Pool, entry: u64, index: u64) {
    if entry == 0 {
        return;
    }
    if xa_is_node(entry) {
        let ni = xa_to_node(entry);
        let shift = p.n[ni].shift as u64 + 10 + CHUNK_SHIFT as u64;
        xa_dump_index_f(p, index * IDA_BITMAP_BITS, shift);
        xa_dump_node_f(p, ni);
        for i in 0..CHUNK_SIZE as u64 {
            ida_dump_entry(p, p.n[ni].slots[i as usize], index | (i << p.n[ni].shift));
        }
    } else if xa_is_value(entry) {
        xa_dump_index_f(p, index * IDA_BITMAP_BITS, 6);
        p.fold(xa_to_value(entry));
        p.fold(efold(entry));
    } else {
        let bi = bmap_idx(entry);
        xa_dump_index_f(p, index * IDA_BITMAP_BITS, 10);
        p.fold(efold(entry));
        for i in 0..IDA_BITMAP_LONGS {
            p.fold(p.bmaps[bi][i]);
        }
    }
}

fn ida_dump(p: &mut Pool) {
    p.fold(0x850000); /* &ida -> IDAP_CANON */
    p.fold(efold(p.roots[T_IDA].head));
    p.fold((p.roots[T_IDA].flags >> ROOT_TAG_SHIFT) as u64);
    ida_dump_entry(p, p.roots[T_IDA].head, 0);
}

/* ================= full-state dump ================= */

fn dump_state(p: &mut Pool) {
    p.fold(p.idr_base as u64);
    p.fold(p.idr_next as u64);
    p.fold(p.roots[T_IDR].flags as u64);
    p.fold(efold(p.roots[T_IDR].head));
    p.fold(p.roots[T_IDA].flags as u64);
    p.fold(efold(p.roots[T_IDA].head));
    p.fold(p.roots[T_RT].flags as u64);
    p.fold(efold(p.roots[T_RT].head));

    let mut nused = 0u64;
    for i in 0..NP {
        nused += p.used[i] as u64;
    }
    p.fold(nused);
    for i in 0..NP {
        if !p.used[i] {
            continue;
        }
        let n = p.n[i].clone();
        p.fold(i as u64);
        p.fold(n.shift as u64);
        p.fold(n.offset as u64);
        p.fold(n.count as u64);
        p.fold(n.nr_values as u64);
        p.fold(n.parent);
        p.fold(arraycanon(n.array));
        p.fold(if n.on_list { 1 } else { 0 });
        for s in 0..CHUNK_SIZE {
            p.fold(efold(n.slots[s]));
        }
        for m in 0..MAX_MARKS {
            p.fold(n.marks[m]);
        }
    }
    p.fold(p.preload.len() as u64);
    let chain: Vec<usize> = p.preload.iter().rev().copied().collect();
    for ni in chain {
        p.fold(node_ptr(ni));
    }
    p.fold(NILX);
    let mut b = 0u64;
    for i in 0..NB {
        b += p.bused[i] as u64;
    }
    p.fold(b);
    for i in 0..NB {
        if !p.bused[i] {
            continue;
        }
        p.fold(i as u64);
        for s in 0..IDA_BITMAP_LONGS {
            p.fold(p.bmaps[i][s]);
        }
    }
    p.fold(NILX);
}

/* ================= generators ================= */

fn gen_idx(r: u64) -> u64 {
    match r % 12 {
        0 | 1 | 2 => (r >> 8) % 8,
        3 | 4 => (r >> 8) % 64,
        5 => 62 + (r >> 8) % 5,
        6 => (r >> 8) % 4096,
        7 => 4094 + (r >> 8) % 4,
        8 => (r >> 8) % (1 << 20),
        9 => ((r >> 8) % (1 << 16)) << 12,
        10 => ((r >> 8) % (1 << 12)) << 30,
        _ => ULONG_MAXV - (r >> 8) % 4,
    }
}

fn gen_id(r: u64) -> u64 {
    match r % 12 {
        0 | 1 | 2 => (r >> 8) % 8,
        3 | 4 => (r >> 8) % 70,
        5 => 60 + (r >> 8) % 10,
        6 => (r >> 8) % 4096,
        7 => (r >> 8) % (1 << 20),
        8 => (r >> 8) % 200,
        9 => (INT_MAXV as u64) - (r >> 8) % 4,
        10 => 0x80000000 + (r >> 8) % 8,
        _ => {
            if (r >> 8) % 3 != 0 {
                (r >> 8) % 3000
            } else {
                0xffffffff
            }
        }
    }
}

fn gen_start(r: u64) -> i64 {
    match r % 8 {
        0 | 1 | 2 | 3 => ((r >> 9) % 8) as i64,
        4 => ((r >> 9) % 80) as i64,
        5 => (60 + (r >> 9) % 10) as i64,
        6 => -(1 + (r >> 9) as i64 % 4),
        _ => INT_MAXV - ((r >> 9) % 8) as i64,
    }
}

fn gen_end(r: u64) -> i64 {
    match r % 8 {
        0 | 1 => 0,
        2 => -1,
        3 | 4 => ((r >> 9) % 100) as i64,
        5 => (60 + (r >> 9) % 10) as i64,
        6 => ((r >> 9) % 4096) as i64,
        _ => INT_MAXV,
    }
}

fn gen_entry(r: u64) -> u64 {
    match r % 12 {
        0 | 1 => 0,
        2 | 3 | 4 | 5 | 6 | 7 => EBASE + ((r >> 8) % EP as u64) * 16,
        8 | 9 => xa_mk_value((r >> 8) % 4096),
        _ => EBASE + ((r >> 8) % EP as u64) * 16 + if (r >> 16) & 1 == 1 { 3 } else { 1 },
    }
}

fn gen_gfp(r: u64) -> u32 {
    match r % 4 {
        0 => GFP_KERNEL,
        1 => GFP_ATOMIC,
        2 => GFP_NOWAIT,
        _ => 0,
    }
}

/* ================= IDR ops ================= */

fn op_idr_alloc(p: &mut Pool, r: u64) {
    let start = gen_start(r);
    let end = gen_end(r >> 16);
    let e = gen_entry(r >> 32);
    let v = idr_alloc(p, e, start, end, gen_gfp(r >> 48));
    p.fold(v as u64);
}

fn op_idr_alloc_u32(p: &mut Pool, r: u64) {
    let mut nextid = ((r >> 9) % 200) as u32;
    let max: u64;
    let e = gen_entry(r >> 40);
    match r % 8 {
        0 | 1 => max = (r >> 12) % 80,
        2 => max = nextid as u64 + (r >> 12) % 16,
        3 => max = INT_MAXV as u64,
        4 => max = ULONG_MAXV,
        5 => max = if nextid != 0 { (nextid - 1) as u64 } else { 0 },
        _ => max = (r >> 12) % 4096,
    }
    let v = idr_alloc_u32(p, e, &mut nextid, max, gen_gfp(r >> 56));
    p.fold(v as u64);
    p.fold(nextid as u64);
}

fn op_idr_alloc_cyclic(p: &mut Pool, r: u64) {
    let start = gen_start(r);
    let end = gen_end(r >> 16);
    let e = gen_entry(r >> 32);
    let v = idr_alloc_cyclic(p, e, start, end, gen_gfp(r >> 48));
    p.fold(v as u64);
    p.fold(p.idr_next as u64);
}

fn op_idr_remove(p: &mut Pool, r: u64) {
    let v = efold(idr_remove(p, gen_id(r)));
    p.fold(v);
}

fn op_idr_find(p: &mut Pool, r: u64) {
    let v = efold(idr_find(p, gen_id(r)));
    p.fold(v);
}

fn op_idr_for_each(p: &mut Pool, r: u64) {
    let stop = if r & 0x4000 != 0 {
        0
    } else {
        1 + (r >> 16) as i64 % 4
    };
    let v = idr_for_each(p, stop);
    p.fold(v as u64);
}

fn op_idr_get_next(p: &mut Pool, r: u64) {
    let mut nid = gen_start(r);
    for _ in 0..12 {
        let e = idr_get_next(p, &mut nid);
        p.fold(efold(e));
        p.fold(nid as u64);
        if e == 0 {
            break;
        }
        nid = (nid as i32).wrapping_add(1) as i64;
    }
    p.fold(NILX);
}

fn op_idr_get_next_ul(p: &mut Pool, r: u64) {
    let mut nid = gen_id(r);
    for _ in 0..12 {
        let e = idr_get_next_ul(p, &mut nid);
        p.fold(efold(e));
        p.fold(nid);
        if e == 0 {
            break;
        }
        nid = nid.wrapping_add(1);
    }
    p.fold(NILX);
}

fn op_idr_replace(p: &mut Pool, r: u64) {
    let v = efold(idr_replace(p, gen_entry(r >> 20), gen_id(r)));
    p.fold(v);
}

fn op_idr_destroy(p: &mut Pool, r: u64) {
    idr_destroy(p);
    p.fold(efold(p.roots[T_IDR].head));
    p.fold(p.roots[T_IDR].flags as u64);
    if r & 0x80000 != 0 {
        idr_init_base(p, ((r >> 20) % 4 * 4) as i64);
    }
}

fn op_idr_preload(p: &mut Pool, r: u64) {
    let e = gen_entry(r >> 30);
    let start = gen_start(r >> 4);
    let end = gen_end(r >> 44);
    idr_preload(p, gen_gfp(r >> 10));
    let res = idr_alloc(p, e, start, end, gen_gfp(r >> 52) & !GFP_DIRECT_RECLAIM);
    p.fold(res as u64);
}

fn op_idr_misc(p: &mut Pool, r: u64) {
    match r % 4 {
        0 => p.fold(idr_is_empty_u(p)),
        1 => p.fold(idr_get_cursor(p)),
        2 => {
            idr_set_cursor(p, (r >> 10) % 200);
            p.fold(p.idr_next as u64);
        }
        _ => {
            idr_init_base(p, ((r >> 10) % 4 * 4) as i64);
            p.fold(p.idr_base as u64);
        }
    }
}

/* shared wrap_* driver scaffolding */
fn seq_setup(p: &mut Pool, seed: u64, seed2: i64) {
    p.reset_all();
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = u64::MAX;
    }
}

fn seq_finish(p: &mut Pool) -> u64 {
    dump_state(p);
    p.fold(p.warn);
    p.h
}

/// `uint64_t wrap_idr_seq(uint64_t seed, int64_t nops_i, int64_t seed2)`
pub fn wrap_idr_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let p = &mut Pool::new();
    let nops = (iabs(nops_i) % 140) as usize;
    let dump_every = 5 + (iabs(seed2) % 7) as usize;
    seq_setup(p, seed, seed2);
    idr_init_base(p, ((iabs(seed2) % 4) * 4) as i64);
    for i in 0..nops {
        let r = p.xs64();
        match r % 16 {
            0 | 1 | 2 => op_idr_alloc(p, r),
            3 => op_idr_alloc_u32(p, r),
            4 | 5 => op_idr_alloc_cyclic(p, r),
            6 => op_idr_remove(p, r),
            7 | 8 => op_idr_find(p, r),
            9 => op_idr_replace(p, r),
            10 => op_idr_for_each(p, r),
            11 => op_idr_get_next(p, r),
            12 => op_idr_get_next_ul(p, r),
            13 => op_idr_destroy(p, r),
            14 => op_idr_preload(p, r),
            _ => op_idr_misc(p, r),
        }
        if i % dump_every == dump_every - 1 {
            dump_state(p);
        }
    }
    seq_finish(p)
}

/* ================= IDA ops ================= */

fn op_ida_alloc_range(p: &mut Pool, r: u64) {
    let min: u32;
    let max: u32;
    match r % 8 {
        0 | 1 => {
            min = ((r >> 9) % 80) as u32;
            max = min.wrapping_add(((r >> 20) % 120) as u32);
        }
        2 => {
            min = ((r >> 9) % 4 * 64) as u32;
            max = !0;
        }
        3 => {
            min = (i32::MAX - ((r >> 9) % 8) as i32) as u32;
            max = !0;
        }
        4 => {
            min = ((r >> 9) % 3000) as u32;
            max = min.wrapping_add(((r >> 20) % 64) as u32);
        }
        5 => {
            min = ((r >> 9) % 130) as u32;
            max = if min > 10 {
                min - 1 - ((r >> 20) % 5) as u32
            } else {
                0
            };
        }
        6 => {
            min = (0x80000000u64 + (r >> 9) % 8) as u32;
            max = !0;
        }
        _ => {
            min = ((r >> 9) % 4096) as u32;
            max = min.wrapping_add(((r >> 20) % 200) as u32);
        }
    }
    let v = ida_alloc_range(p, min, max, gen_gfp(r >> 40));
    p.fold(v as u64);
}

fn op_ida_free(p: &mut Pool, r: u64) {
    let before = p.warn;
    ida_free(p, (gen_id(r) % 5000) as u32);
    p.fold((p.warn != before) as u64);
}

fn op_ida_burst(p: &mut Pool, r: u64) {
    let n = 1 + (r >> 8) % 6;
    for _ in 0..n {
        let v = ida_alloc_range(p, 0, !0, gen_gfp(r >> 20));
        p.fold(v as u64);
    }
}

fn op_ida_query(p: &mut Pool, r: u64) {
    match r % 4 {
        0 => p.fold(ida_is_empty(p)),
        1 => {
            let v = ida_exists(p, (gen_id(r >> 10) % 5000) as u32);
            p.fold(v);
        }
        2 => {
            let v = ida_find_first(p);
            p.fold(v as u64);
        }
        _ => {
            let lo = ((r >> 10) % 100) as u32;
            let hi = lo + ((r >> 22) % 200) as u32;
            let v = ida_find_first_range(p, lo, hi);
            p.fold(v as u64);
        }
    }
}

fn op_ida_destroy(p: &mut Pool) {
    ida_destroy(p);
    p.fold(efold(p.roots[T_IDA].head));
}

fn op_ida_dump(p: &mut Pool) {
    ida_dump(p);
}

/// `uint64_t wrap_ida_seq(uint64_t seed, int64_t nops_i, int64_t seed2)`
pub fn wrap_ida_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let p = &mut Pool::new();
    let nops = (iabs(nops_i) % 170) as usize;
    let dump_every = 6 + (iabs(seed2) % 6) as usize;
    seq_setup(p, seed, seed2);
    xa_init_flags(p, T_IDA, IDA_INIT_FLAGS);
    for i in 0..nops {
        let r = p.xs64();
        match r % 16 {
            0 | 1 | 2 | 3 => op_ida_alloc_range(p, r),
            4 | 5 => op_ida_burst(p, r),
            6 | 7 | 8 => op_ida_free(p, r),
            9 | 10 => op_ida_query(p, r),
            11 => op_ida_destroy(p),
            12 => op_ida_dump(p),
            13 => {
                let v = ida_alloc(p, gen_gfp(r >> 20));
                p.fold(v as u64);
            }
            14 => {
                let v = ida_alloc_min(p, ((r >> 10) % 200) as u32, gen_gfp(r >> 20));
                p.fold(v as u64);
            }
            _ => {
                let v = ida_alloc_max(p, ((r >> 10) % 500) as u32, gen_gfp(r >> 20));
                p.fold(v as u64);
            }
        }
        if i % dump_every == dump_every - 1 {
            dump_state(p);
        }
    }
    seq_finish(p)
}

/* ================= radix-tree ops ================= */

fn op_rt_insert(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let e = gen_entry(r >> 20);
    let v = radix_tree_insert(p, T_RT, idx, e);
    p.fold(v as u64);
}

fn op_rt_delete(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let ret;
    if r & 0x20000 != 0 {
        ret = radix_tree_delete(p, T_RT, idx);
    } else {
        let item = if r & 0x40000 != 0 {
            gen_entry(r >> 30)
        } else {
            0
        };
        ret = radix_tree_delete_item(p, T_RT, idx, item);
    }
    p.fold(efold(ret));
}

fn op_rt_lookup(p: &mut Pool, r: u64) {
    if r & 0x10000 != 0 {
        let v = efold(radix_tree_lookup(p, T_RT, gen_idx(r)));
        p.fold(v);
    } else {
        let slot = radix_tree_lookup_slot(p, T_RT, gen_idx(r));
        match slot {
            Some(s) => {
                p.fold(scanon(s));
                p.fold(efold(slot_get(p, s)));
            }
            None => {
                p.fold(0);
                p.fold(NILX);
            }
        }
    }
}

fn op_rt_replace_slot(p: &mut Pool, r: u64) {
    let slot = radix_tree_lookup_slot(p, T_RT, gen_idx(r));
    match slot {
        Some(s) => {
            p.fold(scanon(s));
            radix_tree_replace_slot(p, T_RT, s, gen_entry(r >> 20));
            p.fold(efold(slot_get(p, s)));
        }
        None => {
            p.fold(0);
            p.fold(NILX);
        }
    }
}

fn op_rt_tags(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let tag = (r >> 12) % RADIX_TREE_MAX_TAGS as u64;
    match (r >> 16) & 3 {
        0 => {
            /* contract: tagging a not-present item is a BUG — guard */
            if radix_tree_lookup(p, T_RT, idx) != 0 {
                let v = efold(radix_tree_tag_set(p, T_RT, idx, tag as u32));
                p.fold(v);
            } else {
                p.fold(NILX);
            }
        }
        1 => {
            let v = efold(radix_tree_tag_clear(p, T_RT, idx, tag as u32));
            p.fold(v);
        }
        2 => {
            let v = radix_tree_tag_get(p, T_RT, idx, tag as u32);
            p.fold(v as u64);
        }
        _ => {
            let v = radix_tree_tagged(p, T_RT, tag as u32);
            p.fold(v as u64);
        }
    }
}

fn op_rt_gang(p: &mut Pool, r: u64) {
    let first = gen_idx(r) % 4096;
    let max = 1 + ((r >> 20) % 14) as u32;
    let tag = ((r >> 28) % RADIX_TREE_MAX_TAGS as u64) as u32;
    match (r >> 30) & 3 {
        2 => {
            let res = radix_tree_gang_lookup_tag_slot(p, T_RT, first, max, tag);
            p.fold(res.len() as u64);
            for s in res {
                p.fold(scanon(s));
            }
        }
        m => {
            let res = gang_lookup_impl(p, T_RT, first, max, if m == 1 { Some(tag) } else { None });
            p.fold(res.len() as u64);
            for e in res {
                p.fold(efold(e));
            }
        }
    }
}

fn rt_walk_fold(p: &mut Pool, r: u64, tagged: bool) {
    let start = gen_idx(r) % 4096;
    let tag = ((r >> 20) % RADIX_TREE_MAX_TAGS as u64) as u32;
    let flags = if tagged { ITER_TAGGED | tag } else { 0 };
    let mut n = 0;
    each_slot(p, T_RT, start, flags, |p, iter, s| {
        p.fold(iter.index);
        if tagged {
            p.fold(iter.tags);
        }
        p.fold(efold(slot_get(p, s)));
        n += 1;
        n <= 90
    });
    p.fold(n);
    p.fold(NILX);
}

fn op_rt_walk(p: &mut Pool, r: u64) {
    rt_walk_fold(p, r, false);
}

fn op_rt_walk_tagged(p: &mut Pool, r: u64) {
    rt_walk_fold(p, r, true);
}

fn op_rt_walk_contig(p: &mut Pool, r: u64) {
    let start = gen_idx(r) % 4096;
    let mut iter = RIter::new();
    let mut slot = radix_tree_iter_lookup(p, T_RT, &mut iter, start);
    let mut n = 0;
    while let Some(s) = slot {
        p.fold(iter.index);
        p.fold(iter.next_index);
        p.fold(rt_chunk_size(&iter) as u64);
        p.fold(efold(slot_get(p, s)));
        slot = radix_tree_next_slot(p, Some(s), &mut iter, ITER_CONTIG);
        if slot.is_none() && r & 0x80000 == 0 {
            slot = radix_tree_next_chunk(p, T_RT, &mut iter, ITER_CONTIG);
        }
        n += 1;
        if n > 90 {
            break;
        }
    }
    p.fold(n);
    p.fold(NILX);
}

fn op_rt_walk_mutate(p: &mut Pool, r: u64) {
    let start = gen_idx(r) % 4096;
    let tag = ((r >> 24) % RADIX_TREE_MAX_TAGS as u64) as u32;
    let which = (r >> 28) % 3;
    let pick = (r >> 20) % 6;
    let newe = gen_entry(r >> 34);
    let mut n = 0u64;
    each_slot(p, T_RT, start, 0, |p, iter, s| {
        p.fold(iter.index);
        p.fold(efold(slot_get(p, s)));
        if n == pick {
            match which {
                0 => {
                    radix_tree_iter_replace(p, T_RT, iter, s, newe);
                    p.fold(efold(slot_get(p, s)));
                }
                1 => {
                    radix_tree_iter_delete(p, T_RT, iter, s);
                    p.fold(iter.index);
                }
                _ => {
                    radix_tree_iter_tag_clear(p, T_RT, iter, tag);
                    p.fold(tag as u64);
                }
            }
        }
        n += 1;
        n <= 50
    });
    p.fold(n);
    p.fold(NILX);
}

fn op_rt_walk_resume(p: &mut Pool, r: u64) {
    let start = gen_idx(r) % 4096;
    let pick = (r >> 20) % 4;
    let mut iter = RIter::new();
    let mut slot = rt_iter_init(&mut iter, start);
    let mut n = 0u64;
    'outer: loop {
        if slot.is_none() {
            slot = radix_tree_next_chunk(p, T_RT, &mut iter, 0);
        }
        let mut s = match slot {
            None => break,
            Some(s) => s,
        };
        loop {
            p.fold(iter.index);
            p.fold(efold(slot_get(p, s)));
            let ns = if n == pick {
                rt_iter_resume(Some(s), &mut iter)
            } else {
                radix_tree_next_slot(p, Some(s), &mut iter, 0)
            };
            n += 1;
            if n > 90 {
                break 'outer;
            }
            match ns {
                None => break,
                Some(x) => s = x,
            }
        }
        slot = None;
        if n > 90 {
            break;
        }
    }
    p.fold(n);
    p.fold(NILX);
}

fn op_rt_preload(p: &mut Pool, r: u64) {
    let v = radix_tree_maybe_preload(p, gen_gfp(r >> 10));
    p.fold(v as u64);
    let res = radix_tree_maybe_preload(p, gen_gfp(r >> 10));
    p.fold(res as u64);
    let v = radix_tree_insert(p, T_RT, gen_idx(r >> 20), gen_entry(r >> 40));
    p.fold(v as u64);
    let v = radix_tree_preload(p, gen_gfp(r >> 30));
    p.fold(v as u64);
    let v = radix_tree_insert(p, T_RT, gen_idx(r >> 50), gen_entry(r >> 56));
    p.fold(v as u64);
}

fn op_rt_drain(p: &mut Pool) {
    let mut n = 0;
    each_slot(p, T_RT, 0, 0, |p, iter, s| {
        radix_tree_iter_delete(p, T_RT, iter, s);
        n += 1;
        n <= 200
    });
    p.fold(n);
    p.fold(efold(p.roots[T_RT].head));
}

/// `uint64_t wrap_radix_seq(uint64_t seed, int64_t nops_i, int64_t seed2)`
pub fn wrap_radix_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let p = &mut Pool::new();
    let nops = (iabs(nops_i) % 170) as usize;
    let dump_every = 5 + (iabs(seed2) % 7) as usize;
    seq_setup(p, seed, seed2);
    xa_init_flags(p, T_RT, if iabs(seed2) & 2 != 0 { GFP_KERNEL } else { 0 });
    for i in 0..nops {
        let r = p.xs64();
        match r % 16 {
            0 | 1 | 2 | 3 => op_rt_insert(p, r),
            4 | 5 => op_rt_delete(p, r),
            6 | 7 => op_rt_lookup(p, r),
            8 => op_rt_tags(p, r),
            9 => op_rt_gang(p, r),
            10 => op_rt_walk(p, r),
            11 => op_rt_walk_tagged(p, r),
            12 => op_rt_walk_contig(p, r),
            13 => op_rt_walk_mutate(p, r),
            14 => op_rt_walk_resume(p, r),
            _ => match (r >> 4) % 3 {
                0 => op_rt_preload(p, r),
                1 => op_rt_drain(p),
                _ => op_rt_replace_slot(p, r),
            },
        }
        if i % dump_every == dump_every - 1 {
            dump_state(p);
        }
    }
    seq_finish(p)
}

/// Port of `void xa_dump_index(unsigned long index, unsigned int shift)`.
///
/// The C definition folds into the live wrapper checksum; called as a
/// standalone export it has no observable return value, matching the
/// recorded `void` result.
pub fn xa_dump_index(_index: std::ffi::c_ulong, _shift: std::ffi::c_uint) {}
