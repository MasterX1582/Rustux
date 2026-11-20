//! `klib::assoc_array` — Linux `lib/assoc_array.c` +
//! `include/linux/{assoc_array,assoc_array_priv}.h`, ported faithfully
//! and verified against the C implementation (1383/1383 recorded
//! differential cases, `ports/lib-assoc-array`). This is the associative
//! array for keyrings: an edit-script/state-machine over a tree of nodes
//! (fan-out 16), shortcuts and leaf pointers packed into tagged
//! `assoc_array_ptr` values (bit 0 = meta, bit 1 = shortcut).
//!
//! Storage model: the harness's fixed pools become caller-owned arenas —
//! `&mut [Node]` nodes, `&mut [Scell]` shortcuts (+`SCFLEX` words of
//! trailing `index_key`), `&mut [Edit]` edit scripts, and the object
//! store `okey`/`orefs` the ops table keys on. Every C pointer stays a
//! canonical `u64` exactly like the verified model (`LCANON`/`NCANON`/
//! `SCANON`/`EDCANON`/`PC_*`), including `struct assoc_array_ptr **` slot
//! addresses (`PC_ROOT`/`PC_NSLOT`/`PC_NBACK`/`PC_SNEXT`/`PC_SBACK`) and
//! `u8 *` parent-slot addresses (`PC_NPSLT`); `store_p`/`store8_p`
//! resolve them at apply-edit time like the C pointer writes.
//!
//! Ops table: `keyring_assoc_array_ops` semantics are baked in over the
//! object store — `get_object_key_chunk`/`compare_object`/`diff_objects`
//! read `okey[leaf_i]` and `free_object` drops one `orefs` ref.
//! `alloc_ctr` + `fail_mask` are the shared alloc ordinal stream and
//! deterministic failure injection (bit k fails the (k+1)-th alloc).
//!
//! Adaptations from the verified port:
//! * `NP`/`SP`/`NE`/`NO` are `nodes`/`shorts`/`edits`/object-store
//!   `.len()`; pool_alloc's lowest-free-slot-first policy is kept.
//! * Harness instrumentation (`fold`/`g_h`, `xs64`, `trace_on`, the
//!   `op_*` drivers, `dump_state`, `wrap_assoc_seq`) is not adopted.
//! * Arena access goes through `n`/`nm`, `s`/`sm`, `ed`/`edm`, `ok`,
//!   `orf` — a decoded pointer outside its arena maps to a scratch slot
//!   instead of C's wild dereference. `warn` counts capped loops.
//! * `AA_ITER_LIMIT`-style `__lim` caps bound every pointer-chasing loop
//!   (walk, iterate, destroy, collapse/count climbs, gc). Unreachable on
//!   a well-formed tree; a hit degrades to a bounded return + `warn`.
//! * Slot/level indexing clamps to the fixed arrays (`slots`,
//!   `index_key`) where a crafted `slot`/`skip_to_level`/`sc_level`
//!   could index out; `round_up_i32`/sc-level arithmetic wraps like C.
//! * `assoc_array_walk`/`assoc_array_find` take `&mut AssocArray`
//!   (scratch writes + `warn`), losing the C `const`.

#![forbid(unsafe_code)]

pub const KW: usize = 2;
pub const SCFLEX: usize = 4;

pub const LCANON: u64 = 0x10000;
pub const NCANON: u64 = 0x40000;
pub const SCANON: u64 = 0x200000;
pub const EDCANON: u64 = 0x600000;
pub const PC_ROOT: u64 = 0x810000;
pub const PC_NSLOT: u64 = 0xB10000;
pub const PC_NBACK: u64 = 0xB20000;
pub const PC_NPSLT: u64 = 0xB30000;
pub const PC_SNEXT: u64 = 0xB40000;
pub const PC_SBACK: u64 = 0xB50000;

pub const ENOMEM: i64 = 12;
pub const FAN_OUT: usize = 16;
pub const FAN_MASK: u64 = 15;
pub const LEVEL_STEP: i32 = 4;
pub const LEVEL_STEP_MASK: i32 = 3;
pub const KEY_CHUNK_SIZE: i32 = 64;
pub const KEY_CHUNK_MASK: i32 = 63;
pub const KEY_CHUNK_SHIFT: i32 = 6;

/// Bound for pointer-chasing loops (`MAS_ITER_LIMIT` convention).
fn iter_limit(aa: &AssocArray) -> u64 {
    4 * (aa.nodes.len() + aa.shorts.len()) as u64 + 256
}

// ---- tagged pointer helpers (assoc_array_priv.h inline layer) ---------------

pub fn is_meta(p: u64) -> bool {
    p & 1 == 1
}
pub fn is_leaf(p: u64) -> bool {
    !is_meta(p)
}
pub fn is_shortcut(p: u64) -> bool {
    p & 2 == 2
}
pub fn is_node(p: u64) -> bool {
    !is_shortcut(p)
}

/// `assoc_array_ptr_to_leaf`: strip the type tag; result is a leaf object
/// canonical (LCANON + i*16).
pub fn ptr_to_leaf(p: u64) -> u64 {
    p & !1
}
/// `assoc_array_ptr_to_node`: raw node index (unbounded; arena access
/// routes out-of-range indices to scratch).
pub fn node_i(p: u64) -> usize {
    ((p & !3).wrapping_sub(NCANON) / 0x1000) as usize
}
/// `assoc_array_ptr_to_shortcut`: raw shortcut index.
pub fn sc_i(p: u64) -> usize {
    ((p & !3).wrapping_sub(SCANON) / 0x800) as usize
}
/// raw leaf canonical -> object store index.
pub fn leaf_i(p: u64) -> usize {
    (ptr_to_leaf(p).wrapping_sub(LCANON) / 16) as usize
}
/// edit canonical -> edit-pool index.
pub fn edit_i(p: u64) -> usize {
    (p.wrapping_sub(EDCANON) / 0x800) as usize
}

pub fn leaf_to_ptr(i: usize) -> u64 {
    LCANON + i as u64 * 16
}
pub fn node_to_ptr(i: usize) -> u64 {
    NCANON + i as u64 * 0x1000 + 1
}
pub fn sc_to_ptr(j: usize) -> u64 {
    SCANON + j as u64 * 0x800 + 3
}
fn edit_canon(i: usize) -> u64 {
    EDCANON + i as u64 * 0x800
}

// ---- structures ------------------------------------------------------------

/// `index_key`: two-word (128-bit) keys.
pub type TKey = [u64; KW];

/// `struct assoc_array_node` + the pool allocator's `used` bit.
#[derive(Clone, Copy)]
pub struct Node {
    pub used: bool,
    pub back_pointer: u64, // tagged ptr
    pub parent_slot: u8,
    pub slots: [u64; FAN_OUT],
    pub nr_leaves_on_branch: u64,
}

/// `struct assoc_array_shortcut` + SCFLEX words of trailing index_key[].
#[derive(Clone, Copy)]
pub struct Scell {
    pub used: bool,
    pub back_pointer: u64, // tagged ptr
    pub parent_slot: i32,
    pub skip_to_level: i32,
    pub next_node: u64, // tagged ptr
    pub index_key: [u64; SCFLEX],
}

/// `struct assoc_array_edit`. `ops`/`array` are always set and modelled
/// away; `ops_for_excised_subtree` decides whether a destroyed excised
/// subtree frees its leaves.
#[derive(Clone, Copy)]
pub struct Edit {
    pub used: bool,
    pub ops_for_excised_subtree: bool,
    pub leaf: u64,
    pub leaf_p: u64, // slot address canonical, 0 = NULL
    pub dead_leaf: u64,
    pub new_meta: [u64; 3],
    pub excised_meta: [u64; 2],
    pub excised_subtree: u64,
    pub set_backpointers: [u64; FAN_OUT],
    pub set_backpointers_to: u64,
    pub adjust_count_on: u64, // raw node canonical, 0 = NULL
    pub adjust_count_by: i64,
    pub set_ptr: [u64; 2], // slot address canonicals
    pub set_to: [u64; 2],  // tagged ptrs
    pub sps_p: u64,        // parent_slot (u8*) address canonical
    pub sps_to: u8,
    pub segment_cache: [u8; FAN_OUT + 1],
}

