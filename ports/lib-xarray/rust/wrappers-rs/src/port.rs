//! Safe Rust port of the lib-xarray differential harness.
//!
//! Two layers are ported, both faithfully to `wrappers.c`:
//!   1. upstream `lib/xarray.c` semantics — nodes, slots, marks,
//!      sibling/multi-index entries, the `xa_state` cursor machine,
//!      node alloc/free and the -ENOMEM retry path — modelled on a
//!      deterministic 160-node pool whose "pointers" are the canonical
//!      integer addresses `efold()`/`ncanon()` produce in C; and
//!   2. the wrapper op loops (`gen_*`, `op_*`, `dump_tree`,
//!      `fold_state`) and the four `wrap_xa_*` drivers.
//!
//! Encoding (identical to the C harness):
//!   * node pool pointer for pool slot i  : NBASE + i*4096   (`& 3 == 0`)
//!   * node entry stored in a slot        : (NBASE + i*4096) | 2
//!   * entry pool pointer &epool[i] (+tag): EBASE + i*16 + tag
//!   * internal/sibling/value/error ents  : their integer encodings
//!   * &xa                                : XA_CANON (0x800000)
//!   * xas->xa_node sentinels             : 0/1/3/xa_error()
//!
//! `#![forbid(unsafe_code)]`: every C pointer is an index/canonical u64.

#![forbid(unsafe_code)]
#![allow(clippy::all)]
#![allow(dead_code)]

/* ================= canonical model ================= */

const NP: usize = 160;
const EP: usize = 96;
const NILX: u64 = 0xffff;
const NBASE: u64 = 0x40000;
const NSTRIDE: u64 = 4096;
const EBASE: u64 = 0x10000;
const XA_CANON: u64 = 0x800000;
const LIST_CANON: u64 = 0x900000;
const XA_RCU_FREE: u64 = 1;

const CHUNK_SHIFT: u32 = 6;
const CHUNK_SIZE: usize = 64;
const CHUNK_MASK: u64 = 63;
const MAX_MARKS: usize = 3;

