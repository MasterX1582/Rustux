//! `lib/radix-tree.c` + the shared node internals it shares with `lib/idr.c`
//! — adopted, verified port (879/879 differential cases + gate in
//! `ports/lib-idr-radix`).
//!
//! Upstream `struct radix_tree_node` IS `struct xa_node` — this engine uses
//! `klib::xarray::Node` as its node type. Nodes live in a caller-owned
//! `&'a mut [Node]` arena (dense indices, canonical `NBASE`-encoded
//! pointers); `used` on each node is the slab bitmap; dirty nodes ride the
//! intrusive `shadow_head` list; `preload` models the per-cpu preload stack
//! (capped at `RADIX_TREE_PRELOAD_SIZE`, the kernel's bound).
//! `alloc_ctr`/`fail_mask` inject allocation failures; `warn`/`warn_once`
//! count `WARN_ON` trips.
//!
//! Invariants: this engine is order-0 — `radix_tree_insert`/`idr_alloc`
//! never create multi-order (sibling) entries, matching the upstream
//! deprecated API surface (which cannot create them either). `RadixTree`
//! is the engine embedded in `Idr`/`Ida` (upstream `idr.idr_rt`,
//! `ida.xa`); keep each arena stable and exclusive to one instance.

use crate::xarray::Node;


/* ---- canonical encodings / constants ---- */
pub(crate) const NILX: u64 = 0xffff;
pub(crate) const NBASE: u64 = 0x40000;
pub(crate) const NSTRIDE: u64 = 4096;
pub(crate) const BBASE: u64 = 0x200000;
pub(crate) const RTIDR_CANON: u64 = 0x810000;
pub(crate) const IDAX_CANON: u64 = 0x820000;
pub(crate) const RRT_CANON: u64 = 0x830000;
pub(crate) const XA_RCU_FREE: u64 = 1;
pub(crate) const CHUNK_SHIFT: u32 = 6;
pub(crate) const CHUNK_SIZE: usize = 64;
pub(crate) const CHUNK_MASK: u64 = 63;
pub(crate) const MAX_MARKS: usize = 3;
pub(crate) const XA_FLAGS_TRACK_FREE: u32 = 4;
pub(crate) const XA_FLAGS_ZERO_BUSY: u32 = 8;
pub(crate) const GFP_BITS_SHIFT: u32 = 24;
pub(crate) const ROOT_TAG_SHIFT: u32 = GFP_BITS_SHIFT;
pub(crate) const ROOT_IS_IDR: u32 = 4;
pub(crate) const IDR_RT_MARKER: u32 = ROOT_IS_IDR | (1 << (ROOT_TAG_SHIFT + IDR_FREE));
pub(crate) const XA_LOCK_IRQ: u32 = 1;
pub(crate) const XA_FLAGS_LOCK_IRQ: u32 = XA_LOCK_IRQ;
pub(crate) const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
pub(crate) const IDA_INIT_FLAGS: u32 = XA_FLAGS_LOCK_IRQ | XA_FLAGS_ALLOC;
pub(crate) const XA_MARK_0: u32 = 0;
pub(crate) const XA_MARK_2: u32 = 2;
pub(crate) const XA_PRESENT: u32 = 8;
pub(crate) const XA_FREE_MARK: u32 = XA_MARK_0;
pub(crate) const XA_MARK_MAX: u32 = XA_MARK_2;
pub(crate) const IDR_FREE: u32 = 0;
pub(crate) const ENOMEM: i64 = 12;
pub(crate) const EINVAL: i64 = 22;
pub(crate) const ENOENT: i64 = 2;
pub(crate) const EEXIST: i64 = 17;
pub(crate) const ENOSPC: i64 = 28;
pub(crate) const MAX_ERRNO: i64 = 4095;
pub(crate) const XAS_BOUNDS: u64 = 1;
pub(crate) const XAS_RESTART: u64 = 3;
pub(crate) const XA_ZERO_ENTRY: u64 = 1030;
pub(crate) const XA_RETRY_ENTRY: u64 = 1026;
pub(crate) const RADIX_TREE_RETRY: u64 = XA_RETRY_ENTRY;
pub const IDA_BITMAP_LONGS: usize = 16;
pub const IDA_BITMAP_BITS: u64 = (IDA_BITMAP_LONGS * 64) as u64;
pub(crate) const BITS_PER_XA_VALUE: u64 = 63;
pub(crate) const BITS_PER_LONG: u64 = 64;
pub const RADIX_TREE_MAX_TAGS: u32 = MAX_MARKS as u32;
pub const RADIX_TREE_MAP_SHIFT: u32 = CHUNK_SHIFT;
pub const RADIX_TREE_MAP_SIZE: u64 = CHUNK_SIZE as u64;
pub const RADIX_TREE_MAP_MASK: u64 = CHUNK_MASK;
pub(crate) const RADIX_TREE_INDEX_BITS: u32 = 64;
pub const RADIX_TREE_MAX_PATH: u32 = RADIX_TREE_INDEX_BITS.div_ceil(CHUNK_SHIFT);
pub const RADIX_TREE_PRELOAD_SIZE: u32 = RADIX_TREE_MAX_PATH * 2 - 1; /* 21 */
pub(crate) const IDR_MAX_PATH: u32 = 31u32.div_ceil(CHUNK_SHIFT); /* 6 */
pub(crate) const IDR_PRELOAD_SIZE: u32 = IDR_MAX_PATH * 2 - 1; /* 11 */
pub const ITER_TAG_MASK: u32 = 0x0f;
pub const ITER_TAGGED: u32 = 0x10;
pub const ITER_CONTIG: u32 = 0x20;
pub(crate) const GFP_DIRECT_RECLAIM: u32 = 1 << 10;
pub(crate) const GFP_KSWAPD_RECLAIM: u32 = 1 << 11;
pub(crate) const GFP_NOWARN: u32 = 1 << 13;
pub(crate) const GFP_ACCOUNT: u32 = 1 << 22;
pub(crate) const GFP_HIGH: u32 = 1 << 5;
pub(crate) const GFP_IO: u32 = 1 << 6;
pub(crate) const GFP_FS: u32 = 1 << 7;
pub(crate) const GFP_BITS_MASK: u32 = (1 << GFP_BITS_SHIFT) - 1;
pub(crate) const GFP_ZONEMASK: u32 = 0x0f;
pub const GFP_KERNEL: u32 = GFP_DIRECT_RECLAIM | GFP_KSWAPD_RECLAIM | GFP_IO | GFP_FS;
pub const GFP_ATOMIC: u32 = GFP_HIGH | GFP_KSWAPD_RECLAIM;
pub const GFP_NOWAIT: u32 = GFP_KSWAPD_RECLAIM | GFP_NOWARN;
pub(crate) const INT_MAXV: i64 = 0x7fffffff;
pub(crate) const ULONG_MAXV: u64 = u64::MAX;

pub(crate) const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