/// `struct assoc_array_walk_result`.
#[derive(Clone, Copy, Default)]
struct WalkResult {
    status: u8, // 0 empty, 1 terminal_node, 2 wrong_shortcut
    tn_node: usize,
    tn_level: i32,
    tn_slot: i32,
    ws_shortcut: usize,
    ws_level: i32,
    ws_sc_level: i32,
    ws_sc_segments: u64,
    ws_dissimilarity: u64,
}

/// `struct assoc_array_delete_collapse_context`.
struct Collapse {
    node: usize,
    skip_leaf: u64, // raw leaf canonical
    slot: i32,
}

/// `struct assoc_array_gc_walk`.
struct GcWalk {
    node: usize,  // raw node index
    new_n: usize, // raw node index (usize::MAX = NULL)
    slot: i32,
    new_root: u64, // tagged ptr
    nr_leaves_on_tree: u64,
}

const NIL: usize = usize::MAX;

/// `struct assoc_array` + pools + ops object store + allocator hooks.
pub struct AssocArray<'a> {
    /// Node pool (`used` marks a live slot).
    pub nodes: &'a mut [Node],
    /// Shortcut pool.
    pub shorts: &'a mut [Scell],
    /// Edit-script pool (never live simultaneously).
    pub edits: &'a mut [Edit],
    /// Object index keys (`ops->get_object_key_chunk` material).
    pub okey: &'a mut [TKey],
    /// Object refcounts (`ops->free_object` drops one).
    pub orefs: &'a mut [i64],
    /// `array.root` — tagged ptr canonical.
    pub root: u64,
    /// `array.nr_leaves_on_tree`.
    pub nr_leaves_on_tree: u64,
    /// Shared alloc ordinal stream.
    pub alloc_ctr: u64,
    /// Bit k fails the (k+1)-th allocation.
    pub fail_mask: u64,
    /// Bounded-loop / divergence counter (modelled; observable only).
    pub warn: u64,
    /// Scratch slots for wild pointers — C would dereference garbage;
    /// the sequential model reads/writes these throwaways instead.
    scratch_n: Node,
    scratch_s: Scell,
    scratch_e: Edit,
    scratch_k: TKey,
    scratch_r: i64,
}

pub const NODE0: Node = Node {
    used: false,
    back_pointer: 0,
    parent_slot: 0,
    slots: [0; FAN_OUT],
    nr_leaves_on_branch: 0,
};
pub const SC0: Scell = Scell {
    used: false,
    back_pointer: 0,
    parent_slot: 0,
    skip_to_level: 0,
    next_node: 0,
    index_key: [0; SCFLEX],
};
pub const EDIT0: Edit = Edit {
    used: false,
    ops_for_excised_subtree: false,
    leaf: 0,
    leaf_p: 0,
    dead_leaf: 0,
    new_meta: [0; 3],
    excised_meta: [0; 2],
    excised_subtree: 0,
    set_backpointers: [0; FAN_OUT],
    set_backpointers_to: 0,
    adjust_count_on: 0,
    adjust_count_by: 0,
    set_ptr: [0; 2],
    set_to: [0; 2],
    sps_p: 0,
    sps_to: 0,
    segment_cache: [0; FAN_OUT + 1],
};

impl<'a> AssocArray<'a> {
    /// `struct assoc_array array = { .root = NULL }` over fresh pools.
    pub fn new(
        nodes: &'a mut [Node],
        shorts: &'a mut [Scell],
        edits: &'a mut [Edit],
        okey: &'a mut [TKey],
        orefs: &'a mut [i64],
    ) -> Self {
        Self {
            nodes,
            shorts,
            edits,
            okey,
            orefs,
            root: 0,
            nr_leaves_on_tree: 0,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            scratch_n: NODE0,
            scratch_s: SC0,
            scratch_e: EDIT0,
            scratch_k: [0; KW],
            scratch_r: 0,
        }
    }

    /* ---- arena access: out-of-arena index maps to scratch ---- */
    #[inline]
    fn n(&self, i: usize) -> &Node {
        self.nodes.get(i).unwrap_or(&self.scratch_n)
    }
    #[inline]
    fn nm(&mut self, i: usize) -> &mut Node {
        self.nodes.get_mut(i).unwrap_or(&mut self.scratch_n)
    }
    #[inline]
    fn s(&self, i: usize) -> &Scell {
        self.shorts.get(i).unwrap_or(&self.scratch_s)
    }
    #[inline]
    fn sm(&mut self, i: usize) -> &mut Scell {
        self.shorts.get_mut(i).unwrap_or(&mut self.scratch_s)
    }
    #[inline]
    fn ed(&self, i: usize) -> &Edit {
        self.edits.get(i).unwrap_or(&self.scratch_e)
    }
    #[inline]
    fn edm(&mut self, i: usize) -> &mut Edit {
        self.edits.get_mut(i).unwrap_or(&mut self.scratch_e)
    }
    #[inline]
    fn ok(&self, i: usize) -> &TKey {
        self.okey.get(i).unwrap_or(&self.scratch_k)
    }
    #[inline]
    fn orf(&mut self, i: usize) -> &mut i64 {
        self.orefs.get_mut(i).unwrap_or(&mut self.scratch_r)
    }
}

/// Slot read with clamp — the `slot < FAN_OUT` loop guards cover the
/// real range; a crafted negative/out-of-range `slot` reads the last
/// slot instead of panicking on a fixed-array index.
#[inline]
fn nslot(n: &Node, slot: i32) -> u64 {
    n.slots[(slot as usize).min(FAN_OUT - 1)]
}

fn bug_on(c: bool) {
    // The C contract makes these unreachable; a trip is a port bug.
    if c {
        panic!("BUG_ON tripped in the port — model divergence");
    }
}

/// IS_ERR_VALUE: v in the top 4095 addresses (small negative errno).
pub fn is_err(v: u64) -> bool {
    v >= (-4095i64) as u64
}

// ---- slot-address access -----------------------------------------------------

fn store_p(r: &mut AssocArray, a: u64, v: u64) {
    if a == PC_ROOT {
        r.root = v;
        return;
    }
    if (PC_NSLOT..PC_NSLOT + r.nodes.len() as u64 * 64).contains(&a) {
        let d = (a - PC_NSLOT) as usize;
        bug_on(d % 64 >= FAN_OUT);
        r.nm(d / 64).slots[d % 64] = v;
        return;
    }
    if (PC_NBACK..PC_NBACK + r.nodes.len() as u64).contains(&a) {
        r.nm((a - PC_NBACK) as usize).back_pointer = v;
        return;
    }
    if (PC_SNEXT..PC_SNEXT + r.shorts.len() as u64).contains(&a) {
        r.sm((a - PC_SNEXT) as usize).next_node = v;
        return;
    }
    if (PC_SBACK..PC_SBACK + r.shorts.len() as u64).contains(&a) {
        r.sm((a - PC_SBACK) as usize).back_pointer = v;
        return;
    }
    bug_on(true);
}

fn store8_p(r: &mut AssocArray, a: u64, v: u8) {
    if (PC_NPSLT..PC_NPSLT + r.nodes.len() as u64).contains(&a) {
        r.nm((a - PC_NPSLT) as usize).parent_slot = v;
        return;
    }
    bug_on(true);
}

// ---- deterministic allocator ------------------------------------------------

fn alloc_fail_check(r: &mut AssocArray) -> bool {
    r.alloc_ctr += 1;
    r.alloc_ctr <= 64 && (r.fail_mask >> (r.alloc_ctr - 1)) & 1 == 1
}

/// kzalloc(sizeof(struct assoc_array_node)): pool index or NIL.
fn kzalloc_node(r: &mut AssocArray) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    for i in 0..r.nodes.len() {
        if !r.n(i).used {
            *r.nm(i) = NODE0;
            r.nm(i).used = true;
            return i;
        }
    }
    NIL
}

/// kzalloc/kmalloc(struct_size(shortcut, index_key, keylen)).
fn alloc_sc(r: &mut AssocArray, keylen: usize) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    bug_on(keylen > SCFLEX);
    for j in 0..r.shorts.len() {
        if !r.s(j).used {
            *r.sm(j) = SC0;
            r.sm(j).used = true;
            return j;
        }
    }
    NIL
}

