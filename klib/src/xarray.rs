//! `klib::xarray` — Linux `lib/xarray.c` (radix-tree successor) ported to
//! safe Rust, verified against the C implementation (983/983 recorded
//! differential cases, `ports/lib-xarray`).
//!
//! Storage model: a caller-owned dense node arena (`&mut [Node]`) replaces
//! the slab allocator. Node "pointers" are canonical addresses
//! `NBASE + i*NSTRIDE`; entries in slots are plain `u64` values tagged
//! exactly like the kernel's `void *` (values `(v<<1)|1`, internal
//! `(v<<2)|2`, node pointers `ptr|2`).
//!
//! Notes:
//! * `xa_find`/`xa_find_after` bound the upstream `xas_retry` loop at 256
//!   attempts — identical to the verified C harness, which bounds the same
//!   way to avoid upstream's livelock on retry entries.
//! * Allocation-failure injection: set `fail_mask` bit `k` to fail the
//!   (k+1)-th node allocation (`alloc_ctr` counts attempts).
//! * The `update` flag on `Xas` models the kernel's `xa_update_t` callback:
//!   nodes are tracked on the intrusive shadow list (`shadow_head`) while
//!   nonempty, mirroring `xa_track_free` behaviour.

#![forbid(unsafe_code)]
#![allow(clippy::all)]

/* ================= canonical model ================= */

/// Default node-pool capacity (matches the verified harness).
pub const NP: usize = 160;
pub const NBASE: u64 = 0x40000;
pub const NSTRIDE: u64 = 4096;
pub const XA_CANON: u64 = 0x800000;
pub const XA_RCU_FREE: u64 = 1;

pub const CHUNK_SHIFT: u32 = 6;
pub const CHUNK_SIZE: usize = 64;
pub const CHUNK_MASK: u64 = 63;
pub const MAX_MARKS: usize = 3;

pub const XA_FLAGS_TRACK_FREE: u32 = 4;
pub const XA_FLAGS_ZERO_BUSY: u32 = 8;
pub const XA_FLAGS_ALLOC_WRAPPED: u32 = 16;
pub const XA_FLAGS_ACCOUNT: u32 = 32;
pub const GFP_BITS_SHIFT: u32 = 24;

pub const XA_MARK_0: u32 = 0;
pub const XA_MARK_1: u32 = 1;
pub const XA_MARK_2: u32 = 2;
pub const XA_PRESENT: u32 = 8;
pub const XA_FREE_MARK: u32 = XA_MARK_0;
pub const XA_MARK_MAX: u32 = XA_MARK_2;

pub const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
pub const XA_FLAGS_ALLOC1: u32 = XA_FLAGS_TRACK_FREE | XA_FLAGS_ZERO_BUSY;

pub fn xa_flags_mark(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}
pub const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

pub const ENOMEM: i64 = 12;
pub const EINVAL: i64 = 22;
pub const EBUSY: i64 = 16;
pub const MAX_ERRNO: i64 = 4095;

pub const XAS_BOUNDS: u64 = 1;
pub const XAS_RESTART: u64 = 3;

pub const XA_ZERO_ENTRY: u64 = 1030; /* xa_mk_internal(257) */
pub const XA_RETRY_ENTRY: u64 = 1026; /* xa_mk_internal(256) */

/// Intrusive shadow-list terminator (not a node index).
const NIL: usize = usize::MAX;

/* ================= structures ================= */

/// One radix-tree node: 64 slots plus per-mark bitmaps.
///
/// The arena (`Xarray.nodes`) is caller-owned; `used` models the slab
/// allocated/free state and `shadow_next` the kernel's shadow-node list.
/// Callers must not modify fields while the node is in use.
#[derive(Clone)]
pub struct Node {
    pub shift: u8,
    pub offset: u8,
    pub count: u8,
    pub nr_values: u8,
    pub parent: u64, /* canonical node ptr or 0 */
    pub array: u64,  /* XA_CANON or XA_RCU_FREE */
    pub on_list: bool,
    pub slots: [u64; CHUNK_SIZE],
    pub marks: [u64; MAX_MARKS],
    pub used: bool,
    pub shadow_next: usize,
}

impl Node {
    pub const fn new() -> Self {
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
            used: false,
            shadow_next: NIL,
        }
    }
}

/// `xa_state` — the iteration/store cursor. All fields are public exactly
/// like the C struct; use `Xas::new`/`Xas::new_order` to construct.
#[derive(Clone)]
pub struct Xas {
    pub index: u64,
    pub shift: u8,
    pub sibs: u8,
    pub offset: u8,
    pub node: u64,
    pub alloc: u64,
    pub update: bool,
}