pub(crate) fn xa_flags_mark(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}
pub(crate) fn allow_blocking(gfp: u32) -> bool {
    gfp & GFP_DIRECT_RECLAIM != 0
}
pub(crate) fn root_gfp_mask(flags: u32) -> u32 {
    flags & (GFP_BITS_MASK & !GFP_ZONEMASK)
}
pub(crate) fn err_ptr(e: i64) -> u64 {
    e.wrapping_neg() as u64
}
pub(crate) fn min_u64(a: u64, b: u64) -> u64 {
    if a < b { a } else { b }
}
pub(crate) fn max_u64(a: u64, b: u64) -> u64 {
    if a > b { a } else { b }
}
pub(crate) fn xa_is_value(e: u64) -> bool {
    e & 1 == 1
}
pub(crate) fn xa_to_value(e: u64) -> u64 {
    e >> 1
}
pub(crate) fn xa_mk_value(v: u64) -> u64 {
    (v << 1) | 1
}
pub(crate) fn xa_is_internal(e: u64) -> bool {
    e & 3 == 2
}
pub(crate) fn xa_mk_internal(v: u64) -> u64 {
    (v << 2) | 2
}
pub(crate) fn xa_is_node(e: u64) -> bool {
    xa_is_internal(e) && e > 4096
}
pub(crate) fn xa_to_node(e: u64) -> usize {
    (e.wrapping_sub(2 + NBASE) / NSTRIDE) as usize
}
pub(crate) fn xa_mk_node(ni: usize) -> u64 {
    (NBASE.wrapping_add((ni as u64).wrapping_mul(NSTRIDE))) | 2
}
pub(crate) fn node_ptr(ni: usize) -> u64 {
    NBASE.wrapping_add((ni as u64).wrapping_mul(NSTRIDE))
}
pub(crate) fn nidx_ptr(p: u64) -> usize {
    (p.wrapping_sub(NBASE) / NSTRIDE) as usize
}
pub(crate) fn xa_is_retry(e: u64) -> bool {
    e == XA_RETRY_ENTRY
}
pub(crate) fn xa_is_zero(e: u64) -> bool {
    e == XA_ZERO_ENTRY
}
pub(crate) fn xa_is_err(e: u64) -> bool {
    xa_is_internal(e) && e >= xa_mk_internal((-MAX_ERRNO) as u64)
}
pub(crate) fn xa_err(e: u64) -> i64 {
    if xa_is_err(e) { (e as i64) >> 2 } else { 0 }
}
pub(crate) fn xa_error(errno: i64) -> u64 {
    ((errno as u64) << 2) | 2
}
pub(crate) fn xa_zero_to_null(e: u64) -> u64 {
    if xa_is_zero(e) { 0 } else { e }
}
pub(crate) fn rt_is_internal(e: u64) -> bool {
    e & 3 == 2
}
pub(crate) fn entry_to_node(e: u64) -> usize {
    (e & !3).wrapping_sub(NBASE) as usize / NSTRIDE as usize
}
pub(crate) fn node_to_entry(ni: usize) -> u64 {
    node_ptr(ni) | 2
}
pub(crate) fn xas_error(xas: &Xas) -> i64 {
    xa_err(xas.node)
}
pub(crate) fn xas_set_err(xas: &mut Xas, err: i64) {
    xas.node = xa_error(err);
}
pub(crate) fn xas_invalid(xas: &Xas) -> bool {
    xas.node & 3 != 0
}
pub(crate) fn xas_valid(xas: &Xas) -> bool {
    !xas_invalid(xas)
}
pub(crate) fn xas_not_node(n: u64) -> bool {
    n & 3 != 0 || n == 0
}
pub(crate) fn xas_top(n: u64) -> bool {
    n <= XAS_RESTART
}
pub(crate) fn slot_next(s: SlotL) -> SlotL {
    match s {
        SlotL::Root => SlotL::Root,
        SlotL::Node(ni, o) => SlotL::Node(ni, o.wrapping_add(1)),
    }
}
pub(crate) fn slot_add(s: SlotL, k: u64) -> SlotL {
    match s {
        SlotL::Root => SlotL::Root,
        SlotL::Node(ni, o) => SlotL::Node(ni, o.wrapping_add(k as usize)),
    }
}
pub(crate) fn slot_off(s: SlotL) -> u64 {
    match s {
        SlotL::Root => 0,
        SlotL::Node(_, o) => o as u64,
    }
}
pub(crate) fn node_get_mark(n: &Node, offset: u64, mark: usize) -> bool {
    (n.marks[(mark).min(MAX_MARKS - 1)].wrapping_shr(offset as u32)) & 1 == 1
}
pub(crate) fn node_set_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[(mark).min(MAX_MARKS - 1)] |= 1u64.wrapping_shl(offset as u32);
    was
}
pub(crate) fn node_clear_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[(mark).min(MAX_MARKS - 1)] &= !(1u64.wrapping_shl(offset as u32));
    was
}
pub(crate) fn node_any_mark(n: &Node, mark: usize) -> bool {
    n.marks[(mark).min(MAX_MARKS - 1)] != 0
}
pub(crate) fn node_mark_all(n: &mut Node, mark: usize) {
    n.marks[(mark).min(MAX_MARKS - 1)] = u64::MAX;
}
pub(crate) fn get_offset(index: u64, n: &Node) -> u64 {
    (index >> n.shift.min(63)) & CHUNK_MASK
}
pub(crate) fn set_bounds(xas: &mut Xas) -> u64 {
    xas.node = XAS_BOUNDS;
    0
}
pub(crate) fn xas_reset(xas: &mut Xas) {
    xas.node = XAS_RESTART;
}
pub(crate) fn xas_retry(xas: &mut Xas, entry: u64) -> bool {
    if xa_is_zero(entry) {
        return true;
    }
    if !xa_is_retry(entry) {
        return false;
    }
    xas_reset(xas);
    true
}
pub(crate) fn xas_set(xas: &mut Xas, index: u64) {
    xas.index = index;
    xas.node = XAS_RESTART;
}
fn find_marked_out(xas: &mut Xas, max: u64) -> u64 {
    if xas.index > max {
        xas.node = XAS_RESTART;
        return 0;
    }
    set_bounds(xas)
}
pub(crate) fn bug_on(c: bool) {
    if c {
        panic!("BUG_ON tripped in the port — model divergence");
    }
}
pub(crate) fn tag_get(n: &Node, tag: u32, offset: u64) -> bool {
    node_get_mark(n, offset, tag as usize)
}
pub(crate) fn tag_set(n: &mut Node, tag: u32, offset: u64) {
    node_set_mark(n, offset, tag as usize);
}
pub(crate) fn tag_clear(n: &mut Node, tag: u32, offset: u64) {
    node_clear_mark(n, offset, tag as usize);
}
pub(crate) fn any_tag_set(n: &Node, tag: u32) -> bool {
    node_any_mark(n, tag as usize)
}
pub(crate) fn all_tag_set(n: &mut Node, tag: u32) {
    node_mark_all(n, tag as usize);
}
pub(crate) fn rt_find_next_bit(n: &Node, tag: u32, offset: u64) -> u64 {
    /* node->tags[tag] is one u64 word (XA_MARK_LONGS == 1) */
    if offset < RADIX_TREE_MAP_SIZE {
        let data = n.marks[(tag as usize).min(MAX_MARKS - 1)] & (u64::MAX.wrapping_shl(offset as u32));
        if data != 0 {
            return data.trailing_zeros() as u64;
        }
    }
    RADIX_TREE_MAP_SIZE
}
pub(crate) fn iter_offset(iter: &RIter) -> u64 {
    iter.index & RADIX_TREE_MAP_MASK
}
pub(crate) fn shift_maxindex(shift: u32) -> u64 {
    ((RADIX_TREE_MAP_SIZE).wrapping_shl(shift)).wrapping_sub(1)
}
pub(crate) fn node_maxindex(n: &Node) -> u64 {
    shift_maxindex(n.shift.min(63) as u32)
}
pub(crate) fn next_index(index: u64, n: &Node, offset: u64) -> u64 {
    (index & !node_maxindex(n)).wrapping_add(offset << n.shift.min(63))
}
pub(crate) fn get_slot_offset(node: Option<usize>, slot: SlotL) -> u64 {
    if node.is_some() { slot_off(slot) } else { 0 }
}
pub fn radix_tree_iter_init(iter: &mut RIter, start: u64) -> Option<SlotL> {
    iter.index = 0;
    iter.next_index = start;
    None
}
pub fn radix_tree_iter_retry(iter: &mut RIter) -> Option<SlotL> {
    iter.next_index = iter.index;
    iter.tags = 0;
    None
}
pub fn radix_tree_iter_add(iter: &RIter, slots: u64) -> u64 {
    iter.index.wrapping_add(slots)
}
pub fn radix_tree_iter_resume(_slot: Option<SlotL>, iter: &mut RIter) -> Option<SlotL> {
    iter.index = radix_tree_iter_add(iter, 1);
    iter.next_index = iter.index;
    iter.tags = 0;
    None
}
pub fn radix_tree_chunk_size(iter: &RIter) -> i64 {
    iter.next_index.wrapping_sub(iter.index) as i64
}

#[derive(Clone, Copy)]
pub(crate) struct Root {
    pub head: u64,
    pub flags: u32,
}

#[derive(Clone)]
pub(crate) struct Xas {
    pub index: u64,
    pub offset: u8,
    pub node: u64,
    pub alloc: u64,
    pub update: bool,
}

impl Xas {
    pub(crate) fn new(index: u64) -> Self {
        Xas { index, offset: 0, node: XAS_RESTART, alloc: 0, update: false }
    }
}

/// Slot lvalue: `&root->xa_head` or `&node->slots[off]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotL {
    Root,
    Node(usize, usize),
}

/// `struct radix_tree_iter` — iteration cursor over chunk granularity.
pub struct RIter {
    pub index: u64,
    pub next_index: u64,
    pub tags: u64,
    pub node: Option<usize>,
}

impl RIter {
    pub fn new() -> Self {
        RIter { index: 0, next_index: 0, tags: 0, node: None }
    }
}

impl Default for RIter {
    fn default() -> Self {
        Self::new()
    }
}


/// A radix tree root plus its caller-owned node arena — the engine for the
/// deprecated `radix_tree_*` API and for `Idr`/`Ida` (which embed it).
pub struct RadixTree<'a> {
    /// Caller-owned node arena (`used` = slab allocation bit).
    pub nodes: &'a mut [Node],
    pub(crate) root: Root,
    pub(crate) canon: u64,
    preload: [usize; RADIX_TREE_PRELOAD_SIZE as usize],
    plen: usize,
    /// Allocation counter paired with `fail_mask` (bit i = fail alloc i).
    pub alloc_ctr: u64,
    /// Allocation-failure injection bitmap (test/model knob).
    pub fail_mask: u64,
    /// `WARN_ON` trip counters.
    pub warn: u64,
    pub warn_once: u64,
    shadow_head: usize,
    /// NULLN-style scratch for wild node pointers — the `nidx`
    /// convention from `maple_tree`: decoding a pointer outside the
    /// arena must not panic (C would dereference garbage; the
    /// sequential model reads/writes this throwaway node instead).
    pub(crate) scratch: Node,
}

/// `nodes[i]` for reads — out-of-arena index maps to `scratch`.
#[inline]
pub(crate) fn nat<'n>(nodes: &'n [Node], scratch: &'n Node, i: usize) -> &'n Node {
    nodes.get(i).unwrap_or(scratch)
}

/// `nodes[i]` for writes — out-of-arena index maps to `scratch`.
#[inline]
pub(crate) fn nmat<'n>(
    nodes: &'n mut [Node],
    scratch: &'n mut Node,
    i: usize,
) -> &'n mut Node {
    match nodes.get_mut(i) {
        Some(n) => n,
        None => scratch,
    }
}

impl<'a> RadixTree<'a> {
    /// `RADIX_TREE(name, mask)`/`INIT_RADIX_TREE` — a fresh radix tree root.
    pub fn new(nodes: &'a mut [Node], gfp_mask: u32) -> Self {
        let mut t = RadixTree {
            nodes,
            root: Root { head: 0, flags: 0 },
            canon: RRT_CANON,
            preload: [0; RADIX_TREE_PRELOAD_SIZE as usize],
            plen: 0,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
            shadow_head: NILX as usize,
            scratch: Node::new(),
        };
        t.xa_init_flags(gfp_mask);
        t
    }