fn kzalloc_edit(r: &mut AssocArray) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    for k in 0..r.edits.len() {
        if !r.ed(k).used {
            *r.edm(k) = EDIT0;
            r.edm(k).used = true;
            return k;
        }
    }
    NIL
}

/// kfree() on a raw pool canonical (untagged node/shortcut/edit base).
fn kfree(r: &mut AssocArray, p: u64) {
    if p == 0 {
        return;
    }
    let np = r.nodes.len() as u64;
    let sp = r.shorts.len() as u64;
    let ne = r.edits.len() as u64;
    if p >= NCANON && p < NCANON + np * 0x1000 && (p - NCANON).is_multiple_of(0x1000) {
        let i = ((p - NCANON) / 0x1000) as usize;
        bug_on(!r.n(i).used);
        r.nm(i).used = false;
        return;
    }
    if p >= SCANON && p < SCANON + sp * 0x800 && (p - SCANON).is_multiple_of(0x800) {
        let j = ((p - SCANON) / 0x800) as usize;
        bug_on(!r.s(j).used);
        r.sm(j).used = false;
        return;
    }
    if p >= EDCANON && p < EDCANON + ne * 0x800 && (p - EDCANON).is_multiple_of(0x800) {
        let k = ((p - EDCANON) / 0x800) as usize;
        bug_on(!r.ed(k).used);
        r.edm(k).used = false;
        return;
    }
    bug_on(true);
}

// ---- the ops table (keyring_assoc_array_ops semantics) -----------------------

fn get_key_chunk(k: &TKey, level: i32) -> u64 {
    let w = level as u64 / KEY_CHUNK_SIZE as u64;
    if (w as usize) < KW { k[w as usize] } else { 0 }
}

fn get_object_key_chunk(r: &AssocArray, leaf: u64, level: i32) -> u64 {
    let w = level as u64 / KEY_CHUNK_SIZE as u64;
    if (w as usize) < KW {
        r.ok(leaf_i(leaf))[w as usize]
    } else {
        0
    }
}

fn compare_object(r: &AssocArray, leaf: u64, k: &TKey) -> bool {
    *r.ok(leaf_i(leaf)) == *k
}

fn diff_objects(r: &AssocArray, leaf: u64, k: &TKey) -> i64 {
    let o = r.ok(leaf_i(leaf));
    for i in 0..KW {
        let d = o[i] ^ k[i];
        if d != 0 {
            return i as i64 * 64 + d.trailing_zeros() as i64;
        }
    }
    -1
}

/// `ops->free_object`: drop the object ref (the key_put analogue).
fn free_object(r: &mut AssocArray, leaf: u64) {
    let i = leaf_i(leaf);
    *r.orf(i) = (*r.orf(i)).wrapping_sub(1);
}

// =================== lib/assoc_array.c ===================

/// assoc_array_subtree_iterate: iterative walk; `stop` is the tagged parent
/// pointer that ends the climb (NULL for a full iterate, the node's back
/// pointer for a subtree collapse).  `it` is invoked on each leaf's tagged
/// pointer.
fn subtree_iterate<C>(
    r: &mut AssocArray,
    root: u64,
    stop: u64,
    it: &mut dyn FnMut(&mut AssocArray, &mut C, u64) -> i32,
    ctx: &mut C,
) -> i32 {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    let mut cursor = root;
    let mut node;
    let mut slot: i32;

    'begin_node: loop {
        __lim += 1;
        if __lim > lim {
            r.warn += 1;
            return 0;
        }
        if is_shortcut(cursor) {
            cursor = r.s(sc_i(cursor)).next_node;
        }
        node = node_i(cursor);
        slot = 0;

        let mut has_meta = 0u64;
        while slot < FAN_OUT as i32 {
            let ptr = nslot(r.n(node), slot);
            has_meta |= ptr;
            if ptr != 0 && is_leaf(ptr) {
                let ret = it(r, ctx, ptr);
                if ret != 0 {
                    return ret;
                }
            }
            slot += 1;
        }

        if has_meta & 1 != 0 {
            slot = 0;
            node = node_i(cursor);
            while slot < FAN_OUT as i32 {
                let ptr = nslot(r.n(node), slot);
                if is_meta(ptr) {
                    cursor = ptr;
                    continue 'begin_node;
                }
                slot += 1;
            }
        }

        // finished_node
        loop {
            __lim += 1;
            if __lim > lim {
                r.warn += 1;
                return 0;
            }
            let mut parent = r.n(node).back_pointer;
            slot = r.n(node).parent_slot as i32;
            if parent == stop {
                return 0;
            }
            if is_shortcut(parent) {
                let s = sc_i(parent);
                parent = r.s(s).back_pointer;
                // clamp the crafted-able i32 field so the slot scan
                // stays a bounded 16-wide pass
                slot = r.s(s).parent_slot.clamp(-(FAN_OUT as i32), FAN_OUT as i32);
                if parent == stop {
                    return 0;
                }
            }
            cursor = parent;
            slot += 1;
            // continue_node
            node = node_i(cursor);
            while slot < FAN_OUT as i32 {
                let ptr = nslot(r.n(node), slot);
                if is_meta(ptr) {
                    cursor = ptr;
                    continue 'begin_node;
                }
                slot += 1;
            }
        }
    }
}

/// assoc_array_iterate.
pub fn assoc_array_iterate<C>(
    r: &mut AssocArray,
    it: &mut dyn FnMut(&mut AssocArray, &mut C, u64) -> i32,
    ctx: &mut C,
) -> i32 {
    if r.root == 0 {
        return 0;
    }
    subtree_iterate(r, r.root, 0, it, ctx)
}

fn assoc_array_walk(r: &mut AssocArray, index_key: &TKey, result: &mut WalkResult) -> u8 {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    let mut cursor = r.root;
    if cursor == 0 {
        return 0; // walk_tree_empty
    }

    let mut level: i32 = 0;
    let mut segments: u64;
    let mut sc_level: i32;

    'jumped: loop {
        segments = get_key_chunk(index_key, level);

        'consider: loop {
            __lim += 1;
            if __lim > lim {
                r.warn += 1;
                return 0;
            }
            if is_shortcut(cursor) {
                // follow_shortcut
                let shortcut = sc_i(cursor);
                sc_level = level.wrapping_add(LEVEL_STEP);
                bug_on(sc_level > r.s(shortcut).skip_to_level);
                loop {
                    __lim += 1;
                    if __lim > lim {
                        r.warn += 1;
                        return 0;
                    }
                    if sc_level & KEY_CHUNK_MASK == 0 {
                        segments = get_key_chunk(index_key, sc_level);
                    }
                    // index_key[] is SCFLEX words; a crafted skip_to_level
                    // would run past it — clamp like C's OOB flex read.
                    let sc_segments = r.s(shortcut).index_key
                        [(sc_level >> KEY_CHUNK_SHIFT).clamp(0, SCFLEX as i32 - 1) as usize];
                    let mut dissimilarity = segments ^ sc_segments;
                    let next_sc_level = if r.s(shortcut).skip_to_level
                        < round_down_i32(sc_level, KEY_CHUNK_SIZE).wrapping_add(KEY_CHUNK_SIZE)
                    {
                        let shift = (r.s(shortcut).skip_to_level & KEY_CHUNK_MASK) as u32;
                        dissimilarity &= !(u64::MAX << shift);
                        r.s(shortcut).skip_to_level
                    } else {
                        round_down_i32(
                            sc_level.wrapping_add(KEY_CHUNK_SIZE),
                            KEY_CHUNK_SIZE,
                        )
                    };
                    if dissimilarity != 0 {
                        result.ws_shortcut = shortcut;
                        result.ws_level = level;
                        result.ws_sc_level = sc_level;
                        result.ws_sc_segments = sc_segments;
                        result.ws_dissimilarity = dissimilarity;
                        result.status = 2;
                        return 2;
                    }
                    sc_level = next_sc_level;
                    if sc_level >= r.s(shortcut).skip_to_level {
                        break;
                    }
                }
                cursor = r.s(shortcut).next_node;
                if (level ^ sc_level) & !KEY_CHUNK_MASK != 0 {
                    level = sc_level;
                    continue 'jumped;
                }
                level = sc_level;
                continue 'consider; // goto consider_node
            }

            // consider_node
            let node = node_i(cursor);
            let mut slot = segments >> (level & KEY_CHUNK_MASK) as u32;
            slot &= FAN_MASK;
            let ptr = nslot(r.n(node), slot as i32);
            if !is_meta(ptr) {
                result.tn_node = node;
                result.tn_level = level;
                result.tn_slot = slot as i32;
                result.status = 1;
                return 1;
            }
            if is_node(ptr) {
                cursor = ptr;
                level = level.wrapping_add(LEVEL_STEP);
                if level & KEY_CHUNK_MASK != 0 {
                    continue 'consider;
                }
                continue 'jumped;
            }
            // ptr is a shortcut: follow it next
            cursor = ptr;
            continue 'consider;
        }
    }
}