impl Xas {
    pub fn new(index: u64) -> Self {
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
    /// `XA_STATE_ORDER`
    pub fn new_order(index: u64, order: u32) -> Self {
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

/// A sparse array (`struct xarray`) over a caller-owned node arena.
///
/// Caller invariants (same contract as the C API):
/// * entries stored via `xa_*`/`xas_*` must not be internal encodings that
///   classify as node pointers (`xa_is_node` — i.e. `&3 == 2` and `> 4096`);
///   `xas_store` WARNs (`warn` counter) when it sees one;
/// * `Xas` fields are public like the C struct, but mutating
///   `node`/`alloc`/`offset`/`shift` mid-operation is out of contract —
///   start each operation with `Xas::new`/`Xas::new_order`;
/// * `xas_split` requires a preceding `xas_split_alloc`;
/// * `order` arguments must be `< 64`; `mark` arguments must be
///   `< MAX_MARKS` (3);
/// * `nodes` must not be modified while in use (`Xarray::new` resets them);
/// * `xa_init_flags` only resets flags+head (faithful to upstream): use it
///   on a freshly-`Xarray::new`ed arena or after `xa_destroy`, never to
///   re-init a live array.
pub struct Xarray<'a> {
    pub nodes: &'a mut [Node],
    pub xa_head: u64,
    pub xa_flags: u32,
    /// Number of node-allocation attempts (for `fail_mask` injection).
    pub alloc_ctr: u64,
    /// Bit `k` set => the (k+1)-th `kmem_cache_alloc_lru` attempt fails.
    pub fail_mask: u64,
    /// WARN_ONCE counters (modelled; observable only).
    pub warn: u64,
    pub warn_once: u64,
    shadow_head: usize,
}

/* ================= entry classification ================= */

pub fn xa_is_value(e: u64) -> bool {
    e & 1 == 1
}
pub fn xa_to_value(e: u64) -> u64 {
    e >> 1
}
pub fn xa_mk_value(v: u64) -> u64 {
    (v << 1) | 1
}
pub fn xa_is_internal(e: u64) -> bool {
    e & 3 == 2
}
pub fn xa_mk_internal(v: u64) -> u64 {
    (v << 2) | 2
}
pub fn xa_to_internal(e: u64) -> u64 {
    e >> 2
}
pub fn xa_is_node(e: u64) -> bool {
    xa_is_internal(e) && e > 4096
}
pub fn xa_to_node(e: u64) -> usize {
    ((e - 2 - NBASE) / NSTRIDE) as usize
}
pub fn xa_mk_node(ni: usize) -> u64 {
    (NBASE + ni as u64 * NSTRIDE) | 2
}
pub fn node_ptr(ni: usize) -> u64 {
    NBASE + ni as u64 * NSTRIDE
}
pub fn nidx_ptr(p: u64) -> usize {
    ((p - NBASE) / NSTRIDE) as usize
}
pub fn xa_is_sibling(e: u64) -> bool {
    xa_is_internal(e) && e < xa_mk_sibling((CHUNK_SIZE - 1) as u64)
}
pub fn xa_to_sibling(e: u64) -> u64 {
    e >> 2
}
pub fn xa_mk_sibling(offset: u64) -> u64 {
    (offset << 2) | 2
}
pub fn xa_is_retry(e: u64) -> bool {
    e == XA_RETRY_ENTRY
}
pub fn xa_is_zero(e: u64) -> bool {
    e == XA_ZERO_ENTRY
}
pub fn xa_is_advanced(e: u64) -> bool {
    xa_is_internal(e) && e <= XA_RETRY_ENTRY
}
pub fn xa_is_err(e: u64) -> bool {
    xa_is_internal(e) && e >= xa_mk_internal((-MAX_ERRNO) as u64)
}
pub fn xa_err(e: u64) -> i64 {
    if xa_is_err(e) { (e as i64) >> 2 } else { 0 }
}
pub fn xa_error(errno: i64) -> u64 {
    ((errno as u64) << 2) | 2
}
pub fn xa_zero_to_null(e: u64) -> u64 {
    if xa_is_zero(e) { 0 } else { e }
}

/* xas->xa_node sentinels */
pub fn xas_error(xas: &Xas) -> i64 {
    xa_err(xas.node)
}
pub fn xas_set_err(xas: &mut Xas, err: i64) {
    xas.node = xa_error(err);
}
pub fn xas_invalid(xas: &Xas) -> bool {
    xas.node & 3 != 0
}
pub fn xas_valid(xas: &Xas) -> bool {
    !xas_invalid(xas)
}
pub fn xas_is_node(xas: &Xas) -> bool {
    xas_valid(xas) && xas.node != 0
}
pub fn xas_not_node(n: u64) -> bool {
    n & 3 != 0 || n == 0
}
pub fn xas_frozen(n: u64) -> bool {
    n & 2 != 0
}
pub fn xas_top(n: u64) -> bool {
    n <= XAS_RESTART
}pub fn node_get_mark(n: &Node, offset: u64, mark: usize) -> bool {
    (n.marks[mark] >> offset) & 1 == 1
}
pub fn node_set_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[mark] |= 1 << offset;
    was
}
pub fn node_clear_mark(n: &mut Node, offset: u64, mark: usize) -> bool {
    let was = node_get_mark(n, offset, mark);
    n.marks[mark] &= !(1 << offset);
    was
}
pub fn node_any_mark(n: &Node, mark: usize) -> bool {
    n.marks[mark] != 0
}
pub fn node_mark_all(n: &mut Node, mark: usize) {
    n.marks[mark] = u64::MAX;
}
pub fn node_get_marks(n: &Node, offset: u64) -> u32 {
    let mut marks = 0u32;
    for m in 0..MAX_MARKS {
        if node_get_mark(n, offset, m) {
            marks |= 1 << m;
        }
    }
    marks
}
pub fn node_mark_slots(n: &mut Node, sibs: u64, mark: usize) {
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
pub fn find_next_bit(word: u64, limit: u64, start: u64) -> u64 {
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

pub fn get_offset(index: u64, n: &Node) -> u64 {
    (index >> n.shift) & CHUNK_MASK
}

pub fn set_bounds(xas: &mut Xas) -> u64 {
    xas.node = XAS_BOUNDS;
    0
}

pub fn xas_size(xas: &Xas) -> u64 {
    ((xas.sibs as u64) + 1) << xas.shift
}

pub fn xas_max(xas: &Xas) -> u64 {
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

pub fn xas_reset(xas: &mut Xas) {
    xas.node = XAS_RESTART;
}

pub fn xas_retry(xas: &mut Xas, entry: u64) -> bool {
    if xa_is_zero(entry) {
        return true;
    }
    if !xa_is_retry(entry) {
        return false;
    }
    xas_reset(xas);
    true
}

pub fn xas_set(xas: &mut Xas, index: u64) {
    xas.index = index;
    xas.node = XAS_RESTART;
}

pub fn xas_set_order(xas: &mut Xas, index: u64, order: u32) {
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

pub fn xas_result(xas: &Xas, mut curr: u64) -> u64 {
    if xas_error(xas) != 0 {
        curr = xas.node;
    }
    curr
}

/* ================= alloc / destroy machinery ================= */

pub fn xas_try_split_min_order(order: u32) -> u32 {
    if order % CHUNK_SHIFT == 0 {
        if order == 0 { 0 } else { order - 1 }
    } else {
        order - (order % CHUNK_SHIFT)
    }
}

pub fn xas_set_range(xas: &mut Xas, first: u64, last: u64) {
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


impl<'a> Xarray<'a> {
    /// Fresh array over `nodes`, equivalent to `reset_all`/`xa_init_flags`:
    /// every node is reset, the array is empty, flags set.
    pub fn new(nodes: &'a mut [Node], flags: u32) -> Self {
        for n in nodes.iter_mut() {
            *n = Node::new();
        }
        Xarray {
            nodes,
            xa_head: 0,
            xa_flags: flags,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
            shadow_head: NIL,
        }
    }

    fn is_node_ptr(&self, p: u64) -> bool {
        p >= NBASE && p < NBASE + self.nodes.len() as u64 * NSTRIDE && p & 3 == 0
    }

    fn shadow_push(&mut self, ni: usize) {
        self.nodes[ni].shadow_next = self.shadow_head;
        self.shadow_head = ni;
    }

    fn shadow_remove(&mut self, ni: usize) {
        let mut cur = self.shadow_head;
        let mut prev = NIL;
        while cur != NIL {
            let next = self.nodes[cur].shadow_next;
            if cur == ni {
                if prev == NIL {
                    self.shadow_head = next;
                } else {
                    self.nodes[prev].shadow_next = next;
                }
                return;
            }
            prev = cur;
            cur = next;
        }
    }
    pub fn xa_track_free(&self) -> bool {
        self.xa_flags & XA_FLAGS_TRACK_FREE != 0
    }
    pub fn xa_zero_busy(&self) -> bool {
        self.xa_flags & XA_FLAGS_ZERO_BUSY != 0
    }
    pub fn xa_marked(&self, mark: u32) -> bool {
        self.xa_flags & xa_flags_mark(mark) != 0
    }
    pub fn xa_mark_set(&mut self, mark: u32) {
        if !self.xa_marked(mark) {
            self.xa_flags |= xa_flags_mark(mark);
        }
    }
    pub fn xa_mark_clear(&mut self, mark: u32) {
        if self.xa_marked(mark) {
            self.xa_flags &= !xa_flags_mark(mark);
        }
    }
    pub fn xa_mark_init(&mut self, mark: u32) {
        if self.xa_track_free() && mark == XA_FREE_MARK {
            self.xa_mark_set(mark);
        } else {
            self.xa_mark_clear(mark);
        }
    }
    pub fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }
    pub fn warn_on_once(&mut self, bit: u32, c: bool) -> bool {
        if c && self.warn_once & (1 << bit) == 0 {
            self.warn_once |= 1 << bit;
            self.warn += 1;
        }
        c
    }
pub fn kmem_cache_alloc_lru(&mut self) -> Option<usize> {
    self.alloc_ctr += 1;
    if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
        return None;
    }
    for i in 0..self.nodes.len() {
        if !self.nodes[i].used {
            self.nodes[i].used = true;
            return Some(i);
        }
    }
    None
}


pub fn kmem_cache_free(&mut self, ni: usize) {
    self.nodes[ni].used = false;
}


pub fn radix_tree_node_rcu_free(&mut self, ni: usize) {
    self.nodes[ni].slots = [0; CHUNK_SIZE];
    self.nodes[ni].marks = [0; MAX_MARKS];
    if self.nodes[ni].on_list {
        self.shadow_remove(ni);
        self.nodes[ni].on_list = false;
    }
    self.kmem_cache_free(ni);
}


pub fn xa_node_free(&mut self, ni: usize) {
    self.nodes[ni].array = XA_RCU_FREE;
    self.radix_tree_node_rcu_free(ni);
}
pub fn upd_cb(&mut self, ni: usize) {
    if self.nodes[ni].count != 0 && !self.nodes[ni].on_list {
        self.shadow_push(ni); /* list_add pushes to the front */
        self.nodes[ni].on_list = true;
    } else if self.nodes[ni].count == 0 && self.nodes[ni].on_list {
        self.shadow_remove(ni);
        self.nodes[ni].on_list = false;
    }
}

pub fn xas_update(&mut self, xas: &Xas, ni: usize) {
    if xas.update {
        self.upd_cb(ni);
    }
}

/* ================= node marks ================= */


pub fn xas_get_mark(&self, xas: &Xas, mark: usize) -> bool {
    if xas_invalid(xas) {
        return false;
    }
    if xas.node == 0 {
        return self.xa_marked(mark as u32);
    }
    if !self.is_node_ptr(xas.node) {
        return false;
    }
    node_get_mark(&self.nodes[nidx_ptr(xas.node)], xas.offset as u64, mark)
}


pub fn xas_set_mark(&mut self, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    while self.is_node_ptr(node) {
        let ni = nidx_ptr(node);
        if node_set_mark(&mut self.nodes[ni], offset, mark) {
            return;
        }
        offset = self.nodes[ni].offset as u64;
        node = self.nodes[ni].parent;
    }
    if !self.xa_marked(mark as u32) {
        self.xa_mark_set(mark as u32);
    }
}


pub fn xas_clear_mark(&mut self, xas: &Xas, mark: usize) {
    if xas_invalid(xas) {
        return;
    }
    let mut node = xas.node;
    let mut offset = xas.offset as u64;
    while self.is_node_ptr(node) {
        let ni = nidx_ptr(node);
        if !node_clear_mark(&mut self.nodes[ni], offset, mark) {
            return;
        }
        if node_any_mark(&self.nodes[ni], mark) {
            return;
        }
        offset = self.nodes[ni].offset as u64;
        node = self.nodes[ni].parent;
    }
    if self.xa_marked(mark as u32) {
        self.xa_mark_clear(mark as u32);
    }
}


pub fn xas_init_marks(&mut self, xas: &Xas) {
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


pub fn xas_squash_marks(&mut self, xas: &Xas) {
    /* upstream: merge all marks to the first entry — if any sibling slot
     * carries a mark, set it on xa_offset and clear it on the sibs range */
    let ni = nidx_ptr(xas.node);
    let limit = xas.offset as u64 + xas.sibs as u64 + 1;
    let mut mark = 0usize;
    loop {
        if find_next_bit(self.nodes[ni].marks[mark], limit, xas.offset as u64 + 1) != limit {
            let n = &mut self.nodes[ni];
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


pub fn xas_find_chunk(&self, xas: &Xas, advance: bool, mark: usize) -> u64 {
    let word = self.nodes[nidx_ptr(xas.node)].marks[mark];
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


pub fn xas_set_offset(&self, xas: &mut Xas) {
    if !self.is_node_ptr(xas.node) {
        return;
    }
    xas.offset = get_offset(xas.index, &self.nodes[nidx_ptr(xas.node)]) as u8;
}


pub fn xas_move_index(&self, xas: &mut Xas, offset: u64) {
    if !self.is_node_ptr(xas.node) {
        return;
    }
    let shift = self.nodes[nidx_ptr(xas.node)].shift;
    xas.index &= (!CHUNK_MASK) << shift;
    xas.index = xas.index.wrapping_add(offset << shift);
}


pub fn xas_next_offset(&self, xas: &mut Xas) {
    xas.offset = xas.offset.wrapping_add(1);
    self.xas_move_index(xas, xas.offset as u64);
}


pub fn max_index(&self, entry: u64) -> u64 {
    if !xa_is_node(entry) {
        return 0;
    }
    ((CHUNK_SIZE as u64) << self.nodes[xa_to_node(entry)].shift).wrapping_sub(1)
}


pub fn xas_reload(&mut self, xas: &mut Xas) -> u64 {
    if xas.node == 0 {
        return self.xa_head;
    }
    /* sentinels (BOUNDS/RESTART/error) carry no node; C would deref them */
    if !self.is_node_ptr(xas.node) {
        return 0;
    }
    let ni = nidx_ptr(xas.node);
    let mut offset = (xas.index >> self.nodes[ni].shift) & CHUNK_MASK;
    let mut entry = self.nodes[ni].slots[offset as usize];
    if !xa_is_sibling(entry) {
        return entry;
    }
    offset = xa_to_sibling(entry);
    entry = self.nodes[ni].slots[offset as usize];
    entry
}


pub fn xas_start(&mut self, xas: &mut Xas) -> u64 {
    if xas_valid(xas) {
        return self.xas_reload(xas);
    }
    if xas_error(xas) != 0 {
        return 0;
    }
    let entry = self.xa_head;
    if !xa_is_node(entry) {
        if xas.index != 0 {
            return set_bounds(xas);
        }
    } else if (xas.index >> self.nodes[xa_to_node(entry)].shift) > CHUNK_MASK {
        return set_bounds(xas);
    }
    xas.node = 0;
    entry
}


pub fn xas_descend(&self, xas: &mut Xas, ni: usize) -> u64 {
    let mut offset = get_offset(xas.index, &self.nodes[ni]);
    let mut entry = self.nodes[ni].slots[offset as usize];
    xas.node = node_ptr(ni);
    while xa_is_sibling(entry) {
        offset = xa_to_sibling(entry);
        entry = self.nodes[ni].slots[offset as usize];
        if self.nodes[ni].shift != 0 && xa_is_node(entry) {
            entry = XA_RETRY_ENTRY;
        }
    }
    xas.offset = offset as u8;
    entry
}


pub fn xas_load(&mut self, xas: &mut Xas) -> u64 {
    let mut entry = self.xas_start(xas);
    while xa_is_node(entry) {
        let ni = xa_to_node(entry);
        if xas.shift > self.nodes[ni].shift as u8 {
            break;
        }
        entry = self.xas_descend(xas, ni);
        if self.nodes[ni].shift == 0 {
            break;
        }
    }
    entry
}


pub fn xas_advance(&self, xas: &mut Xas, index: u64) {
    let shift = if xas_is_node(xas) && self.is_node_ptr(xas.node) {
        self.nodes[nidx_ptr(xas.node)].shift
    } else {
        0
    };
    xas.index = index;
    xas.offset = ((index >> shift) & CHUNK_MASK) as u8;
}


pub fn xas_pause(&self, xas: &mut Xas) {
    let node = xas.node;
    if xas_invalid(xas) {
        return;
    }
    xas.node = XAS_RESTART;
    if self.is_node_ptr(node) {
        let ni = nidx_ptr(node);
        let mut offset = xas.offset as u64;
        loop {
            offset += 1;
            if offset >= CHUNK_SIZE as u64 {
                break;
            }
            if !xa_is_sibling(self.nodes[ni].slots[offset as usize]) {
                break;
            }
        }
        xas.index &= u64::MAX << self.nodes[ni].shift;
        xas.index = xas
            .index
            .wrapping_add((offset - xas.offset as u64) << self.nodes[ni].shift);
        if xas.index == 0 {
            xas.node = XAS_BOUNDS;
        }
    } else {
        xas.index = xas.index.wrapping_add(1);
    }
}


pub fn xas_destroy(&mut self, xas: &mut Xas) {
    let mut node = xas.alloc;
    while node != 0 {
        let ni = nidx_ptr(node);
        let next = self.nodes[ni].parent;
        self.radix_tree_node_rcu_free(ni);
        node = next;
    }
    xas.alloc = 0;
}


pub fn xas_nomem(&mut self, xas: &mut Xas) -> bool {
    if xas.node != xa_error(-ENOMEM) {
        self.xas_destroy(xas);
        return false;
    }
    match self.kmem_cache_alloc_lru() {
        None => false,
        Some(ni) => {
            xas.alloc = node_ptr(ni);
            self.nodes[ni].parent = 0;
            xas.node = XAS_RESTART;
            true
        }
    }
}


pub fn __xas_nomem(&mut self, xas: &mut Xas) -> bool {
    self.xas_nomem(xas)
}


pub fn xas_alloc(&mut self, xas: &mut Xas, shift: u8) -> Option<usize> {
    let parent = xas.node;
    if xas_invalid(xas) {
        return None;
    }
    let ni = if xas.alloc != 0 {
        let ni = nidx_ptr(xas.alloc);
        xas.alloc = 0;
        ni
    } else {
        match self.kmem_cache_alloc_lru() {
            None => {
                xas_set_err(xas, -ENOMEM);
                return None;
            }
            Some(i) => i,
        }
    };
    if parent != 0 {
        let pni = nidx_ptr(parent);
        self.nodes[ni].offset = xas.offset;
        self.nodes[pni].count = self.nodes[pni].count.wrapping_add(1);
        self.xas_update(xas, pni);
    }
    self.nodes[ni].shift = shift;
    self.nodes[ni].count = 0;
    self.nodes[ni].nr_values = 0;
    self.nodes[ni].parent = parent;
    self.nodes[ni].array = XA_CANON;
    Some(ni)
}


pub fn xas_free_nodes(&mut self, xas: &Xas, top: usize) {
    let mut offset = 0u32;
    let mut node = top;
    loop {
        let entry = self.nodes[node].slots[offset as usize];
        if self.nodes[node].shift != 0 && xa_is_node(entry) {
            node = xa_to_node(entry);
            offset = 0;
            continue;
        }
        if entry != 0 {
            self.nodes[node].slots[offset as usize] = XA_RETRY_ENTRY;
        }
        offset += 1;
        while offset >= CHUNK_SIZE as u32 {
            let parent = self.nodes[node].parent;
            offset = self.nodes[node].offset as u32 + 1;
            self.nodes[node].count = 0;
            self.nodes[node].nr_values = 0;
            self.xas_update(xas, node);
            self.xa_node_free(node);
            if node == top {
                return;
            }
            node = nidx_ptr(parent);
        }
    }
}


pub fn xas_shrink(&mut self, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    loop {
        if self.nodes[node].count != 1 {
            break;
        }
        let mut entry = self.nodes[node].slots[0];
        if entry == 0 {
            break;
        }
        if !xa_is_node(entry) && self.nodes[node].shift != 0 {
            break;
        }
        if self.xa_zero_busy() {
            entry = xa_zero_to_null(entry);
        }
        xas.node = XAS_BOUNDS;
        self.xa_head = entry;
        if self.xa_track_free() && !node_get_mark(&self.nodes[node], 0, XA_FREE_MARK as usize) {
            self.xa_mark_clear(XA_FREE_MARK);
        }
        self.nodes[node].count = 0;
        self.nodes[node].nr_values = 0;
        if !xa_is_node(entry) {
            self.nodes[node].slots[0] = XA_RETRY_ENTRY;
        }
        self.xas_update(xas, node);
        self.xa_node_free(node);
        if !xa_is_node(entry) {
            break;
        }
        node = xa_to_node(entry);
        self.nodes[node].parent = 0;
    }
}


pub fn xas_delete_node(&mut self, xas: &mut Xas) {
    let mut node = nidx_ptr(xas.node);
    loop {
        if self.nodes[node].count != 0 {
            break;
        }
        let parent = self.nodes[node].parent;
        xas.node = parent;
        xas.offset = self.nodes[node].offset;
        self.xa_node_free(node);
        if parent == 0 {
            self.xa_head = 0;
            xas.node = XAS_BOUNDS;
            return;
        }
        let pni = nidx_ptr(parent);
        self.nodes[pni].slots[xas.offset as usize] = 0;
        self.nodes[pni].count = self.nodes[pni].count.wrapping_sub(1);
        node = pni;
        self.xas_update(xas, node);
    }
    if self.nodes[node].parent == 0 {
        self.xas_shrink(xas);
    }
}

/* ================= expand / create ================= */


pub fn xas_expand(&mut self, xas: &mut Xas, mut head: u64) -> i64 {
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
        shift = self.nodes[ni].shift as u32 + CHUNK_SHIFT;
    }
    xas.node = 0;

    while max > self.max_index(head) {
        let ni = match self.xas_alloc(xas, shift as u8) {
            None => return -ENOMEM,
            Some(i) => i,
        };
        self.nodes[ni].count = 1;
        if xa_is_value(head) {
            self.nodes[ni].nr_values = 1;
        }
        self.nodes[ni].slots[0] = head;

        let mut mark = 0u32;
        loop {
            if self.xa_track_free() && mark == XA_FREE_MARK {
                node_mark_all(&mut self.nodes[ni], XA_FREE_MARK as usize);
                if !self.xa_marked(XA_FREE_MARK) {
                    node_clear_mark(&mut self.nodes[ni], 0, XA_FREE_MARK as usize);
                    self.xa_mark_set(XA_FREE_MARK);
                }
            } else if self.xa_marked(mark) {
                node_set_mark(&mut self.nodes[ni], 0, mark as usize);
            }
            if mark == XA_MARK_MAX {
                break;
            }
            mark += 1;
        }

        if xa_is_node(head) {
            let hni = xa_to_node(head);
            self.nodes[hni].offset = 0;
            self.nodes[hni].parent = node_ptr(ni);
        }
        head = xa_mk_node(ni);
        self.xa_head = head;
        self.xas_update(xas, ni);

        shift += CHUNK_SHIFT;
        node = node_ptr(ni);
    }
    xas.node = node;
    shift as i64
}

/* returns the entry found at the created slot (or NULL) */

pub fn xas_create(&mut self, xas: &mut Xas, allow_root: bool) -> u64 {
    let order = xas.shift as u32;
    let mut entry: u64;
    let mut shift: i64;
    /* slot location: None = &xa_head, Some((ni, off)) = node slot */
    let mut slot: Option<(usize, u64)>;

    if xas_top(xas.node) {
        entry = self.xa_head;
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
        entry = self.xa_head;
        slot = None;
    } else if xas_error(xas) != 0 {
        return 0;
    } else if xas.node != 0 {
        let ni = nidx_ptr(xas.node);
        let offset = xas.offset;
        shift = self.nodes[ni].shift as i64;
        entry = self.nodes[ni].slots[offset as usize];
        slot = Some((ni, offset as u64));
    } else {
        shift = 0;
        entry = self.xa_head;
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
                        node_mark_all(&mut self.nodes[ni], XA_FREE_MARK as usize);
                    }
                    match slot {
                        None => self.xa_head = xa_mk_node(ni),
                        Some((sni, soff)) => self.nodes[sni].slots[soff as usize] = xa_mk_node(ni),
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


pub fn xas_create_range(&mut self, xas: &mut Xas) {
    let index = xas.index;
    let shift = xas.shift;
    let sibs = xas.sibs;

    xas.index |= ((sibs as u64 + 1) << shift) - 1;
    if xas_is_node(xas) && self.is_node_ptr(xas.node) && self.nodes[nidx_ptr(xas.node)].shift == xas.shift {
        xas.offset |= sibs;
    }
    xas.shift = 0;
    xas.sibs = 0;

    let mut ok = false;
    loop {
        self.xas_create(xas, true);
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
            if !self.is_node_ptr(node) {
                break;
            }
            let ni = nidx_ptr(node);
            if self.nodes[ni].shift >= shift {
                break;
            }
            xas.node = self.nodes[ni].parent;
            xas.offset = self.nodes[ni].offset.wrapping_sub(1);
            if self.nodes[ni].offset != 0 {
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
        if xas.node != 0 && self.is_node_ptr(xas.node) {
            self.xas_set_offset(xas);
        }
    }
}


pub fn update_node(&mut self, xas: &mut Xas, node: u64, count: i64, values: i64) {
    if node == 0 || (count == 0 && values == 0) {
        return;
    }
    if !self.is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    self.nodes[ni].count = self.nodes[ni].count.wrapping_add(count as u8);
    self.nodes[ni].nr_values = self.nodes[ni].nr_values.wrapping_add(values as u8);
    self.xas_update(xas, ni);
    if count < 0 {
        self.xas_delete_node(xas);
    }
}

/* ================= xas_store ================= */


pub fn xas_store(&mut self, xas: &mut Xas, mut entry: u64) -> u64 {
    /* A caller-visible entry must never encode a node pointer: storing one
     * grafts a (possibly live) arena node into the tree. C corrupts
     * silently; WARN to make the contract violation observable. */
    self.warn_on(entry != 0 && xa_is_node(entry));
    let mut count: i64 = 0;
    let mut values: i64 = 0;
    let value = xa_is_value(entry);
    let mut first: u64;

    if entry != 0 {
        let allow_root = !xa_is_node(entry) && !xa_is_zero(entry);
        first = self.xas_create(xas, allow_root);
    } else {
        first = self.xas_load(xas);
    }

    if xas_invalid(xas) {
        return first;
    }
    let node = xas.node;
    if self.is_node_ptr(node) && (xas.shift < self.nodes[nidx_ptr(node)].shift) {
        xas.sibs = 0;
    }
    if first == entry && xas.sibs == 0 {
        return first;
    }

    let mut next = first;
    let mut offset = xas.offset as u32;
    let max = xas.offset as u32 + xas.sibs as u32;
    let ni_opt: Option<usize> = if self.is_node_ptr(node) {
        Some(nidx_ptr(node))
    } else {
        None
    };
    if ni_opt.is_some() && xas.sibs != 0 {
        self.xas_squash_marks(xas);
    }
    if entry == 0 {
        self.xas_init_marks(xas);
    }

    loop {
        match ni_opt {
            None => self.xa_head = entry,
            Some(ni) => self.nodes[ni].slots[offset as usize] = entry,
        }
        if xa_is_node(next) && (ni_opt.is_none() || self.nodes[ni_opt.unwrap()].shift != 0) {
            self.xas_free_nodes(xas, xa_to_node(next));
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
        next = self.nodes[ni].slots[offset as usize];
        if !xa_is_sibling(next) {
            if entry == 0 && offset > max {
                break;
            }
            first = next;
        }
    }

    self.update_node(xas, node, count, values);
    first
}

/* ================= find / iterate ================= */


pub fn __xas_prev(&mut self, xas: &mut Xas) -> u64 {
    if !xas_frozen(xas.node) {
        xas.index = xas.index.wrapping_sub(1);
    }
    if xas.node == 0 {
        return set_bounds(xas);
    }
    if xas_not_node(xas.node) {
        return self.xas_load(xas);
    }
    if xas.offset as u64 != get_offset(xas.index, &self.nodes[nidx_ptr(xas.node)]) {
        xas.offset = xas.offset.wrapping_sub(1);
    }
    while xas.offset >= 255 {
        xas.offset = self.nodes[nidx_ptr(xas.node)].offset.wrapping_sub(1);
        xas.node = self.nodes[nidx_ptr(xas.node)].parent;
        if xas.node == 0 {
            return set_bounds(xas);
        }
    }
    loop {
        let ni = nidx_ptr(xas.node);
        let entry = self.nodes[ni].slots[xas.offset as usize];
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        self.xas_set_offset(xas);
    }
}


pub fn xas_prev(&mut self, xas: &mut Xas) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (self.is_node_ptr(node) && self.nodes[nidx_ptr(node)].shift != 0)
        || xas.offset == 0
    {
        return self.__xas_prev(xas);
    }
    xas.index = xas.index.wrapping_sub(1);
    xas.offset = xas.offset.wrapping_sub(1);
    self.nodes[nidx_ptr(node)].slots[xas.offset as usize]
}


pub fn __xas_next(&mut self, xas: &mut Xas) -> u64 {
    if !xas_frozen(xas.node) {
        xas.index = xas.index.wrapping_add(1);
    }
    if xas.node == 0 {
        return set_bounds(xas);
    }
    if xas_not_node(xas.node) {
        return self.xas_load(xas);
    }
    if xas.offset as u64 != get_offset(xas.index, &self.nodes[nidx_ptr(xas.node)]) {
        xas.offset = xas.offset.wrapping_add(1);
    }
    while xas.offset as u64 >= CHUNK_SIZE as u64 {
        xas.offset = self.nodes[nidx_ptr(xas.node)].offset.wrapping_add(1);
        xas.node = self.nodes[nidx_ptr(xas.node)].parent;
        if xas.node == 0 {
            return set_bounds(xas);
        }
    }
    loop {
        let ni = nidx_ptr(xas.node);
        let entry = self.nodes[ni].slots[xas.offset as usize];
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        self.xas_set_offset(xas);
    }
}


pub fn xas_next(&mut self, xas: &mut Xas) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (self.is_node_ptr(node) && self.nodes[nidx_ptr(node)].shift != 0)
        || xas.offset == CHUNK_MASK as u8
    {
        return self.__xas_next(xas);
    }
    xas.index = xas.index.wrapping_add(1);
    xas.offset = xas.offset.wrapping_add(1);
    self.nodes[nidx_ptr(node)].slots[xas.offset as usize]
}


pub fn xas_find(&mut self, xas: &mut Xas, max: u64) -> u64 {
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
        && self.nodes[nidx_ptr(xas.node)].shift == 0
        && xas.offset as u64 != (xas.index & CHUNK_MASK)
    {
        xas.offset = (((xas.index.wrapping_sub(1)) & CHUNK_MASK) + 1) as u8;
    }

    self.xas_next_offset(xas);

    while self.is_node_ptr(xas.node) && xas.index <= max {
        if xas.offset as u64 >= CHUNK_SIZE as u64 {
            xas.offset = self.nodes[nidx_ptr(xas.node)].offset.wrapping_add(1);
            xas.node = self.nodes[nidx_ptr(xas.node)].parent;
            continue;
        }
        let ni = nidx_ptr(xas.node);
        entry = self.nodes[ni].slots[xas.offset as usize];
        if xa_is_node(entry) {
            xas.node = node_ptr(xa_to_node(entry));
            xas.offset = 0;
            continue;
        }
        if entry != 0 && !xa_is_sibling(entry) {
            return entry;
        }
        self.xas_next_offset(xas);
    }

    if xas.node == 0 {
        xas.node = XAS_BOUNDS;
    }
    0
}


pub fn xas_find_marked(&mut self, xas: &mut Xas, max: u64, mark: usize) -> u64 {
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
        return self.find_marked_out(xas, max);
    } else if xas_top(xas.node) {
        advance = false;
        let entry = self.xa_head;
        xas.node = 0;
        if xas.index > self.max_index(entry) {
            return self.find_marked_out(xas, max);
        }
        if !xa_is_node(entry) {
            if self.xa_marked(mark as u32) {
                return entry;
            }
            xas.index = 1;
            return self.find_marked_out(xas, max);
        }
        xas.node = node_ptr(xa_to_node(entry));
        xas.offset = (xas.index >> self.nodes[nidx_ptr(xas.node)].shift) as u8;
    }

    while xas.index <= max {
        if xas.offset as u64 >= CHUNK_SIZE as u64 {
            let ni = nidx_ptr(xas.node);
            xas.offset = self.nodes[ni].offset.wrapping_add(1);
            xas.node = self.nodes[ni].parent;
            if xas.node == 0 {
                break;
            }
            advance = false;
            continue;
        }
        if !advance {
            let ni = nidx_ptr(xas.node);
            let entry = self.nodes[ni].slots[xas.offset as usize];
            if xa_is_sibling(entry) {
                xas.offset = xa_to_sibling(entry) as u8;
                self.xas_move_index(xas, xas.offset as u64);
            }
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
            if offset >= CHUNK_SIZE as u64 {
                continue;
            }
        }
        let ni = nidx_ptr(xas.node);
        let entry = self.nodes[ni].slots[xas.offset as usize];
        if entry == 0 && !(self.xa_track_free() && mark as u32 == XA_FREE_MARK) {
            continue;
        }
        if xa_is_sibling(entry) {
            continue;
        }
        if !xa_is_node(entry) {
            return entry;
        }
        xas.node = node_ptr(xa_to_node(entry));
        self.xas_set_offset(xas);
    }

    self.find_marked_out(xas, max)
}


pub fn find_marked_out(&mut self, xas: &mut Xas, max: u64) -> u64 {
    let _ = self;
    if xas.index > max {
        xas.node = XAS_RESTART;
        return 0;
    }
    set_bounds(xas)
}


pub fn xas_find_conflict(&mut self, xas: &mut Xas) -> u64 {
    if xas_error(xas) != 0 {
        return 0;
    }
    if xas.node == 0 {
        return 0;
    }
    if xas_top(xas.node) {
        let mut curr = self.xas_start(xas);
        if curr == 0 {
            return 0;
        }
        while xa_is_node(curr) {
            let ni = xa_to_node(curr);
            curr = self.xas_descend(xas, ni);
        }
        if curr != 0 {
            return curr;
        }
    }
    if !self.is_node_ptr(xas.node) {
        return 0;
    }
    if self.nodes[nidx_ptr(xas.node)].shift > xas.shift {
        return 0;
    }
    loop {
        let ni = nidx_ptr(xas.node);
        if self.nodes[ni].shift == xas.shift {
            if (xas.offset & xas.sibs) == xas.sibs {
                break;
            }
        } else if xas.offset >= CHUNK_MASK as u8 {
            xas.offset = self.nodes[ni].offset;
            xas.node = self.nodes[ni].parent;
            if !self.is_node_ptr(xas.node) {
                break;
            }
            continue;
        }
        xas.offset = xas.offset.wrapping_add(1);
        let ni = nidx_ptr(xas.node);
        let mut curr = self.nodes[ni].slots[xas.offset as usize];
        if xa_is_sibling(curr) {
            continue;
        }
        while xa_is_node(curr) {
            let cni = xa_to_node(curr);
            xas.node = node_ptr(cni);
            xas.offset = 0;
            curr = self.nodes[cni].slots[0];
        }
        if curr != 0 {
            return curr;
        }
    }
    xas.offset = xas.offset.wrapping_sub(xas.sibs);
    0
}


pub fn xas_next_entry(&mut self, xas: &mut Xas, max: u64) -> u64 {
    let node = xas.node;
    if xas_not_node(node)
        || (self.is_node_ptr(node) && self.nodes[nidx_ptr(node)].shift != 0)
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
        let entry = self.nodes[ni].slots[xas.offset as usize + 1];
        if xa_is_internal(entry) {
            return self.xas_find(xas, max);
        }
        xas.offset += 1;
        xas.index += 1;
        if entry != 0 {
            return entry;
        }
    }
}


pub fn xas_next_marked(&mut self, xas: &mut Xas, max: u64, mark: usize) -> u64 {
    let node = xas.node;
    if xas_not_node(node) || (self.is_node_ptr(node) && self.nodes[nidx_ptr(node)].shift != 0) {
        return self.xas_find_marked(xas, max, mark);
    }
    let offset = self.xas_find_chunk(xas, true, mark);
    xas.offset = offset as u8;
    xas.index = (xas.index & !CHUNK_MASK).wrapping_add(offset);
    if xas.index > max {
        return 0;
    }
    if offset >= CHUNK_SIZE as u64 {
        return self.xas_find_marked(xas, max, mark);
    }
    let entry = self.nodes[nidx_ptr(node)].slots[offset as usize];
    if entry == 0 {
        return self.xas_find_marked(xas, max, mark);
    }
    entry
}

/* ================= split ================= */


pub fn __xas_init_node_for_split(&mut self, xas: &Xas, node: u64, entry: u64) {
    if node == 0 {
        return;
    }
    let ni = nidx_ptr(node);
    let mask = xas.sibs as u64;
    let mut sibling = 0u64;
    self.nodes[ni].array = XA_CANON;
    for i in 0..CHUNK_SIZE as u64 {
        if (i & mask) == 0 {
            self.nodes[ni].slots[i as usize] = entry;
            sibling = xa_mk_sibling(i);
        } else {
            self.nodes[ni].slots[i as usize] = sibling;
        }
    }
}


pub fn xas_split_alloc(&mut self, xas: &mut Xas, entry: u64, order: u32) {
    let mut sibs = (1u32 << (order % CHUNK_SHIFT)) - 1;
    if self.warn_on(xas.shift as u32 + 2 * CHUNK_SHIFT <= order) {
        /* goto nomem */
        self.xas_destroy(xas);
        xas_set_err(xas, -ENOMEM);
        return;
    }
    if xas.shift as u32 + CHUNK_SHIFT > order {
        return;
    }
    loop {
        match self.kmem_cache_alloc_lru() {
            None => {
                self.xas_destroy(xas);
                xas_set_err(xas, -ENOMEM);
                return;
            }
            Some(ni) => {
                self.__xas_init_node_for_split(xas, node_ptr(ni), entry);
                self.nodes[ni].parent = xas.alloc;
                xas.alloc = node_ptr(ni);
            }
        }
        if sibs == 0 {
            break;
        }
        sibs -= 1;
    }
}


pub fn xas_split(&mut self, xas: &mut Xas, entry: u64, order: u32) {
    let sibs = (1u64 << (order % CHUNK_SHIFT)) - 1;
    let mut values: i64 = 0;
    let mut curr = self.xas_load(xas);

    let node = xas.node;
    if xas_top(node) || !self.is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    let marks = node_get_marks(&self.nodes[ni], xas.offset as u64);

    let mut offset = xas.offset as u32 + sibs as u32;
    loop {
        if (xas.shift as u8) < self.nodes[ni].shift {
            let child = xas.alloc;
            /* contract: xas_split_alloc must prealloc; C would NULL-deref */
            if self.warn_on(child == 0) {
                return;
            }
            let cni = nidx_ptr(child);
            xas.alloc = self.nodes[cni].parent;
            self.nodes[cni].shift = self.nodes[ni].shift - CHUNK_SHIFT as u8;
            self.nodes[cni].offset = offset as u8;
            self.nodes[cni].count = CHUNK_SIZE as u8;
            self.nodes[cni].nr_values = if xa_is_value(entry) {
                CHUNK_SIZE as u8
            } else {
                0
            };
            self.nodes[cni].parent = node;
            {
                let marks_c = marks;
                let sibs_c = xas.sibs as u64;
                /* node_set_marks(node, offset, child, sibs, marks) */
                for m in 0..MAX_MARKS {
                    if marks_c & (1 << m) != 0 {
                        node_set_mark(&mut self.nodes[ni], offset as u64, m);
                        node_mark_slots(&mut self.nodes[cni], sibs_c, m);
                    }
                }
            }
            self.nodes[ni].slots[offset as usize] = xa_mk_node(cni);
            if xa_is_value(curr) {
                values -= 1;
            }
            self.xas_update(xas, cni);
        } else {
            let canon = offset - xas.sibs as u32;
            /* node_set_marks(node, canon, NULL, 0, marks) */
            let marks_c = marks;
            for m in 0..MAX_MARKS {
                if marks_c & (1 << m) != 0 {
                    node_set_mark(&mut self.nodes[ni], canon as u64, m);
                }
            }
            self.nodes[ni].slots[canon as usize] = entry;
            while offset > canon {
                self.nodes[ni].slots[offset as usize] = xa_mk_sibling(canon as u64);
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
    self.nodes[ni].nr_values = self.nodes[ni].nr_values.wrapping_add(values as u8);
    self.xas_update(xas, ni);
    let _ = &mut curr;
}


pub fn xas_try_split(&mut self, xas: &mut Xas, entry: u64, order: u32) {
    let sibs = (1u64 << (order % CHUNK_SHIFT)) - 1;
    let mut values: i64 = 0;
    let curr = self.xas_load(xas);

    let node = xas.node;
    if xas_top(node) || !self.is_node_ptr(node) {
        return;
    }
    let ni = nidx_ptr(node);
    let marks = node_get_marks(&self.nodes[ni], xas.offset as u64);
    let mut offset = xas.offset as u32 + sibs as u32;

    if xas.shift < self.nodes[ni].shift {
        let mut child = xas.alloc;
        let expected_sibs = (1u64 << (order.wrapping_sub(1) % CHUNK_SHIFT)) - 1;
        if sibs != 0 || xas.sibs as u64 != expected_sibs {
            self.xas_destroy(xas);
            xas_set_err(xas, -EINVAL);
            return;
        }
        if child == 0 {
            match self.kmem_cache_alloc_lru() {
                None => {
                    self.xas_destroy(xas);
                    xas_set_err(xas, -ENOMEM);
                    return;
                }
                Some(i) => {
                    child = node_ptr(i);
                    self.nodes[i].parent = xas.alloc;
                }
            }
        }
        self.__xas_init_node_for_split(xas, child, entry);
        let cni = nidx_ptr(child);
        xas.alloc = self.nodes[cni].parent;
        self.nodes[cni].shift = self.nodes[ni].shift - CHUNK_SHIFT as u8;
        self.nodes[cni].offset = offset as u8;
        self.nodes[cni].count = CHUNK_SIZE as u8;
        self.nodes[cni].nr_values = if xa_is_value(entry) {
            CHUNK_SIZE as u8
        } else {
            0
        };
        self.nodes[cni].parent = node;
        {
            let sibs_c = xas.sibs as u64;
            for m in 0..MAX_MARKS {
                if marks & (1 << m) != 0 {
                    node_set_mark(&mut self.nodes[ni], offset as u64, m);
                    node_mark_slots(&mut self.nodes[cni], sibs_c, m);
                }
            }
        }
        self.nodes[ni].slots[offset as usize] = xa_mk_node(cni);
        if xa_is_value(curr) {
            values -= 1;
        }
        self.xas_update(xas, cni);
    } else {
        loop {
            let canon = offset - xas.sibs as u32;
            for m in 0..MAX_MARKS {
                if marks & (1 << m) != 0 {
                    node_set_mark(&mut self.nodes[ni], canon as u64, m);
                }
            }
            self.nodes[ni].slots[canon as usize] = entry;
            while offset > canon {
                self.nodes[ni].slots[offset as usize] = xa_mk_sibling(canon as u64);
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
    self.nodes[ni].nr_values = self.nodes[ni].nr_values.wrapping_add(values as u8);
    self.xas_update(xas, ni);
}


pub fn xas_get_order(&self, xas: &Xas) -> i64 {
    let mut order = 0i64;
    if xas.node == 0 || !self.is_node_ptr(xas.node) {
        return 0;
    }
    let ni = nidx_ptr(xas.node);
    loop {
        let slot = xas.offset as u64 + (1 << order);
        if slot >= CHUNK_SIZE as u64 {
            break;
        }
        if !xa_is_sibling(self.nodes[ni].slots[slot as usize]) {
            break;
        }
        order += 1;
    }
    order + self.nodes[ni].shift as i64
}

/* ================= public xa_* API ================= */


pub fn xa_load(&mut self, index: u64) -> u64 {
    let mut xas = Xas::new(index);
    let mut entry;
    /* Retry entries are transient under RCU upstream; in this sequential
     * model a sibling pointing at a node-child master can persist, so the
     * loop is bounded exactly like xa_find/xa_find_after. */
    let mut tries = 0u32;
    loop {
        entry = xa_zero_to_null(self.xas_load(&mut xas));
        tries += 1;
        if !xas_retry(&mut xas, entry) || tries >= 256 {
            break;
        }
    }
    entry
}


pub fn __xa_erase(&mut self, index: u64) -> u64 {
    let mut xas = Xas::new(index);
    let cur = self.xas_store(&mut xas, 0);
    xas_result(&xas, xa_zero_to_null(cur))
}


pub fn xa_erase(&mut self, index: u64) -> u64 {
    self.__xa_erase(index)
}


pub fn __xa_store(&mut self, index: u64, mut entry: u64) -> u64 {
    let mut xas = Xas::new(index);
    if self.warn_on_once(0, xa_is_advanced(entry)) {
        return xa_error(-EINVAL);
    }
    if self.xa_track_free() && entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    let mut curr;
    loop {
        curr = self.xas_store(&mut xas, entry);
        if self.xa_track_free() {
            self.xas_clear_mark(&xas, XA_FREE_MARK as usize);
        }
        if !self.__xas_nomem(&mut xas) {
            break;
        }
    }
    xas_result(&xas, xa_zero_to_null(curr))
}


pub fn xa_store(&mut self, index: u64, entry: u64) -> u64 {
    self.__xa_store(index, entry)
}


pub fn __xa_cmpxchg_raw(&mut self, index: u64, old: u64, entry: u64) -> u64 {
    let mut xas = Xas::new(index);
    if self.warn_on_once(1, xa_is_advanced(entry)) {
        return xa_error(-EINVAL);
    }
    let mut curr;
    loop {
        curr = self.xas_load(&mut xas);
        if curr == old {
            self.xas_store(&mut xas, entry);
            if self.xa_track_free() && entry != 0 && curr == 0 {
                self.xas_clear_mark(&xas, XA_FREE_MARK as usize);
            }
        }
        if !self.__xas_nomem(&mut xas) {
            break;
        }
    }
    xas_result(&xas, curr)
}


pub fn __xa_cmpxchg(&mut self, index: u64, old: u64, entry: u64) -> u64 {
    xa_zero_to_null(self.__xa_cmpxchg_raw(index, old, entry))
}


pub fn xa_cmpxchg(&mut self, index: u64, old: u64, entry: u64) -> u64 {
    self.__xa_cmpxchg(index, old, entry)
}


pub fn __xa_insert(&mut self, index: u64, mut entry: u64) -> i64 {
    if entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    let curr = self.__xa_cmpxchg_raw(index, 0, entry);
    let errno = xa_err(curr);
    if errno != 0 {
        return errno;
    }
    if curr != 0 { -EBUSY } else { 0 }
}


pub fn xa_insert(&mut self, index: u64, entry: u64) -> i64 {
    self.__xa_insert(index, entry)
}


pub fn xa_reserve(&mut self, index: u64) -> i64 {
    xa_err(self.xa_cmpxchg(index, 0, XA_ZERO_ENTRY))
}


pub fn xa_release(&mut self, index: u64) {
    let _ = self.xa_cmpxchg(index, XA_ZERO_ENTRY, 0);
}


pub fn xa_store_range(&mut self, mut first: u64, last: u64, entry: u64) -> u64 {
    let mut xas = Xas::new(0);
    if self.warn_on_once(4, xa_is_internal(entry)) {
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
            self.xas_create(&mut xas, true);
            if xas_error(&xas) != 0 {
                unlock = true;
            }
        }
        if !unlock {
            loop {
                xas_set_range(&mut xas, first, last);
                self.xas_store(&mut xas, entry);
                if xas_error(&xas) != 0 {
                    break;
                }
                /* [0, u64::MAX] produces shift 60/sibs 15: xas_size wraps to
                 * 0 and `first` never advances (upstream hangs identically);
                 * break instead of livelocking. */
                let sz = xas_size(&xas);
                if sz == 0 {
                    break;
                }
                first = first.wrapping_add(sz);
                if first > last {
                    break;
                }
            }
        }
        if !self.xas_nomem(&mut xas) {
            break;
        }
    }
    xas_result(&xas, 0)
}


pub fn xa_get_order(&mut self, index: u64) -> i64 {
    let mut xas = Xas::new(index);
    let entry = self.xas_load(&mut xas);
    if entry != 0 {
        self.xas_get_order(&xas)
    } else {
        0
    }
}


pub fn __xa_alloc(&mut self, id: &mut u32, mut entry: u64, min: u32, max: u32) -> i64 {
    let mut xas = Xas::new(0);
    if self.warn_on_once(2, xa_is_advanced(entry)) {
        return -EINVAL;
    }
    if self.warn_on_once(3, !self.xa_track_free()) {
        return -EINVAL;
    }
    if entry == 0 {
        entry = XA_ZERO_ENTRY;
    }
    loop {
        xas.index = min as u64;
        self.xas_find_marked(&mut xas, max as u64, XA_FREE_MARK as usize);
        if xas.node == XAS_RESTART {
            xas_set_err(&mut xas, -EBUSY);
        } else {
            *id = xas.index as u32;
        }
        self.xas_store(&mut xas, entry);
        self.xas_clear_mark(&xas, XA_FREE_MARK as usize);
        if !self.__xas_nomem(&mut xas) {
            break;
        }
    }
    xas_error(&xas)
}


pub fn xa_alloc(&mut self, id: &mut u32, entry: u64, min: u32, max: u32) -> i64 {
    self.__xa_alloc(id, entry, min, max)
}


pub fn __xa_alloc_cyclic(
    &mut self,
    id: &mut u32,
    entry: u64,
    min: u32,
    max: u32,
    next: &mut u32,
) -> i64 {
    let m = min;
    let lmin = m.max(*next);
    let mut ret = self.__xa_alloc(id, entry, lmin, max);
    if (self.xa_flags & XA_FLAGS_ALLOC_WRAPPED) != 0 && ret == 0 {
        self.xa_flags &= !XA_FLAGS_ALLOC_WRAPPED;
        ret = 1;
    }
    if ret < 0 && lmin > m {
        ret = self.__xa_alloc(id, entry, m, max);
        if ret == 0 {
            ret = 1;
        }
    }
    if ret >= 0 {
        *next = id.wrapping_add(1);
        if *next == 0 {
            self.xa_flags |= XA_FLAGS_ALLOC_WRAPPED;
        }
    }
    ret
}


pub fn xa_alloc_cyclic(
    &mut self,
    id: &mut u32,
    entry: u64,
    min: u32,
    max: u32,
    next: &mut u32,
) -> i64 {
    /* the locked variants hide the "wrapped" indicator:
     * upstream returns `err < 0 ? err : 0` */
    let err = self.__xa_alloc_cyclic(id, entry, min, max, next);
    if err < 0 { err } else { 0 }
}


pub fn __xa_set_mark(&mut self, index: u64, mark: usize) {
    let mut xas = Xas::new(index);
    let entry = self.xas_load(&mut xas);
    if entry != 0 {
        self.xas_set_mark(&xas, mark);
    }
}


pub fn __xa_clear_mark(&mut self, index: u64, mark: usize) {
    let mut xas = Xas::new(index);
    let entry = self.xas_load(&mut xas);
    if entry != 0 {
        self.xas_clear_mark(&xas, mark);
    }
}


pub fn xa_set_mark(&mut self, index: u64, mark: usize) {
    self.__xa_set_mark(index, mark);
}


pub fn xa_clear_mark(&mut self, index: u64, mark: usize) {
    self.__xa_clear_mark(index, mark);
}


pub fn xa_get_mark(&mut self, index: u64, mark: usize) -> bool {
    let mut xas = Xas::new(index);
    let mut entry = self.xas_start(&mut xas);
    while self.xas_get_mark(&xas, mark) {
        if !xa_is_node(entry) {
            return true;
        }
        let ni = xa_to_node(entry);
        entry = self.xas_descend(&mut xas, ni);
    }
    false
}

pub fn xas_sibling_check(&self, xas: &Xas) -> bool {
    if xas.node == 0 || !self.is_node_ptr(xas.node) {
        return false;
    }
    let ni = nidx_ptr(xas.node);
    let mask = ((CHUNK_SIZE as u64) << self.nodes[ni].shift).wrapping_sub(1);
    (xas.index & mask) > ((xas.offset as u64) << self.nodes[ni].shift)
}
pub fn xas_extract_present(&mut self, xas: &mut Xas, dst: &mut [u64; 8], max: u64, n: u32) -> u32 {
    let n = n.min(dst.len() as u32);
    if n == 0 {
        return 0;
    }
    let mut i = 0u32;
    let mut tries = 0u32;
    let mut entry = self.xas_find(xas, max);
    while entry != 0 {
        if xas_retry(xas, entry) {
            tries += 1;
            if tries >= 256 {
                break;
            }
            entry = self.xas_next_entry(xas, max);
            continue;
        }
        dst[i as usize] = entry;
        i += 1;
        if i == n {
            break;
        }
        entry = self.xas_next_entry(xas, max);
    }
    i
}


pub fn xas_extract_marked(
    &mut self,
    xas: &mut Xas,
    dst: &mut [u64; 8],
    max: u64,
    n: u32,
    mark: usize,
) -> u32 {
    let n = n.min(dst.len() as u32);
    if n == 0 {
        return 0;
    }
    let mut i = 0u32;
    let mut tries = 0u32;
    let mut entry = self.xas_find_marked(xas, max, mark);
    while entry != 0 {
        if xas_retry(xas, entry) {
            tries += 1;
            if tries >= 256 {
                break;
            }
            entry = self.xas_next_marked(xas, max, mark);
            continue;
        }
        dst[i as usize] = entry;
        i += 1;
        if i == n {
            break;
        }
        entry = self.xas_next_marked(xas, max, mark);
    }
    i
}


pub fn xa_extract(&mut self, dst: &mut [u64; 8], start: u64, max: u64, n: u32, filter: u32) -> u32 {
    let mut xas = Xas::new(start);
    if n == 0 {
        return 0;
    }
    if filter < MAX_MARKS as u32 {
        self.xas_extract_marked(&mut xas, dst, max, n, filter as usize)
    } else {
        self.xas_extract_present(&mut xas, dst, max, n)
    }
}


pub fn xa_destroy(&mut self) {
    let mut xas = Xas::new(0);
    xas.node = 0;
    let entry = self.xa_head;
    self.xa_head = 0;
    self.xas_init_marks(&xas);
    if self.xa_zero_busy() {
        self.xa_mark_clear(XA_FREE_MARK);
    }
    if xa_is_node(entry) {
        self.xas_free_nodes(&xas, xa_to_node(entry));
    }
}


pub fn xa_init_flags(&mut self, flags: u32) {
    self.xa_flags = flags;
    self.xa_head = 0;
}

/* ================= generators ================= */

pub fn xa_find(&mut self, index: &mut u64, max: u64, filter: u32) -> u64 {
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


pub fn xa_find_after(&mut self, index: &mut u64, max: u64, filter: u32) -> u64 {
    let mut xas = Xas::new(index.wrapping_add(1));
    let mut entry;
    if xas.index == 0 {
        return 0;
    }
    let mut tries = 0u32;
    loop {
        if filter < MAX_MARKS as u32 {
            entry = self.xas_find_marked(&mut xas, max, filter as usize);
        } else {
            entry = self.xas_find(&mut xas, max);
        }
        if xas_invalid(&xas) {
            break;
        }
        if self.xas_sibling_check(&xas) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("xarray_tests.inc");
}