    pub(crate) fn new_idr(nodes: &'a mut [Node]) -> Self {
        let mut t = RadixTree::new(nodes, 0);
        t.canon = RTIDR_CANON;
        t.xa_init_flags(IDR_RT_MARKER);
        t
    }

    pub(crate) fn new_ida(nodes: &'a mut [Node]) -> Self {
        let mut t = RadixTree::new(nodes, 0);
        t.canon = IDAX_CANON;
        t.xa_init_flags(IDA_INIT_FLAGS);
        t
    }

    pub(crate) fn is_node_ptr(&self, p: u64) -> bool {
        p >= NBASE && p < NBASE.wrapping_add((self.nodes.len() as u64).wrapping_mul(NSTRIDE)) && p & 3 == 0
    }

    fn shadow_push(&mut self, ni: usize) {
        nmat(self.nodes, &mut self.scratch, ni).shadow_next = self.shadow_head;
        self.shadow_head = ni;
    }

    fn shadow_remove(&mut self, ni: usize) {
        let mut cur = self.shadow_head;
        let mut prev = NILX as usize;
        let mut __lim = self.nodes.len() + 1;
        while cur != NILX as usize {
            if __lim == 0 { break; }
            __lim -= 1;
            let next = nat(self.nodes, &self.scratch, cur).shadow_next;
            if cur == ni {
                if prev == NILX as usize {
                    self.shadow_head = next;
                } else {
                    nmat(self.nodes, &mut self.scratch, prev).shadow_next = next;
                }
                return;
            }
            prev = cur;
            cur = next;
        }
    }

    fn pl_push(&mut self, ni: usize) {
        if self.plen < self.preload.len() {
            self.preload[self.plen] = ni;
            self.plen += 1;
        }
    }

    fn pl_pop(&mut self) -> Option<usize> {
        if self.plen == 0 {
            None
        } else {
            self.plen -= 1;
            Some(self.preload[self.plen])
        }
    }

    fn pl_top(&self) -> Option<usize> {
        if self.plen == 0 { None } else { Some(self.preload[self.plen - 1]) }
    }

    pub(crate) fn xa_track_free(&self) -> bool {
        self.root.flags & XA_FLAGS_TRACK_FREE != 0
    }
    pub(crate) fn xa_zero_busy(&self) -> bool {
        self.root.flags & XA_FLAGS_ZERO_BUSY != 0
    }
    pub(crate) fn xa_marked(&self, mark: u32) -> bool {
        self.root.flags & xa_flags_mark(mark) != 0
    }
    pub(crate) fn xa_mark_set(&mut self, mark: u32) {
        if !self.xa_marked(mark) {
            self.root.flags |= xa_flags_mark(mark);
        }
    }
    pub(crate) fn xa_mark_clear(&mut self, mark: u32) {
        if self.xa_marked(mark) {
            self.root.flags &= !xa_flags_mark(mark);
        }
    }
    pub(crate) fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }

pub(crate) fn kmem_cache_alloc(&mut self) -> Option<usize> {
    self.alloc_ctr += 1;
    if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
        return None;
    }
    for i in 0..self.nodes.len() {
        if !nat(self.nodes, &self.scratch, i).used {
            nmat(self.nodes, &mut self.scratch, i).used = true;
            return Some(i);
        }
    }
    None
}

pub(crate) fn kmem_cache_free(&mut self, ni: usize) {
    nmat(self.nodes, &mut self.scratch, ni).used = false;
}

pub(crate) fn radix_tree_node_rcu_free(&mut self, ni: usize) {
    nmat(self.nodes, &mut self.scratch, ni).slots = [0; CHUNK_SIZE];
    nmat(self.nodes, &mut self.scratch, ni).marks = [0; MAX_MARKS];
    if nat(self.nodes, &self.scratch, ni).on_list {
        self.shadow_remove(ni);
        nmat(self.nodes, &mut self.scratch, ni).on_list = false;
    }
    self.kmem_cache_free(ni);
}

pub(crate) fn xa_node_free(&mut self, ni: usize) {
    nmat(self.nodes, &mut self.scratch, ni).array = XA_RCU_FREE;
    self.radix_tree_node_rcu_free(ni);
}

pub(crate) fn slot_get(&self, s: SlotL) -> u64 {
    match s {
        SlotL::Root => self.root.head,
        SlotL::Node(ni, o) if ni < self.nodes.len() && o < CHUNK_SIZE => {
            nat(self.nodes, &self.scratch, ni).slots[(o).min(CHUNK_SIZE - 1)]
        }
        _ => 0,
    }
}

pub(crate) fn slot_set(&mut self, s: SlotL, v: u64) {
    match s {
        SlotL::Root => self.root.head = v,
        SlotL::Node(ni, o) => {
            if self.warn_on(ni >= self.nodes.len() || o >= CHUNK_SIZE) {
                return;
            }
            nmat(self.nodes, &mut self.scratch, ni).slots[(o).min(CHUNK_SIZE - 1)] = v;
        }
    }
}

pub(crate) fn upd_cb(&mut self, ni: usize) {
    if nat(self.nodes, &self.scratch, ni).count != 0 && !nat(self.nodes, &self.scratch, ni).on_list {
        self.shadow_push(ni);
        nmat(self.nodes, &mut self.scratch, ni).on_list = true;
    } else if nat(self.nodes, &self.scratch, ni).count == 0 && nat(self.nodes, &self.scratch, ni).on_list {
        self.shadow_remove(ni);
        nmat(self.nodes, &mut self.scratch, ni).on_list = false;
    }
}

pub(crate) fn xas_update(&mut self, xas: &Xas, ni: usize) {
    if xas.update {
        self.upd_cb(ni);
    }
}

pub(crate) fn xas_set_mark(&mut self, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    let mut __lim = self.nodes.len() + 1;
    while self.is_node_ptr(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = nidx_ptr(node);
        if node_set_mark(nmat(self.nodes, &mut self.scratch, ni), offset, mark) {
            return;
        }
        offset = nat(self.nodes, &self.scratch, ni).offset as u64;
        node = nat(self.nodes, &self.scratch, ni).parent;
    }
    if !self.xa_marked(mark as u32) {
        self.xa_mark_set(mark as u32);
    }
}

pub(crate) fn xas_clear_mark(&mut self, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    let mut __lim = self.nodes.len() + 1;
    while self.is_node_ptr(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = nidx_ptr(node);
        if !node_clear_mark(nmat(self.nodes, &mut self.scratch, ni), offset, mark) {
            return;
        }
        if node_any_mark(nat(self.nodes, &self.scratch, ni), mark) {
            return;
        }
        offset = nat(self.nodes, &self.scratch, ni).offset as u64;
        node = nat(self.nodes, &self.scratch, ni).parent;
    }
    if self.xa_marked(mark as u32) {
        self.xa_mark_clear(mark as u32);
    }
}

pub(crate) fn xas_init_marks(&mut self, xas: &Xas) {
    let mut mark = 0usize;
    loop {
        if self.xa_track_free() && mark as u32 == XA_FREE_MARK {
            self.xas_set_mark(xas, mark);
        } else {
            self.xas_clear_mark(xas, mark);
        }
        if mark as u32 == XA_MARK_MAX {
            break;
        }
        mark += 1;
    }
}

pub(crate) fn xas_find_chunk(&self, xas: &Xas, advance: bool, mark: usize) -> u64 {
    let word = nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).marks[(mark).min(MAX_MARKS - 1)];
    let mut offset = xas.offset as u64;
    if advance {
        offset += 1;
    }
    if offset < CHUNK_SIZE as u64 {
        let data = word & (u64::MAX.wrapping_shl(offset as u32));
        if data != 0 {
            return data.trailing_zeros() as u64;
        }
    }
    CHUNK_SIZE as u64
}

pub(crate) fn xas_set_offset(&self, xas: &mut Xas) {
    xas.offset = get_offset(xas.index, nat(self.nodes, &self.scratch, nidx_ptr(xas.node))) as u8;
}

pub(crate) fn xas_move_index(&self, xas: &mut Xas, offset: u64) {
    let shift = nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).shift.min(63);
    xas.index &= (!CHUNK_MASK) << shift;
    xas.index = xas.index.wrapping_add(offset << shift);
}

pub(crate) fn xas_next_offset(&self, xas: &mut Xas) {
    xas.offset = xas.offset.wrapping_add(1);
    self.xas_move_index(xas, xas.offset as u64);
}

pub(crate) fn max_index(&self, entry: u64) -> u64 {
    if !xa_is_node(entry) {
        return 0;
    }
    ((CHUNK_SIZE as u64) << nat(self.nodes, &self.scratch, xa_to_node(entry)).shift.min(63)).wrapping_sub(1)
}

pub(crate) fn xas_reload(&mut self, xas: &mut Xas) -> u64 {
    if xas.node == 0 {
        return self.root.head;
    }
    let ni = nidx_ptr(xas.node);
    nat(self.nodes, &self.scratch, ni).slots[(((xas.index >> nat(self.nodes, &self.scratch, ni).shift.min(63)) & CHUNK_MASK) as usize).min(CHUNK_SIZE - 1)]
}