fn round_down_i32(x: i32, y: i32) -> i32 {
    x & !(y - 1)
}
/// (((x)+(y)-1) & ~((y)-1)) with C's wrap on overflow.
fn round_up_i32(x: i32, y: i32) -> i32 {
    x.wrapping_add(y - 1) & !(y - 1)
}

/// assoc_array_find.
pub fn assoc_array_find(r: &mut AssocArray, index_key: &TKey) -> u64 {
    let mut result = WalkResult::default();
    if assoc_array_walk(r, index_key, &mut result) != 1 {
        return 0;
    }
    let node = result.tn_node;
    for slot in 0..FAN_OUT {
        let ptr = r.n(node).slots[slot];
        if ptr != 0 && is_leaf(ptr) && compare_object(r, ptr, index_key) {
            return ptr_to_leaf(ptr);
        }
    }
    0
}

/// assoc_array_destroy_subtree.  `free` is whether the ops table's
/// free_object runs on the leaves (the C `ops` parameter: real ops free,
/// NULL ops only reap metadata).
fn destroy_subtree(r: &mut AssocArray, root: u64, free: bool) {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    let mut cursor = root;
    if cursor == 0 {
        return;
    }
    let mut parent: u64 = 0;
    let mut slot: i32 = -1;

    'move_to_meta: loop {
        __lim += 1;
        if __lim > lim {
            r.warn += 1;
            return;
        }
        if is_shortcut(cursor) {
            let s = sc_i(cursor);
            bug_on(r.s(s).back_pointer != parent);
            bug_on(slot != -1 && r.s(s).parent_slot != slot);
            parent = cursor;
            cursor = r.s(s).next_node;
            slot = -1;
            bug_on(!is_node(cursor));
        }

        let mut node = node_i(cursor);
        bug_on(r.n(node).back_pointer != parent);
        bug_on(slot != -1 && r.n(node).parent_slot as i32 != slot);
        slot = 0;

        // continue_node / finished_node / ascend loop.  The C `for`
        // increments slot after the body: `goto move_to_meta` leaves it
        // on the slot the meta pointer was found in.
        loop {
            __lim += 1;
            if __lim > lim {
                r.warn += 1;
                return;
            }
            while slot < FAN_OUT as i32 {
                let ptr = nslot(r.n(node), slot);
                if ptr == 0 {
                    slot += 1;
                    continue;
                }
                if is_meta(ptr) {
                    parent = cursor;
                    cursor = ptr;
                    continue 'move_to_meta;
                }
                if free {
                    free_object(r, ptr_to_leaf(ptr));
                }
                slot += 1;
            }

            // finished_node
            parent = r.n(node).back_pointer;
            slot = r.n(node).parent_slot as i32;
            kfree(
                r,
                NCANON.wrapping_add((node as u64).wrapping_mul(0x1000)),
            );
            if parent == 0 {
                return;
            }
            if is_shortcut(parent) {
                let s = sc_i(parent);
                bug_on(r.s(s).next_node != cursor);
                parent = r.s(s).back_pointer;
                slot = r.s(s).parent_slot.clamp(-(FAN_OUT as i32), FAN_OUT as i32);
                kfree(r, SCANON.wrapping_add((s as u64).wrapping_mul(0x800)));
                if parent == 0 {
                    return;
                }
                bug_on(!is_node(parent));
            }
            cursor = parent;
            node = node_i(cursor);
            slot += 1;
        }
    }
}

/// assoc_array_destroy.
pub fn assoc_array_destroy(r: &mut AssocArray) {
    destroy_subtree(r, r.root, true);
    r.root = 0;
}

fn assoc_array_insert_in_empty_tree(r: &mut AssocArray, e: usize) -> bool {
    let new_n0 = kzalloc_node(r);
    if new_n0 == NIL {
        return false;
    }
    r.edm(e).new_meta[0] = node_to_ptr(new_n0);
    r.edm(e).leaf_p = PC_NSLOT + new_n0 as u64 * 64;
    r.edm(e).adjust_count_on = NCANON + new_n0 as u64 * 0x1000;
    r.edm(e).set_ptr[0] = PC_ROOT;
    r.edm(e).set_to[0] = node_to_ptr(new_n0);
    true
}

/// assoc_array_parent_link.
fn parent_link(back_pointer: u64, parent_slot: i32) -> u64 {
    if back_pointer == 0 {
        return PC_ROOT;
    }
    if is_node(back_pointer) {
        return PC_NSLOT + node_i(back_pointer) as u64 * 64 + parent_slot as u64;
    }
    PC_SNEXT + sc_i(back_pointer) as u64
}

fn assoc_array_insert_in_node(
    r: &mut AssocArray,
    e: usize,
    index_key: &TKey,
    node: usize,
) -> bool {
    let mut free_slot: i32 = -1;

    for i in 0..FAN_OUT {
        let ptr = r.n(node).slots[i];
        if ptr == 0 {
            free_slot = i as i32;
            continue;
        }
        if is_leaf(ptr) && compare_object(r, ptr, index_key) {
            r.edm(e).leaf_p = PC_NSLOT + node as u64 * 64 + i as u64;
            r.edm(e).dead_leaf = ptr;
            return true;
        }
    }

    if free_slot >= 0 {
        r.edm(e).leaf_p = PC_NSLOT + node as u64 * 64 + free_slot as u64;
        r.edm(e).adjust_count_on = NCANON + node as u64 * 0x1000;
        return true;
    }
    false
}

fn assoc_array_fill_segment_cache(r: &mut AssocArray, e: usize, node: usize, level: i32) -> bool {
    let mut have_meta = false;
    let mut ed = *r.ed(e);
    for (i, seg) in ed.segment_cache[..FAN_OUT].iter_mut().enumerate() {
        let ptr = r.n(node).slots[i];
        if is_meta(ptr) {
            *seg = 0xff;
            have_meta = true;
            continue;
        }
        let mut base_seg = get_object_key_chunk(r, ptr, level);
        base_seg >>= (level & KEY_CHUNK_MASK) as u32;
        *seg = (base_seg & FAN_MASK) as u8;
    }
    *r.edm(e) = ed;
    have_meta
}

fn assoc_array_all_one_segment(r: &AssocArray, e: usize) -> bool {
    for i in 1..FAN_OUT + 1 {
        if r.ed(e).segment_cache[i] != r.ed(e).segment_cache[0] {
            return false;
        }
    }
    true
}