const XA_FLAGS_TRACK_FREE: u32 = 4;
const XA_FLAGS_ZERO_BUSY: u32 = 8;
const XA_FLAGS_ALLOC_WRAPPED: u32 = 16;
const XA_FLAGS_ACCOUNT: u32 = 32;
const GFP_BITS_SHIFT: u32 = 24;
fn xa_flags_mark(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

const XA_MARK_0: u32 = 0;
const XA_MARK_1: u32 = 1;
const XA_MARK_2: u32 = 2;
const XA_PRESENT: u32 = 8;
const XA_FREE_MARK: u32 = XA_MARK_0;
const XA_MARK_MAX: u32 = XA_MARK_2;

const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
const XA_FLAGS_ALLOC1: u32 = XA_FLAGS_TRACK_FREE | XA_FLAGS_ZERO_BUSY;
const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

const ENOMEM: i64 = 12;
const EINVAL: i64 = 22;
const EBUSY: i64 = 16;
const MAX_ERRNO: i64 = 4095;

const XAS_BOUNDS: u64 = 1;
const XAS_RESTART: u64 = 3;

const XA_ZERO_ENTRY: u64 = 1030; /* xa_mk_internal(257) */
const XA_RETRY_ENTRY: u64 = 1026; /* xa_mk_internal(256) */

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
fn xa_to_internal(e: u64) -> u64 {
    e >> 2
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
fn xa_is_sibling(e: u64) -> bool {
    xa_is_internal(e) && e < xa_mk_sibling((CHUNK_SIZE - 1) as u64)
}
fn xa_to_sibling(e: u64) -> u64 {
    e >> 2
}
fn xa_mk_sibling(offset: u64) -> u64 {
    (offset << 2) | 2
}
fn xa_is_retry(e: u64) -> bool {
    e == XA_RETRY_ENTRY
}
fn xa_is_zero(e: u64) -> bool {
    e == XA_ZERO_ENTRY
}
fn xa_is_advanced(e: u64) -> bool {
    xa_is_internal(e) && e <= XA_RETRY_ENTRY
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
fn xas_is_node(xas: &Xas) -> bool {
    xas_valid(xas) && xas.node != 0
}
fn xas_not_node(n: u64) -> bool {
    n & 3 != 0 || n == 0
}
fn xas_frozen(n: u64) -> bool {
    n & 2 != 0
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
    array: u64,  /* XA_CANON or XA_RCU_FREE */
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
    index: u64,
    shift: u8,
    sibs: u8,
    offset: u8,
    node: u64,
    alloc: u64,
    update: bool,
}

impl Xas {
    fn new(index: u64) -> Self {
        Xas {
            index,
            shift: 0,
            sibs: 0,
            offset: 0,
            node: XAS_RESTART,
            alloc: 0,
            update: false,
        }
    }
    /* XA_STATE_ORDER */
    fn new_order(index: u64, order: u32) -> Self {
        let mut x = Xas::new(if order < 64 {
            (index >> order) << order
        } else {
            0
        });
        x.shift = (order - order % CHUNK_SHIFT) as u8;
        x.sibs = ((1u32 << (order % CHUNK_SHIFT)) - 1) as u8;
        x
    }
}

struct Pool {
    n: Vec<Node>,
    used: [bool; NP],
    xa_head: u64,
    xa_flags: u32,
    shadow: Vec<usize>,
    alloc_ctr: u64,
    fail_mask: u64,
    r: u64,
    h: u64,
    warn: u64,
    warn_once: u64,
}

impl Pool {
    fn new() -> Self {
        Pool {
            n: vec![Node::new(); NP],
            used: [false; NP],
            xa_head: 0,
            xa_flags: 0,
            shadow: Vec::new(),
            alloc_ctr: 0,
            fail_mask: 0,
            r: 0,
            h: 0x9e3779b97f4a7c15,
            warn: 0,
            warn_once: 0,
        }
    }
    fn reset_all(&mut self, flags: u32) {
        for i in 0..NP {
            self.n[i] = Node::new();
            self.used[i] = false;
        }
        self.shadow.clear();
        self.xa_head = 0;
        self.xa_flags = flags;
        self.alloc_ctr = 0;
        self.warn = 0;
        self.warn_once = 0;
        self.h = 0x9e3779b97f4a7c15;
    }
    fn xa_track_free(&self) -> bool {
        self.xa_flags & XA_FLAGS_TRACK_FREE != 0
    }
    fn xa_zero_busy(&self) -> bool {
        self.xa_flags & XA_FLAGS_ZERO_BUSY != 0
    }
    fn xa_marked(&self, mark: u32) -> bool {
        self.xa_flags & xa_flags_mark(mark) != 0
    }
    fn xa_mark_set(&mut self, mark: u32) {
        if !self.xa_marked(mark) {
            self.xa_flags |= xa_flags_mark(mark);
        }
    }
    fn xa_mark_clear(&mut self, mark: u32) {
        if self.xa_marked(mark) {
            self.xa_flags &= !xa_flags_mark(mark);
        }
    }
    fn xa_mark_init(&mut self, mark: u32) {
        if self.xa_track_free() && mark == XA_FREE_MARK {
            self.xa_mark_set(mark);
        } else {
            self.xa_mark_clear(mark);
        }
    }
    fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }
    fn warn_on_once(&mut self, bit: u32, c: bool) -> bool {
        if c && self.warn_once & (1 << bit) == 0 {
            self.warn_once |= 1 << bit;
            self.warn += 1;
        }
        c
    }
    fn fold(&mut self, v: u64) {
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("F {:016x} (h was {:016x})", v, self.h);
        }
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

static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Debug hook used by the local diffcheck binary.
pub fn set_trace(on: bool) {
    TRACE.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn iabs(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}

/* ================= slab ================= */

fn kmem_cache_alloc_lru(p: &mut Pool) -> Option<usize> {
    p.alloc_ctr += 1;
    if p.alloc_ctr <= 64 && (p.fail_mask >> (p.alloc_ctr - 1)) & 1 == 1 {
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("AL {} INJ", p.alloc_ctr);
        }
        return None;
    }
    for i in 0..NP {
        if !p.used[i] {
            p.used[i] = true;
            if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
                eprintln!("AL {} ok {}", p.alloc_ctr, i);
            }
            return Some(i);
        }
    }
    if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("AL {} fail", p.alloc_ctr);
    }
    None
}

fn kmem_cache_free(p: &mut Pool, ni: usize) {
    p.used[ni] = false;
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

fn efold(e: u64) -> u64 {
    if e == 0 {
        return 0;
    }
    if xa_is_node(e) {
        /* mirrors the wrappers.c curation: XA_ERROR() encodings also
         * satisfy xa_is_node() but fold raw (not through ncanon) */
        let base = e & !3;
        if base >= NBASE && base < NBASE + NP as u64 * NSTRIDE {
            return base | 2;
        }
        return e;
    }
    e
}

fn dump_tree(p: &mut Pool) {
    p.fold(p.xa_flags as u64);
    let __v1 = efold(p.xa_head);
    p.fold(__v1);
    let mut nused = 0u64;
    for i in 0..NP {
        nused += p.used[i] as u64;
    }
    p.fold(nused);
    for i in 0..NP {
        if !p.used[i] {
            continue;
        }
        p.fold(i as u64);
        let (shift, offset, count, nr_values, parent, array, on_list) = {
            let n = &p.n[i];
            (
                n.shift,
                n.offset,
                n.count,
                n.nr_values,
                n.parent,
                n.array,
                n.on_list,
            )
        };
        p.fold(shift as u64);
        p.fold(offset as u64);
        p.fold(count as u64);
        p.fold(nr_values as u64);
        p.fold(parent);
        p.fold(if array == XA_CANON {
            XA_CANON
        } else if array == XA_RCU_FREE {
            1
        } else {
            2
        });
        p.fold(if on_list { 1 } else { 0 });
        for s in 0..CHUNK_SIZE {
            let __v2 = efold(p.n[i].slots[s]);
            p.fold(__v2);
        }
        for m in 0..MAX_MARKS {
            p.fold(p.n[i].marks[m]);
        }
    }
    let shadow = p.shadow.clone();
    for ni in shadow {
        p.fold(node_ptr(ni));
    }
    p.fold(NILX);
}

fn fold_state(p: &mut Pool, xas: &Xas) {
    p.fold(xas.index);
    p.fold(xas.shift as u64);
    p.fold(xas.sibs as u64);
    p.fold(xas.offset as u64);
    let nf = if xas.node == 0 {
        0
    } else if xas.node == XAS_BOUNDS {
        1
    } else if xas.node == XAS_RESTART {
        3
    } else if xa_err(xas.node) != 0 {
        xas.node
    } else {
        xas.node
    };
    p.fold(nf);
    let e = xas_error(xas);
    p.fold(if e < 0 { (-e) as u64 } else { 0 });
}

/* ================= update callback ================= */

fn upd_cb(p: &mut Pool, ni: usize) {
    if p.n[ni].count != 0 && !p.n[ni].on_list {
        p.shadow.insert(0, ni); /* list_add pushes to the front */
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
fn node_get_marks(n: &Node, offset: u64) -> u32 {
    let mut marks = 0u32;
    for m in 0..MAX_MARKS {
        if node_get_mark(n, offset, m) {
            marks |= 1 << m;
        }
    }
    marks
}
fn node_mark_slots(n: &mut Node, sibs: u64, mark: usize) {
    if sibs == 0 {
        node_mark_all(n, mark);
    } else {
        let mut i = 0u64;
        while i < CHUNK_SIZE as u64 {
            node_set_mark(n, i, mark);
            i += sibs + 1;
        }
    }
}
fn find_next_bit(word: u64, limit: u64, start: u64) -> u64 {
    let mut i = start;
    while i < limit {
        if (word >> i) & 1 == 1 {
            return i;
        }
        i += 1;
    }
    limit
}

/* ================= xas marks ================= */

fn xas_get_mark(p: &Pool, xas: &Xas, mark: usize) -> bool {
    if xas_invalid(xas) {
        return false;
    }
    if xas.node == 0 {
        return p.xa_marked(mark as u32);
    }
    node_get_mark(&p.n[nidx_ptr(xas.node)], xas.offset as u64, mark)
}

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
    if !p.xa_marked(mark as u32) {
        p.xa_mark_set(mark as u32);
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
    if p.xa_marked(mark as u32) {
        p.xa_mark_clear(mark as u32);
    }
}

fn xas_init_marks(p: &mut Pool, xas: &Xas) {
    let mut mark = 0usize;
    loop {
        if p.xa_track_free() && mark as u32 == XA_FREE_MARK {
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

fn xas_squash_marks(p: &mut Pool, xas: &Xas) {
    /* upstream: merge all marks to the first entry — if any sibling slot
     * carries a mark, set it on xa_offset and clear it on the sibs range */
    let ni = nidx_ptr(xas.node);
    let limit = xas.offset as u64 + xas.sibs as u64 + 1;
    let mut mark = 0usize;
    loop {
        if find_next_bit(p.n[ni].marks[mark], limit, xas.offset as u64 + 1) != limit {
            let n = &mut p.n[ni];
            n.marks[mark] |= 1 << xas.offset as u64;
            let lo = xas.offset as u64 + 1;
            let cnt = xas.sibs as u64;
            if cnt >= 64 {
                n.marks[mark] &= !(u64::MAX << lo);
            } else if cnt > 0 {
                n.marks[mark] &= !(((1u64 << cnt) - 1) << lo);
            }
        }
        if mark as u32 == XA_MARK_MAX {
            break;
        }
        mark += 1;
    }
}

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

fn xas_size(xas: &Xas) -> u64 {
    ((xas.sibs as u64) + 1) << xas.shift
}

fn xas_max(xas: &Xas) -> u64 {
    let mut max = xas.index;
    if xas.shift != 0 || xas.sibs != 0 {
        let mask = xas_size(xas).wrapping_sub(1);
        max |= mask;
        if mask == max {
            max = max.wrapping_add(1);
        }
    }
    max
}

fn max_index(p: &Pool, entry: u64) -> u64 {
    if !xa_is_node(entry) {
        return 0;
    }
    ((CHUNK_SIZE as u64) << p.n[xa_to_node(entry)].shift).wrapping_sub(1)
}

fn xas_reload(p: &mut Pool, xas: &mut Xas) -> u64 {
    if xas.node == 0 {
        return p.xa_head;
    }
    let ni = nidx_ptr(xas.node);
    let mut offset = (xas.index >> p.n[ni].shift) & CHUNK_MASK;
    let mut entry = p.n[ni].slots[offset as usize];
    if !xa_is_sibling(entry) {
        return entry;
    }
    offset = xa_to_sibling(entry);
    entry = p.n[ni].slots[offset as usize];
    entry
}

fn xas_start(p: &mut Pool, xas: &mut Xas) -> u64 {
    if xas_valid(xas) {
        return xas_reload(p, xas);
    }
    if xas_error(xas) != 0 {
        return 0;
    }
    let entry = p.xa_head;
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
    let mut offset = get_offset(xas.index, &p.n[ni]);
    let mut entry = p.n[ni].slots[offset as usize];
    xas.node = node_ptr(ni);
    while xa_is_sibling(entry) {
        offset = xa_to_sibling(entry);
        entry = p.n[ni].slots[offset as usize];
        if p.n[ni].shift != 0 && xa_is_node(entry) {
            entry = XA_RETRY_ENTRY;
        }
    }
    xas.offset = offset as u8;
    entry
}

fn xas_load(p: &mut Pool, xas: &mut Xas) -> u64 {
    let mut entry = xas_start(p, xas);
    while xa_is_node(entry) {
        let ni = xa_to_node(entry);
        if xas.shift > p.n[ni].shift as u8 {
            break;
        }
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

fn xas_advance(p: &Pool, xas: &mut Xas, index: u64) {
    let shift = if xas_is_node(xas) && is_node_ptr(xas.node) {
        p.n[nidx_ptr(xas.node)].shift
    } else {
        0
    };
    xas.index = index;
    xas.offset = ((index >> shift) & CHUNK_MASK) as u8;
}

fn xas_set_order(xas: &mut Xas, index: u64, order: u32) {
    /* upstream sets only these four fields — xa_alloc must survive so
     * that a node preallocated by xas_nomem() is still consumed by the
     * retried xas_alloc() (clobbering it leaks nodes and diverges) */
    xas.index = if order < 64 {
        (index >> order) << order
    } else {
        0
    };
    xas.shift = (order - order % CHUNK_SHIFT) as u8;
    xas.sibs = ((1u32 << (order % CHUNK_SHIFT)) - 1) as u8;
    xas.node = XAS_RESTART;
}

fn xas_pause(p: &Pool, xas: &mut Xas) {
    let node = xas.node;
    if xas_invalid(xas) {
        return;
    }
    xas.node = XAS_RESTART;
    if is_node_ptr(node) {
        let ni = nidx_ptr(node);
        let mut offset = xas.offset as u64;
        loop {
            offset += 1;
            if offset >= CHUNK_SIZE as u64 {
                break;
            }
            if !xa_is_sibling(p.n[ni].slots[offset as usize]) {
                break;
            }
        }
        xas.index &= u64::MAX << p.n[ni].shift;
        xas.index = xas
            .index
            .wrapping_add((offset - xas.offset as u64) << p.n[ni].shift);
        if xas.index == 0 {
            xas.node = XAS_BOUNDS;
        }
    } else {
        xas.index = xas.index.wrapping_add(1);
    }
}

fn xas_result(xas: &Xas, mut curr: u64) -> u64 {
    if xas_error(xas) != 0 {
        curr = xas.node;
    }
    curr
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
    match kmem_cache_alloc_lru(p) {
        None => false,
        Some(ni) => {
            xas.alloc = node_ptr(ni);
            p.n[ni].parent = 0;
            xas.node = XAS_RESTART;
            true
        }
    }
}

fn __xas_nomem(p: &mut Pool, xas: &mut Xas) -> bool {
    xas_nomem(p, xas)
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
        match kmem_cache_alloc_lru(p) {
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
    p.n[ni].array = XA_CANON;
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
        if p.xa_zero_busy() {
            entry = xa_zero_to_null(entry);
        }
        xas.node = XAS_BOUNDS;
        p.xa_head = entry;
        if p.xa_track_free() && !node_get_mark(&p.n[node], 0, XA_FREE_MARK as usize) {
            p.xa_mark_clear(XA_FREE_MARK);
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
            p.xa_head = 0;
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
    let max = xas_max(xas);

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
            if p.xa_track_free() && mark == XA_FREE_MARK {
                node_mark_all(&mut p.n[ni], XA_FREE_MARK as usize);
                if !p.xa_marked(XA_FREE_MARK) {
                    node_clear_mark(&mut p.n[ni], 0, XA_FREE_MARK as usize);
                    p.xa_mark_set(XA_FREE_MARK);
                }
            } else if p.xa_marked(mark) {
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
        p.xa_head = head;
        xas_update(p, xas, ni);

        shift += CHUNK_SHIFT;
        node = node_ptr(ni);
    }
    xas.node = node;
    shift as i64
}

/* returns the entry found at the created slot (or NULL) */
fn xas_create(p: &mut Pool, xas: &mut Xas, allow_root: bool) -> u64 {
    let order = xas.shift as u32;
    let mut entry: u64;
    let mut shift: i64;
    /* slot location: None = &xa_head, Some((ni, off)) = node slot */
    let mut slot: Option<(usize, u64)>;

    if xas_top(xas.node) {
        entry = p.xa_head;
        xas.node = 0;
        if entry == 0 && p.xa_zero_busy() {
            entry = XA_ZERO_ENTRY;
        }
        shift = xas_expand(p, xas, entry);
        if shift < 0 {
            return 0;
        }
        if shift == 0 && !allow_root {
            shift = CHUNK_SHIFT as i64;
        }
        entry = p.xa_head;
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
        entry = p.xa_head;
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
                    if p.xa_track_free() {
                        node_mark_all(&mut p.n[ni], XA_FREE_MARK as usize);
                    }
                    match slot {
                        None => p.xa_head = xa_mk_node(ni),
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

fn xas_create_range(p: &mut Pool, xas: &mut Xas) {
    let index = xas.index;
    let shift = xas.shift;
    let sibs = xas.sibs;

    xas.index |= ((sibs as u64 + 1) << shift) - 1;
    if xas_is_node(xas) && is_node_ptr(xas.node) && p.n[nidx_ptr(xas.node)].shift == xas.shift {
        xas.offset |= sibs;
    }
    xas.shift = 0;
    xas.sibs = 0;

    let mut ok = false;
    loop {
        xas_create(p, xas, true);
        if xas_error(xas) != 0 {
            break;
        }
        if xas.index <= (index | CHUNK_MASK) {
            ok = true;
            break;
        }
        xas.index = xas.index.wrapping_sub(CHUNK_SIZE as u64);
        loop {
            let node = xas.node;
            if !is_node_ptr(node) {
                break;
            }
            let ni = nidx_ptr(node);
            if p.n[ni].shift >= shift {
                break;
            }
            xas.node = p.n[ni].parent;
            xas.offset = p.n[ni].offset.wrapping_sub(1);
            if p.n[ni].offset != 0 {
                break;
            }
        }
    }

    if !ok {
        /* restore */
        xas.shift = shift;
        xas.sibs = sibs;
        xas.index = index;
    } else {
        xas.index = index;
        if xas.node != 0 && is_node_ptr(xas.node) {
            xas_set_offset(p, xas);
        }
    }
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

fn xas_store(p: &mut Pool, xas: &mut Xas, mut entry: u64) -> u64 {
    let mut count: i64 = 0;
    let mut values: i64 = 0;
    let value = xa_is_value(entry);
    let mut first: u64;

    if entry != 0 {
        let allow_root = !xa_is_node(entry) && !xa_is_zero(entry);
        first = xas_create(p, xas, allow_root);
    } else {
        first = xas_load(p, xas);
    }

    if xas_invalid(xas) {
        return first;
    }
    let node = xas.node;
    if is_node_ptr(node) && (xas.shift < p.n[nidx_ptr(node)].shift) {
        xas.sibs = 0;
    }
    if first == entry && xas.sibs == 0 {
        return first;
    }

    let mut next = first;
    let mut offset = xas.offset as u32;
    let max = xas.offset as u32 + xas.sibs as u32;
    let ni_opt: Option<usize> = if is_node_ptr(node) {
        Some(nidx_ptr(node))
    } else {
        None
    };
    if ni_opt.is_some() && xas.sibs != 0 {
        xas_squash_marks(p, xas);
    }
    if entry == 0 {
        xas_init_marks(p, xas);
    }

    loop {
        match ni_opt {
            None => p.xa_head = entry,
            Some(ni) => p.n[ni].slots[offset as usize] = entry,
        }
        if xa_is_node(next) && (ni_opt.is_none() || p.n[ni_opt.unwrap()].shift != 0) {
            xas_free_nodes(p, xas, xa_to_node(next));
        }
        if ni_opt.is_none() {
            break;
        }
        let ni = ni_opt.unwrap();
        count += (next == 0) as i64 - (entry == 0) as i64;
        values += (!xa_is_value(first)) as i64 - (!value) as i64;
        if entry != 0 {
            if offset == max {
                break;
            }
            if !xa_is_sibling(entry) {
                entry = xa_mk_sibling(xas.offset as u64);
            }
        } else if offset == CHUNK_MASK as u32 {
            break;
        }
        offset += 1;
        next = p.n[ni].slots[offset as usize];
        if !xa_is_sibling(next) {
            if entry == 0 && offset > max {
                break;
            }
            first = next;
        }
    }

    update_node(p, xas, node, count, values);
    first
}

/* ================= find / iterate ================= */

fn __xas_prev(p: &mut Pool, xas: &mut Xas) -> u64 {
    if !xas_frozen(xas.node) {
        xas.index = xas.index.wrapping_sub(1);
    }
    if xas.node == 0 {
        return set_bounds(xas);
    }
    if xas_not_node(xas.node) {
        return xas_load(p, xas);
    }
    if xas.offset as u64 != get_offset(xas.index, &p.n[nidx_ptr(xas.node)]) {
        xas.offset = xas.offset.wrapping_sub(1);
    }
    while xas.offset == 255 {
        xas.offset = p.n[nidx_ptr(xas.node)].offset.wrapping_sub(1);
        xas.node = p.n[nidx_ptr(xas.node)].parent;
        if xas.node == 0 {
            return set_bounds(xas);
        }
    }
    loop {
        let ni = nidx_ptr(xas.node);
        let entry = p.n[ni].slots[xas.offset as usize];
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas_set_offset(p, xas);
    }
}

fn xas_prev(p: &mut Pool, xas: &mut Xas) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (is_node_ptr(node) && p.n[nidx_ptr(node)].shift != 0)
        || xas.offset == 0
    {
        return __xas_prev(p, xas);
    }
    xas.index = xas.index.wrapping_sub(1);
    xas.offset = xas.offset.wrapping_sub(1);
    p.n[nidx_ptr(node)].slots[xas.offset as usize]
}

fn __xas_next(p: &mut Pool, xas: &mut Xas) -> u64 {
    if !xas_frozen(xas.node) {
        xas.index = xas.index.wrapping_add(1);
    }
    if xas.node == 0 {
        return set_bounds(xas);
    }
    if xas_not_node(xas.node) {
        return xas_load(p, xas);
    }
    if xas.offset as u64 != get_offset(xas.index, &p.n[nidx_ptr(xas.node)]) {
        xas.offset = xas.offset.wrapping_add(1);
    }
    while xas.offset as u64 == CHUNK_SIZE as u64 {
        xas.offset = p.n[nidx_ptr(xas.node)].offset.wrapping_add(1);
        xas.node = p.n[nidx_ptr(xas.node)].parent;
        if xas.node == 0 {
            return set_bounds(xas);
        }
    }
    loop {
        let ni = nidx_ptr(xas.node);
        let entry = p.n[ni].slots[xas.offset as usize];
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas_set_offset(p, xas);
    }
}

fn xas_next(p: &mut Pool, xas: &mut Xas) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (is_node_ptr(node) && p.n[nidx_ptr(node)].shift != 0)
        || xas.offset == CHUNK_MASK as u8
    {
        return __xas_next(p, xas);
    }
    xas.index = xas.index.wrapping_add(1);
    xas.offset = xas.offset.wrapping_add(1);
    p.n[nidx_ptr(node)].slots[xas.offset as usize]
}

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
        if entry != 0 && !xa_is_sibling(entry) {
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
        /* goto out */
        return find_marked_out(p, xas, max);
    } else if xas_top(xas.node) {
        advance = false;
        let entry = p.xa_head;
        xas.node = 0;
        if xas.index > max_index(p, entry) {
            return find_marked_out(p, xas, max);
        }
        if !xa_is_node(entry) {
            if p.xa_marked(mark as u32) {
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
        if !advance {
            let ni = nidx_ptr(xas.node);
            let entry = p.n[ni].slots[xas.offset as usize];
            if xa_is_sibling(entry) {
                xas.offset = xa_to_sibling(entry) as u8;
                xas_move_index(p, xas, xas.offset as u64);
            }
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
        if entry == 0 && !(p.xa_track_free() && mark as u32 == XA_FREE_MARK) {
            continue;
        }
        if xa_is_sibling(entry) {
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

fn xas_find_conflict(p: &mut Pool, xas: &mut Xas) -> u64 {
    if xas_error(xas) != 0 {
        return 0;
    }
    if xas.node == 0 {
        return 0;
    }
    if xas_top(xas.node) {
        let mut curr = xas_start(p, xas);
        if curr == 0 {
            return 0;
        }
        while xa_is_node(curr) {
            let ni = xa_to_node(curr);
            curr = xas_descend(p, xas, ni);
        }
        if curr != 0 {
            return curr;
        }
    }
    if !is_node_ptr(xas.node) {
        return 0;
    }
    if p.n[nidx_ptr(xas.node)].shift > xas.shift {
        return 0;
    }
    loop {
        let ni = nidx_ptr(xas.node);
        if p.n[ni].shift == xas.shift {
            if (xas.offset & xas.sibs) == xas.sibs {
                break;
            }
        } else if xas.offset == CHUNK_MASK as u8 {
            xas.offset = p.n[ni].offset;
            xas.node = p.n[ni].parent;
            if !is_node_ptr(xas.node) {
                break;
            }
            continue;
        }
        xas.offset = xas.offset.wrapping_add(1);
        let ni = nidx_ptr(xas.node);
        let mut curr = p.n[ni].slots[xas.offset as usize];
        if xa_is_sibling(curr) {
            continue;
        }
        while xa_is_node(curr) {
            let cni = xa_to_node(curr);
            xas.node = node_ptr(cni);
            xas.offset = 0;
            curr = p.n[cni].slots[0];
        }
        if curr != 0 {
            return curr;
        }
    }
    xas.offset = xas.offset.wrapping_sub(xas.sibs);
    0
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

fn xas_next_marked(p: &mut Pool, xas: &mut Xas, max: u64, mark: usize) -> u64 {
    let node = xas.node;
    if xas_not_node(node) || (is_node_ptr(node) && p.n[nidx_ptr(node)].shift != 0) {
        return xas_find_marked(p, xas, max, mark);
    }
    let offset = xas_find_chunk(p, xas, true, mark);
    xas.offset = offset as u8;
    xas.index = (xas.index & !CHUNK_MASK).wrapping_add(offset);
    if xas.index > max {
        return 0;
    }
    if offset == CHUNK_SIZE as u64 {
        return xas_find_marked(p, xas, max, mark);
    }
    let entry = p.n[nidx_ptr(node)].slots[offset as usize];
    if entry == 0 {
        return xas_find_marked(p, xas, max, mark);
    }
    entry
}

/* ================= split ================= */

fn __xas_init_node_for_split(p: &mut Pool, xas: &Xas, node: u64, entry: u64) {
    if node == 0 {
        return;
    }
    let ni = nidx_ptr(node);
    let mask = xas.sibs as u64;
    let mut sibling = 0u64;
    p.n[ni].array = XA_CANON;
    for i in 0..CHUNK_SIZE as u64 {
        if (i & mask) == 0 {
            p.n[ni].slots[i as usize] = entry;
            sibling = xa_mk_sibling(i);
        } else {
            p.n[ni].slots[i as usize] = sibling;
        }
    }
}

fn xas_split_alloc(p: &mut Pool, xas: &mut Xas, entry: u64, order: u32) {
    let mut sibs = (1u32 << (order % CHUNK_SHIFT)) - 1;
    if p.warn_on(xas.shift as u32 + 2 * CHUNK_SHIFT <= order) {
        /* goto nomem */
        xas_destroy(p, xas);
        xas_set_err(xas, -ENOMEM);
        return;
    }
    if xas.shift as u32 + CHUNK_SHIFT > order {
        return;
    }
    loop {
        match kmem_cache_alloc_lru(p) {
            None => {
                xas_destroy(p, xas);
                xas_set_err(xas, -ENOMEM);
                return;
            }
            Some(ni) => {
                __xas_init_node_for_split(p, xas, node_ptr(ni), entry);
                p.n[ni].parent = xas.alloc;
                xas.alloc = node_ptr(ni);
            }
        }
        if sibs == 0 {
            break;
        }
        sibs -= 1;
    }
}

fn xas_split(p: &mut Pool, xas: &mut Xas, entry: u64, order: u32) {
    let sibs = (1u64 << (order % CHUNK_SHIFT)) - 1;
    let mut values: i64 = 0;
    let mut curr = xas_load(p, xas);

    let node = xas.node;
    if xas_top(node) || !is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    let marks = node_get_marks(&p.n[ni], xas.offset as u64);

    let mut offset = xas.offset as u32 + sibs as u32;
    loop {
        if (xas.shift as u8) < p.n[ni].shift {
            let child = xas.alloc;
            let cni = nidx_ptr(child);
            xas.alloc = p.n[cni].parent;
            p.n[cni].shift = p.n[ni].shift - CHUNK_SHIFT as u8;
            p.n[cni].offset = offset as u8;
            p.n[cni].count = CHUNK_SIZE as u8;
            p.n[cni].nr_values = if xa_is_value(entry) {
                CHUNK_SIZE as u8
            } else {
                0
            };
            p.n[cni].parent = node;
            {
                let marks_c = marks;
                let sibs_c = xas.sibs as u64;
                /* node_set_marks(node, offset, child, sibs, marks) */
                for m in 0..MAX_MARKS {
                    if marks_c & (1 << m) != 0 {
                        node_set_mark(&mut p.n[ni], offset as u64, m);
                        node_mark_slots(&mut p.n[cni], sibs_c, m);
                    }
                }
            }
            p.n[ni].slots[offset as usize] = xa_mk_node(cni);
            if xa_is_value(curr) {
                values -= 1;
            }
            xas_update(p, xas, cni);
        } else {
            let canon = offset - xas.sibs as u32;
            /* node_set_marks(node, canon, NULL, 0, marks) */
            let marks_c = marks;
            for m in 0..MAX_MARKS {
                if marks_c & (1 << m) != 0 {
                    node_set_mark(&mut p.n[ni], canon as u64, m);
                }
            }
            p.n[ni].slots[canon as usize] = entry;
            while offset > canon {
                p.n[ni].slots[offset as usize] = xa_mk_sibling(canon as u64);
                offset -= 1;
            }
            values +=
                (xa_is_value(entry) as i64 - xa_is_value(curr) as i64) * (xas.sibs as i64 + 1);
        }
        let cond = offset > xas.offset as u32;
        offset = offset.wrapping_sub(1);
        if !cond {
            break;
        }
    }
    p.n[ni].nr_values = p.n[ni].nr_values.wrapping_add(values as u8);
    xas_update(p, xas, ni);
    let _ = &mut curr;
}

fn xas_try_split_min_order(order: u32) -> u32 {
    if order % CHUNK_SHIFT == 0 {
        if order == 0 { 0 } else { order - 1 }
    } else {
        order - (order % CHUNK_SHIFT)
    }
}

fn xas_try_split(p: &mut Pool, xas: &mut Xas, entry: u64, order: u32) {
    let sibs = (1u64 << (order % CHUNK_SHIFT)) - 1;
    let mut values: i64 = 0;
    let curr = xas_load(p, xas);

    let node = xas.node;
    if xas_top(node) || !is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    let marks = node_get_marks(&p.n[ni], xas.offset as u64);
    let mut offset = xas.offset as u32 + sibs as u32;

    if xas.shift < p.n[ni].shift {
        let mut child = xas.alloc;
        let expected_sibs = (1u64 << ((order - 1) % CHUNK_SHIFT)) - 1;
        if sibs != 0 || xas.sibs as u64 != expected_sibs {
            xas_destroy(p, xas);
            xas_set_err(xas, -EINVAL);
            return;
        }
        if child == 0 {
            match kmem_cache_alloc_lru(p) {
                None => {
                    xas_destroy(p, xas);
                    xas_set_err(xas, -ENOMEM);
                    return;
                }
                Some(i) => {
                    child = node_ptr(i);
                    p.n[i].parent = xas.alloc;
                }
            }
        }
        __xas_init_node_for_split(p, xas, child, entry);
        let cni = nidx_ptr(child);
        xas.alloc = p.n[cni].parent;
        p.n[cni].shift = p.n[ni].shift - CHUNK_SHIFT as u8;
        p.n[cni].offset = offset as u8;
        p.n[cni].count = CHUNK_SIZE as u8;
        p.n[cni].nr_values = if xa_is_value(entry) {
            CHUNK_SIZE as u8
        } else {
            0
        };
        p.n[cni].parent = node;
        {
            let sibs_c = xas.sibs as u64;
            for m in 0..MAX_MARKS {
                if marks & (1 << m) != 0 {
                    node_set_mark(&mut p.n[ni], offset as u64, m);
                    node_mark_slots(&mut p.n[cni], sibs_c, m);
                }
            }
        }
        p.n[ni].slots[offset as usize] = xa_mk_node(cni);
        if xa_is_value(curr) {
            values -= 1;
        }
        xas_update(p, xas, cni);
    } else {
        loop {
            let canon = offset - xas.sibs as u32;
            for m in 0..MAX_MARKS {
                if marks & (1 << m) != 0 {
                    node_set_mark(&mut p.n[ni], canon as u64, m);
                }
            }
            p.n[ni].slots[canon as usize] = entry;
            while offset > canon {
                p.n[ni].slots[offset as usize] = xa_mk_sibling(canon as u64);
                offset -= 1;
            }
            values +=
                (xa_is_value(entry) as i64 - xa_is_value(curr) as i64) * (xas.sibs as i64 + 1);
            let cond = offset > xas.offset as u32;
            offset = offset.wrapping_sub(1);
            if !cond {
                break;
            }
        }
    }
    p.n[ni].nr_values = p.n[ni].nr_values.wrapping_add(values as u8);
    xas_update(p, xas, ni);
}

fn xas_get_order(p: &Pool, xas: &Xas) -> i64 {
    let mut order = 0i64;
    if xas.node == 0 || !is_node_ptr(xas.node) {
        return 0;
    }
    let ni = nidx_ptr(xas.node);
    loop {
        let slot = xas.offset as u64 + (1 << order);
        if slot >= CHUNK_SIZE as u64 {
            break;
        }
        if !xa_is_sibling(p.n[ni].slots[slot as usize]) {
            break;
        }
        order += 1;
    }
    order + p.n[ni].shift as i64
}

/* ================= public xa_* API ================= */

fn xa_load(p: &mut Pool, index: u64) -> u64 {
    let mut xas = Xas::new(index);
    let mut entry;
    loop {
        entry = xa_zero_to_null(xas_load(p, &mut xas));
        if !xas_retry(&mut xas, entry) {
            break;
        }
    }
    entry
}

fn __xa_erase(p: &mut Pool, index: u64) -> u64 {
    let mut xas = Xas::new(index);
    let cur = xas_store(p, &mut xas, 0);
    xas_result(&xas, xa_zero_to_null(cur))
}

fn xa_erase(p: &mut Pool, index: u64) -> u64 {
    __xa_erase(p, index)
}

fn __xa_store(p: &mut Pool, index: u64, mut entry: u64) -> u64 {
    let mut xas = Xas::new(index);
    if p.warn_on_once(0, xa_is_advanced(entry)) {
        return xa_error(-EINVAL);
    }
    if p.xa_track_free() && entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    let mut curr;
    loop {
        curr = xas_store(p, &mut xas, entry);
        if p.xa_track_free() {
            xas_clear_mark(p, &xas, XA_FREE_MARK as usize);
        }
        if !__xas_nomem(p, &mut xas) {
            break;
        }
    }
    xas_result(&xas, xa_zero_to_null(curr))
}

fn xa_store(p: &mut Pool, index: u64, entry: u64) -> u64 {
    __xa_store(p, index, entry)
}

fn __xa_cmpxchg_raw(p: &mut Pool, index: u64, old: u64, entry: u64) -> u64 {
    let mut xas = Xas::new(index);
    if p.warn_on_once(1, xa_is_advanced(entry)) {
        return xa_error(-EINVAL);
    }
    let mut curr;
    loop {
        curr = xas_load(p, &mut xas);
        if curr == old {
            xas_store(p, &mut xas, entry);
            if p.xa_track_free() && entry != 0 && curr == 0 {
                xas_clear_mark(p, &xas, XA_FREE_MARK as usize);
            }
        }
        if !__xas_nomem(p, &mut xas) {
            break;
        }
    }
    xas_result(&xas, curr)
}

fn __xa_cmpxchg(p: &mut Pool, index: u64, old: u64, entry: u64) -> u64 {
    xa_zero_to_null(__xa_cmpxchg_raw(p, index, old, entry))
}

fn xa_cmpxchg(p: &mut Pool, index: u64, old: u64, entry: u64) -> u64 {
    __xa_cmpxchg(p, index, old, entry)
}

fn __xa_insert(p: &mut Pool, index: u64, mut entry: u64) -> i64 {
    if entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    let curr = __xa_cmpxchg_raw(p, index, 0, entry);
    let errno = xa_err(curr);
    if errno != 0 {
        return errno;
    }
    if curr != 0 { -EBUSY } else { 0 }
}

fn xa_insert(p: &mut Pool, index: u64, entry: u64) -> i64 {
    __xa_insert(p, index, entry)
}

fn xa_reserve(p: &mut Pool, index: u64) -> i64 {
    xa_err(xa_cmpxchg(p, index, 0, XA_ZERO_ENTRY))
}

fn xa_release(p: &mut Pool, index: u64) {
    let _ = xa_cmpxchg(p, index, XA_ZERO_ENTRY, 0);
}

fn xas_set_range(xas: &mut Xas, first: u64, last: u64) {
    let mut shift = 0u32;
    let mut sibs = last.wrapping_sub(first);
    let mut offset = CHUNK_MASK;

    xas_set(xas, first);
    let mut first_v = first;
    while (first_v & CHUNK_MASK) == 0 {
        if sibs < CHUNK_MASK {
            break;
        }
        if sibs == CHUNK_MASK && offset < CHUNK_MASK {
            break;
        }
        shift += CHUNK_SHIFT;
        if offset == CHUNK_MASK {
            offset = sibs & CHUNK_MASK;
        }
        sibs >>= CHUNK_SHIFT;
        first_v >>= CHUNK_SHIFT;
    }
    offset = first_v & CHUNK_MASK;
    if offset + sibs > CHUNK_MASK {
        sibs = CHUNK_MASK - offset;
    }
    if (((first_v + sibs + 1) << shift).wrapping_sub(1)) > last {
        sibs -= 1;
    }
    xas.shift = shift as u8;
    xas.sibs = sibs as u8;
}

fn xa_store_range(p: &mut Pool, mut first: u64, last: u64, entry: u64) -> u64 {
    let mut xas = Xas::new(0);
    if p.warn_on_once(4, xa_is_internal(entry)) {
        return xa_error(-EINVAL);
    }
    if last < first {
        return xa_error(-EINVAL);
    }
    loop {
        let mut unlock = false;
        if entry != 0 {
            let mut order = 64u32;
            if last.wrapping_add(1) != 0 {
                order = (last.wrapping_add(1)).trailing_zeros();
            }
            xas_set_order(&mut xas, last, order);
            xas_create(p, &mut xas, true);
            if xas_error(&xas) != 0 {
                unlock = true;
            }
        }
        if !unlock {
            loop {
                xas_set_range(&mut xas, first, last);
                xas_store(p, &mut xas, entry);
                if xas_error(&xas) != 0 {
                    break;
                }
                first = first.wrapping_add(xas_size(&xas));
                if first > last {
                    break;
                }
            }
        }
        if !xas_nomem(p, &mut xas) {
            break;
        }
    }
    xas_result(&xas, 0)
}

fn xa_get_order(p: &mut Pool, index: u64) -> i64 {
    let mut xas = Xas::new(index);
    let entry = xas_load(p, &mut xas);
    if entry != 0 {
        xas_get_order(p, &xas)
    } else {
        0
    }
}

fn __xa_alloc(p: &mut Pool, id: &mut u32, mut entry: u64, min: u32, max: u32) -> i64 {
    let mut xas = Xas::new(0);
    if p.warn_on_once(2, xa_is_advanced(entry)) {
        return -EINVAL;
    }
    if p.warn_on_once(3, !p.xa_track_free()) {
        return -EINVAL;
    }
    if entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    loop {
        xas.index = min as u64;
        xas_find_marked(p, &mut xas, max as u64, XA_FREE_MARK as usize);
        if xas.node == XAS_RESTART {
            xas_set_err(&mut xas, -EBUSY);
        } else {
            *id = xas.index as u32;
        }
        xas_store(p, &mut xas, entry);
        xas_clear_mark(p, &xas, XA_FREE_MARK as usize);
        if !__xas_nomem(p, &mut xas) {
            break;
        }
    }
    xas_error(&xas)
}

fn xa_alloc(p: &mut Pool, id: &mut u32, entry: u64, min: u32, max: u32) -> i64 {
    __xa_alloc(p, id, entry, min, max)
}

fn __xa_alloc_cyclic(
    p: &mut Pool,
    id: &mut u32,
    entry: u64,
    min: u32,
    max: u32,
    next: &mut u32,
) -> i64 {
    let m = min;
    let lmin = m.max(*next);
    let mut ret = __xa_alloc(p, id, entry, lmin, max);
    if (p.xa_flags & XA_FLAGS_ALLOC_WRAPPED) != 0 && ret == 0 {
        p.xa_flags &= !XA_FLAGS_ALLOC_WRAPPED;
        ret = 1;
    }
    if ret < 0 && lmin > m {
        ret = __xa_alloc(p, id, entry, m, max);
        if ret == 0 {
            ret = 1;
        }
    }
    if ret >= 0 {
        *next = id.wrapping_add(1);
        if *next == 0 {
            p.xa_flags |= XA_FLAGS_ALLOC_WRAPPED;
        }
    }
    ret
}

fn xa_alloc_cyclic(
    p: &mut Pool,
    id: &mut u32,
    entry: u64,
    min: u32,
    max: u32,
    next: &mut u32,
) -> i64 {
    /* the locked variants hide the "wrapped" indicator:
     * upstream returns `err < 0 ? err : 0` */
    let err = __xa_alloc_cyclic(p, id, entry, min, max, next);
    if err < 0 { err } else { 0 }
}

fn __xa_set_mark(p: &mut Pool, index: u64, mark: usize) {
    let mut xas = Xas::new(index);
    let entry = xas_load(p, &mut xas);
    if entry != 0 {
        xas_set_mark(p, &xas, mark);
    }
}

fn __xa_clear_mark(p: &mut Pool, index: u64, mark: usize) {
    let mut xas = Xas::new(index);
    let entry = xas_load(p, &mut xas);
    if entry != 0 {
        xas_clear_mark(p, &xas, mark);
    }
}

fn xa_set_mark(p: &mut Pool, index: u64, mark: usize) {
    __xa_set_mark(p, index, mark);
}

fn xa_clear_mark(p: &mut Pool, index: u64, mark: usize) {
    __xa_clear_mark(p, index, mark);
}

fn xa_get_mark(p: &mut Pool, index: u64, mark: usize) -> bool {
    let mut xas = Xas::new(index);
    let mut entry = xas_start(p, &mut xas);
    while xas_get_mark(p, &xas, mark) {
        if !xa_is_node(entry) {
            return true;
        }
        let ni = xa_to_node(entry);
        entry = xas_descend(p, &mut xas, ni);
    }
    false
}

fn xa_find(p: &mut Pool, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(*index);
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

fn xas_sibling_check(p: &Pool, xas: &Xas) -> bool {
    if xas.node == 0 || !is_node_ptr(xas.node) {
        return false;
    }
    let ni = nidx_ptr(xas.node);
    let mask = ((CHUNK_SIZE as u64) << p.n[ni].shift) - 1;
    (xas.index & mask) > ((xas.offset as u64) << p.n[ni].shift)
}

fn xa_find_after(p: &mut Pool, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(index.wrapping_add(1));
    let mut entry;
    if xas.index == 0 {
        return 0;
    }
    loop {
        if filter < MAX_MARKS as u32 {
            entry = xas_find_marked(p, &mut xas, max, filter as usize);
        } else {
            entry = xas_find(p, &mut xas, max);
        }
        if xas_invalid(&xas) {
            break;
        }
        if xas_sibling_check(p, &xas) {
            continue;
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

fn xas_extract_present(p: &mut Pool, xas: &mut Xas, dst: &mut [u64; 8], max: u64, n: u32) -> u32 {
    let mut i = 0u32;
    let mut entry = xas_find(p, xas, max);
    while entry != 0 {
        if xas_retry(xas, entry) {
            entry = xas_next_entry(p, xas, max);
            continue;
        }
        dst[i as usize] = entry;
        i += 1;
        if i == n {
            break;
        }
        entry = xas_next_entry(p, xas, max);
    }
    i
}

fn xas_extract_marked(
    p: &mut Pool,
    xas: &mut Xas,
    dst: &mut [u64; 8],
    max: u64,
    n: u32,
    mark: usize,
) -> u32 {
    let mut i = 0u32;
    let mut entry = xas_find_marked(p, xas, max, mark);
    while entry != 0 {
        if xas_retry(xas, entry) {
            entry = xas_next_marked(p, xas, max, mark);
            continue;
        }
        dst[i as usize] = entry;
        i += 1;
        if i == n {
            break;
        }
        entry = xas_next_marked(p, xas, max, mark);
    }
    i
}

fn xa_extract(p: &mut Pool, dst: &mut [u64; 8], start: u64, max: u64, n: u32, filter: u32) -> u32 {
    let mut xas = Xas::new(start);
    if n == 0 {
        return 0;
    }
    if filter < MAX_MARKS as u32 {
        xas_extract_marked(p, &mut xas, dst, max, n, filter as usize)
    } else {
        xas_extract_present(p, &mut xas, dst, max, n)
    }
}

fn xa_destroy(p: &mut Pool) {
    let mut xas = Xas::new(0);
    xas.node = 0;
    let entry = p.xa_head;
    p.xa_head = 0;
    xas_init_marks(p, &xas);
    if p.xa_zero_busy() {
        p.xa_mark_clear(XA_FREE_MARK);
    }
    if xa_is_node(entry) {
        xas_free_nodes(p, &xas, xa_to_node(entry));
    }
}

fn xa_init_flags(p: &mut Pool, flags: u32) {
    p.xa_flags = flags;
    p.xa_head = 0;
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
        _ => u64::MAX - (r >> 8) % 4,
    }
}

fn gen_entry(r: u64) -> u64 {
    match r % 12 {
        0 | 1 => 0,
        2..=7 => EBASE + ((r >> 8) % EP as u64) * 16,
        8 | 9 => xa_mk_value((r >> 8) % 4096),
        10 => EBASE + ((r >> 8) % EP as u64) * 16 + if (r >> 16) & 1 == 1 { 3 } else { 1 },
        _ => XA_ZERO_ENTRY,
    }
}

fn gen_mark(r: u64) -> u32 {
    match r % 4 {
        0 => XA_MARK_0,
        1 => XA_MARK_1,
        2 => XA_MARK_2,
        _ => XA_PRESENT,
    }
}

/* ================= op helpers ================= */

const FLAG_TAB: [u32; 6] = [
    0,
    XA_FLAGS_ZERO_BUSY,
    XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_MARK_0),
    XA_FLAGS_ALLOC,
    XA_FLAGS_ALLOC1,
    XA_FLAGS_ACCOUNT,
];

fn op_store(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let e = gen_entry(r >> 7);
    let old = xa_store(p, idx, e);
    p.fold(efold(old));
    p.fold(if xa_err(old) < 0 {
        (-xa_err(old)) as u64
    } else {
        0
    });
}

fn op_load(p: &mut Pool, r: u64) {
    let __v3 = efold(xa_load(p, gen_idx(r)));
    p.fold(__v3);
}

fn op_erase(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let old = if r & 0x8000 != 0 {
        xa_erase(p, idx)
    } else {
        __xa_erase(p, idx)
    };
    p.fold(efold(old));
}

fn op_cmpxchg(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let s = r >> 9;
    let old = match s % 3 {
        0 => xa_load(p, idx),
        1 => gen_entry(s >> 8),
        _ => 0,
    };
    let newe = gen_entry(s >> 20);
    let ret = xa_cmpxchg(p, idx, old, newe);
    p.fold(efold(ret));
    p.fold(if xa_err(ret) < 0 {
        (-xa_err(ret)) as u64
    } else {
        0
    });
}

fn op_insert(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let e = if r & 0x40000 != 0 {
        0
    } else {
        gen_entry(r >> 24)
    };
    let __v4 = xa_insert(p, idx, e) as u64;
    p.fold(__v4);
}

fn op_reserve(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    if r & 0x800 != 0 {
        let __v5 = xa_reserve(p, idx) as u64;
        p.fold(__v5);
    } else {
        xa_release(p, idx);
    }
    let __v6 = efold(xa_load(p, idx));
    p.fold(__v6);
}

fn op_mark(p: &mut Pool, r: u64) {
    let idx = gen_idx(r);
    let m = ((r >> 11) % MAX_MARKS as u64) as usize;
    match (r >> 14) & 3 {
        0 => xa_set_mark(p, idx, m),
        1 => xa_clear_mark(p, idx, m),
        2 => __xa_set_mark(p, idx, m),
        _ => __xa_clear_mark(p, idx, m),
    }
    let __v7 = xa_get_mark(p, idx, m) as u64;
    p.fold(__v7);
    p.fold(p.xa_marked(m as u32) as u64);
}

/* UB curation, mirrors wrappers.c xa_find_b / xa_find_after_b: upstream
 * xa_find()/xa_find_after() retry unbounded and livelock when a walk
 * resolves an internal-level sibling to a node-child master (endless
 * XA_RETRY_ENTRY).  Identical semantics, bounded at 256 iterations. */
fn xa_find_b(p: &mut Pool, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(*index);
    let mut entry;
    let mut tries = 0u32;
    loop {
        if filter < MAX_MARKS as u32 {
            entry = xas_find_marked(p, &mut xas, max, filter as usize);
        } else {
            entry = xas_find(p, &mut xas, max);
        }
        tries += 1;
        if !xas_retry(&mut xas, entry) || tries >= 256 {
            break;
        }
    }
    if entry != 0 {
        *index = xas.index;
    }
    entry
}

fn xa_find_after_b(p: &mut Pool, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(index.wrapping_add(1));
    let mut entry;
    if xas.index == 0 {
        return 0;
    }
    let mut tries = 0u32;
    loop {
        if filter < MAX_MARKS as u32 {
            entry = xas_find_marked(p, &mut xas, max, filter as usize);
        } else {
            entry = xas_find(p, &mut xas, max);
        }
        if xas_invalid(&xas) {
            break;
        }
        if xas_sibling_check(p, &xas) {
            tries += 1;
            if tries >= 256 {
                break;
            }
            continue;
        }
        if !xas_retry(&mut xas, entry) {
            break;
        }
        tries += 1;
        if tries >= 256 {
            break;
        }
    }
    if entry != 0 {
        *index = xas.index;
    }
    entry
}

fn op_find(p: &mut Pool, r: u64) {
    let mut idx = gen_idx(r);
    let max = if r & 0x100000 != 0 {
        u64::MAX
    } else {
        gen_idx(r >> 21)
    };
    let f = gen_mark(r >> 42);
    let e = xa_find_b(p, &mut idx, max, f);
    p.fold(efold(e));
    p.fold(idx);
}

fn op_find_after(p: &mut Pool, r: u64) {
    let mut idx = gen_idx(r);
    let max = if r & 0x100000 != 0 {
        u64::MAX
    } else {
        gen_idx(r >> 21)
    };
    let f = gen_mark(r >> 42);
    let e = xa_find_after_b(p, &mut idx, max, f);
    p.fold(efold(e));
    p.fold(idx);
}

fn op_extract(p: &mut Pool, r: u64) {
    let start = gen_idx(r);
    let max = if r & 0x200000 != 0 {
        u64::MAX
    } else {
        start + (r >> 12) % 257
    };
    let n = 1 + (r >> 30) % 8;
    let f = gen_mark(r >> 40);
    let mut dst = [0u64; 8];
    let got = xa_extract(p, &mut dst, start, max, n as u32, f);
    p.fold(got as u64);
    for i in 0..got as usize {
        p.fold(efold(dst[i]));
    }
}

fn op_get_order(p: &mut Pool, r: u64) {
    let __v8 = xa_get_order(p, gen_idx(r)) as u64;
    p.fold(__v8);
}

fn op_xas(p: &mut Pool, r: u64, use_update: bool) {
    let idx = gen_idx(r);
    let mut xas = Xas::new(idx);
    if use_update {
        xas.update = true;
    }
    let e = xas_load(p, &mut xas);
    p.fold(efold(e));
    fold_state(p, &xas);

    match (r >> 20) & 7 {
        0 => {
            if e != 0 {
                xas_set_mark(p, &xas, ((r >> 24) % MAX_MARKS as u64) as usize);
            }
            let __v9 = xas_get_mark(p, &xas, XA_MARK_0 as usize) as u64;
            p.fold(__v9);
            let __v10 = xas_get_mark(p, &xas, XA_MARK_1 as usize) as u64;
            p.fold(__v10);
            let __v11 = xas_get_mark(p, &xas, XA_MARK_2 as usize) as u64;
            p.fold(__v11);
        }
        1 => {
            xas_init_marks(p, &xas);
            let __v12 = xas_get_mark(p, &xas, XA_MARK_0 as usize) as u64;
            p.fold(__v12);
            let __v13 = xas_get_mark(p, &xas, XA_MARK_1 as usize) as u64;
            p.fold(__v13);
            let __v14 = xas_get_mark(p, &xas, XA_MARK_2 as usize) as u64;
            p.fold(__v14);
        }
        2 => {
            xas_pause(p, &mut xas);
            fold_state(p, &xas);
            let __v15 = efold(xas_load(p, &mut xas));
            p.fold(__v15);
        }
        3 => {
            for _ in 0..3 {
                let e = xas_next(p, &mut xas);
                p.fold(efold(e));
                fold_state(p, &xas);
            }
        }
        4 => {
            for _ in 0..3 {
                let e = xas_prev(p, &mut xas);
                p.fold(efold(e));
                fold_state(p, &xas);
            }
        }
        5 => {
            xas_reset(&mut xas);
            let __v16 = efold(xas_load(p, &mut xas));
            p.fold(__v16);
        }
        6 => {
            let mut old = xas_store(p, &mut xas, gen_entry(r >> 30));
            while xas_nomem(p, &mut xas) {
                old = xas_store(p, &mut xas, gen_entry(r >> 30));
            }
            p.fold(efold(old));
        }
        _ => {
            let __v17 = efold(xas_store(p, &mut xas, 0));
            p.fold(__v17);
        }
    }
    xas_destroy(p, &mut xas);
}

fn op_order_store(p: &mut Pool, r: u64, maxord: u32) {
    let order = ((r >> 22) % (maxord as u64 + 1)) as u32;
    let idx = gen_idx(r) & !((1u64 << if order > 8 { 8 } else { order }) - 1);
    let e = gen_entry(r >> 40);
    let mut xas = Xas::new_order(idx, order);
    let mut old;
    if e == 0 {
        old = xas_store(p, &mut xas, 0);
        p.fold(efold(old));
        p.fold(if xas_error(&xas) < 0 { 1 } else { 0 });
        xas_destroy(p, &mut xas);
        return;
    }
    loop {
        old = xas_store(p, &mut xas, e);
        if !xas_nomem(p, &mut xas) {
            break;
        }
    }
    p.fold(efold(old));
    p.fold(if xas_error(&xas) < 0 {
        0u64.wrapping_sub(xas_error(&xas) as u64)
    } else {
        0
    });
    fold_state(p, &xas);
    xas_destroy(p, &mut xas);
}

fn op_create_range(p: &mut Pool, r: u64) {
    let order = ((r >> 22) % 5) as u32;
    let idx = gen_idx(r) & !((1u64 << if order > 6 { 6 } else { order }) - 1);
    let mut xas = Xas::new_order(idx, order);
    xas_create_range(p, &mut xas);
    fold_state(p, &xas);
    let __v18 = efold(xas_store(p, &mut xas, gen_entry(r >> 40)));
    p.fold(__v18);
    while xas_nomem(p, &mut xas) {}
    xas_destroy(p, &mut xas);
}

fn op_conflict(p: &mut Pool, r: u64) {
    let order = ((r >> 22) % 7) as u32;
    let idx = gen_idx(r) & !((1u64 << order) - 1);
    let mut xas = Xas::new_order(idx, order);
    let mut cnt = 0i64;
    loop {
        let e = xas_find_conflict(p, &mut xas);
        if e == 0 {
            break;
        }
        p.fold(efold(e));
        p.fold(xas.index);
        cnt += 1;
        if cnt > 8 {
            break;
        }
    }
    p.fold(cnt as u64);
}

fn node_base(p: &Pool, start: usize) -> u64 {
    let mut idx = 0u64;
    let mut n = start;
    while p.n[n].parent != 0 && is_node_ptr(p.n[n].parent) {
        let pni = nidx_ptr(p.n[n].parent);
        idx |= (p.n[n].offset as u64) << p.n[pni].shift;
        n = pni;
    }
    idx
}

fn op_delete_node(p: &mut Pool, r: u64) {
    let mut nth = ((r >> 9) % 8) as i64;
    let mut victim: i64 = -1;
    for i in 0..NP {
        if p.used[i] && p.n[i].array == XA_CANON && p.n[i].parent != 0 && p.n[i].shift == 0 {
            if nth == 0 {
                victim = i as i64;
                break;
            }
            nth -= 1;
        }
    }
    if victim < 0 {
        for i in 0..NP {
            if p.used[i] && p.n[i].array == XA_CANON && p.n[i].shift == 0 {
                victim = i as i64;
                break;
            }
        }
    }
    if victim < 0 {
        p.fold(NILX);
        return;
    }
    let vi = victim as usize;
    let base = node_base(p, vi);
    p.fold(vi as u64);
    for i in 0..CHUNK_SIZE {
        if !p.used[vi] {
            break;
        }
        let e = p.n[vi].slots[i];
        if e != 0 && !xa_is_sibling(e) && !xa_is_node(e) {
            let __v19 = efold(__xa_erase(p, base + i as u64));
            p.fold(__v19);
        }
    }
    p.fold(p.used[vi] as u64);
}

fn op_destroy(p: &mut Pool, flags: u32) {
    xa_destroy(p);
    let __v20 = efold(p.xa_head);
    p.fold(__v20);
    p.fold(p.xa_flags as u64);
    xa_init_flags(p, flags);
    p.fold(flags as u64);
}

/* ================= wrap_xa_multi helpers ================= */

#[derive(Clone, Copy, Default)]
struct Mrec {
    base: u64,
    order: u8,
    live: u8,
}

struct Mult {
    m: [Mrec; 24],
}

impl Mult {
    fn new() -> Self {
        Mult {
            m: [Mrec::default(); 24],
        }
    }
    fn note(&mut self, base: u64, order: u32) {
        for i in 0..24 {
            if self.m[i].live == 0 {
                self.m[i].base = base;
                self.m[i].order = order as u8;
                self.m[i].live = 1;
                return;
            }
        }
    }
    fn kill(&mut self, idx: u64) {
        for i in 0..24 {
            if self.m[i].live != 0
                && idx >= self.m[i].base
                && idx - self.m[i].base < (1u64 << self.m[i].order)
            {
                self.m[i].live = 0;
            }
        }
    }
    fn pick(&self, r: u64) -> i64 {
        let mut nth = (r % 4) as i64;
        for i in 0..24 {
            if self.m[i].live != 0 {
                if nth == 0 {
                    return i as i64;
                }
                nth -= 1;
            }
        }
        for i in 0..24 {
            if self.m[i].live != 0 {
                return i as i64;
            }
        }
        -1
    }
}

fn op_multi_store(p: &mut Pool, mult: &mut Mult, r: u64, maxord: u32) {
    let order = ((r >> 22) % (maxord as u64 + 1)) as u32;
    let idx = gen_idx(r) & !((1u64 << order) - 1);
    let e = gen_entry(r >> 40);
    let mut xas = Xas::new_order(idx, order);
    mult.kill(idx);
    mult.kill(idx.wrapping_add((1u64 << order) - 1));
    let mut old;
    loop {
        old = xas_store(p, &mut xas, e);
        if !xas_nomem(p, &mut xas) {
            break;
        }
    }
    p.fold(efold(old));
    fold_state(p, &xas);
    if e != 0 && xas_error(&xas) == 0 && order != 0 {
        mult.note(idx, order);
    }
    xas_destroy(p, &mut xas);
}

fn op_split(p: &mut Pool, mult: &mut Mult, r: u64) {
    let mi = mult.pick(r >> 9);
    let mut xas = Xas::new(0);
    if mi < 0 {
        p.fold(NILX);
        return;
    }
    let idx = mult.m[mi as usize].base;
    let order = xa_get_order(p, idx);
    if order == 0 {
        mult.m[mi as usize].live = 0;
        p.fold(NILX);
        return;
    }
    let order = order as u32;
    let neword = ((r >> 20) % order as u64) as u32;
    let mut e = gen_entry(r >> 30);
    if e == 0 || xa_is_advanced(e) {
        e = EBASE + ((r >> 40) % EP as u64) * 16;
    }
    xas_set_order(
        &mut xas,
        idx.wrapping_add((r >> 44) % (1u64 << order)),
        neword,
    );
    let _ = xas_load(p, &mut xas);
    if xas.node > XAS_RESTART
        && is_node_ptr(xas.node)
        && p.n[nidx_ptr(xas.node)].shift as u32 == order - (order % CHUNK_SHIFT)
    {
        xas_split_alloc(p, &mut xas, e, order);
        let err = xas_error(&xas);
        p.fold(if err < 0 { (-err) as u64 } else { 0 });
        if err == 0 {
            xas_split(p, &mut xas, e, order);
        }
    } else {
        p.fold(NILX);
    }
    fold_state(p, &xas);
    mult.m[mi as usize].live = 0;
    if neword != 0 {
        mult.note(xas.index, neword);
    }
    xas_destroy(p, &mut xas);
}

fn op_try_split(p: &mut Pool, mult: &mut Mult, r: u64) {
    let mi = mult.pick(r >> 9);
    let mut xas = Xas::new(0);
    if mi < 0 {
        p.fold(NILX);
        return;
    }
    let idx = mult.m[mi as usize].base;
    let order = xa_get_order(p, idx);
    if order == 0 {
        mult.m[mi as usize].live = 0;
        p.fold(NILX);
        return;
    }
    let order = order as u32;
    let neword = if (r >> 20) & 1 == 1 {
        order - 1
    } else {
        xas_try_split_min_order(order)
    };
    let mut e = gen_entry(r >> 30);
    if e == 0 || xa_is_advanced(e) {
        e = EBASE + ((r >> 40) % EP as u64) * 16;
    }
    xas_set_order(
        &mut xas,
        idx.wrapping_add((r >> 44) % (1u64 << order)),
        neword,
    );
    let _ = xas_load(p, &mut xas);
    if xas.node > XAS_RESTART
        && is_node_ptr(xas.node)
        && p.n[nidx_ptr(xas.node)].shift as u32 == order - (order % CHUNK_SHIFT)
    {
        xas_try_split(p, &mut xas, e, order);
        let err = xas_error(&xas);
        p.fold(if err < 0 { (-err) as u64 } else { 0 });
    } else {
        p.fold(NILX);
    }
    fold_state(p, &xas);
    mult.m[mi as usize].live = 0;
    if neword != 0 {
        mult.note(xas.index, neword);
    }
    xas_destroy(p, &mut xas);
}

fn op_store_range(p: &mut Pool, mult: &mut Mult, r: u64) {
    let mut first = gen_idx(r) % 8192;
    let mut last = first + (r >> 21) % 300;
    if r & 0x800000 != 0 {
        std::mem::swap(&mut first, &mut last);
    }
    let e = gen_entry(r >> 34);
    let __v21 = efold(xa_store_range(p, first, last, e));
    p.fold(__v21);
    mult.kill(first);
    mult.kill(last);
}

/* ================= drivers ================= */

pub fn wrap_xa_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    const MIX: [i64; 8] = [20, 15, 12, 8, 8, 10, 15, 12];
    let nops = (iabs(nops_i) % 160) as usize;
    let flags = FLAG_TAB[(iabs(seed2) % 6) as usize];
    let use_update = ((iabs(seed2) >> 4) & 1) == 1;
    let dump_every = 5 + (iabs(seed2) % 7) as usize;
    let mut p = Pool::new();

    p.reset_all(flags);
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = u64::MAX;
    }

    for i in 0..nops {
        let r = p.xs64();
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("OP {} {:016x}", i, r);
        }
        let t = (r % 100) as i64;
        let mut c = 0i64;
        if t < {
            c += MIX[0];
            c
        } {
            op_store(&mut p, r);
        } else if t < {
            c += MIX[1];
            c
        } {
            op_load(&mut p, r);
        } else if t < {
            c += MIX[2];
            c
        } {
            op_erase(&mut p, r);
        } else if t < {
            c += MIX[3];
            c
        } {
            op_cmpxchg(&mut p, r);
        } else if t < {
            c += MIX[4];
            c
        } {
            op_insert(&mut p, r);
        } else if t < {
            c += MIX[5];
            c
        } {
            match (r >> 4) % 4 {
                0 => op_mark(&mut p, r),
                1 => op_find(&mut p, r),
                2 => op_find_after(&mut p, r),
                _ => op_reserve(&mut p, r),
            }
        } else if t < {
            c += MIX[6];
            c
        } {
            match (r >> 4) % 6 {
                0 => op_xas(&mut p, r, use_update),
                1 => op_extract(&mut p, r),
                2 => op_get_order(&mut p, r),
                3 => op_create_range(&mut p, r),
                4 => op_order_store(&mut p, r, 4),
                _ => op_delete_node(&mut p, r),
            }
        } else {
            match (r >> 4) % 6 {
                0 => op_order_store(&mut p, r, 7),
                1 => op_conflict(&mut p, r),
                2 => op_destroy(&mut p, flags),
                3 => {
                    let mut id: u32 = 0xdead;
                    let lo = ((r >> 8) % 16) as u32;
                    let hi = (32 + (r >> 16) % 96) as u32;
                    let e = gen_entry(r >> 40);
                    let err = __xa_alloc(&mut p, &mut id, e, lo, hi);
                    p.fold(err as u64);
                    p.fold(id as u64);
                }
                4 => {
                    let mut xas = Xas::new(gen_idx(r >> 9));
                    let max = if r & 0x4000 != 0 {
                        u64::MAX
                    } else {
                        gen_idx(r >> 30)
                    };
                    let e = xas_find(&mut p, &mut xas, max);
                    p.fold(efold(e));
                    fold_state(&mut p, &xas);
                }
                _ => {
                    let mut xas = Xas::new(gen_idx(r >> 9));
                    let max = if r & 0x4000 != 0 {
                        u64::MAX
                    } else {
                        gen_idx(r >> 30)
                    };
                    let e = xas_find_marked(
                        &mut p,
                        &mut xas,
                        max,
                        ((r >> 44) % MAX_MARKS as u64) as usize,
                    );
                    p.fold(efold(e));
                    fold_state(&mut p, &xas);
                }
            }
        }
        if i % dump_every == dump_every - 1 {
            dump_tree(&mut p);
        }
    }
    dump_tree(&mut p);
    p.fold(p.warn);
    p.h
}

pub fn wrap_xa_multi(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let nops = (iabs(nops_i) % 140) as usize;
    let flags = FLAG_TAB[(iabs(seed2) % 6) as usize] & !XA_FLAGS_TRACK_FREE;
    let dump_every = 4 + (iabs(seed2) % 6) as usize;
    let mut p = Pool::new();
    let mut mult = Mult::new();

    p.reset_all(flags);
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = u64::MAX;
    }

    for i in 0..nops {
        let r = p.xs64();
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("OP {} {:016x}", i, r);
        }
        match (r % 16) as u32 {
            0..=3 => op_multi_store(&mut p, &mut mult, r, 6),
            4 => op_multi_store(&mut p, &mut mult, r, 10),
            5 | 6 => op_split(&mut p, &mut mult, r),
            7 => op_try_split(&mut p, &mut mult, r),
            8 => op_store_range(&mut p, &mut mult, r),
            9 => op_load(&mut p, r),
            10 => {
                op_erase(&mut p, r);
                mult.kill(gen_idx(r));
            }
            11 => op_get_order(&mut p, r),
            12 => op_conflict(&mut p, r),
            13 => op_create_range(&mut p, r),
            14 => op_find(&mut p, r),
            _ => op_xas(&mut p, r, false),
        }
        if i % dump_every == dump_every - 1 {
            dump_tree(&mut p);
        }
    }
    dump_tree(&mut p);
    p.fold(p.warn);
    p.h
}

pub fn wrap_xa_alloc(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let nops = (iabs(nops_i) % 200) as usize;
    let flags = if iabs(seed2) & 2 != 0 {
        XA_FLAGS_ALLOC1
    } else {
        XA_FLAGS_ALLOC
    };
    let mut next: u32 = (iabs(seed2) % 8) as u32;
    let dump_every = 6 + (iabs(seed2) % 6) as usize;
    let mut p = Pool::new();

    p.reset_all(flags);
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = u64::MAX;
    }

    for i in 0..nops {
        let r = p.xs64();
        let lo = ((r >> 7) % 24) as u32;
        let hi = lo + ((r >> 15) % 80) as u32;
        let mut id: u32 = 0xbad;
        let err: i64;
        match (r % 12) as u32 {
            0..=3 => {
                let e = gen_entry(r >> 30);
                err = xa_alloc(&mut p, &mut id, e, lo, hi);
                p.fold(err as u64);
                p.fold(id as u64);
            }
            4 | 5 => {
                let e = gen_entry(r >> 30);
                err = __xa_alloc(&mut p, &mut id, e, lo, hi);
                p.fold(err as u64);
                p.fold(id as u64);
            }
            6 | 7 => {
                let e = gen_entry(r >> 30);
                err = __xa_alloc_cyclic(&mut p, &mut id, e, lo, hi, &mut next);
                p.fold(err as u64);
                p.fold(id as u64);
                p.fold(next as u64);
            }
            8 => {
                let e = gen_entry(r >> 30);
                err = xa_alloc_cyclic(&mut p, &mut id, e, lo, hi, &mut next);
                p.fold(err as u64);
                p.fold(id as u64);
                p.fold(next as u64);
            }
            9 => op_erase(&mut p, r),
            10 => op_load(&mut p, r),
            _ => op_find(&mut p, r),
        }
        if i % dump_every == dump_every - 1 {
            dump_tree(&mut p);
        }
    }
    dump_tree(&mut p);
    p.fold(p.warn);
    p.h
}

pub fn wrap_xa_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    let n = (iabs(n_i) % 90) as usize;
    let walk = (iabs(seed2) % 4) as usize;
    let mut p = Pool::new();

    p.reset_all(FLAG_TAB[(iabs(seed2) % 6) as usize]);
    p.r = seed | 1;
    p.fail_mask = 0;

    /* build phase */
    for i in 0..n {
        let r = p.xs64();
        if r % 5 == 0 {
            let order = ((r >> 9) % 5) as u32;
            let idx = gen_idx(r) & !((1u64 << order) - 1);
            let e = EBASE + ((r >> 20) % EP as u64) * 16;
            let mut xas = Xas::new_order(idx, order);
            let mut old;
            loop {
                old = xas_store(&mut p, &mut xas, e);
                if !xas_nomem(&mut p, &mut xas) {
                    break;
                }
            }
            p.fold(efold(old));
            xas_destroy(&mut p, &mut xas);
        } else {
            let idx = gen_idx(r);
            let mut e = gen_entry(r >> 9);
            if e == 0 {
                e = EBASE + ((r >> 20) % EP as u64) * 16;
            }
            let __v22 = efold(xa_store(&mut p, idx, e));
            p.fold(__v22);
        }
        if r % 7 == 0 {
            xa_set_mark(&mut p, gen_idx(r >> 25), ((r >> 33) % 3) as usize);
        }
        if r % 11 == 0 {
            xa_clear_mark(&mut p, gen_idx(r >> 25), ((r >> 33) % 3) as usize);
        }
        let _ = i;
    }
    dump_tree(&mut p);

    /* walk phase */
    if walk == 0 {
        let mut index: u64 = 0;
        let mut steps = 0i64;
        let mut entry = xa_find_b(&mut p, &mut index, u64::MAX, XA_PRESENT);
        while entry != 0 {
            p.fold(index);
            p.fold(efold(entry));
            steps += 1;
            if steps > 80 {
                break;
            }
            entry = xa_find_after_b(&mut p, &mut index, u64::MAX, XA_PRESENT);
        }
        p.fold(NILX);
        p.fold((steps > 80) as u64);
        {
            let s = gen_idx(p.xs64());
            let l = s + p.xs64() % 5000;
            steps = 0;
            let mut index = s;
            let mut entry = xa_find_b(&mut p, &mut index, l, XA_PRESENT);
            while entry != 0 {
                p.fold(index);
                p.fold(efold(entry));
                steps += 1;
                if steps > 80 {
                    break;
                }
                entry = xa_find_after_b(&mut p, &mut index, l, XA_PRESENT);
            }
            p.fold(NILX);
            p.fold((steps > 80) as u64);
        }
        steps = 0;
        let mark = (iabs(seed2) % MAX_MARKS as u64) as u32;
        let mut index: u64 = 0;
        let mut entry = xa_find_b(&mut p, &mut index, u64::MAX, mark);
        while entry != 0 {
            p.fold(index);
            p.fold(efold(entry));
            steps += 1;
            if steps > 80 {
                break;
            }
            entry = xa_find_after_b(&mut p, &mut index, u64::MAX, mark);
        }
        p.fold(NILX);
        p.fold((steps > 80) as u64);
    } else if walk == 1 {
        let mut xas = Xas::new(gen_idx(p.xs64()));
        let max = if p.xs64() % 3 != 0 {
            u64::MAX
        } else {
            gen_idx(p.xs64())
        };
        let mut steps = 0i64;
        let mut entry = xas_find(&mut p, &mut xas, max);
        while entry != 0 {
            p.fold(xas.index);
            p.fold(efold(entry));
            steps += 1;
            if steps % 3 == 0 {
                xas_pause(&p, &mut xas);
            }
            if steps > 60 {
                break;
            }
            entry = xas_next_entry(&mut p, &mut xas, max);
        }
        p.fold(NILX);
        {
            let mut xm = Xas::new(0);
            let mk = (p.xs64() % 3) as usize;
            steps = 0;
            let mut entry = xas_find_marked(&mut p, &mut xm, u64::MAX, mk);
            while entry != 0 {
                p.fold(xm.index);
                p.fold(efold(entry));
                steps += 1;
                if steps > 60 {
                    break;
                }
                entry = xas_next_marked(&mut p, &mut xm, u64::MAX, mk);
            }
            p.fold(NILX);
        }
    } else if walk == 2 {
        let start = gen_idx(p.xs64()) % 512;
        let mut xas = Xas::new(start);
        let mut e = xas_load(&mut p, &mut xas);
        p.fold(efold(e));
        for _ in 0..80 {
            e = xas_next(&mut p, &mut xas);
            p.fold(efold(e));
            p.fold(xas.index);
            if xas.node == XAS_BOUNDS {
                break;
            }
        }
        p.fold(NILX);
        {
            let mut xp = Xas::new(xas.index);
            for _ in 0..80 {
                e = xas_prev(&mut p, &mut xp);
                p.fold(efold(e));
                p.fold(xp.index);
                if xp.node == XAS_BOUNDS {
                    break;
                }
            }
        }
        p.fold(NILX);
    } else {
        let mut index: u64 = 0;
        let mut k = 0i64;
        loop {
            let entry = xa_find_b(&mut p, &mut index, u64::MAX, XA_PRESENT);
            if entry == 0 {
                break;
            }
            p.fold(index);
            if k % 2 == 0 {
                let __v23 = efold(__xa_erase(&mut p, index));
                p.fold(__v23);
            }
            k += 1;
            if k > 200 || index == u64::MAX {
                break;
            }
            index += 1;
        }
        p.fold(NILX);
        let mut index: u64 = 0;
        let mut entry = xa_find_b(&mut p, &mut index, u64::MAX, XA_PRESENT);
        while entry != 0 {
            p.fold(index);
            p.fold(efold(entry));
            entry = xa_find_after_b(&mut p, &mut index, u64::MAX, XA_PRESENT);
        }
        p.fold(NILX);
    }
    dump_tree(&mut p);
    p.fold(p.warn);
    p.h
}