pub(crate) fn xas_start(&mut self, xas: &mut Xas) -> u64 {
    if xas_valid(xas) {
        return self.xas_reload(xas);
    }
    if xas_error(xas) != 0 {
        return 0;
    }
    let entry = self.root.head;
    if !xa_is_node(entry) {
        if xas.index != 0 {
            return set_bounds(xas);
        }
    } else if (xas.index >> nat(self.nodes, &self.scratch, xa_to_node(entry)).shift.min(63)) > CHUNK_MASK {
        return set_bounds(xas);
    }
    xas.node = 0;
    entry
}

pub(crate) fn xas_descend(&self, xas: &mut Xas, ni: usize) -> u64 {
    let offset = get_offset(xas.index, nat(self.nodes, &self.scratch, ni));
    let entry = nat(self.nodes, &self.scratch, ni).slots[(offset as usize).min(CHUNK_SIZE - 1)];
    xas.node = node_ptr(ni);
    xas.offset = offset as u8;
    entry
}

pub(crate) fn xas_load(&mut self, xas: &mut Xas) -> u64 {
    let mut entry = self.xas_start(xas);
    let mut __lim = self.nodes.len() + 1;
    while xa_is_node(entry) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = xa_to_node(entry);
        entry = self.xas_descend(xas, ni);
        if nat(self.nodes, &self.scratch, ni).shift.min(63) == 0 {
            break;
        }
    }
    entry
}

pub(crate) fn xas_destroy(&mut self, xas: &mut Xas) {
    let mut node = xas.alloc;
    let mut __lim = self.nodes.len() + 1;
    while node != 0 {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = nidx_ptr(node);
        let next = nat(self.nodes, &self.scratch, ni).parent;
        self.radix_tree_node_rcu_free(ni);
        node = next;
    }
    xas.alloc = 0;
}

pub(crate) fn xas_nomem(&mut self, xas: &mut Xas) -> bool {
    if xas.node != xa_error(-ENOMEM) {
        self.xas_destroy(xas);
        return false;
    }
    match self.kmem_cache_alloc() {
        None => false,
        Some(ni) => {
            xas.alloc = node_ptr(ni);
            nmat(self.nodes, &mut self.scratch, ni).parent = 0;
            xas.node = XAS_RESTART;
            true
        }
    }
}

pub(crate) fn xas_alloc(&mut self, xas: &mut Xas, shift: u8) -> Option<usize> {
    let parent = xas.node;
    if xas_invalid(xas) {
        return None;
    }
    let ni = if xas.alloc != 0 {
        let ni = nidx_ptr(xas.alloc);
        xas.alloc = 0;
        ni
    } else {
        match self.kmem_cache_alloc() {
            None => {
                xas_set_err(xas, -ENOMEM);
                return None;
            }
            Some(i) => i,
        }
    };
    if parent != 0 {
        let pni = nidx_ptr(parent);
        nmat(self.nodes, &mut self.scratch, ni).offset = xas.offset;
        nmat(self.nodes, &mut self.scratch, pni).count = nat(self.nodes, &self.scratch, pni).count.wrapping_add(1);
        self.xas_update(xas, pni);
    }
    nmat(self.nodes, &mut self.scratch, ni).shift = shift;
    nmat(self.nodes, &mut self.scratch, ni).count = 0;
    nmat(self.nodes, &mut self.scratch, ni).nr_values = 0;
    nmat(self.nodes, &mut self.scratch, ni).parent = parent;
    nmat(self.nodes, &mut self.scratch, ni).array = self.canon;
    Some(ni)
}

pub(crate) fn xas_free_nodes(&mut self, xas: &Xas, top: usize) {
    let mut offset = 0u32;
    let mut node = top;
    let mut __lim = (self.nodes.len() + 1) * 66;
    loop {
        if __lim == 0 { break; }
        __lim -= 1;
        let entry = nat(self.nodes, &self.scratch, node).slots[(offset as usize).min(CHUNK_SIZE - 1)];
        if nat(self.nodes, &self.scratch, node).shift.min(63) != 0 && xa_is_node(entry) {
            node = xa_to_node(entry);
            offset = 0;
            continue;
        }
        if entry != 0 {
            nmat(self.nodes, &mut self.scratch, node).slots[(offset as usize).min(CHUNK_SIZE - 1)] = XA_RETRY_ENTRY;
        }
        offset += 1;
        while offset == CHUNK_SIZE as u32 {
            let parent = nat(self.nodes, &self.scratch, node).parent;
            offset = nat(self.nodes, &self.scratch, node).offset as u32 + 1;
            nmat(self.nodes, &mut self.scratch, node).count = 0;
            nmat(self.nodes, &mut self.scratch, node).nr_values = 0;
            self.xas_update(xas, node);
            self.xa_node_free(node);
            if node == top {
                return;
            }
            node = nidx_ptr(parent);
        }
    }
}

pub(crate) fn xas_shrink(&mut self, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    let mut __lim = self.nodes.len() + 1;
    loop {
        if __lim == 0 { break; }
        __lim -= 1;
        if nat(self.nodes, &self.scratch, node).count != 1 {
            break;
        }
        let mut entry = nat(self.nodes, &self.scratch, node).slots[0];
        if entry == 0 {
            break;
        }
        if !xa_is_node(entry) && nat(self.nodes, &self.scratch, node).shift.min(63) != 0 {
            break;
        }
        if self.xa_zero_busy() {
            entry = xa_zero_to_null(entry);
        }
        xas.node = XAS_BOUNDS;
        self.root.head = entry;
        if self.xa_track_free() && !node_get_mark(nat(self.nodes, &self.scratch, node), 0, XA_FREE_MARK as usize) {
            self.xa_mark_clear(XA_FREE_MARK);
        }
        nmat(self.nodes, &mut self.scratch, node).count = 0;
        nmat(self.nodes, &mut self.scratch, node).nr_values = 0;
        if !xa_is_node(entry) {
            nmat(self.nodes, &mut self.scratch, node).slots[0] = XA_RETRY_ENTRY;
        }
        self.xas_update(xas, node);
        self.xa_node_free(node);
        if !xa_is_node(entry) {
            break;
        }
        node = xa_to_node(entry);
        nmat(self.nodes, &mut self.scratch, node).parent = 0;
    }
}

pub(crate) fn xas_delete_node(&mut self, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    let mut __lim = self.nodes.len() + 1;
    loop {
        if __lim == 0 { break; }
        __lim -= 1;
        if nat(self.nodes, &self.scratch, node).count != 0 {
            break;
        }
        let parent = nat(self.nodes, &self.scratch, node).parent;
        xas.node = parent;
        xas.offset = nat(self.nodes, &self.scratch, node).offset;
        self.xa_node_free(node);
        if parent == 0 {
            self.root.head = 0;
            xas.node = XAS_BOUNDS;
            return;
        }
        let pni = nidx_ptr(parent);
        nmat(self.nodes, &mut self.scratch, pni).slots[(xas.offset as usize).min(CHUNK_SIZE - 1)] = 0;
        nmat(self.nodes, &mut self.scratch, pni).count = nat(self.nodes, &self.scratch, pni).count.wrapping_sub(1);
        node = pni;
        self.xas_update(xas, node);
    }
    if nat(self.nodes, &self.scratch, node).parent == 0 {
        self.xas_shrink(xas);
    }
}

pub(crate) fn xas_expand(&mut self, xas: &mut Xas, mut head: u64) -> i64 {
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
        shift = nat(self.nodes, &self.scratch, ni).shift.min(63) as u32 + CHUNK_SHIFT;
    }
    xas.node = 0;

    while max > self.max_index(head) {
        let ni = match self.xas_alloc(xas, shift as u8) {
            None => return -ENOMEM,
            Some(i) => i,
        };
        nmat(self.nodes, &mut self.scratch, ni).count = 1;
        if xa_is_value(head) {
            nmat(self.nodes, &mut self.scratch, ni).nr_values = 1;
        }
        nmat(self.nodes, &mut self.scratch, ni).slots[0] = head;

        let mut mark = 0u32;
        loop {
            if self.xa_track_free() && mark == XA_FREE_MARK {
                node_mark_all(nmat(self.nodes, &mut self.scratch, ni), XA_FREE_MARK as usize);
                if !self.xa_marked(XA_FREE_MARK) {
                    node_clear_mark(nmat(self.nodes, &mut self.scratch, ni), 0, XA_FREE_MARK as usize);
                    self.xa_mark_set(XA_FREE_MARK);
                }
            } else if self.xa_marked(mark) {
                node_set_mark(nmat(self.nodes, &mut self.scratch, ni), 0, mark as usize);
            }
            if mark == XA_MARK_MAX {
                break;
            }
            mark += 1;
        }

        if xa_is_node(head) {
            let hni = xa_to_node(head);
            nmat(self.nodes, &mut self.scratch, hni).offset = 0;
            nmat(self.nodes, &mut self.scratch, hni).parent = node_ptr(ni);
        }
        head = xa_mk_node(ni);
        self.root.head = head;
        self.xas_update(xas, ni);

        shift += CHUNK_SHIFT;
        node = node_ptr(ni);
    }
    xas.node = node;
    shift as i64
}