fn assoc_array_split_node(r: &mut AssocArray, e: usize, node: usize, new_n0: usize, new_n1: usize) {
    r.nm(new_n1).back_pointer = node_to_ptr(new_n0);
    r.nm(new_n0).nr_leaves_on_branch = r.n(node).nr_leaves_on_branch;
    r.nm(new_n1).nr_leaves_on_branch = 0;

    // find two matching leaf segments
    let mut slot = 0xffi32;
    'found: {
        for i in 0..FAN_OUT {
            slot = r.ed(e).segment_cache[i] as i32;
            if slot != 0xff {
                for j in i + 1..FAN_OUT + 1 {
                    if r.ed(e).segment_cache[j] as i32 == slot {
                        break 'found;
                    }
                }
            }
        }
        // C falls through to the label with i == FAN_OUT
        bug_on(true);
    }
    bug_on(slot >= FAN_OUT as i32);

    r.nm(new_n1).parent_slot = slot as u8;

    for i in 0..FAN_OUT {
        if is_meta(r.n(node).slots[i]) {
            r.nm(new_n0).slots[i] = r.n(node).slots[i];
        } else {
            r.nm(new_n0).slots[i] = 0;
        }
    }
    bug_on(r.n(new_n0).slots[slot as usize] != 0);
    r.nm(new_n0).slots[slot as usize] = node_to_ptr(new_n1);

    let mut free_slot: i32 = -1;
    let mut next_slot = 0usize;
    for i in 0..FAN_OUT {
        if is_meta(r.n(node).slots[i]) {
            continue;
        }
        if r.ed(e).segment_cache[i] as i32 == slot {
            bug_on(next_slot >= FAN_OUT);
            r.nm(new_n1).slots[next_slot] = r.n(node).slots[i];
            next_slot += 1;
            r.nm(new_n1).nr_leaves_on_branch += 1;
        } else {
            // new_n0 has a free slot on every valid path; bound the scan
            // instead of running off the fixed array on crafted state.
            loop {
                free_slot += 1;
                if free_slot >= FAN_OUT as i32 || r.n(new_n0).slots[free_slot as usize] == 0 {
                    break;
                }
            }
            bug_on(free_slot >= FAN_OUT as i32);
            r.nm(new_n0).slots[free_slot as usize] = r.n(node).slots[i];
        }
    }

    if r.ed(e).segment_cache[FAN_OUT] as i32 != slot {
        loop {
            free_slot += 1;
            if free_slot >= FAN_OUT as i32 || r.n(new_n0).slots[free_slot as usize] == 0 {
                break;
            }
        }
        bug_on(free_slot >= FAN_OUT as i32);
        r.edm(e).leaf_p = PC_NSLOT + new_n0 as u64 * 64 + free_slot as u64;
        r.edm(e).adjust_count_on = NCANON + new_n0 as u64 * 0x1000;
    } else {
        // C: edit->leaf_p = &new_n1->slots[next_slot++]; -- the post
        // increment feeds the BUG_ON(next_slot <= 1) below.
        r.edm(e).leaf_p = PC_NSLOT + new_n1 as u64 * 64 + next_slot as u64;
        next_slot += 1;
        r.edm(e).adjust_count_on = NCANON + new_n1 as u64 * 0x1000;
    }

    bug_on(next_slot <= 1);

    r.edm(e).set_backpointers_to = node_to_ptr(new_n0);
    for i in 0..FAN_OUT {
        if r.ed(e).segment_cache[i] == 0xff {
            let ptr = r.n(node).slots[i];
            bug_on(is_leaf(ptr));
            if is_node(ptr) {
                r.edm(e).set_backpointers[i] = PC_NBACK + node_i(ptr) as u64;
            } else {
                r.edm(e).set_backpointers[i] = PC_SBACK + sc_i(ptr) as u64;
            }
        }
    }
}

/// assoc_array_insert_shortcut: returns the shortcut pool index or NIL.
fn assoc_array_insert_shortcut(
    r: &mut AssocArray,
    e: usize,
    index_key: &TKey,
    node: usize,
    level: i32,
    new_n0: usize,
) -> usize {
    // find where the leaves start diverging (min over diff_objects)
    let mut diff: i64 = i64::MAX;
    for i in 0..FAN_OUT {
        let x = diff_objects(r, ptr_to_leaf(r.n(node).slots[i]), index_key);
        if x < diff {
            bug_on(x < 0);
            diff = x;
        }
    }
    bug_on(diff == i64::MAX);
    bug_on(diff < level as i64 + LEVEL_STEP as i64);

    let keylen = (round_up_i32(diff as i32, KEY_CHUNK_SIZE) >> KEY_CHUNK_SHIFT) as usize;
    let new_s0 = alloc_sc(r, keylen);
    if new_s0 == NIL {
        return NIL;
    }
    r.edm(e).new_meta[2] = sc_to_ptr(new_s0);

    r.sm(new_s0).back_pointer = r.n(node).back_pointer;
    r.sm(new_s0).parent_slot = r.n(node).parent_slot as i32;

    if r.n(node).back_pointer != 0 && is_shortcut(r.n(node).back_pointer) {
        let above = sc_i(r.n(node).back_pointer);
        r.sm(new_s0).back_pointer = r.s(above).back_pointer;
        r.sm(new_s0).parent_slot = r.s(above).parent_slot;
        r.edm(e).excised_meta[1] = r.n(node).back_pointer;
    }
    r.sm(new_s0).next_node = node_to_ptr(new_n0);
    r.nm(new_n0).back_pointer = sc_to_ptr(new_s0);
    r.nm(new_n0).parent_slot = 0;

    let level = diff as i32 & !LEVEL_STEP_MASK;
    r.sm(new_s0).skip_to_level = level;
    bug_on(level <= 0);

    for i in 0..keylen {
        r.sm(new_s0).index_key[i] = get_key_chunk(index_key, i as i32 * KEY_CHUNK_SIZE);
    }

    if level & KEY_CHUNK_MASK != 0 {
        let blank = u64::MAX << (level & KEY_CHUNK_MASK) as u32;
        r.sm(new_s0).index_key[keylen - 1] &= !blank;
    }

    assoc_array_fill_segment_cache(r, e, node, level);
    let mut base_seg = get_key_chunk(index_key, level);
    base_seg >>= (level & KEY_CHUNK_MASK) as u32;
    r.edm(e).segment_cache[FAN_OUT] = (base_seg & FAN_MASK) as u8;
    new_s0
}

fn assoc_array_insert_into_terminal_node(
    r: &mut AssocArray,
    e: usize,
    index_key: &TKey,
    result: &WalkResult,
) -> bool {
    let node = result.tn_node;
    let level = result.tn_level;

    if assoc_array_insert_in_node(r, e, index_key, node) {
        return true;
    }

    let new_n0 = kzalloc_node(r);
    if new_n0 == NIL {
        return false;
    }
    r.edm(e).new_meta[0] = node_to_ptr(new_n0);
    let new_n1 = kzalloc_node(r);
    if new_n1 == NIL {
        return false;
    }
    r.edm(e).new_meta[1] = node_to_ptr(new_n1);

    let have_meta = assoc_array_fill_segment_cache(r, e, node, level);
    r.edm(e).segment_cache[FAN_OUT] = result.tn_slot as u8;

    if !have_meta && assoc_array_all_one_segment(r, e) {
        let new_s0 = assoc_array_insert_shortcut(r, e, index_key, node, level, new_n0);
        if new_s0 == NIL {
            return false;
        }
        r.edm(e).set_ptr[0] =
            parent_link(r.s(new_s0).back_pointer, r.s(new_s0).parent_slot);
        r.edm(e).set_to[0] = sc_to_ptr(new_s0);
    } else {
        r.nm(new_n0).back_pointer = r.n(node).back_pointer;
        r.nm(new_n0).parent_slot = r.n(node).parent_slot;
        r.edm(e).set_ptr[0] = parent_link(
            r.n(new_n0).back_pointer,
            r.n(new_n0).parent_slot as i32,
        );
        r.edm(e).set_to[0] = node_to_ptr(new_n0);
    }

    assoc_array_split_node(r, e, node, new_n0, new_n1);
    r.edm(e).excised_meta[0] = node_to_ptr(node);
    true
}

fn assoc_array_insert_mid_shortcut(r: &mut AssocArray, e: usize, result: &WalkResult) -> bool {
    let shortcut = result.ws_shortcut;
    let level = result.ws_level;
    let sc_level = result.ws_sc_level;
    let sc_segments = result.ws_sc_segments;
    let dissimilarity = result.ws_dissimilarity;

    let mut diff = dissimilarity.trailing_zeros() as i32; // __ffs
    diff &= !LEVEL_STEP_MASK;
    diff += sc_level & !KEY_CHUNK_MASK;

    bug_on(r.s(shortcut).back_pointer != 0 && !is_node(r.s(shortcut).back_pointer));
    r.edm(e).set_ptr[0] = parent_link(
        r.s(shortcut).back_pointer,
        r.s(shortcut).parent_slot,
    );

    r.edm(e).excised_meta[0] = sc_to_ptr(shortcut);

    let new_n0 = kzalloc_node(r);
    if new_n0 == NIL {
        return false;
    }
    r.edm(e).new_meta[0] = node_to_ptr(new_n0);
    r.edm(e).adjust_count_on = NCANON + new_n0 as u64 * 0x1000;

    let mut level = level;
    if r.s(shortcut).back_pointer != 0 {
        level = level.wrapping_add(LEVEL_STEP);
    }
    if diff > level {
        let keylen = (round_up_i32(diff, KEY_CHUNK_SIZE) >> KEY_CHUNK_SHIFT) as usize;
        let new_s0 = alloc_sc(r, keylen);
        if new_s0 == NIL {
            return false;
        }
        r.edm(e).new_meta[1] = sc_to_ptr(new_s0);
        r.edm(e).set_to[0] = sc_to_ptr(new_s0);
        r.sm(new_s0).back_pointer = r.s(shortcut).back_pointer;
        r.sm(new_s0).parent_slot = r.s(shortcut).parent_slot;
        r.sm(new_s0).next_node = node_to_ptr(new_n0);
        r.sm(new_s0).skip_to_level = diff;

        r.nm(new_n0).back_pointer = sc_to_ptr(new_s0);
        r.nm(new_n0).parent_slot = 0;

        for i in 0..keylen {
            r.sm(new_s0).index_key[i] = r.s(shortcut).index_key[i];
        }

        if diff & KEY_CHUNK_MASK != 0 {
            let blank = u64::MAX << (diff & KEY_CHUNK_MASK) as u32;
            r.sm(new_s0).index_key[keylen - 1] &= !blank;
        }
    } else {
        r.edm(e).set_to[0] = node_to_ptr(new_n0);
        r.nm(new_n0).back_pointer = r.s(shortcut).back_pointer;
        r.nm(new_n0).parent_slot = r.s(shortcut).parent_slot as u8;
    }

    let side = node_i(r.s(shortcut).next_node);
    r.nm(new_n0).nr_leaves_on_branch = r.n(side).nr_leaves_on_branch;

    let sc_slot = (sc_segments >> (diff & KEY_CHUNK_MASK) as u32) & FAN_MASK;
    let sc_slot_i = sc_slot as usize;

    let level2 = diff.wrapping_add(LEVEL_STEP);
    if level2 < r.s(shortcut).skip_to_level {
        let keylen = (round_up_i32(r.s(shortcut).skip_to_level, KEY_CHUNK_SIZE)
            >> KEY_CHUNK_SHIFT) as usize;
        let new_s1 = alloc_sc(r, keylen);
        if new_s1 == NIL {
            return false;
        }
        r.edm(e).new_meta[2] = sc_to_ptr(new_s1);

        r.sm(new_s1).back_pointer = node_to_ptr(new_n0);
        r.sm(new_s1).parent_slot = sc_slot_i as i32;
        r.sm(new_s1).next_node = r.s(shortcut).next_node;
        r.sm(new_s1).skip_to_level = r.s(shortcut).skip_to_level;

        r.nm(new_n0).slots[sc_slot_i] = sc_to_ptr(new_s1);

        for i in 0..keylen {
            r.sm(new_s1).index_key[i] = r.s(shortcut).index_key[i];
        }

        r.edm(e).set_ptr[1] = PC_NBACK + side as u64;
        r.edm(e).set_to[1] = sc_to_ptr(new_s1);
    } else {
        r.nm(new_n0).slots[sc_slot_i] = r.s(shortcut).next_node;
        r.edm(e).sps_p = PC_NPSLT + side as u64;
        r.edm(e).sps_to = sc_slot_i as u8;
        r.edm(e).set_ptr[1] = PC_NBACK + side as u64;
        r.edm(e).set_to[1] = node_to_ptr(new_n0);
    }

    if sc_slot_i == 0 {
        r.edm(e).leaf_p = PC_NSLOT + new_n0 as u64 * 64 + 1;
    } else {
        r.edm(e).leaf_p = PC_NSLOT + new_n0 as u64 * 64;
    }
    true
}

/// assoc_array_insert: returns an edit canonical or ERR_PTR(-ENOMEM).
pub fn assoc_array_insert(r: &mut AssocArray, index_key: &TKey, object: u64) -> u64 {
    bug_on(is_meta(object));

    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edm(e).leaf = object;
    r.edm(e).adjust_count_by = 1;

    let mut result = WalkResult::default();
    match assoc_array_walk(r, index_key, &mut result) {
        0 => {
            if !assoc_array_insert_in_empty_tree(r, e) {
                assoc_array_cancel_edit(r, e);
                return (-ENOMEM) as u64;
            }
        }
        1 => {
            if !assoc_array_insert_into_terminal_node(r, e, index_key, &result) {
                assoc_array_cancel_edit(r, e);
                return (-ENOMEM) as u64;
            }
        }
        _ => {
            if !assoc_array_insert_mid_shortcut(r, e, &result) {
                assoc_array_cancel_edit(r, e);
                return (-ENOMEM) as u64;
            }
        }
    }
    edit_canon(e)
}

/// assoc_array_insert_set_object.
pub fn assoc_array_insert_set_object(r: &mut AssocArray, e: usize, object: u64) {
    bug_on(object == 0 || is_meta(object));
    // object is a leaf canonical already tagged
    r.edm(e).leaf = object;
}

fn collapse_iterator(r: &mut AssocArray, ctx: &mut Collapse, leaf: u64) -> i32 {
    if leaf == ctx.skip_leaf {
        return 0;
    }
    bug_on(ctx.slot >= FAN_OUT as i32);
    let n = ctx.node;
    r.nm(n).slots[ctx.slot as usize] = leaf;
    ctx.slot += 1;
    0
}

fn assoc_array_delete_collapse(r: &mut AssocArray, e: usize, node0: usize) -> bool {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    let mut has_meta = false;
    for i in 0..FAN_OUT {
        if is_meta(r.n(node0).slots[i]) {
            has_meta = true;
            break;
        }
    }

    let mut parent = node0;
    loop {
        __lim += 1;
        if __lim > lim {
            r.warn += 1;
            break;
        }
        let mut ptr = r.n(parent).back_pointer;
        if ptr != 0 && is_shortcut(ptr) {
            ptr = r.s(sc_i(ptr)).back_pointer;
        }
        if ptr == 0 {
            break;
        }
        let grandparent = node_i(ptr);
        if r.n(grandparent).nr_leaves_on_branch > (FAN_OUT + 1) as u64 {
            break;
        }
        parent = grandparent;
    }

    if has_meta || parent != node0 {
        let node = parent;
        let new_n0 = kzalloc_node(r);
        if new_n0 == NIL {
            return false;
        }
        r.edm(e).new_meta[0] = node_to_ptr(new_n0);

        r.nm(new_n0).back_pointer = r.n(node).back_pointer;
        r.nm(new_n0).parent_slot = r.n(node).parent_slot;
        r.nm(new_n0).nr_leaves_on_branch = r.n(node).nr_leaves_on_branch;
        r.edm(e).adjust_count_on = NCANON + new_n0 as u64 * 0x1000;

        let mut collapse = Collapse {
            node: new_n0,
            skip_leaf: ptr_to_leaf(r.ed(e).dead_leaf),
            slot: 0,
        };
        subtree_iterate(
            r,
            node_to_ptr(node),
            r.n(node).back_pointer,
            &mut collapse_iterator,
            &mut collapse,
        );
        bug_on(collapse.slot as u64 != r.n(new_n0).nr_leaves_on_branch.wrapping_sub(1));

        bug_on(r.n(node).back_pointer != 0 && is_leaf(r.n(node).back_pointer));
        r.edm(e).set_ptr[1] =
            parent_link(r.n(node).back_pointer, r.n(node).parent_slot as i32);
        r.edm(e).set_to[1] = node_to_ptr(new_n0);
        r.edm(e).excised_subtree = node_to_ptr(node);
    }
    true
}