pub(crate) fn xas_create(&mut self, xas: &mut Xas, allow_root: bool) -> u64 {
    let order = 0u32; /* xa_shift is always 0 in this harness */
    let mut entry: u64;
    let mut shift: i64;
    let mut slot: Option<(usize, u64)>;

    if xas_top(xas.node) {
        entry = self.root.head;
        xas.node = 0;
        if entry == 0 && self.xa_zero_busy() {
            entry = XA_ZERO_ENTRY;
        }
        shift = self.xas_expand(xas, entry);
        if shift < 0 {
            return 0;
        }
        if shift == 0 && !allow_root {
            shift = CHUNK_SHIFT as i64;
        }
        entry = self.root.head;
        slot = None;
    } else if xas_error(xas) != 0 {
        return 0;
    } else if xas.node != 0 {
        let ni = nidx_ptr(xas.node);
        let offset = xas.offset;
        shift = nat(self.nodes, &self.scratch, ni).shift.min(63) as i64;
        entry = nat(self.nodes, &self.scratch, ni).slots[(offset as usize).min(CHUNK_SIZE - 1)];
        slot = Some((ni, offset as u64));
    } else {
        shift = 0;
        entry = self.root.head;
        slot = None;
    }

    while shift > order as i64 {
        shift -= CHUNK_SHIFT as i64;
        let node: usize;
        if entry == 0 {
            match self.xas_alloc(xas, shift as u8) {
                None => break,
                Some(ni) => {
                    node = ni;
                    if self.xa_track_free() {
                        node_mark_all(nmat(self.nodes, &mut self.scratch, ni), XA_FREE_MARK as usize);
                    }
                    match slot {
                        None => self.root.head = xa_mk_node(ni),
                        Some((sni, soff)) => nmat(self.nodes, &mut self.scratch, sni).slots[(soff as usize).min(CHUNK_SIZE - 1)] = xa_mk_node(ni),
                    }
                }
            }
        } else if xa_is_node(entry) {
            node = xa_to_node(entry);
        } else {
            break;
        }
        entry = self.xas_descend(xas, node);
        slot = Some((node, xas.offset as u64));
    }
    entry
}

pub(crate) fn update_node(&mut self, xas: &mut Xas, node: u64, count: i64, values: i64) {
    if node == 0 || (count == 0 && values == 0) {
        return;
    }
    if !self.is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    nmat(self.nodes, &mut self.scratch, ni).count = nat(self.nodes, &self.scratch, ni).count.wrapping_add(count as u8);
    nmat(self.nodes, &mut self.scratch, ni).nr_values = nat(self.nodes, &self.scratch, ni).nr_values.wrapping_add(values as u8);
    self.xas_update(xas, ni);
    if count < 0 {
        self.xas_delete_node(xas);
    }
}

pub(crate) fn xas_store(&mut self, xas: &mut Xas, entry: u64) -> u64 {
    let value = xa_is_value(entry);
    let first = if entry != 0 {
        self.xas_create(xas, !xa_is_node(entry) && !xa_is_zero(entry))
    } else {
        self.xas_load(xas)
    };
    if xas_invalid(xas) || first == entry {
        return first;
    }
    let node = xas.node;
    let ni_opt: Option<usize> = if self.is_node_ptr(node) {
        Some(nidx_ptr(node))
    } else {
        None
    };
    if entry == 0 {
        self.xas_init_marks(xas);
    }
    /* sibs == 0 always, so the store loop collapses to a single slot */
    match ni_opt {
        None => self.root.head = entry,
        Some(ni) => nmat(self.nodes, &mut self.scratch, ni).slots[(xas.offset as usize).min(CHUNK_SIZE - 1)] = entry,
    }
    if xa_is_node(first) && (ni_opt.is_none() || nat(self.nodes, &self.scratch, ni_opt.unwrap()).shift.min(63) != 0) {
        self.xas_free_nodes(xas, xa_to_node(first));
    }
    if let Some(_ni) = ni_opt {
        let count = (first == 0) as i64 - (entry == 0) as i64;
        let values = (!xa_is_value(first)) as i64 - (!value) as i64;
        self.update_node(xas, node, count, values);
    }
    first
}

pub(crate) fn xas_find(&mut self, xas: &mut Xas, max: u64) -> u64 {
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
        entry = self.xas_load(xas);
        if entry != 0 || xas_not_node(xas.node) {
            return entry;
        }
    } else if self.is_node_ptr(xas.node)
        && nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).shift.min(63) == 0
        && xas.offset as u64 != (xas.index & CHUNK_MASK)
    {
        xas.offset = (((xas.index.wrapping_sub(1)) & CHUNK_MASK) + 1) as u8;
    }

    self.xas_next_offset(xas);

    let mut __lim = (self.nodes.len() + 1) * 66;
    while self.is_node_ptr(xas.node) && xas.index <= max {
        if __lim == 0 { break; }
        __lim -= 1;
        if xas.offset as u64 == CHUNK_SIZE as u64 {
            xas.offset = nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).offset.wrapping_add(1);
            xas.node = nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).parent;
            continue;
        }
        let ni = nidx_ptr(xas.node);
        entry = nat(self.nodes, &self.scratch, ni).slots[(xas.offset as usize).min(CHUNK_SIZE - 1)];
        if xa_is_node(entry) {
            xas.node = node_ptr(xa_to_node(entry));
            xas.offset = 0;
            continue;
        }
        if entry != 0 {
            return entry;
        }
        self.xas_next_offset(xas);
    }

    if xas.node == 0 {
        xas.node = XAS_BOUNDS;
    }
    0
}

pub(crate) fn xas_find_marked(&mut self, xas: &mut Xas, max: u64, mark: usize) -> u64 {
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
        return find_marked_out(xas, max);
    } else if xas_top(xas.node) {
        advance = false;
        let entry = self.root.head;
        xas.node = 0;
        if xas.index > self.max_index(entry) {
            return find_marked_out(xas, max);
        }
        if !xa_is_node(entry) {
            if self.xa_marked(mark as u32) {
                return entry;
            }
            xas.index = 1;
            return find_marked_out(xas, max);
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas.offset = (xas.index >> nat(self.nodes, &self.scratch, nidx_ptr(xas.node)).shift.min(63)) as u8;
    }

    let mut __lim = (self.nodes.len() + 1) * 66;
    while xas.index <= max {
        if __lim == 0 { break; }
        __lim -= 1;
        if xas.offset as u64 == CHUNK_SIZE as u64 {
            let ni = nidx_ptr(xas.node);
            xas.offset = nat(self.nodes, &self.scratch, ni).offset.wrapping_add(1);
            xas.node = nat(self.nodes, &self.scratch, ni).parent;
            if xas.node == 0 {
                break;
            }
            advance = false;
            continue;
        }
        let offset = self.xas_find_chunk(xas, advance, mark);
        if offset > xas.offset as u64 {
            advance = false;
            self.xas_move_index(xas, offset);
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
        let entry = nat(self.nodes, &self.scratch, ni).slots[(xas.offset as usize).min(CHUNK_SIZE - 1)];
        if entry == 0 && !(self.xa_track_free() && mark as u32 == XA_FREE_MARK) {
            continue;
        }
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        self.xas_set_offset(xas);
    }

    find_marked_out(xas, max)
}

pub(crate) fn xas_next_entry(&mut self, xas: &mut Xas, max: u64) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (self.is_node_ptr(node) && nat(self.nodes, &self.scratch, nidx_ptr(node)).shift.min(63) != 0)
        || xas.offset as u64 != (xas.index & CHUNK_MASK)
    {
        return self.xas_find(xas, max);
    }
    let ni = nidx_ptr(node);
    loop {
        if xas.index >= max {
            return self.xas_find(xas, max);
        }
        if xas.offset == CHUNK_MASK as u8 {
            return self.xas_find(xas, max);
        }
        let entry = nat(self.nodes, &self.scratch, ni).slots[(xas.offset as usize + 1).min(CHUNK_SIZE - 1)];
        if xa_is_internal(entry) {
            return self.xas_find(xas, max);
        }
        xas.offset = xas.offset.wrapping_add(1);
        xas.index = xas.index.wrapping_add(1);
        if entry != 0 {
            return entry;
        }
    }
}

pub(crate) fn xa_find(&mut self, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(*index);
    let mut entry;
    let mut tries = 0u32;
    loop {
        if filter < MAX_MARKS as u32 {
            entry = self.xas_find_marked(&mut xas, max, filter as usize);
        } else {
            entry = self.xas_find(&mut xas, max);
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

pub(crate) fn xa_init_flags(&mut self, flags: u32) {
    self.root.flags = flags;
    self.root.head = 0;
}

pub(crate) fn xa_empty(&self) -> bool {
    self.root.head == 0
}

pub(crate) fn root_tag_set(&mut self, tag: u32) {
    self.root.flags |= 1 << (ROOT_TAG_SHIFT + tag);
}

pub(crate) fn root_tag_clear(&mut self, tag: u32) {
    self.root.flags &= !(1 << (ROOT_TAG_SHIFT + tag));
}

pub(crate) fn root_tag_clear_all(&mut self) {
    self.root.flags &= (1 << ROOT_TAG_SHIFT) - 1;
}

pub(crate) fn root_tag_get(&self, tag: u32) -> bool {
    self.root.flags & (1 << (ROOT_TAG_SHIFT + tag)) != 0
}

pub(crate) fn root_tags_get(&self) -> u32 {
    self.root.flags >> ROOT_TAG_SHIFT
}

pub(crate) fn is_idr(&self) -> bool {
    self.root.flags & ROOT_IS_IDR != 0
}

pub(crate) fn rt_descend(&self, ni: usize, index: u64) -> (u64, u64) {
    let offset = (index >> nat(self.nodes, &self.scratch, ni).shift.min(63)) & RADIX_TREE_MAP_MASK;
    (nat(self.nodes, &self.scratch, ni).slots[(offset as usize).min(CHUNK_SIZE - 1)], offset)
}

pub(crate) fn rt_load_root(&self) -> (u64, u64, u32) {
    let node = self.root.head;
    if rt_is_internal(node) {
        let ni = entry_to_node(node);
        (
            node,
            node_maxindex(nat(self.nodes, &self.scratch, ni)),
            nat(self.nodes, &self.scratch, ni).shift.min(63) as u32 + RADIX_TREE_MAP_SHIFT,
        )
    } else {
        (node, 0, 0)
    }
}

pub fn radix_tree_preload(&mut self, gfp_mask: u32) -> i64 {
    self.warn_on(!allow_blocking(gfp_mask));
    self.__radix_tree_preload(gfp_mask, RADIX_TREE_PRELOAD_SIZE)
}

pub fn radix_tree_maybe_preload(&mut self, gfp_mask: u32) -> i64 {
    if allow_blocking(gfp_mask) {
        return self.__radix_tree_preload(gfp_mask, RADIX_TREE_PRELOAD_SIZE);
    }
    0
}

pub(crate) fn radix_tree_node_alloc(
    &mut self,
    gfp_mask: u32,
    parent: Option<usize>,
    shift: u8,
    offset: u8,
    count: u8,
    nr_values: u8,
) -> Option<usize> {
    let mut ret: Option<usize> = None;
    let mut used_cache = false;
    if !allow_blocking(gfp_mask) {
        /* try the slab first, then the per-cpu preload stack */
        ret = self.kmem_cache_alloc();
        if ret.is_some() {
            used_cache = true;
        } else {
            ret = self.pl_pop();
        }
    }
    let _ = used_cache;
    if ret.is_none() && allow_blocking(gfp_mask) {
        ret = self.kmem_cache_alloc();
    }
    if ret.is_none() && !allow_blocking(gfp_mask) {
        return None;
    }
    let ni = ret?;
    nmat(self.nodes, &mut self.scratch, ni).shift = shift;
    nmat(self.nodes, &mut self.scratch, ni).offset = offset;
    nmat(self.nodes, &mut self.scratch, ni).count = count;
    nmat(self.nodes, &mut self.scratch, ni).nr_values = nr_values;
    nmat(self.nodes, &mut self.scratch, ni).parent = parent.map(node_ptr).unwrap_or(0);
    nmat(self.nodes, &mut self.scratch, ni).array = self.canon;
    Some(ni)
}

pub(crate) fn radix_tree_node_free(&mut self, ni: usize) {
    self.radix_tree_node_rcu_free(ni);
}

pub(crate) fn radix_tree_extend(&mut self, gfp: u32, index: u64, shift: u32) -> i64 {
    let mut maxshift = shift;
    let mut shift = shift;
    while index > shift_maxindex(maxshift) {
        maxshift += RADIX_TREE_MAP_SHIFT;
    }

    let mut entry = self.root.head;
    if entry == 0 && (!self.is_idr() || self.root_tag_get(IDR_FREE)) {
        return (maxshift + RADIX_TREE_MAP_SHIFT) as i64;
    }

    loop {
        let ni = match self.radix_tree_node_alloc(gfp, None, shift as u8, 0, 1, 0) {
            None => return -ENOMEM,
            Some(n) => n,
        };
        if self.is_idr() {
            all_tag_set(nmat(self.nodes, &mut self.scratch, ni), IDR_FREE);
            if !self.root_tag_get(IDR_FREE) {
                tag_clear(nmat(self.nodes, &mut self.scratch, ni), IDR_FREE, 0);
                self.root_tag_set(IDR_FREE);
            }
        } else {
            for tag in 0..RADIX_TREE_MAX_TAGS {
                if self.root_tag_get(tag) {
                    tag_set(nmat(self.nodes, &mut self.scratch, ni), tag, 0);
                }
            }
        }
        bug_on(shift > BITS_PER_LONG as u32);
        if rt_is_internal(entry) {
            let cni = entry_to_node(entry);
            nmat(self.nodes, &mut self.scratch, cni).parent = node_ptr(ni);
        } else if xa_is_value(entry) {
            nmat(self.nodes, &mut self.scratch, ni).nr_values = 1;
        }
        nmat(self.nodes, &mut self.scratch, ni).slots[0] = entry;
        entry = node_to_entry(ni);
        self.root.head = entry;
        shift += RADIX_TREE_MAP_SHIFT;
        if shift > maxshift {
            break;
        }
    }
    (maxshift + RADIX_TREE_MAP_SHIFT) as i64
}

pub(crate) fn radix_tree_shrink(&mut self) -> bool {
    let mut shrunk = false;
    let mut __lim = self.nodes.len() + 1;
    loop {
        if __lim == 0 { break; }
        __lim -= 1;
        let node_e = self.root.head;
        if !rt_is_internal(node_e) {
            break;
        }
        let ni = entry_to_node(node_e);
        if nat(self.nodes, &self.scratch, ni).count != 1 {
            break;
        }
        let child = nat(self.nodes, &self.scratch, ni).slots[0];
        if child == 0 {
            break;
        }
        if nat(self.nodes, &self.scratch, ni).shift.min(63) == 0 && self.is_idr() {
            break;
        }
        if rt_is_internal(child) {
            let cni = entry_to_node(child);
            nmat(self.nodes, &mut self.scratch, cni).parent = 0;
        }
        self.root.head = child;
        if self.is_idr() && !tag_get(nat(self.nodes, &self.scratch, ni), IDR_FREE, 0) {
            self.root_tag_clear(IDR_FREE);
        }
        nmat(self.nodes, &mut self.scratch, ni).count = 0;
        if !rt_is_internal(child) {
            nmat(self.nodes, &mut self.scratch, ni).slots[0] = RADIX_TREE_RETRY;
        }
        self.warn_on(nat(self.nodes, &self.scratch, ni).on_list);
        self.radix_tree_node_free(ni);
        shrunk = true;
    }
    shrunk
}

pub(crate) fn delete_node(&mut self, node0: usize) -> bool {
    let mut deleted = false;
    let mut node = Some(node0);
    let mut __lim = self.nodes.len() + 1;
    while let Some(ni) = node {
        if __lim == 0 { break; }
        __lim -= 1;
        if nat(self.nodes, &self.scratch, ni).count != 0 {
            if node_to_entry(ni) == self.root.head {
                deleted |= self.radix_tree_shrink();
            }
            return deleted;
        }
        let parent = nat(self.nodes, &self.scratch, ni).parent;
        if parent != 0 {
            let pni = nidx_ptr(parent);
            let off = nat(self.nodes, &self.scratch, ni).offset as usize;
            nmat(self.nodes, &mut self.scratch, pni).slots[(off).min(CHUNK_SIZE - 1)] = 0;
            nmat(self.nodes, &mut self.scratch, pni).count = nat(self.nodes, &self.scratch, pni).count.wrapping_sub(1);
        } else {
            if !self.is_idr() {
                self.root_tag_clear_all();
            }
            self.root.head = 0;
        }
        self.warn_on(nat(self.nodes, &self.scratch, ni).on_list);
        self.radix_tree_node_free(ni);
        deleted = true;
        node = if parent != 0 {
            Some(nidx_ptr(parent))
        } else {
            None
        };
    }
    deleted
}

pub(crate) fn __radix_tree_create(&mut self, index: u64) -> Result<(Option<usize>, SlotL), i64> {
    let mut node: Option<usize> = None;
    let mut slot = SlotL::Root;
    let mut offset = 0u64;
    let gfp = root_gfp_mask(self.root.flags);

    let (mut child, maxindex, mut shift) = self.rt_load_root();

    if index > maxindex {
        let error = self.radix_tree_extend(gfp, index, shift);
        if error < 0 {
            return Err(error);
        }
        shift = error as u32;
        child = self.root.head;
    }

    while shift > 0 {
        shift -= RADIX_TREE_MAP_SHIFT;
        if child == 0 {
            let ni = match self.radix_tree_node_alloc(gfp, node, shift as u8, offset as u8, 0, 0)
            {
                None => return Err(-ENOMEM),
                Some(n) => n,
            };
            self.slot_set(slot, node_to_entry(ni));
            if let Some(pn) = node {
                nmat(self.nodes, &mut self.scratch, pn).count = nat(self.nodes, &self.scratch, pn).count.wrapping_add(1);
            }
            child = node_to_entry(ni);
        } else if !rt_is_internal(child) {
            break;
        }
        let ni = entry_to_node(child);
        node = Some(ni);
        let (c, off) = self.rt_descend(ni, index);
        child = c;
        offset = off;
        slot = SlotL::Node(ni, off as usize);
    }
    Ok((node, slot))
}

pub(crate) fn radix_tree_free_nodes(&mut self, node_e: u64) {
    let mut offset = 0u32;
    let mut child = entry_to_node(node_e);
    let top = child;
    let mut __lim = (self.nodes.len() + 1) * 66;
    loop {
        if __lim == 0 { break; }
        __lim -= 1;
        let entry = nat(self.nodes, &self.scratch, child).slots[(offset as usize).min(CHUNK_SIZE - 1)];
        if xa_is_node(entry) && nat(self.nodes, &self.scratch, child).shift.min(63) != 0 {
            child = entry_to_node(entry);
            offset = 0;
            continue;
        }
        offset += 1;
        while offset == RADIX_TREE_MAP_SIZE as u32 {
            let old = child;
            offset = nat(self.nodes, &self.scratch, child).offset as u32 + 1;
            child = if nat(self.nodes, &self.scratch, child).parent != 0 {
                nidx_ptr(nat(self.nodes, &self.scratch, child).parent)
            } else {
                usize::MAX
            };
            self.warn_on(nat(self.nodes, &self.scratch, old).on_list);
            self.radix_tree_node_free(old);
            if old == top {
                return;
            }
        }
    }
}

pub(crate) fn insert_entries(&mut self, node: Option<usize>, slot: SlotL, item: u64) -> i64 {
    if self.slot_get(slot) != 0 {
        return -EEXIST;
    }
    self.slot_set(slot, item);
    if let Some(ni) = node {
        nmat(self.nodes, &mut self.scratch, ni).count = nat(self.nodes, &self.scratch, ni).count.wrapping_add(1);
        if xa_is_value(item) {
            nmat(self.nodes, &mut self.scratch, ni).nr_values = nat(self.nodes, &self.scratch, ni).nr_values.wrapping_add(1);
        }
    }
    1
}

pub fn radix_tree_insert(&mut self, index: u64, item: u64) -> i64 {
    bug_on(rt_is_internal(item));
    let (node, slot) = match self.__radix_tree_create(index) {
        Err(e) => return e,
        Ok(v) => v,
    };
    let error = self.insert_entries(node, slot, item);
    if error < 0 {
        return error;
    }
    if let Some(ni) = node {
        let offset = slot_off(slot);
        bug_on(tag_get(nat(self.nodes, &self.scratch, ni), 0, offset));
        bug_on(tag_get(nat(self.nodes, &self.scratch, ni), 1, offset));
        bug_on(tag_get(nat(self.nodes, &self.scratch, ni), 2, offset));
    } else {
        bug_on(self.root_tags_get() != 0);
    }
    0
}

pub(crate) fn __radix_tree_lookup(&self, index: u64) -> (u64, Option<usize>, Option<SlotL>) {
    let mut parent: Option<usize> = None;
    let mut slot = SlotL::Root;
    let (mut node, maxindex, _) = self.rt_load_root();
    if index > maxindex {
        return (0, None, None);
    }
    let mut __lim = self.nodes.len() + 1;
    while rt_is_internal(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = entry_to_node(node);
        parent = Some(ni);
        let (c, off) = self.rt_descend(ni, index);
        node = c;
        slot = SlotL::Node(ni, off as usize);
        if node == RADIX_TREE_RETRY {
            return self.__radix_tree_lookup(index);
        }
        if nat(self.nodes, &self.scratch, ni).shift.min(63) == 0 {
            break;
        }
    }
    (node, parent, Some(slot))
}

pub fn radix_tree_lookup_slot(&self, index: u64) -> Option<SlotL> {
    let (entry, _, slot) = self.__radix_tree_lookup(index);
    if entry == 0 {
        return None;
    }
    slot
}

pub fn radix_tree_lookup(&self, index: u64) -> u64 {
    self.__radix_tree_lookup(index).0
}

pub(crate) fn replace_slot(
    &mut self,
    slot: SlotL,
    item: u64,
    node: Option<usize>,
    count: i64,
    values: i64,
) {
    if let Some(ni) = node {
        if count != 0 || values != 0 {
            nmat(self.nodes, &mut self.scratch, ni).count = nat(self.nodes, &self.scratch, ni).count.wrapping_add(count as u8);
            nmat(self.nodes, &mut self.scratch, ni).nr_values = nat(self.nodes, &self.scratch, ni).nr_values.wrapping_add(values as u8);
        }
    }
    self.slot_set(slot, item);
}

pub(crate) fn node_tag_get(&self, node: Option<usize>, tag: u32, offset: u64) -> bool {
    match node {
        Some(ni) => tag_get(nat(self.nodes, &self.scratch, ni), tag, offset),
        None => self.root_tag_get(tag),
    }
}

pub(crate) fn calculate_count(
    &self,
    node: Option<usize>,
    slot: SlotL,
    item: u64,
    old: u64,
) -> i64 {
    if self.is_idr() {
        let offset = get_slot_offset(node, slot);
        let free = self.node_tag_get(node, IDR_FREE, offset);
        if !free {
            return 0;
        }
        if old == 0 {
            return 1;
        }
    }
    (item != 0) as i64 - (old != 0) as i64
}

pub(crate) fn __radix_tree_replace(&mut self, node: Option<usize>, slot: SlotL, item: u64) {
    let old = self.slot_get(slot);
    let values = xa_is_value(item) as i64 - xa_is_value(old) as i64;
    let count = self.calculate_count(node, slot, item, old);
    self.warn_on(node.is_none() && slot != SlotL::Root && (count != 0 || values != 0));
    self.replace_slot(slot, item, node, count, values);
    if node.is_none() {
        return;
    }
    self.delete_node(node.unwrap());
}

pub fn radix_tree_replace_slot(&mut self, slot: SlotL, item: u64) {
    self.__radix_tree_replace(None, slot, item);
}

pub fn radix_tree_iter_replace(&mut self, iter: &RIter, slot: SlotL, item: u64) {
    self.__radix_tree_replace(iter.node, slot, item);
}

pub(crate) fn node_tag_set(&mut self, node: Option<usize>, tag: u32, offset: u64) {
    let mut node = node;
    let mut offset = offset;
    let mut __lim = self.nodes.len() + 1;
    while let Some(ni) = node {
        if __lim == 0 { break; }
        __lim -= 1;
        if tag_get(nat(self.nodes, &self.scratch, ni), tag, offset) {
            return;
        }
        tag_set(nmat(self.nodes, &mut self.scratch, ni), tag, offset);
        offset = nat(self.nodes, &self.scratch, ni).offset as u64;
        node = if nat(self.nodes, &self.scratch, ni).parent != 0 {
            Some(nidx_ptr(nat(self.nodes, &self.scratch, ni).parent))
        } else {
            None
        };
    }
    if !self.root_tag_get(tag) {
        self.root_tag_set(tag);
    }
}

pub fn radix_tree_tag_set(&mut self, index: u64, tag: u32) -> u64 {
    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return 0; }
    let (mut node, maxindex, _) = self.rt_load_root();
    bug_on(index > maxindex);
    let mut __lim = self.nodes.len() + 1;
    while rt_is_internal(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = entry_to_node(node);
        let (c, off) = self.rt_descend(ni, index);
        node = c;
        bug_on(node == 0);
        if !tag_get(nat(self.nodes, &self.scratch, ni), tag, off) {
            tag_set(nmat(self.nodes, &mut self.scratch, ni), tag, off);
        }
    }
    if !self.root_tag_get(tag) {
        self.root_tag_set(tag);
    }
    node
}

pub(crate) fn node_tag_clear(&mut self, node: Option<usize>, tag: u32, offset: u64) {
    let mut node = node;
    let mut offset = offset;
    let mut __lim = self.nodes.len() + 1;
    while let Some(ni) = node {
        if __lim == 0 { break; }
        __lim -= 1;
        if !tag_get(nat(self.nodes, &self.scratch, ni), tag, offset) {
            return;
        }
        tag_clear(nmat(self.nodes, &mut self.scratch, ni), tag, offset);
        if any_tag_set(nat(self.nodes, &self.scratch, ni), tag) {
            return;
        }
        offset = nat(self.nodes, &self.scratch, ni).offset as u64;
        node = if nat(self.nodes, &self.scratch, ni).parent != 0 {
            Some(nidx_ptr(nat(self.nodes, &self.scratch, ni).parent))
        } else {
            None
        };
    }
    if self.root_tag_get(tag) {
        self.root_tag_clear(tag);
    }
}

pub fn radix_tree_tag_clear(&mut self, index: u64, tag: u32) -> u64 {
    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return 0; }
    let (mut node, maxindex, _) = self.rt_load_root();
    if index > maxindex {
        return 0;
    }
    let mut parent: Option<usize> = None;
    let mut offset = 0u64;
    let mut __lim = self.nodes.len() + 1;
    while rt_is_internal(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = entry_to_node(node);
        parent = Some(ni);
        let (c, off) = self.rt_descend(ni, index);
        node = c;
        offset = off;
    }
    if node != 0 {
        self.node_tag_clear(parent, tag, offset);
    }
    node
}

pub fn radix_tree_iter_tag_clear(&mut self, iter: &RIter, tag: u32) {
    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return; }
    self.node_tag_clear(iter.node, tag, iter_offset(iter));
}