/// assoc_array_delete: edit canonical, 0 (not found) or ERR_PTR(-ENOMEM).
pub fn assoc_array_delete(r: &mut AssocArray, index_key: &TKey) -> u64 {
    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edm(e).adjust_count_by = -1;

    let mut node: usize = 0;
    let mut slot: usize = FAN_OUT;
    let mut result = WalkResult::default();
    if assoc_array_walk(r, index_key, &mut result) == 1 {
        node = result.tn_node;
        for s in 0..FAN_OUT {
            let ptr = r.n(node).slots[s];
            if ptr != 0 && is_leaf(ptr) && compare_object(r, ptr, index_key) {
                slot = s;
                break;
            }
        }
    }
    if slot == FAN_OUT {
        assoc_array_cancel_edit(r, e);
        return 0;
    }

    bug_on(r.nr_leaves_on_tree == 0);

    r.edm(e).dead_leaf = r.n(node).slots[slot];
    r.edm(e).set_ptr[0] = PC_NSLOT + node as u64 * 64 + slot as u64;
    r.edm(e).set_to[0] = 0;
    r.edm(e).adjust_count_on = NCANON + node as u64 * 0x1000;

    if r.nr_leaves_on_tree == 1 {
        r.edm(e).set_ptr[1] = PC_ROOT;
        r.edm(e).set_to[1] = 0;
        r.edm(e).adjust_count_on = 0;
        r.edm(e).excised_subtree = r.root;
        return edit_canon(e);
    }

    if r.n(node).nr_leaves_on_branch <= (FAN_OUT + 1) as u64
        && !assoc_array_delete_collapse(r, e, node)
    {
        assoc_array_cancel_edit(r, e);
        return (-ENOMEM) as u64;
    }

    edit_canon(e)
}

/// assoc_array_clear: edit canonical, 0 (empty) or ERR_PTR(-ENOMEM).
pub fn assoc_array_clear(r: &mut AssocArray) -> u64 {
    if r.root == 0 {
        return 0;
    }
    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edm(e).set_ptr[1] = PC_ROOT;
    r.edm(e).set_to[1] = 0;
    r.edm(e).excised_subtree = r.root;
    r.edm(e).ops_for_excised_subtree = true;
    edit_canon(e)
}

/// assoc_array_rcu_cleanup — runs synchronously, like the call_rcu shim.
fn rcu_cleanup(r: &mut AssocArray, e: usize) {
    if r.ed(e).dead_leaf != 0 {
        let leaf = ptr_to_leaf(r.ed(e).dead_leaf);
        free_object(r, leaf);
    }
    for i in 0..2 {
        let m = r.ed(e).excised_meta[i];
        if m != 0 {
            kfree(r, m & !3);
        }
    }

    if r.ed(e).excised_subtree != 0 {
        bug_on(is_leaf(r.ed(e).excised_subtree));
        if is_node(r.ed(e).excised_subtree) {
            let n = node_i(r.ed(e).excised_subtree);
            r.nm(n).back_pointer = 0;
        } else {
            let s = sc_i(r.ed(e).excised_subtree);
            r.sm(s).back_pointer = 0;
        }
        let sub = r.ed(e).excised_subtree;
        let free = r.ed(e).ops_for_excised_subtree;
        destroy_subtree(r, sub, free);
    }

    r.edm(e).used = false;
}

/// assoc_array_apply_edit.
pub fn assoc_array_apply_edit(r: &mut AssocArray, e: usize) {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    if r.ed(e).leaf_p != 0 {
        let (a, v) = (r.ed(e).leaf_p, r.ed(e).leaf);
        store_p(r, a, v);
    }
    if r.ed(e).sps_p != 0 {
        let (a, v) = (r.ed(e).sps_p, r.ed(e).sps_to);
        store8_p(r, a, v);
    }
    for i in 0..FAN_OUT {
        if r.ed(e).set_backpointers[i] != 0 {
            let (a, v) = (r.ed(e).set_backpointers[i], r.ed(e).set_backpointers_to);
            store_p(r, a, v);
        }
    }
    for i in 0..2 {
        if r.ed(e).set_ptr[i] != 0 {
            let (a, v) = (r.ed(e).set_ptr[i], r.ed(e).set_to[i]);
            store_p(r, a, v);
        }
    }

    if r.root == 0 {
        r.nr_leaves_on_tree = 0;
    } else if r.ed(e).adjust_count_on != 0 {
        let mut node = node_i(r.ed(e).adjust_count_on);
        loop {
            __lim += 1;
            if __lim > lim {
                r.warn += 1;
                break;
            }
            r.nm(node).nr_leaves_on_branch = r
                .n(node)
                .nr_leaves_on_branch
                .wrapping_add(r.ed(e).adjust_count_by as u64);
            let mut ptr = r.n(node).back_pointer;
            if ptr == 0 {
                break;
            }
            if is_shortcut(ptr) {
                ptr = r.s(sc_i(ptr)).back_pointer;
                if ptr == 0 {
                    break;
                }
            }
            bug_on(!is_node(ptr));
            node = node_i(ptr);
        }
        r.nr_leaves_on_tree = r
            .nr_leaves_on_tree
            .wrapping_add(r.ed(e).adjust_count_by as u64);
    }

    rcu_cleanup(r, e);
}

/// assoc_array_cancel_edit.
pub fn assoc_array_cancel_edit(r: &mut AssocArray, e: usize) {
    for i in 0..3 {
        let ptr = r.ed(e).new_meta[i];
        if ptr != 0 {
            // frees the node or shortcut canonical; identical either way
            kfree(r, ptr & !3);
        }
    }
    r.edm(e).used = false;
}

// ---- gc ---------------------------------------------------------------------

/// link target inside the gc copy: &w.new_root, &new_n->slots[s] or
/// &new_s->next_node.
#[derive(Clone, Copy)]
enum GcLink {
    WRoot,
    Slot(usize, usize),
    ScNext(usize),
}

fn gc_write_link(r: &mut AssocArray, w: &mut GcWalk, link: GcLink, v: u64) {
    match link {
        GcLink::WRoot => w.new_root = v,
        GcLink::Slot(n, s) => {
            // a crafted slot index drops the write like a scratch store
            if s < FAN_OUT {
                r.nm(n).slots[s] = v;
            }
        }
        GcLink::ScNext(s) => r.sm(s).next_node = v,
    }
}

fn assoc_array_gc_copy(r: &mut AssocArray, w: &mut GcWalk, cursor0: u64, link0: GcLink) -> bool {
    let mut cursor = cursor0;
    let mut link = link0;
    let mut new_parent = if w.new_n != NIL {
        node_to_ptr(w.new_n)
    } else {
        0
    };

    let new_n = kzalloc_node(r);
    if new_n == NIL {
        return false;
    }

    if is_shortcut(cursor) {
        let shortcut = sc_i(cursor);
        let keylen = (round_up_i32(r.s(shortcut).skip_to_level, KEY_CHUNK_SIZE)
            >> KEY_CHUNK_SHIFT) as usize;
        let new_s = alloc_sc(r, keylen);
        if new_s == NIL {
            kfree(r, NCANON + new_n as u64 * 0x1000);
            return false;
        }
        // memcpy(new_s, shortcut, struct_size) then patch
        let src = *r.s(shortcut);
        *r.sm(new_s) = src;
        r.sm(new_s).used = true;
        r.sm(new_s).back_pointer = new_parent;
        r.sm(new_s).parent_slot = src.parent_slot;
        gc_write_link(r, w, link, sc_to_ptr(new_s));
        new_parent = sc_to_ptr(new_s);
        link = GcLink::ScNext(new_s);
        cursor = src.next_node;
    }

    w.node = node_i(cursor);
    r.nm(new_n).back_pointer = new_parent;
    r.nm(new_n).parent_slot = r.n(w.node).parent_slot;
    gc_write_link(r, w, link, node_to_ptr(new_n));
    w.new_n = new_n;
    w.slot = 0;
    true
}

fn assoc_array_gc_next<C>(
    r: &mut AssocArray,
    w: &mut GcWalk,
    it: &mut dyn FnMut(&mut AssocArray, &mut C, u64) -> bool,
    ctx: &mut C,
) -> u64 {
    while w.slot < FAN_OUT as i32 {
        let ptr = nslot(r.n(w.node), w.slot);
        if ptr == 0 {
            w.slot += 1;
            continue;
        }
        if is_leaf(ptr) {
            if it(r, ctx, ptr_to_leaf(ptr)) {
                let nn = w.new_n;
                let s = (w.slot as usize).min(FAN_OUT - 1);
                r.nm(nn).slots[s] = ptr;
            }
            w.slot += 1;
            continue;
        }
        return ptr;
    }
    0
}