pub fn radix_tree_tag_get(&self, index: u64, tag: u32) -> i64 {
    if tag >= RADIX_TREE_MAX_TAGS { return 0; }
    if !self.root_tag_get(tag) {
        return 0;
    }
    let (mut node, maxindex, _) = self.rt_load_root();
    if index > maxindex {
        return 0;
    }
    let mut __lim = self.nodes.len() + 1;
    while rt_is_internal(node) {
        if __lim == 0 { break; }
        __lim -= 1;
        let ni = entry_to_node(node);
        let (c, off) = self.rt_descend(ni, index);
        node = c;
        if !tag_get(nat(self.nodes, &self.scratch, ni), tag, off) {
            return 0;
        }
        if node == RADIX_TREE_RETRY {
            break;
        }
    }
    1
}

pub fn radix_tree_tagged(&self, tag: u32) -> i64 {
    if tag >= RADIX_TREE_MAX_TAGS { return 0; }
    /* upstream root_tag_get() returns the raw masked bit, not 0/1 */
    (self.root.flags & (1 << (ROOT_TAG_SHIFT + tag))) as i64
}

pub(crate) fn set_iter_tags(&self, iter: &mut RIter, node: Option<usize>, offset: u64, tag: u32) {
    match node {
        None => {
            iter.tags = 1;
        }
        Some(ni) => {
            iter.tags = nat(self.nodes, &self.scratch, ni).marks[(tag as usize).min(MAX_MARKS - 1)] >> offset;
        }
    }
}