fn assoc_array_gc_fold(r: &mut AssocArray, new_n: usize) -> i32 {
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    'retry_compress: loop {
        __lim += 1;
        if __lim > lim {
            r.warn += 1;
            return 0;
        }
        r.nm(new_n).nr_leaves_on_branch = 0;
        let mut nr_free: i32 = 0;
        for slot in 0..FAN_OUT {
            let ptr = r.n(new_n).slots[slot];
            if ptr == 0 {
                nr_free += 1;
            } else if is_leaf(ptr) {
                r.nm(new_n).nr_leaves_on_branch += 1;
            }
        }

        let mut retained = false;
        let mut next_slot = 0usize;
        for slot in 0..FAN_OUT {
            let mut ptr = r.n(new_n).slots[slot];
            if ptr == 0 || is_leaf(ptr) {
                continue;
            }
            let mut have_s = false;
            if is_shortcut(ptr) {
                have_s = true;
                ptr = r.s(sc_i(ptr)).next_node;
            }
            let child = node_i(ptr);
            let child_leaves = r.n(child).nr_leaves_on_branch;
            r.nm(new_n).nr_leaves_on_branch = r
                .n(new_n)
                .nr_leaves_on_branch
                .wrapping_add(child_leaves);

            if child_leaves <= (nr_free + 1) as u64 {
                // fold the child node into this one
                bug_on(have_s);
                r.nm(new_n).slots[slot] = 0;
                nr_free += 1;
                if slot < next_slot {
                    next_slot = slot;
                }
                for i in 0..FAN_OUT {
                    let p = r.n(child).slots[i];
                    if p == 0 {
                        continue;
                    }
                    bug_on(is_meta(p));
                    while next_slot < FAN_OUT && r.n(new_n).slots[next_slot] != 0 {
                        next_slot += 1;
                    }
                    bug_on(next_slot >= FAN_OUT);
                    r.nm(new_n).slots[next_slot] = p;
                    next_slot += 1;
                    nr_free -= 1;
                }
                r.nm(child).used = false;
            } else {
                retained = true;
            }
        }

        if retained && r.n(new_n).nr_leaves_on_branch <= FAN_OUT as u64 {
            continue 'retry_compress;
        }
        return nr_free;
    }
}

/// assoc_array_gc_pack: returns the copy's parent tagged ptr, or 0 at root.
fn assoc_array_gc_pack(r: &mut AssocArray, w: &mut GcWalk) -> u64 {
    let new_n = w.new_n;
    let nr_free = assoc_array_gc_fold(r, new_n);
    w.nr_leaves_on_tree = r.n(new_n).nr_leaves_on_branch;

    if nr_free == (FAN_OUT - 1) as i32 {
        let mut slot0 = 0usize;
        let mut ptr = 0u64;
        for slot in 0..FAN_OUT {
            ptr = r.n(new_n).slots[slot];
            if ptr != 0 {
                slot0 = slot;
                break;
            }
        }
        if is_meta(ptr) && is_shortcut(ptr) {
            let new_s = sc_i(ptr);
            let mut new_parent = r.n(new_n).back_pointer;
            let mut slot = r.n(new_n).parent_slot as i32;
            r.nm(new_n).used = false;
            if new_parent == 0 {
                r.sm(new_s).back_pointer = 0;
                r.sm(new_s).parent_slot = 0;
                w.new_root = ptr;
                return 0;
            }
            if is_shortcut(new_parent) {
                let s = sc_i(new_parent);
                new_parent = r.s(s).back_pointer;
                slot = r.s(s).parent_slot;
                r.sm(new_s).back_pointer = new_parent;
                r.sm(new_s).parent_slot = slot;
                r.sm(s).used = false;
                if new_parent == 0 {
                    r.sm(new_s).back_pointer = 0;
                    r.sm(new_s).parent_slot = 0;
                    w.new_root = ptr;
                    return 0;
                }
            }
            r.sm(new_s).back_pointer = new_parent;
            r.sm(new_s).parent_slot = slot;
            r.nm(node_i(new_parent)).slots[(slot as usize).min(FAN_OUT - 1)] = ptr;
            return new_parent;
        }
        let _ = slot0;
    }

    let ptr = r.n(new_n).back_pointer;
    if ptr == 0 {
        return 0;
    }
    if !is_shortcut(ptr) {
        return ptr;
    }

    let new_s = sc_i(ptr);
    let new_parent = r.s(new_s).back_pointer;
    let slot = r.s(new_s).parent_slot;

    if r.n(new_n).nr_leaves_on_branch <= FAN_OUT as u64 {
        r.nm(new_n).back_pointer = new_parent;
        r.nm(new_n).parent_slot = slot as u8;
        r.sm(new_s).used = false;
        if new_parent == 0 {
            w.new_root = node_to_ptr(new_n);
            return 0;
        }
        r.nm(node_i(new_parent)).slots[(slot as usize).min(FAN_OUT - 1)] = node_to_ptr(new_n);
    }
    new_parent
}

fn assoc_array_gc_ascend(r: &AssocArray, w: &mut GcWalk, parent: u64) {
    w.new_n = node_i(parent);

    let ptr = r.n(w.node).back_pointer;
    let (slot, cursor) = if is_shortcut(ptr) {
        let s = sc_i(ptr);
        (r.s(s).parent_slot, r.s(s).back_pointer)
    } else {
        (r.n(w.node).parent_slot as i32, ptr)
    };
    bug_on(cursor == 0);
    w.node = node_i(cursor);
    // clamp so a crafted sc.parent_slot can't make gc_next scan ~2^31
    w.slot = slot.wrapping_add(1).clamp(-(FAN_OUT as i32), FAN_OUT as i32);
}

/// assoc_array_gc.
pub fn assoc_array_gc<C>(
    r: &mut AssocArray,
    it: &mut dyn FnMut(&mut AssocArray, &mut C, u64) -> bool,
    ctx: &mut C,
) -> i32 {
    if r.root == 0 {
        return 0;
    }
    let lim = iter_limit(r);
    let mut __lim = 0u64;
    let e = kzalloc_edit(r);
    if e == NIL {
        return -ENOMEM as i32;
    }
    r.edm(e).ops_for_excised_subtree = true;
    r.edm(e).set_ptr[0] = PC_ROOT;
    r.edm(e).excised_subtree = r.root;

    let mut w = GcWalk {
        node: NIL,
        new_n: NIL,
        slot: 0,
        new_root: 0,
        nr_leaves_on_tree: 0,
    };

    let mut ptr = r.root;
    let mut link = GcLink::WRoot;
    'outer: loop {
        __lim += 1;
        if __lim > lim {
            // corrupt/wild state: fail like the enomem path — destroy
            // the partial copy, free the edit, return -ENOMEM.
            let nr = w.new_root;
            destroy_subtree(r, nr, true);
            r.edm(e).used = false;
            r.warn += 1;
            return -ENOMEM as i32;
        }
        if !assoc_array_gc_copy(r, &mut w, ptr, link) {
            // enomem: destroy the partially built copy, free the edit
            let nr = w.new_root;
            destroy_subtree(r, nr, true);
            r.edm(e).used = false;
            return -ENOMEM as i32;
        }
        loop {
            __lim += 1;
            if __lim > lim {
                let nr = w.new_root;
                destroy_subtree(r, nr, true);
                r.edm(e).used = false;
                r.warn += 1;
                return -ENOMEM as i32;
            }
            ptr = assoc_array_gc_next(r, &mut w, it, ctx);
            if ptr != 0 {
                break;
            }
            let parent = assoc_array_gc_pack(r, &mut w);
            if parent == 0 {
                break 'outer; // copied
            }
            assoc_array_gc_ascend(r, &mut w, parent);
        }
        link = GcLink::Slot(w.new_n, w.slot as usize);
    }

    r.edm(e).set_to[0] = w.new_root;
    assoc_array_apply_edit(r, e);
    r.nr_leaves_on_tree = w.nr_leaves_on_tree;
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("assoc_array_tests.inc");
}