pub fn radix_tree_next_chunk(&mut self, iter: &mut RIter, flags: u32) -> Option<SlotL> {
    let tag = flags & ITER_TAG_MASK;
    let tagged = flags & ITER_TAGGED != 0;
    let contig = flags & ITER_CONTIG != 0;
    if tagged && self.warn_on(tag >= RADIX_TREE_MAX_TAGS) {
        return None;
    }

    if tagged && !self.root_tag_get(tag) {
        return None;
    }

    let mut index = iter.next_index;
    if index == 0 && iter.index != 0 {
        return None;
    }

    let mut __lim = (self.nodes.len() + 1) * 66;
    'restart: loop {
        if __lim == 0 { break None; }
        __lim -= 1;
        let (mut child, maxindex, _) = self.rt_load_root();
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
            return Some(SlotL::Root);
        }

        let mut node = 0usize;
        let mut offset = 0u64;
        let mut __lim = self.nodes.len() + 1;
        loop {
            if __lim == 0 { break; }
            __lim -= 1;
            node = entry_to_node(child);
            let (c, off) = self.rt_descend(node, index);
            child = c;
            offset = off;

            if if tagged {
                !tag_get(nat(self.nodes, &self.scratch, node), tag, offset)
            } else {
                child == 0
            } {
                /* hole detected */
                if contig {
                    return None;
                }
                if tagged {
                    offset = rt_find_next_bit(nat(self.nodes, &self.scratch, node), tag, offset + 1);
                } else {
                    loop {
                        offset += 1;
                        if offset >= RADIX_TREE_MAP_SIZE {
                            break;
                        }
                        if nat(self.nodes, &self.scratch, node).slots[(offset as usize).min(CHUNK_SIZE - 1)] != 0 {
                            break;
                        }
                    }
                }
                index &= !node_maxindex(nat(self.nodes, &self.scratch, node));
                index = index.wrapping_add(offset << nat(self.nodes, &self.scratch, node).shift.min(63));
                if index == 0 {
                    return None;
                }
                if offset == RADIX_TREE_MAP_SIZE {
                    continue 'restart;
                }
                child = nat(self.nodes, &self.scratch, node).slots[(offset as usize).min(CHUNK_SIZE - 1)];
            }
            if child == 0 {
                continue 'restart;
            }
            if child == RADIX_TREE_RETRY {
                break;
            }
            if !(nat(self.nodes, &self.scratch, node).shift.min(63) != 0 && rt_is_internal(child)) {
                break;
            }
        }

        iter.index = (index & !node_maxindex(nat(self.nodes, &self.scratch, node))) | offset;
        iter.next_index = (index | node_maxindex(nat(self.nodes, &self.scratch, node))).wrapping_add(1);
        iter.node = Some(node);
        if tagged {
            self.set_iter_tags(iter, Some(node), offset, tag);
        }
        return Some(SlotL::Node(node, offset as usize));
    }
}

pub fn radix_tree_iter_lookup(&mut self, iter: &mut RIter, index: u64) -> Option<SlotL> {
    radix_tree_iter_init(iter, index);
    self.radix_tree_next_chunk(iter, ITER_CONTIG)
}

pub fn radix_tree_next_slot(
    &self,
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
            iter.index = radix_tree_iter_add(iter, 1);
            return Some(slot_next(slot));
        }
        if flags & ITER_CONTIG == 0 {
            let mut offset = iter.tags.trailing_zeros() as u64;
            iter.tags >>= offset;
            offset += 1;
            iter.index = radix_tree_iter_add(iter, offset);
            return Some(slot_add(slot, offset));
        }
        return None;
    }
    let mut count = radix_tree_chunk_size(iter);
    let mut s = slot;
    loop {
        count -= 1;
        if count <= 0 {
            break;
        }
        s = slot_next(s);
        iter.index = radix_tree_iter_add(iter, 1);
        if self.slot_get(s) != 0 {
            return Some(s);
        }
        if flags & ITER_CONTIG != 0 {
            iter.next_index = 0;
            break;
        }
    }
    None
}

pub(crate) fn __radix_tree_delete(&mut self, node: Option<usize>, slot: SlotL) -> bool {
    let old = self.slot_get(slot);
    let values = if xa_is_value(old) { -1 } else { 0 };
    let offset = get_slot_offset(node, slot);
    if self.is_idr() {
        self.node_tag_set(node, IDR_FREE, offset);
    } else {
        for tag in 0..RADIX_TREE_MAX_TAGS {
            self.node_tag_clear(node, tag, offset);
        }
    }
    self.replace_slot(slot, 0, node, -1, values);
    match node {
        Some(ni) => self.delete_node(ni),
        None => false,
    }
}

pub fn radix_tree_iter_delete(&mut self, iter: &mut RIter, slot: SlotL) {
    if self.__radix_tree_delete(iter.node, slot) {
        iter.index = iter.next_index;
    }
}

pub fn radix_tree_delete_item(&mut self, index: u64, item: u64) -> u64 {
    let (entry, node, slot) = self.__radix_tree_lookup(index);
    let slot = match slot {
        None => return 0,
        Some(s) => s,
    };
    if entry == 0
        && (!self.is_idr() || self.node_tag_get(node, IDR_FREE, get_slot_offset(node, slot)))
    {
        return 0;
    }
    if item != 0 && entry != item {
        return 0;
    }
    self.__radix_tree_delete(node, slot);
    entry
}

pub fn radix_tree_delete(&mut self, index: u64) -> u64 {
    self.radix_tree_delete_item(index, 0)
}

    fn preload_push(&mut self, ni: usize) {
        let head = self.pl_top().map(node_ptr).unwrap_or(0);
        nmat(self.nodes, &mut self.scratch, ni).parent = head;
        self.pl_push(ni);
    }

    pub(crate) fn __radix_tree_preload(&mut self, gfp_mask: u32, nr: u32) -> i64 {
        let _gfp = gfp_mask & !GFP_ACCOUNT;
        while self.plen < self.preload.len() && (self.plen as u32) < nr {
            let ni = match self.kmem_cache_alloc() {
                None => return -ENOMEM,
                Some(i) => i,
            };
            self.preload_push(ni);
        }
        0
    }

    /// `radix_tree_preload_end` — drops the per-cpu preload list upstream;
    /// here the preload stack is simply model state (no preemption).
    pub fn radix_tree_preload_end(&mut self) {}

    pub(crate) fn each_slot<F>(&mut self, start: u64, flags: u32, mut body: F)
    where
        F: FnMut(&mut Self, &mut RIter, SlotL) -> bool,
    {
        let mut iter = RIter::new();
        let mut slot = radix_tree_iter_init(&mut iter, start);
        loop {
            let s = match slot {
                Some(s) => s,
                None => match self.radix_tree_next_chunk(&mut iter, flags) {
                    None => break,
                    Some(s) => s,
                },
            };
            if !body(self, &mut iter, s) {
                break;
            }
            slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
        }
    }

    fn gang_entries(&mut self, first: u64, out: &mut [u64], tag: Option<u32>) -> u32 {
        let max_items = out.len() as u32;
        if max_items == 0 {
            return 0;
        }
        let flags = tag.map(|t| ITER_TAGGED | t).unwrap_or(0);
        let mut iter = RIter::new();
        let mut slot = radix_tree_iter_init(&mut iter, first);
        let mut n = 0u32;
        loop {
            let s = match slot {
                Some(s) => s,
                None => match self.radix_tree_next_chunk(&mut iter, flags) {
                    None => break,
                    Some(s) => s,
                },
            };
            let item = self.slot_get(s);
            if item == 0 {
                slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
                continue;
            }
            if rt_is_internal(item) {
                slot = radix_tree_iter_retry(&mut iter);
                continue;
            }
            out[n as usize] = item;
            n += 1;
            if n == max_items {
                break;
            }
            slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
        }
        n
    }

    fn gang_slots(&mut self, first: u64, out: &mut [SlotL], tag: Option<u32>) -> u32 {
        let max_items = out.len() as u32;
        if max_items == 0 {
            return 0;
        }
        let flags = tag.map(|t| ITER_TAGGED | t).unwrap_or(0);
        let mut n = 0u32;
        self.each_slot(first, flags, |_, _, s| {
            out[n as usize] = s;
            n += 1;
            n != max_items
        });
        n
    }

    /// `radix_tree_gang_lookup` — collect up to `out.len()` entries starting
    /// at or after `first`; returns the count collected.
    pub fn radix_tree_gang_lookup(&mut self, out: &mut [u64], first: u64) -> u32 {
        self.gang_entries(first, out, None)
    }

    /// `radix_tree_gang_lookup_tag` — tag-filtered `radix_tree_gang_lookup`.
    pub fn radix_tree_gang_lookup_tag(&mut self, out: &mut [u64], first: u64, tag: u32) -> u32 {
        self.gang_entries(first, out, Some(tag))
    }

    /// `radix_tree_gang_lookup_slot` — collect slot lvalues (untagged).
    pub fn radix_tree_gang_lookup_slot(&mut self, out: &mut [SlotL], first: u64) -> u32 {
        self.gang_slots(first, out, None)
    }

    /// `radix_tree_gang_lookup_tag_slot` — collect slot lvalues (tagged).
    pub fn radix_tree_gang_lookup_tag_slot(&mut self, out: &mut [SlotL], first: u64, tag: u32) -> u32 {
        self.gang_slots(first, out, Some(tag))
    }

    /// `radix_tree_empty` — is the tree empty?
    pub fn radix_tree_empty(&self) -> bool {
        self.xa_empty()
    }

    /// `radix_tree_shrink` — shrink the tree (internal helper, also used by
    /// the deferred free path).
    pub fn radix_tree_shrink_pub(&mut self) -> bool {
        self.radix_tree_shrink()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    include!("radix_tree_tests.inc");
}
