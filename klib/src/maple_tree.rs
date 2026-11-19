//! `klib::maple_tree` — Linux `lib/maple_tree.c` ported to safe Rust,
//! verified against the C implementation (666/666 recorded differential
//! cases, `ports/lib-maple-tree`).
//!
//! Storage model: a caller-owned dense node arena (`&mut [Node]` of 256-byte
//! nodes, 32 `u64` words each) plus a caller-owned sheaf arena
//! (`&mut [Sheaf]`) replace the maple slab cache and its per-CPU sheaf
//! magazine layer. Every C pointer is a canonical `u64`: pool node `i` is
//! `NBASE + i*256`, sheaf `i` is `SHBASE + i*256`, and interior
//! `unsigned long *`/`void **` pointers are `node_base + byte_offset`, so
//! the union views (slots/pivots/gaps/meta/copy/topiary/rcu) share storage
//! byte-for-byte exactly like C on x86-64. The four reserved pseudo-node
//! slots (`CPBASE`/`NULLN`/`REUSE` + one spare) live in `spare`, at the
//! fixed canonical indices `NP..NP+NEXTRA`; `nidx` maps any out-of-range
//! pointer to the `NULLN` scratch slot, mirroring the verified model.
//!
//! Tree identity: upstream takes `struct maple_tree *`; here the `mt: u64`
//! argument is a canonical tree id — `MT_CANON` for the first tree,
//! `MT2_CANON` for the second. `MapleTree` hosts both heads
//! (`ma_root`/`ma_flags`, indexed by `tidx`) because the verified harness
//! shares one slab between `&mt` and `&mt2` for `mtree_dup`; callers that
//! need a single tree always pass `MT_CANON`. `struct ma_state` is `Mas`,
//! `ma_wr_state` is `MaWrState`, `ma_topiary` is `MaTopiary`
//! (`Mas::new(tree, index, last)` ≡ `MA_STATE(mas, mt, index, last)`).
//!
//! Notes:
//! * `nodes.len()` must be `<= NP` (448) so pool addresses can never
//!   collide with the reserved pseudo-node range; `sheafs` may be any
//!   length (upstream `maple_node_cache` sheaf pool sized to taste).
//! * Allocation-failure injection: set `fail_mask` bit `k` to fail the
//!   (k+1)-th node/sheaf allocation (`alloc_ctr` counts attempts, bits
//!   0..64 effective). `warn`/`warn_once` count MAS_BUG_ON/WARN_ON hits.
//! * `node_free` of a scratch/wild canonical pointer is ignored rather
//!   than panicking (the verified model indexed out of bounds there; no
//!   valid path frees such pointers, so behaviour is unchanged).
//! * Divergence guards: upstream `mas_dup_build` can livelock on a corrupt
//!   tree; the port bounds the outer copy loop at 100000 iterations
//!   (`panic!("dup spin")`, as verified) and the inner ascend at 256
//!   (`panic!("asc spin")` — the model only checked this under trace).
//!   Neither bound is reachable on a valid tree.
//! * The harness fold/trace/RNG machinery (`fold`, `h`, `r`, `xs64`,
//!   `trace`, `epool`, `hot`, `eprintln!`) was harness residue and is not
//!   adopted; `Vec<Mas>` scratch in `mas_topiary_replace` became `[Mas; 3]`.

#![forbid(unsafe_code)]
#![allow(clippy::all)]
#![allow(unused)]

/* ================= canonical model ================= */

pub const NP: usize = 448;
pub const EP: usize = 96;
pub const NSP: usize = 12;
pub const NILX: u64 = 0xffff;
/* Reserved pseudo-node slots after the real pool: [NP] = stack
 * maple_copy storage (a `struct maple_copy` local is 256B, same layout
 * as a node), [NP+1] = scratch for writes C would send to NULL.  These
 * indices are never handed out by node_alloc. */
pub const NEXTRA: usize = 4;
pub const CPBASE: u64 = NBASE + (NP as u64) * 256;
pub const NULLN: u64 = NBASE + ((NP as u64) + 1) * 256;

pub const NBASE: u64 = 0x0010_0000_0000_0000;
pub const EBASE: u64 = 0x0020_0000_0000_0000;
/* Canonical tree pointers: base + byte-offset within the struct (C's
 * efold folds p - &mt / p - &mt2 so the encoding is layout-stable).
 * mas_tree_parent = tree | MA_ROOT_PARENT folds as base | 1. */
pub const MT_CANON: u64 = 0x0080_0000_0000_0000;
pub const MT2_CANON: u64 = 0x0080_0000_0000_1000;
pub const SHBASE: u64 = 0x00A0_0000_0000_0000;

pub const ENOMEM: i64 = 12;
pub const EINVAL: i64 = 22;
pub const EBUSY: i64 = 16;
pub const EEXIST: i64 = 17;

pub const GFP_NOWAIT: u32 = 0x2800;
pub const GFP_KERNEL: u32 = 0xcc0;
pub const GFP_ATOMIC: u32 = 0x820;
pub const GFP_NOFAIL: u32 = 1 << 15;
pub const GFP_DIRECT_RECLAIM: u32 = 1 << 10;

pub const ULONG_MAX: u64 = u64::MAX;

pub fn gfpflags_allow_blocking(gfp: u32) -> bool {
    gfp & GFP_DIRECT_RECLAIM != 0
}

/* maple_tree.h constants */
pub const MAPLE_NODE_SLOTS: usize = 31;
pub const MAPLE_RANGE64_SLOTS: usize = 16;
pub const MAPLE_ARANGE64_SLOTS: usize = 10;
pub const MAPLE_NODE_MASK: u64 = 255;
pub const MAPLE_NODE_TYPE_MASK: u64 = 0x0F;
pub const MAPLE_NODE_TYPE_SHIFT: u64 = 0x03;
pub const MAPLE_RESERVED_RANGE: u64 = 4096;

pub const MT_FLAGS_ALLOC_RANGE: u32 = 0x01;
pub const MT_FLAGS_USE_RCU: u32 = 0x02;
pub const MT_FLAGS_HEIGHT_OFFSET: u32 = 0x02;
pub const MT_FLAGS_HEIGHT_MASK: u32 = 0x7C;
pub const MT_FLAGS_LOCK_MASK: u32 = 0x300;
pub const MT_FLAGS_LOCK_EXTERN: u32 = 0x300;
pub const MAPLE_HEIGHT_MAX: u8 = 31;

pub const MA_STATE_PREALLOC: u8 = 1;

/* enum maple_type */
pub const MAPLE_DENSE: u8 = 0;
pub const MAPLE_LEAF_64: u8 = 1;
pub const MAPLE_RANGE_64: u8 = 2;
pub const MAPLE_ARANGE_64: u8 = 3;
pub const MAPLE_COPY: u8 = 4;

/* enum maple_status */
pub const MA_ACTIVE: u8 = 0;
pub const MA_START: u8 = 1;
pub const MA_ROOT: u8 = 2;
pub const MA_NONE: u8 = 3;
pub const MA_PAUSE: u8 = 4;
pub const MA_OVERFLOW: u8 = 5;
pub const MA_UNDERFLOW: u8 = 6;
pub const MA_ERROR_S: u8 = 7;

/* enum store_type */
pub const WR_INVALID: u8 = 0;
pub const WR_NEW_ROOT: u8 = 1;
pub const WR_STORE_ROOT: u8 = 2;
pub const WR_EXACT_FIT: u8 = 3;
pub const WR_SPANNING_STORE: u8 = 4;
pub const WR_SPLIT_STORE: u8 = 5;
pub const WR_REBALANCE: u8 = 6;
pub const WR_APPEND: u8 = 7;
pub const WR_NODE_STORE: u8 = 8;
pub const WR_SLOT_STORE: u8 = 9;

pub const MA_ROOT_PARENT: u64 = 1;
pub const MAPLE_ROOT_NODE: u64 = 0x02;
pub const MAPLE_ENODE_TYPE_SHIFT: u64 = 0x03;
pub const MAPLE_ENODE_NULL: u64 = 0x04;

pub const MAPLE_PARENT_ROOT: u64 = 0x01;
pub const MAPLE_PARENT_SLOT_SHIFT: u64 = 0x03;
pub const MAPLE_PARENT_SLOT_MASK: u64 = 0xF8;
pub const MAPLE_PARENT_16B_SLOT_SHIFT: u64 = 0x02;
pub const MAPLE_PARENT_16B_SLOT_MASK: u64 = 0xFC;
pub const MAPLE_PARENT_RANGE64: u64 = 0x06;
pub const MAPLE_PARENT_NOT_RANGE16: u64 = 0x02;

pub fn ma_error(err: i64) -> u64 {
    ((err as u64) << 2) | 2
}
pub fn xa_err(entry: u64) -> i64 {
    if xa_is_err(entry) {
        (entry as i64) >> 2
    } else {
        0
    }
}
pub fn xa_is_internal(e: u64) -> bool {
    e & 3 == 2
}
pub fn xa_mk_internal(v: u64) -> u64 {
    (v << 2) | 2
}
pub const XA_ZERO_ENTRY: u64 = xa_mk_internal_const(257);
pub const fn xa_mk_internal_const(v: u64) -> u64 {
    (v << 2) | 2
}
pub fn xa_is_zero(e: u64) -> bool {
    e == XA_ZERO_ENTRY
}
pub const MAX_ERRNO: u64 = 4095;
pub fn xa_is_err(e: u64) -> bool {
    xa_is_internal(e) && e >= xa_mk_internal((0u64).wrapping_sub(MAX_ERRNO))
}
pub fn xa_is_advanced(e: u64) -> bool {
    xa_is_internal(e)
}
pub fn xa_is_node(e: u64) -> bool {
    xa_is_internal(e) && e > 4096
}
pub fn xa_to_node(e: u64) -> u64 {
    e & !3
}

#[derive(Clone)]
pub struct Sheaf {
    pub used: bool,
    pub size: u32,
    pub objs: [u64; 32],
}
impl Sheaf {
    pub const fn new() -> Self {
        Sheaf {
            used: false,
            size: 0,
            objs: [0; 32],
        }
    }
}

pub fn iabs(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}
/* ================= ma_state ================= */

#[derive(Clone)]
pub struct Mas {
    pub tree: u64, /* canonical: MT_CANON / MT2_CANON */
    pub index: u64,
    pub last: u64,
    pub node: u64, /* maple_enode (canonical) or MA_ERROR */
    pub min: u64,
    pub max: u64,
    pub sheaf: u64, /* canonical sheaf ptr or 0 */
    pub alloc: u64, /* canonical sheaf ptr or 0 */
    pub node_request: u8,
    pub status: u8,
    pub depth: u8,
    pub offset: u8,
    pub mas_flags: u8,
    pub end: u8,
    pub store_type: u8,
}

impl Mas {
    pub fn new(tree: u64, index: u64, last: u64) -> Self {
        Mas {
            tree,
            index,
            last,
            node: 0,
            min: 0,
            max: ULONG_MAX, /* MA_STATE initialiser */
            sheaf: 0,
            alloc: 0,
            node_request: 0,
            status: MA_START,
            depth: 0,
            offset: 0,
            mas_flags: 0,
            end: 0,
            store_type: WR_INVALID,
        }
    }
}

#[derive(Clone)]
pub struct MaWrState {
    /* struct ma_state *mas is passed alongside as &mut Mas */
    pub node: u64, /* maple_enode */
    pub r_min: u64,
    pub r_max: u64,
    pub type_: u8,
    pub offset_end: u32,
    pub pivots: u64,  /* interior ptr into wr node; 0 = NULL */
    pub end_piv: u64, /* interior ptr */
    pub slots: u64,   /* interior ptr */
    pub entry: u64,
    pub content: u64,
    pub vacant_height: u8,
    pub sufficient_height: u8,
}

impl MaWrState {
    pub fn new(_mas: &Mas, entry: u64) -> Self {
        MaWrState {
            node: 0,
            r_min: 0,
            r_max: 0,
            type_: 0,
            offset_end: 0,
            pivots: 0,
            end_piv: 0,
            slots: 0,
            entry,
            content: 0,
            vacant_height: 0,
            sufficient_height: 0,
        }
    }
}

/* MA_TOPIARY: struct ma_topiary { head, tail, mtree } on stack.
 * struct maple_topiary (in-node view) is { parent: word0, next: word1 }. */
#[derive(Clone)]
pub struct MaTopiary {
    pub head: u64, /* maple_enode */
    pub tail: u64,
    pub mtree: u64,
}
impl MaTopiary {
    pub fn new(tree: u64) -> Self {
        MaTopiary {
            head: 0,
            tail: 0,
            mtree: tree,
        }
    }
}

/* ================= structures ================= */

/// One 256-byte maple node: 32 `u64` words.
///
/// The arena (`MapleTree.nodes`) is caller-owned; `used` models the slab
/// allocated/free state. All C access is through the canonical word/byte
/// helpers (`w`/`sw`/`b`/`sb`/`g32`/`s32`/`rd`/`wr`), so `w` must not be
/// touched directly while the node is in use.
#[derive(Clone)]
pub struct Node {
    pub w: [u64; 32],
    pub used: bool,
}

impl Node {
    pub const fn new() -> Self {
        Node {
            w: [0; 32],
            used: false,
        }
    }
}

/// `struct maple_tree` world over caller-owned arenas.
///
/// Caller invariants (same contract as the C API):
/// * `nodes.len() <= NP` (checked in `new`); `nodes`/`sheafs` must not be
///   modified while in use (`MapleTree::new`/`reset_all` resets them);
/// * stored entries are the kernel's `void *` — any `u64` except the
///   internal/reserved encodings (`xa_is_advanced`, `mt_is_reserved`);
///   `mtree_store_range` WARNs (`warn_once` bit 9) on `xa_is_advanced`;
/// * `Mas` fields are public like the C struct, but mutating
///   `node`/`status`/`offset` mid-operation is out of contract — start
///   each operation with `Mas::new`/`mas_set`/`mas_set_range`;
/// * `gfp` takes the upstream flag values (`GFP_KERNEL`/`GFP_ATOMIC`/…);
///   `mas_preallocate`/`mas_nomem` model the blocking prealloc path.
pub struct MapleTree<'a> {
    /// Allocatable node pool; node `i` is canonical `NBASE + i*256`.
    pub nodes: &'a mut [Node],
    /// Sheaf pool (`maple_node_cache` magazines); sheaf `i` is `SHBASE + i*256`.
    pub sheafs: &'a mut [Sheaf],
    /// Reserved pseudo-nodes at canonical indices `NP..NP+NEXTRA`
    /// (`CPBASE` copy buffer, `NULLN`, `REUSE`, one spare).
    spare: [Node; NEXTRA],
    /// `struct maple_tree.ma_flags` for `MT_CANON`/`MT2_CANON` (`tidx`).
    pub ma_flags: [u32; 2],
    /// `struct maple_tree.ma_root` for `MT_CANON`/`MT2_CANON` (`tidx`).
    pub ma_root: [u64; 2],
    /// Number of node/sheaf allocation attempts (for `fail_mask`).
    pub alloc_ctr: u64,
    /// Bit `k` set => the (k+1)-th allocation attempt fails.
    pub fail_mask: u64,
    /// MAS_BUG_ON/WARN_ON counters (modelled; observable only).
    pub warn: u64,
    pub warn_once: u64,
}


/* ================= constants (interleaved decls) ================= */


pub const MT_SLOTS: [usize; 5] = [
    MAPLE_NODE_SLOTS,
    MAPLE_RANGE64_SLOTS,
    MAPLE_RANGE64_SLOTS,
    MAPLE_ARANGE64_SLOTS,
    3, /* copy */
];

pub const MT_PIVOTS: [usize; 5] = [
    0,
    MAPLE_RANGE64_SLOTS - 1,
    MAPLE_RANGE64_SLOTS - 1,
    MAPLE_ARANGE64_SLOTS - 1,
    3, /* copy */
];

pub const MT_MIN_SLOTS: [usize; 5] = [
    MAPLE_NODE_SLOTS / 2,
    (MAPLE_RANGE64_SLOTS / 2) - 2,
    (MAPLE_RANGE64_SLOTS / 2) - 2,
    (MAPLE_ARANGE64_SLOTS / 2) - 1,
    1, /* copy */
];


pub const REUSE: u64 = NBASE + ((NP as u64) + 2) * 256;


/* Iteration cap for pointer-chasing loops on crafted state; matches
 * the existing `retries > 1_000_000` bound in `mas_erase`. */
pub const MAS_ITER_LIMIT: u64 = 1_000_000;

/* ================= free helpers (no pool) ================= */

pub fn ma_parent_ptr(x: u64) -> u64 {
    x
}


pub fn mte_node_type(entry: u64) -> u8 {
    ((entry >> MAPLE_NODE_TYPE_SHIFT) & MAPLE_NODE_TYPE_MASK) as u8
}

pub fn ma_is_dense(t: u8) -> bool {
    t < MAPLE_LEAF_64
}

pub fn ma_is_leaf(t: u8) -> bool {
    t < MAPLE_RANGE_64
}

pub fn mte_is_leaf(entry: u64) -> bool {
    ma_is_leaf(mte_node_type(entry))
}

pub fn mt_is_reserved(entry: u64) -> bool {
    entry < MAPLE_RESERVED_RANGE && xa_is_internal(entry)
}

pub fn mas_set_err(mas: &mut Mas, err: i64) {
    mas.node = ma_error(err);
    mas.status = MA_ERROR_S;
}

pub fn mas_is_ptr(mas: &Mas) -> bool {
    mas.status == MA_ROOT
}

pub fn mas_is_start(mas: &Mas) -> bool {
    mas.status == MA_START
}

pub fn mas_is_none(mas: &Mas) -> bool {
    mas.status == MA_NONE
}

pub fn mas_is_paused(mas: &Mas) -> bool {
    mas.status == MA_PAUSE
}

pub fn mas_is_overflow(mas: &Mas) -> bool {
    mas.status == MA_OVERFLOW
}

pub fn mas_is_underflow(mas: &Mas) -> bool {
    mas.status == MA_UNDERFLOW
}

pub fn mas_is_active(mas: &Mas) -> bool {
    mas.status == MA_ACTIVE
}

pub fn mas_is_err(mas: &Mas) -> bool {
    mas.status == MA_ERROR_S
}

pub fn mas_make_walkable(mas: &mut Mas) {
    if !mas_is_active(mas) && !mas_is_start(mas) {
        mas.status = MA_START;
    }
}

pub fn mte_to_node(entry: u64) -> u64 {
    entry & !MAPLE_NODE_MASK
}

pub fn mte_to_mat(entry: u64) -> u64 {
    entry & !MAPLE_NODE_MASK
}

pub fn mas_mn(mas: &Mas) -> u64 {
    mte_to_node(mas.node)
}

pub fn mt_mk_node(node: u64, t: u8) -> u64 {
    node | ((t as u64) << MAPLE_ENODE_TYPE_SHIFT) | MAPLE_ENODE_NULL
}

pub fn mte_mk_root(node: u64) -> u64 {
    node | MAPLE_ROOT_NODE
}

pub fn mte_safe_root(node: u64) -> u64 {
    node & !MAPLE_ROOT_NODE
}

pub fn mte_set_full(node: u64) -> u64 {
    node & !MAPLE_ENODE_NULL
}

pub fn mte_clear_full(node: u64) -> u64 {
    node | MAPLE_ENODE_NULL
}

pub fn mte_has_null(node: u64) -> bool {
    node & MAPLE_ENODE_NULL != 0
}

pub fn mte_parent_shift(parent: u64) -> u64 {
    if parent & MAPLE_PARENT_NOT_RANGE16 != 0 {
        MAPLE_PARENT_SLOT_SHIFT
    } else {
        MAPLE_PARENT_16B_SLOT_SHIFT
    }
}

pub fn mte_parent_slot_mask(parent: u64) -> u64 {
    if parent & MAPLE_PARENT_NOT_RANGE16 != 0 {
        MAPLE_PARENT_SLOT_MASK
    } else {
        MAPLE_PARENT_16B_SLOT_MASK
    }
}

pub fn ma_pivots(node: u64, t: u8) -> u64 {
    match t {
        MAPLE_ARANGE_64 => node + 8,
        MAPLE_RANGE_64 | MAPLE_LEAF_64 => node + 8,
        MAPLE_COPY => node + 224,
        _ => 0,
    }
}

pub fn ma_gaps(node: u64, t: u8) -> u64 {
    match t {
        MAPLE_ARANGE_64 => node + 160,
        MAPLE_COPY => node + 192,
        _ => 0,
    }
}

pub fn ma_slots(mn: u64, mt: u8) -> u64 {
    match mt {
        MAPLE_ARANGE_64 => mn + 80,
        MAPLE_RANGE_64 | MAPLE_LEAF_64 => mn + 128,
        MAPLE_COPY => mn + 168,
        _ => mn + 8, /* maple_dense: mn->slot */
    }
}

pub fn ma_meta(mn: u64, mt: u8) -> u64 {
    if mt == MAPLE_ARANGE_64 {
        mn + 240
    } else {
        mn + 248
    }
}

pub fn mas_tree_parent(mas: &Mas) -> u64 {
    mas.tree | MA_ROOT_PARENT
}


pub fn cp_dst_node(cp: u64, i: usize) -> u64 {
    cp + (i * 24) as u64
}

pub fn cp_dst_max(cp: u64, i: usize) -> u64 {
    cp + 8 + (i * 24) as u64
}

pub fn cp_dst_mt(cp: u64, i: usize) -> u64 {
    cp + 16 + (i * 24) as u64
}

pub fn cp_src_node(cp: u64, i: usize) -> u64 {
    cp + 72 + (i * 24) as u64
}

pub fn cp_src_max(cp: u64, i: usize) -> u64 {
    cp + 80 + (i * 24) as u64
}

pub fn cp_src_start(cp: u64, i: usize) -> usize {
    88 + i * 24
}

pub fn cp_src_end(cp: u64, i: usize) -> usize {
    89 + i * 24
}

pub fn cp_src_mt(cp: u64, i: usize) -> usize {
    90 + i * 24
}


pub fn copy_tree_location(src: &Mas, dst: &mut Mas) {
    dst.node = src.node;
    dst.offset = src.offset;
    dst.min = src.min;
    dst.max = src.max;
    dst.end = src.end;
    dst.depth = src.depth;
}


pub fn mas_wr_walk_traverse(wr: &mut MaWrState, mas: &mut Mas) {
    mas.max = wr.r_max;
    mas.min = wr.r_min;
    mas.node = wr.content;
    mas.offset = 0;
    mas.depth += 1;
}


pub fn mas_wr_new_end(wr: &MaWrState, mas: &Mas) -> u8 {
    let mut new_end = mas.end.wrapping_add(2);
    new_end = new_end.wrapping_sub((wr.offset_end as u8).wrapping_sub(mas.offset));
    if wr.r_min == mas.index {
        new_end = new_end.wrapping_sub(1);
    }
    if wr.end_piv == mas.last {
        new_end = new_end.wrapping_sub(1);
    }
    new_end
}


pub fn mas_sparse_area(mas: &mut Mas, min: u64, max: u64, size: u64, fwd: bool) -> i64 {
    let mut min = min;
    if !mas_is_none(mas) && min == 0 {
        min = min.wrapping_add(1);
        if min > max || max.wrapping_sub(min).wrapping_add(1) < size {
            return -EBUSY;
        }
    }
    if fwd {
        mas.index = min;
        mas.last = min.wrapping_add(size).wrapping_sub(1);
    } else {
        mas.last = max;
        mas.index = max.wrapping_sub(size).wrapping_add(1);
    }
    0
}


pub fn mt_slot_count(enode: u64) -> usize {
    MT_SLOTS[(mte_node_type(enode) as usize).min(MT_SLOTS.len() - 1)]
}

pub fn ma_enode_ptr(x: u64) -> u64 {
    x
}


impl<'a> MapleTree<'a> {
    /// Fresh world over `nodes` + `sheafs`: every node/sheaf is reset, both
    /// tree heads are empty, `ma_flags` set — equivalent to `reset_all` on a
    /// caller arena, i.e. `mt_init_flags`/`MT_INIT` for both trees.
    ///
    /// Panics if `nodes.len() > NP` (pool addresses would collide with the
    /// reserved pseudo-node range).
    pub fn new(nodes: &'a mut [Node], sheafs: &'a mut [Sheaf], flags: u32) -> Self {
        assert!(nodes.len() <= NP, "maple node arena must be <= NP");
        let mut t = MapleTree {
            nodes,
            sheafs,
            spare: [Node::new(), Node::new(), Node::new(), Node::new()],
            ma_flags: [0; 2],
            ma_root: [0; 2],
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
        };
        t.reset_all(flags);
        t
    }

    /// `wrappers.c` reset / `mt_init_flags`: zero every node word, free all
    /// nodes and sheafs, reset both tree heads and the observable counters.
    /// `fail_mask` persists, faithful to the verified harness.
    pub fn reset_all(&mut self, flags: u32) {
        for n in self.nodes.iter_mut() {
            *n = Node::new();
        }
        for n in self.spare.iter_mut() {
            *n = Node::new();
        }
        for s in self.sheafs.iter_mut() {
            *s = Sheaf::new();
        }
        self.ma_flags = [flags, flags];
        self.ma_root = [0, 0];
        self.alloc_ctr = 0;
        self.warn = 0;
        self.warn_once = 0;
    }

    /* ---- node word / byte access over the canonical space ---- */
    /* `p` may carry low encoding bits; the node index is p & ~0xff.
     * Returns a pool index (< nodes.len()) or a reserved index in
     * NP..NP+NEXTRA (`spare`); out-of-range pointers hit the NULLN scratch
     * slot exactly like the verified model. */
    #[inline]
    fn nidx(&self, p: u64) -> usize {
        let raw = (p & !0xff).wrapping_sub(NBASE) >> 8;
        let raw = raw as usize;
        if raw < self.nodes.len() || (NP..NP + NEXTRA).contains(&raw) {
            return raw;
        }
        /* C would dereference an invalid pointer; use scratch */
        NP + 1
    }
    #[inline]
    pub fn w(&self, p: u64, k: usize) -> u64 {
        /* word index k >= 32 is out of node bounds: C reads adjacent
         * memory; route to the scratch slot like wild pointers. */
        let i = if k < 32 { self.nidx(p) } else { NP + 1 };
        if i < self.nodes.len() {
            self.nodes[i].w[k % 32]
        } else {
            self.spare[i - NP].w[k % 32]
        }
    }
    #[inline]
    pub fn sw(&mut self, p: u64, k: usize, v: u64) {
        let i = if k < 32 { self.nidx(p) } else { NP + 1 };
        if i < self.nodes.len() {
            self.nodes[i].w[k % 32] = v;
        } else {
            self.spare[i - NP].w[k % 32] = v;
        }
    }
    /* little-endian byte access inside node p */
    #[inline]
    pub fn b(&self, p: u64, off: usize) -> u8 {
        (self.w(p, off / 8) >> ((off % 8) * 8)) as u8
    }
    #[inline]
    pub fn sb(&mut self, p: u64, off: usize, v: u8) {
        let sh = (off % 8) * 8;
        let m = !(0xffu64 << sh);
        let nv = (self.w(p, off / 8) & m) | ((v as u64) << sh);
        self.sw(p, off / 8, nv);
    }
    #[inline]
    pub fn g32(&self, p: u64, off: usize) -> u32 {
        (self.w(p, off / 8) >> ((off % 8) * 8)) as u32
    }
    #[inline]
    pub fn s32(&mut self, p: u64, off: usize, v: u32) {
        let sh = (off % 8) * 8;
        let m = !(0xffff_ffffu64 << sh);
        let nv = (self.w(p, off / 8) & m) | ((v as u64) << sh);
        self.sw(p, off / 8, nv);
    }
    /* interior pointer access: canonical node_base + byte_offset */
    #[inline]
    pub fn rd(&self, p: u64) -> u64 {
        self.w(p, ((p & 0xff) >> 3) as usize)
    }
    #[inline]
    pub fn wr(&mut self, p: u64, v: u64) {
        self.sw(p, ((p & 0xff) >> 3) as usize, v);
    }
    /* copy n u64 words node-to-node (memcpy semantics) */
    pub fn wcopy(&mut self, dst: u64, src: u64, n: usize) {
        /* C would run off the end of the 32-word node for n > 32;
         * bound the copy like the model's bounded scratch. */
        let n = n.min(32);
        let mut tmp = [0u64; 32];
        for k in 0..n {
            tmp[k] = self.w(src, k);
        }
        for k in 0..n {
            self.sw(dst, k, tmp[k]);
        }
    }
    /* copy n u64 words between interior pointers */
    pub fn rcopy(&mut self, dst: u64, src: u64, n: usize) {
        let n = n.min(32);
        let mut tmp = [0u64; 32];
        for k in 0..n {
            tmp[k] = self.rd(src.wrapping_add(8 * k as u64));
        }
        for k in 0..n {
            self.wr(dst.wrapping_add(8 * k as u64), tmp[k]);
        }
    }
    pub fn wzero(&mut self, p: u64, k: usize, n: usize) {
        let n = n.min(32);
        for i in 0..n {
            self.sw(p, k.wrapping_add(i), 0);
        }
    }
    pub fn rzero(&mut self, p: u64, n: usize) {
        let n = n.min(32);
        for i in 0..n {
            self.wr(p.wrapping_add(8 * i as u64), 0);
        }
    }

    /* ---- warn counters (MAS_BUG_ON / MAS_WARN_ON / warn_on_once) ---- */
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

    /* ---- allocator (mirrors wrappers.c slab + sheaf layer) ---- */
    pub fn node_alloc(&mut self) -> Option<u64> {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            return None;
        }
        for i in 0..self.nodes.len() {
            if !self.nodes[i].used {
                self.nodes[i].used = true;
                return Some(NBASE + i as u64 * 256);
            }
        }
        None
    }
    pub fn node_free(&mut self, p: u64) {
        if p == 0 {
            return;
        }
        let i = self.nidx(p);
        if i < self.nodes.len() {
            self.nodes[i].used = false;
        }
    }
    pub fn sheaf_alloc(&mut self) -> Option<u64> {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            return None;
        }
        for i in 0..self.sheafs.len() {
            if !self.sheafs[i].used {
                self.sheafs[i].used = true;
                return Some(SHBASE + i as u64 * 256);
            }
        }
        None
    }
    pub fn sheaf_free(&mut self, sh: u64) {
        if sh == 0 {
            return;
        }
        let i = self.shidx(sh);
        if self.warn_on(i == usize::MAX) {
            return;
        }
        self.sheafs[i].used = false;
    }
    /// Sheaf pointer -> pool index; a wild `sh` (below `SHBASE`, or past the
    /// caller-owned arena) maps to `usize::MAX` — the `nidx` scratch
    /// convention expressed as "invalid index". Callers must guard it.
    pub fn shidx(&self, sh: u64) -> usize {
        let raw = sh.wrapping_sub(SHBASE) >> 8;
        if raw as usize >= self.sheafs.len() {
            usize::MAX
        } else {
            raw as usize
        }
    }

    /* kmem_cache API */
    pub fn kmem_cache_alloc(&mut self) -> Option<u64> {
        self.node_alloc()
    }
    pub fn kmem_cache_free(&mut self, p: u64) {
        self.node_free(p);
    }
    pub fn kfree(&mut self, p: u64) {
        self.node_free(p);
    }
    pub fn kmem_cache_free_bulk(&mut self, size: usize, p_: &[u64]) {
        for i in 0..size {
            self.node_free(p_[i]);
        }
    }
    pub fn kmem_cache_prefill_sheaf(&mut self, size: usize) -> Option<u64> {
        let sh = self.sheaf_alloc()?;
        let si = self.shidx(sh);
        self.sheafs[si].objs = [0; 32];
        self.sheafs[si].size = 0;
        let mut i = 0usize;
        while i < size && i < 32 {
            match self.node_alloc() {
                Some(n) => self.sheafs[si].objs[i] = n,
                None => {
                    let objs = self.sheafs[si].objs;
                    for j in 0..i {
                        self.node_free(objs[j]);
                    }
                    self.sheaf_free(sh);
                    return None;
                }
            }
            i += 1;
        }
        self.sheafs[si].size = i as u32;
        Some(sh)
    }
    pub fn kmem_cache_refill_sheaf(&mut self, sheaf: u64, size: usize) -> i64 {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return -EINVAL;
        }
        let mut n = self.sheafs[si].size as usize;
        while n < size && n < 32 {
            match self.node_alloc() {
                Some(nn) => {
                    self.sheafs[si].objs[n] = nn;
                    n += 1;
                }
                None => {
                    self.sheafs[si].size = n as u32;
                    return -ENOMEM;
                }
            }
        }
        self.sheafs[si].size = n as u32;
        0
    }
    pub fn kmem_cache_return_sheaf(&mut self, sheaf: u64) {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return;
        }
        let n = self.sheafs[si].size as usize;
        let objs = self.sheafs[si].objs;
        for i in 0..n {
            self.node_free(objs[i]);
        }
        self.sheafs[si].size = 0;
        self.sheaf_free(sheaf);
    }
    pub fn kmem_cache_alloc_from_sheaf(&mut self, sheaf: u64) -> u64 {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return 0;
        }
        let n = self.sheafs[si].size as usize;
        if n == 0 {
            self.warn += 1;
            return 0;
        }
        let v = self.sheafs[si].objs[n - 1];
        self.sheafs[si].objs[n - 1] = 0;
        self.sheafs[si].size = (n - 1) as u32;
        v
    }
    pub fn sheaf_size(&self, sheaf: u64) -> u32 {
        let si = self.shidx(sheaf);
        if si == usize::MAX {
            return 0;
        }
        self.sheafs[si].size
    }

    /* ---- tree-head access (mt canonical id -> tidx) ---- */
    pub fn tidx(&self, mt: u64) -> usize {
        ((mt >> 12) & 1) as usize
    }
    pub fn tflags(&self, mt: u64) -> u32 {
        self.ma_flags[self.tidx(mt)]
    }
    pub fn set_tflags(&mut self, mt: u64, v: u32) {
        let i = self.tidx(mt);
        self.ma_flags[i] = v;
    }
    pub fn troot(&self, mt: u64) -> u64 {
        self.ma_root[self.tidx(mt)]
    }
    pub fn set_troot(&mut self, mt: u64, v: u64) {
        let i = self.tidx(mt);
        self.ma_root[i] = v;
    }

pub fn mt_height(&self, mt: u64) -> u32 {
    (self.tflags(mt) & MT_FLAGS_HEIGHT_MASK) >> MT_FLAGS_HEIGHT_OFFSET
}

pub fn mt_external_lock(&self, mt: u64) -> bool {
    (self.tflags(mt) & MT_FLAGS_LOCK_MASK) == MT_FLAGS_LOCK_EXTERN
}

pub fn mt_lock_is_held(&self, _mt: u64) -> bool {
    true
}

pub fn mt_write_lock_is_held(&self, _mt: u64) -> bool {
    true
}

/* ================= maple_tree.c translation ================= */


pub fn mt_alloc_one(&mut self, _gfp: u32) -> Option<u64> {
    self.kmem_cache_alloc()
}

pub fn mt_free_bulk(&mut self, size: usize, nodes: &[u64]) {
    self.kmem_cache_free_bulk(size, nodes);
}

pub fn mt_return_sheaf(&mut self, sheaf: u64) {
    self.kmem_cache_return_sheaf(sheaf);
}

pub fn mt_get_sheaf(&mut self, _gfp: u32, count: usize) -> Option<u64> {
    self.kmem_cache_prefill_sheaf(count)
}

pub fn mt_refill_sheaf(&mut self, _gfp: u32, sheaf: u64, size: usize) -> i64 {
    self.kmem_cache_refill_sheaf(sheaf, size)
}


pub fn ma_free_rcu(&mut self, node: u64) {
    let par = self.w(node, 0);
    self.warn_on(par != ma_parent_ptr(node));
    self.kfree(node);
}


pub fn mt_set_height(&mut self, mt: u64, height: u8) {
    let mut nf = self.tflags(mt);
    nf &= !MT_FLAGS_HEIGHT_MASK;
    nf |= (height as u32) << MT_FLAGS_HEIGHT_OFFSET;
    self.set_tflags(mt, nf);
}

pub fn mas_mt_height(&self, mas: &Mas) -> u32 {
    self.mt_height(mas.tree)
}

pub fn mt_attr(&self, mt: u64) -> u32 {
    self.tflags(mt) & !MT_FLAGS_HEIGHT_MASK
}


pub fn mte_set_node_dead(&mut self, mn: u64) {
    let n = mte_to_node(mn);
    let v = ma_parent_ptr(n);
    self.sw(n, 0, v);
}

pub fn ma_init_slot(&mut self, slot: u64, mn: u64, mt: u8) {
    let v = mt_mk_node(mn, mt);
    self.wr(slot, v);
}

pub fn ma_is_root(&self, node: u64) -> bool {
    self.w(node, 0) & MA_ROOT_PARENT != 0
}

pub fn mte_is_root(&self, node: u64) -> bool {
    self.ma_is_root(mte_to_node(node))
}

pub fn mt_is_alloc(&self, mt: u64) -> bool {
    self.tflags(mt) & MT_FLAGS_ALLOC_RANGE != 0
}

pub fn mas_parent_type(&mut self, mas: &Mas, enode: u64) -> u8 {
    let mut p_type = self.w(mte_to_node(enode), 0);
    if self.warn_on(p_type & MAPLE_PARENT_ROOT != 0) {
        return 0;
    }
    p_type &= MAPLE_NODE_MASK;
    p_type &= !mte_parent_slot_mask(p_type);
    if p_type == MAPLE_PARENT_RANGE64 {
        if self.mt_is_alloc(mas.tree) {
            return MAPLE_ARANGE_64;
        }
        return MAPLE_RANGE_64;
    }
    0
}

pub fn mte_parent_slot(&self, enode: u64) -> u32 {
    let val = self.w(mte_to_node(enode), 0);
    if val & MA_ROOT_PARENT != 0 {
        return 0;
    }
    ((val & MAPLE_PARENT_16B_SLOT_MASK) >> mte_parent_shift(val)) as u32
}

pub fn mte_parent(&self, enode: u64) -> u64 {
    self.w(mte_to_node(enode), 0) & !MAPLE_NODE_MASK
}

pub fn ma_dead_node(&self, node: u64) -> bool {
    let parent = self.w(node, 0) & !MAPLE_NODE_MASK;
    parent == node
}

pub fn mte_dead_node(&self, enode: u64) -> bool {
    self.ma_dead_node(mte_to_node(enode))
}

pub fn mas_safe_pivot(&self, mas: &Mas, pivots: u64, piv: u8, t: u8) -> u64 {
    if piv as usize >= MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] {
        return mas.max;
    }
    self.rd(pivots + 8 * piv as u64)
}

pub fn mas_safe_min(&self, mas: &Mas, pivots: u64, offset: u8) -> u64 {
    if offset != 0 {
        return self.rd(pivots + 8u64.wrapping_mul((offset as u64).wrapping_sub(1))) + 1;
    }
    mas.min
}

pub fn mte_set_pivot(&mut self, mn: u64, piv: u8, val: u64) {
    let node = mte_to_node(mn);
    let t = mte_node_type(mn);
    match t {
        MAPLE_RANGE_64 | MAPLE_LEAF_64 | MAPLE_ARANGE_64 => {
            self.wr(node + 8 + 8 * piv as u64, val);
        }
        _ => {}
    }
}

pub fn mt_write_locked(&self, mt: u64) -> bool {
    if self.mt_external_lock(mt) {
        self.mt_write_lock_is_held(mt)
    } else {
        true
    }
}

pub fn mt_locked(&self, mt: u64) -> bool {
    if self.mt_external_lock(mt) {
        self.mt_lock_is_held(mt)
    } else {
        true
    }
}

pub fn mt_slot(&self, _mt: u64, slots: u64, offset: u8) -> u64 {
    self.rd(slots + 8 * offset as u64)
}

pub fn mt_slot_locked(&self, _mt: u64, slots: u64, offset: u8) -> u64 {
    self.rd(slots + 8 * offset as u64)
}

pub fn mas_slot_locked(&self, mas: &Mas, slots: u64, offset: u8) -> u64 {
    self.mt_slot_locked(mas.tree, slots, offset)
}

pub fn mas_slot(&self, mas: &Mas, slots: u64, offset: u8) -> u64 {
    self.mt_slot(mas.tree, slots, offset)
}


pub fn mas_root(&self, mas: &Mas) -> u64 {
    self.troot(mas.tree)
}

pub fn mt_root_locked(&self, mt: u64) -> u64 {
    self.troot(mt)
}

pub fn mas_root_locked(&self, mas: &Mas) -> u64 {
    self.mt_root_locked(mas.tree)
}

/* metadata view: ma64.meta at +240, mr64.meta at +248; {end,gap} u8s */

pub fn ma_set_meta(&mut self, mn: u64, mt: u8, offset: u8, end: u8) {
    let m = ma_meta(mn, mt);
    self.sb(m, (m & 0xff) as usize, end);
    self.sb(m, ((m & 0xff) + 1) as usize, offset);
}

pub fn ma_meta_end(&self, mn: u64, mt: u8) -> u8 {
    let m = ma_meta(mn, mt);
    self.b(m, (m & 0xff) as usize)
}

pub fn ma_meta_gap(&self, mn: u64) -> u8 {
    /* C reads mn->ma64.meta.gap regardless of type: byte 241 */
    self.b(mn, 241)
}

pub fn ma_set_meta_gap(&mut self, mn: u64, mt: u8, offset: u8) {
    let m = ma_meta(mn, mt);
    self.sb(m, ((m & 0xff) + 1) as usize, offset);
}


pub fn mas_set_parent_slots(
    &mut self,
    mas: &Mas,
    parent: u64,
    slots: u64,
    start_slot: u8,
    count: u8,
) {
    let p_type = mte_node_type(parent);
    if p_type != MAPLE_RANGE_64 && p_type != MAPLE_ARANGE_64 {
        self.warn += 1; /* MAS_BUG_ON */
    }
    let shift = MAPLE_PARENT_SLOT_SHIFT;
    let type_ = MAPLE_PARENT_RANGE64;
    let val = parent & !MAPLE_NODE_MASK;
    for i in 0..count {
        let pval = val | (((start_slot + i) as u64) << shift) | type_;
        let child = self.mt_slot_locked(mas.tree, slots, i);
        let v = ma_parent_ptr(pval);
        self.sw(mte_to_node(child), 0, v);
    }
}


pub fn mat_add(&mut self, mat: &mut MaTopiary, dead_enode: u64) {
    self.mte_set_node_dead(dead_enode);
    self.sw(mte_to_mat(dead_enode), 1, 0); /* ->next = NULL */
    if mat.tail == 0 {
        mat.tail = dead_enode;
        mat.head = dead_enode;
        return;
    }
    self.sw(mte_to_mat(mat.tail), 1, dead_enode);
    mat.tail = dead_enode;
}


pub fn mt_in_rcu(&self, mt: u64) -> bool {
    self.tflags(mt) & MT_FLAGS_USE_RCU != 0
}


pub fn mas_mat_destroy(&mut self, mas: &Mas, mat: &mut MaTopiary) {
    let in_rcu = self.mt_in_rcu(mas.tree);
    let mut __limw = 0u64;
    while mat.head != 0 {
        __limw += 1;
        if __limw > MAS_ITER_LIMIT { self.warn += 1; return; }
        let next = self.w(mte_to_mat(mat.head), 1);
        let node = mte_to_node(mat.head);
        self.mt_destroy_walk(mat.head, mas.tree, !in_rcu);
        if in_rcu {
            self.mt_free_walk(node); /* call_rcu -> immediate */
        }
        mat.head = next;
    }
}


pub fn mas_descend(&self, mas: &mut Mas) {
    let node = mas_mn(mas);
    let t = mte_node_type(mas.node);
    let pivots = ma_pivots(node, t);
    let slots = ma_slots(node, t);
    if mas.offset != 0 {
        mas.min = self.rd(pivots + 8u64.wrapping_mul((mas.offset as u64).wrapping_sub(1))) + 1;
    }
    mas.max = self.mas_safe_pivot(mas, pivots, mas.offset, t);
    mas.node = self.mas_slot(mas, slots, mas.offset);
}


pub fn mas_ascend(&mut self, mas: &mut Mas) -> i32 {
    let mut a_node = mas_mn(mas);
    if self.ma_is_root(a_node) {
        mas.offset = 0;
        return 0;
    }
    let p_node = self.mte_parent(mas.node);
    if a_node == p_node {
        return 1;
    }
    let mut a_type = self.mas_parent_type(mas, mas.node);
    mas.offset = self.mte_parent_slot(mas.node) as u8;
    let mut a_enode = mt_mk_node(p_node, a_type);
    if p_node != self.mte_parent(mas.node) {
        return 1;
    }
    mas.node = a_enode;
    if self.mte_is_root(a_enode) {
        mas.max = ULONG_MAX;
        mas.min = 0;
        return 0;
    }
    let mut min = 0u64;
    let mut max = ULONG_MAX;
    let mut set_max = false;
    let mut set_min = false;
    if mas.offset == 0 {
        min = mas.min;
        set_min = true;
    }
    if mas.max == ULONG_MAX {
        set_max = true;
    }
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        let p_enode = a_enode;
        a_type = self.mas_parent_type(mas, p_enode);
        a_node = self.mte_parent(p_enode);
        let a_slot = self.mte_parent_slot(p_enode) as u8;
        a_enode = mt_mk_node(a_node, a_type);
        let pivots = ma_pivots(a_node, a_type);
        if self.ma_dead_node(a_node) {
            return 1;
        }
        if !set_min && a_slot != 0 {
            set_min = true;
            min = self.rd(pivots + 8u64.wrapping_mul((a_slot as u64).wrapping_sub(1))) + 1;
        }
        if !set_max && (a_slot as usize) < MT_PIVOTS[(a_type as usize).min(MT_PIVOTS.len() - 1)] {
            set_max = true;
            max = self.rd(pivots + 8 * a_slot as u64);
        }
        if self.ma_dead_node(a_node) {
            return 1;
        }
        if self.ma_is_root(a_node) {
            break;
        }
        if set_min && set_max {
            break;
        }
    }
    mas.max = max;
    mas.min = min;
    0
}


pub fn mas_pop_node(&mut self, mas: &mut Mas) -> Option<u64> {
    let ret;
    if mas.alloc != 0 {
        ret = mas.alloc;
        mas.alloc = 0;
    } else {
        if self.warn_on_once(0, mas.sheaf == 0) {
            return None;
        }
        ret = self.kmem_cache_alloc_from_sheaf(mas.sheaf);
        if ret == 0 {
            return None;
        }
    }
    self.wzero(ret, 0, 32);
    Some(ret)
}


pub fn mas_alloc_nodes(&mut self, mas: &mut Mas, gfp: u32) {
    if mas.node_request == 0 {
        return;
    }
    if mas.node_request == 1 && mas.sheaf == 0 {
        if mas.alloc != 0 {
            return;
        }
        mas.alloc = match self.mt_alloc_one(gfp) {
            Some(x) => x,
            None => {
                mas_set_err(mas, -ENOMEM);
                return;
            }
        };
        mas.node_request = 0;
        return;
    }
    /* use_sheaf: */
    if mas.alloc != 0 {
        self.kfree(mas.alloc);
        mas.alloc = 0;
    }
    if mas.sheaf != 0 {
        let refill = mas.node_request as usize;
        if self.sheaf_size(mas.sheaf) >= refill as u32 {
            mas.node_request = 0;
            return;
        }
        if self.mt_refill_sheaf(gfp, mas.sheaf, refill) != 0 {
            mas_set_err(mas, -ENOMEM);
            return;
        }
        mas.node_request = 0;
        return;
    }
    mas.sheaf = match self.mt_get_sheaf(gfp, mas.node_request as usize) {
        Some(s) => s,
        None => {
            mas_set_err(mas, -ENOMEM);
            return;
        }
    };
    mas.node_request = 0;
}


pub fn mas_empty_nodes(&mut self, mas: &mut Mas) {
    mas.node_request = 0;
    if mas.sheaf != 0 {
        self.mt_return_sheaf(mas.sheaf);
        mas.sheaf = 0;
    }
    if mas.alloc != 0 {
        self.kfree(mas.alloc);
        mas.alloc = 0;
    }
}


pub fn mas_free(&mut self, _mas: &Mas, used: u64) {
    self.ma_free_rcu(mte_to_node(used));
}


pub fn mas_start(&mut self, mas: &mut Mas) -> u64 {
    if mas_is_start(mas) {
        mas.min = 0;
        mas.max = ULONG_MAX;
        let mut __lim = 0u64;
        loop {
            __lim += 1;
            if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
            mas.depth = 0;
            let root = self.mas_root(mas);
            if xa_is_node(root) {
                mas.depth = 0;
                mas.status = MA_ACTIVE;
                mas.node = mte_safe_root(root);
                mas.offset = 0;
                if self.mte_dead_node(mas.node) {
                    continue; /* retry */
                }
                return 0;
            }
            mas.node = 0;
            if root == 0 {
                mas.status = MA_NONE;
                mas.offset = MAPLE_NODE_SLOTS as u8;
                return 0;
            }
            mas.status = MA_ROOT;
            mas.offset = MAPLE_NODE_SLOTS as u8;
            if mas.index > 0 {
                return 0;
            }
            return root;
        }
    }
    0
}


pub fn ma_data_end(&self, node: u64, t: u8, pivots: u64, max: u64) -> u8 {
    if pivots == 0 {
        return 0;
    }
    if t == MAPLE_ARANGE_64 {
        return self.ma_meta_end(node, t);
    }
    let offset = (MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] - 1) as u8;
    if self.rd(pivots + 8 * offset as u64) == 0 {
        return self.ma_meta_end(node, t);
    }
    if self.rd(pivots + 8 * offset as u64) == max {
        return offset;
    }
    MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] as u8
}


pub fn mas_data_end(&self, mas: &Mas) -> u8 {
    let t = mte_node_type(mas.node);
    let node = mas_mn(mas);
    if t == MAPLE_ARANGE_64 {
        return self.ma_meta_end(node, t);
    }
    let pivots = ma_pivots(node, t);
    if pivots == 0 || self.ma_dead_node(node) {
        return 0;
    }
    let offset = (MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] - 1) as u8;
    if self.rd(pivots + 8 * offset as u64) == 0 {
        return self.ma_meta_end(node, t);
    }
    if self.rd(pivots + 8 * offset as u64) == mas.max {
        return offset;
    }
    MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] as u8
}


pub fn wr_mas_setup(&self, wr: &mut MaWrState, mas: &Mas) {
    wr.node = mas_mn(mas);
    wr.type_ = mte_node_type(mas.node);
    wr.pivots = ma_pivots(wr.node, wr.type_);
    wr.slots = ma_slots(wr.node, wr.type_);
    wr.r_min = self.mas_safe_min(mas, wr.pivots, mas.offset);
    wr.r_max = self.mas_safe_pivot(mas, wr.pivots, mas.offset, wr.type_);
}


pub fn wr_mas_ascend(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    self.mas_ascend(mas);
    self.wr_mas_setup(wr, mas);
    mas.end = self.ma_data_end(wr.node, wr.type_, wr.pivots, mas.max);
    wr.end_piv = wr.r_max;
    wr.offset_end = mas.offset as u32;
}


pub fn ma_leaf_max_gap(
    &self,
    mn: u64,
    mt: u8,
    min: u64,
    max: u64,
    pivots: u64,
    slots: u64,
) -> u64 {
    let mut max_gap = 0u64;
    if ma_is_dense(mt) {
        let mut gap = 0u64;
        for i in 0..MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] {
            if self.rd(slots + 8 * i as u64) != 0 {
                if gap > max_gap {
                    max_gap = gap;
                }
                gap = 0;
            } else {
                gap += 1;
            }
        }
        if gap > max_gap {
            max_gap = gap;
        }
        return max_gap;
    }
    let mut i: u8;
    if self.rd(slots) == 0 {
        max_gap = self.rd(pivots).wrapping_sub(min).wrapping_add(1);
        i = 2;
    } else {
        i = 1;
    }
    let max_piv = self.ma_data_end(mn, mt, pivots, max).wrapping_sub(1);
    if max == ULONG_MAX && self.rd(slots + 8 * (max_piv as u64 + 1)) == 0 {
        let gap = ULONG_MAX - self.rd(pivots + 8 * max_piv as u64);
        if gap > max_gap {
            max_gap = gap;
        }
        if max_gap > self.rd(pivots + 8 * max_piv as u64).wrapping_sub(min) {
            return max_gap;
        }
    }
    while i <= max_piv {
        if self.rd(slots + 8 * i as u64) != 0 {
            i += 1;
            continue;
        }
        let pstart = self.rd(pivots + 8u64.wrapping_mul((i as u64).wrapping_sub(1)));
        let gap = self.rd(pivots + 8 * i as u64).wrapping_sub(pstart);
        if gap > max_gap {
            max_gap = gap;
        }
        i += 2;
    }
    max_gap
}


pub fn mas_leaf_max_gap(&self, mas: &Mas) -> u64 {
    let mn = mas_mn(mas);
    let mt = mte_node_type(mas.node);
    let slots = ma_slots(mn, mt);
    let pivots = ma_pivots(mn, mt);
    self.ma_leaf_max_gap(mn, mt, mas.min, mas.max, pivots, slots)
}


pub fn ma_max_gap(&self, node: u64, gaps: u64, mt: u8, off: &mut u8) -> u64 {
    let mut max_gap = 0u64;
    let mut offset = self.ma_meta_end(node, mt);
    let mut i = offset;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { return 0; }
        let g = self.rd(gaps + 8 * i as u64);
        if g > max_gap {
            max_gap = g;
            offset = i;
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    *off = offset;
    max_gap
}


pub fn mas_max_gap(&mut self, mas: &Mas) -> u64 {
    let mt = mte_node_type(mas.node);
    if ma_is_leaf(mt) {
        return self.mas_leaf_max_gap(mas);
    }
    let node = mas_mn(mas);
    if mt != MAPLE_ARANGE_64 {
        self.warn += 1; /* MAS_BUG_ON */
    }
    let offset = self.ma_meta_gap(node);
    let gaps = ma_gaps(node, mt);
    self.rd(gaps + 8 * offset as u64)
}


pub fn mas_parent_gap(&mut self, mas: &mut Mas, offset: u8, new: u64) {
    let mut meta_gap: u64;
    let mut pnode = self.mte_parent(mas.node);
    let mut pmt = self.mas_parent_type(mas, mas.node);
    let mut penode = mt_mk_node(pnode, pmt);
    let mut pgaps = ma_gaps(pnode, pmt);
    let mut offset = offset;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        /* ascend: */
        if pmt != MAPLE_ARANGE_64 {
            self.warn += 1; /* MAS_BUG_ON */
        }
        let mut meta_offset = self.ma_meta_gap(pnode);
        meta_gap = self.rd(pgaps + 8 * meta_offset as u64);
        self.wr(pgaps + 8 * offset as u64, new);
        if meta_gap == new {
            return;
        }
        let mut newv = new;
        if offset != meta_offset {
            if meta_gap > new {
                return;
            }
            self.ma_set_meta_gap(pnode, pmt, offset);
        } else if new < meta_gap {
            newv = self.ma_max_gap(pnode, pgaps, pmt, &mut meta_offset);
            self.ma_set_meta_gap(pnode, pmt, meta_offset);
        }
        if self.ma_is_root(pnode) {
            return;
        }
        let _ = newv;
        pnode = self.mte_parent(penode);
        pmt = self.mas_parent_type(mas, penode);
        pgaps = ma_gaps(pnode, pmt);
        offset = self.mte_parent_slot(penode) as u8;
        penode = mt_mk_node(pnode, pmt);
        /* goto ascend */
    }
}


pub fn mas_update_gap_known(&mut self, mas: &mut Mas, gap: u64) {
    let pslot = self.mte_parent_slot(mas.node) as u8;
    let pgaps = ma_gaps(self.mte_parent(mas.node), self.mas_parent_type(mas, mas.node));
    let p_gap = self.rd(pgaps + 8 * pslot as u64);
    if p_gap != gap {
        self.mas_parent_gap(mas, pslot, gap);
    }
}


pub fn mas_update_gap(&mut self, mas: &mut Mas) {
    if !self.mt_is_alloc(mas.tree) {
        return;
    }
    if self.mte_is_root(mas.node) {
        return;
    }
    let max_gap = self.mas_max_gap(mas);
    self.mas_update_gap_known(mas, max_gap);
}


pub fn mas_adopt_children(&mut self, mas: &Mas, parent: u64) {
    let t = mte_node_type(parent);
    let node = mte_to_node(parent);
    let slots = ma_slots(node, t);
    let pivots = ma_pivots(node, t);
    let end = self.ma_data_end(node, t, pivots, mas.max);
    self.mas_set_parent_slots(mas, parent, slots, 0, end + 1);
}


pub fn mas_put_in_tree(&mut self, mas: &mut Mas, old_enode: u64, new_height: u8) {
    if self.mte_is_root(mas.node) {
        let par = ma_parent_ptr(mas_tree_parent(mas));
        let n = mas_mn(mas);
        self.sw(n, 0, par);
        let r = mte_mk_root(mas.node);
        self.set_troot(mas.tree, r);
        self.mt_set_height(mas.tree, new_height);
    } else {
        let offset = self.mte_parent_slot(mas.node) as u8;
        let slots = ma_slots(self.mte_parent(mas.node), self.mas_parent_type(mas, mas.node));
        self.wr(slots + 8 * offset as u64, mas.node);
    }
    self.mte_set_node_dead(old_enode);
}


pub fn mas_replace_node(&mut self, mas: &mut Mas, old_enode: u64, new_height: u8) {
    self.mas_put_in_tree(mas, old_enode, new_height);
    self.mas_free(mas, old_enode);
}


pub fn mas_find_child(&mut self, mas: &mut Mas, child: &mut Mas) -> bool {
    let mt = mte_node_type(mas.node);
    let node = mas_mn(mas);
    let slots = ma_slots(node, mt);
    let pivots = ma_pivots(node, mt);
    let end = self.ma_data_end(node, mt, pivots, mas.max);
    let mut offset = mas.offset;
    while offset <= end {
        let entry = self.mas_slot_locked(mas, slots, offset);
        if self.mte_parent(entry) == node {
            *child = mas.clone();
            mas.offset = offset.wrapping_add(1);
            child.offset = offset;
            self.mas_descend(child);
            child.offset = 0;
            return true;
        }
        offset += 1;
    }
    false
}


pub fn mas_leaf_set_meta(&mut self, node: u64, mt: u8, end: u8) {
    if (end as usize) < MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] - 1 {
        self.ma_set_meta(node, mt, 0, end);
    }
}


pub fn mas_prev_sibling(&mut self, mas: &mut Mas) -> bool {
    let p_slot = self.mte_parent_slot(mas.node);
    if p_slot == 0 {
        return false;
    }
    self.mas_ascend(mas);
    mas.offset = p_slot.wrapping_sub(1) as u8;
    self.mas_descend(mas);
    true
}


pub fn mas_next_sibling(&mut self, mas: &mut Mas) -> bool {
    if self.mte_is_root(mas.node) {
        return false;
    }
    let mut parent = mas.clone();
    self.mas_ascend(&mut parent);
    parent.offset = (self.mte_parent_slot(mas.node) + 1) as u8;
    if parent.offset > self.mas_data_end(&parent) {
        return false;
    }
    *mas = parent;
    self.mas_descend(mas);
    true
}


pub fn mas_wr_node_walk(&self, wr: &mut MaWrState, mas: &mut Mas) {
    if ma_is_dense(wr.type_) {
        wr.r_max = mas.index;
        wr.r_min = mas.index;
        mas.offset = mas.min as u8;
        mas.index = mas.min;
        return;
    }
    wr.node = mas_mn(mas);
    wr.pivots = ma_pivots(wr.node, wr.type_);
    let count = self.ma_data_end(wr.node, wr.type_, wr.pivots, mas.max);
    mas.end = count;
    let mut offset = mas.offset;
    while offset < count && mas.index > self.rd(wr.pivots + 8 * offset as u64) {
        offset += 1;
    }
    wr.r_max = if offset < count {
        self.rd(wr.pivots + 8 * offset as u64)
    } else {
        mas.max
    };
    wr.r_min = self.mas_safe_min(mas, wr.pivots, offset);
    wr.offset_end = offset as u32;
    mas.offset = offset;
}


pub fn rebalance_sib(&self, parent: &Mas, sib: &mut Mas) {
    *sib = parent.clone();
    if sib.offset < sib.end {
        sib.offset += 1;
    } else {
        sib.offset = sib.offset.wrapping_sub(1);
    }
    self.mas_descend(sib);
    sib.end = self.mas_data_end(sib);
}


pub fn spanning_sib(&mut self, l_mas: &Mas, r_mas: &Mas, nneighbour: &mut Mas) {
    let mut l_tmp = l_mas.clone();
    let mut r_tmp = r_mas.clone();
    let mut depth: u8 = 0;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        self.mas_ascend(&mut r_tmp);
        self.mas_ascend(&mut l_tmp);
        depth += 1;
        if r_tmp.offset < self.mas_data_end(&r_tmp) {
            r_tmp.offset += 1;
            self.mas_descend(&mut r_tmp);
            r_tmp.offset = 0;
            depth -= 1;
            while depth != 0 {
                self.mas_descend(&mut r_tmp);
                depth -= 1;
            }
            r_tmp.end = self.mas_data_end(&r_tmp);
            *nneighbour = r_tmp;
            return;
        } else if l_tmp.offset != 0 {
            l_tmp.offset = l_tmp.offset.wrapping_sub(1);
            let mut __lim = 0u64;
            loop {
                __lim += 1;
                if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
                self.mas_descend(&mut l_tmp);
                l_tmp.offset = self.mas_data_end(&l_tmp);
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            l_tmp.end = l_tmp.offset;
            *nneighbour = l_tmp;
            return;
        }
        if self.mte_is_root(r_tmp.node) {
            break;
        }
    }
    self.warn_on_once(1, true);
}


pub fn mas_topiary_node(&mut self, mas: &Mas, tmp_mas: &Mas, _in_rcu: bool) {
    if mas_is_none(tmp_mas) {
        return;
    }
    let enode = tmp_mas.node;
    let tmp = mte_to_node(enode);
    self.mte_set_node_dead(enode);
    self.ma_free_rcu(tmp);
    let _ = mas;
}


pub fn mas_topiary_replace(&mut self, mas: &mut Mas, old_enode: u64, new_height: u8) {
    let mut tmp = [Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0)];
    let mut tmp_next = [Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0)];
    let mut subtrees = MaTopiary::new(mas.tree);
    self.mas_put_in_tree(mas, old_enode, new_height);
    tmp[0] = mas.clone();
    tmp[0].offset = 0;
    tmp[1].status = MA_NONE;
    tmp[2].status = MA_NONE;
    let mut __limw = 0u64;
    while !mte_is_leaf(tmp[0].node) {
        __limw += 1;
        if __limw > MAS_ITER_LIMIT { self.warn += 1; return; }
        let mut n = 0usize;
        for i in 0..3 {
            if mas_is_none(&tmp[i]) {
                continue;
            }
            while n < 3 {
                let (mut ti, mut tn) = (tmp[i].clone(), tmp_next[n].clone());
                let found = self.mas_find_child(&mut ti, &mut tn);
                tmp[i] = ti;
                tmp_next[n] = tn;
                if !found {
                    break;
                }
                n += 1;
            }
            self.mas_adopt_children(&tmp[i], tmp[i].node);
        }
        if n == 0 {
            self.warn += 1; /* MAS_WARN_ON */
            break;
        }
        while n < 3 {
            tmp_next[n].status = MA_NONE;
            n += 1;
        }
        for i in 0..3 {
            tmp[i] = tmp_next[i].clone();
        }
    }
    if mte_is_leaf(old_enode) {
        self.mas_free(mas, old_enode);
        return;
    }
    tmp[0] = mas.clone();
    tmp[0].offset = 0;
    tmp[0].node = old_enode;
    tmp[1].status = MA_NONE;
    tmp[2].status = MA_NONE;
    let in_rcu = self.mt_in_rcu(mas.tree);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        let mut n = 0usize;
        for i in 0..3 {
            if mas_is_none(&tmp[i]) {
                continue;
            }
            while n < 3 {
                let (mut ti, mut tn) = (tmp[i].clone(), tmp_next[n].clone());
                let found = self.mas_find_child(&mut ti, &mut tn);
                tmp[i] = ti;
                tmp_next[n] = tn;
                if !found {
                    break;
                }
                /* tmp_next->index == tmp_next[0].index in C */
                if tmp_next[n].min >= tmp_next[0].index && tmp_next[n].max <= tmp_next[0].last {
                    let dead = tmp_next[n].node;
                    self.mat_add(&mut subtrees, dead);
                    tmp_next[n].status = MA_NONE;
                } else {
                    n += 1;
                }
            }
        }
        if n == 0 {
            self.warn += 1; /* MAS_WARN_ON */
            break;
        }
        while n < 3 {
            tmp_next[n].status = MA_NONE;
            n += 1;
        }
        for i in 0..3 {
            let tm = tmp[i].clone();
            self.mas_topiary_node(mas, &tm, in_rcu);
            tmp[i] = tmp_next[i].clone();
        }
        if mte_is_leaf(tmp[0].node) {
            break;
        }
    }
    for i in 0..3 {
        let tm = tmp[i].clone();
        self.mas_topiary_node(mas, &tm, in_rcu);
    }
    self.mas_mat_destroy(mas, &mut subtrees);
}


pub fn node_copy(
    &mut self,
    mas: &Mas,
    src: u64,
    start: u8,
    size: u8,
    s_max: u64,
    s_mt: u8,
    dst: u64,
    d_start: u8,
    d_mt: u8,
) -> u64 {
    let d_slots = ma_slots(dst, d_mt) + 8 * d_start as u64;
    let d_pivots = ma_pivots(dst, d_mt) + 8 * d_start as u64;
    let s_slots = ma_slots(src, s_mt) + 8 * start as u64;
    let s_pivots = ma_pivots(src, s_mt) + 8 * start as u64;
    self.rcopy(d_slots, s_slots, size as usize);
    if !ma_is_leaf(d_mt) && s_mt == MAPLE_COPY {
        self.mas_set_parent_slots(mas, mt_mk_node(dst, d_mt), d_slots, d_start, size);
    }
    let d_gaps = ma_gaps(dst, d_mt);
    if d_gaps != 0 {
        let s_gaps = ma_gaps(src, s_mt) + 8 * start as u64;
        self.rcopy(d_gaps + 8 * d_start as u64, s_gaps, size as usize);
    }
    let d_max;
    if (start as usize).wrapping_add(size as usize).wrapping_sub(1) < MT_PIVOTS[(s_mt as usize).min(MT_PIVOTS.len() - 1)] {
        d_max = self.rd(s_pivots + 8u64.wrapping_mul((size as u64).wrapping_sub(1)));
    } else {
        d_max = s_max;
    }
    if (d_start as usize + size as usize) <= MT_PIVOTS[(d_mt as usize).min(MT_PIVOTS.len() - 1)] {
        self.wr(d_pivots + 8u64.wrapping_mul((size as u64).wrapping_sub(1)), d_max);
    }
    let size = size - 1;
    if size != 0 {
        self.rcopy(d_pivots, s_pivots, size as usize);
    }
    d_max
}


pub fn node_finalise(&mut self, node: u64, mt: u8, end: u8) {
    let max_end = MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] as u8;
    let gaps = ma_gaps(node, mt);
    if end < max_end - 1 {
        let mut size = max_end - end;
        self.rzero(ma_slots(node, mt) + 8 * end as u64, size as usize);
        if gaps != 0 {
            self.rzero(gaps + 8 * end as u64, size as usize);
        }
        size -= 1;
        if size != 0 {
            self.rzero(ma_pivots(node, mt) + 8 * end as u64, size as usize);
        }
    }
    let mut gap_slot = 0u8;
    if gaps != 0 && !ma_is_leaf(mt) {
        let mut max_gap = 0u64;
        let mut i = 0u8;
        while i <= end {
            let g = self.rd(gaps + 8 * i as u64);
            if g > max_gap {
                gap_slot = i;
                max_gap = g;
            }
            i += 1;
        }
    }
    if mt == MAPLE_ARANGE_64 {
        self.ma_set_meta(node, mt, gap_slot, end - 1);
    } else if end <= max_end - 1 {
        self.ma_set_meta(node, mt, gap_slot, end - 1);
    }
}


pub fn mtree_range_walk(&mut self, mas: &mut Mas) -> u64 {
    let mut next = mas.node;
    let mut min = mas.min;
    let mut max = mas.max;
    let mut last;
    let mut offset;
    let mut end;
    let mut prev_min;
    let mut prev_max;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        last = next;
        let node = mte_to_node(next);
        let t = mte_node_type(next);
        let pivots = ma_pivots(node, t);
        end = self.ma_data_end(node, t, pivots, max);
        prev_min = min;
        prev_max = max;
        if self.rd(pivots) >= mas.index {
            offset = 0;
            max = self.rd(pivots);
        } else {
            let mut off = 1u8;
            while off < end {
                if self.rd(pivots + 8 * off as u64) >= mas.index {
                    max = self.rd(pivots + 8 * off as u64);
                    break;
                }
                off += 1;
            }
            offset = off;
            min = self.rd(pivots + 8u64.wrapping_mul((offset as u64).wrapping_sub(1))) + 1;
        }
        let slots = ma_slots(node, t);
        next = self.mt_slot(mas.tree, slots, offset);
        if self.ma_dead_node(node) {
            self.mas_reset(mas);
            return 0;
        }
        if ma_is_leaf(t) {
            break;
        }
    }
    mas.end = end;
    mas.offset = offset;
    mas.index = min;
    mas.last = max;
    mas.min = prev_min;
    mas.max = prev_max;
    mas.node = last;
    next
}


pub fn mas_wmb_replace(&mut self, mas: &mut Mas, cp: u64) {
    let old_enode = mas.node;
    mas.node = self.mt_slot_locked(mas.tree, cp + 168, 0);
    let height = self.b(cp, 253);
    self.mas_topiary_replace(mas, old_enode, height);
    if self.mt_is_alloc(mas.tree) && !self.mte_is_root(mas.node) {
        let g0 = self.rd(cp + 192);
        self.mas_update_gap_known(mas, g0);
    }
    self.mtree_range_walk(mas);
}


pub fn mas_reset(&mut self, mas: &mut Mas) {
    mas.status = MA_START;
    mas.node = 0;
}

/* maple_copy field offsets:
 * dst[i]: node +0+i*24, max +8+i*24, mt +16+i*24 (i<3)
 * src[i]: node +72+i*24, max +80+i*24, start +88+i*24, end +89+i*24,
 *         mt +90+i*24 (i<4)
 * slot +168[3], gap +192[3], min +216, pivot +224[3] (union: max=+240)
 * end +248, s_count +249, d_count +250, split +251, data +252, height +253
 */

pub fn cp_leaf_init(&mut self, cp: u64, mas: &Mas, l_wr: &MaWrState, r_wr: &MaWrState) {
    let mut end = 0u8;
    self.sb(cp, 253, 1); /* cp->height = 1 */
    if l_wr.r_min < mas.index {
        end += 1;
        self.wr(cp + 168, l_wr.content); /* cp->slot[0] */
        self.wr(cp + 224, mas.index.wrapping_sub(1)); /* cp->pivot[0] */
    }
    self.wr(cp + 168 + 8 * end as u64, l_wr.entry);
    self.wr(cp + 224 + 8 * end as u64, mas.last);
    if r_wr.end_piv > mas.last {
        end += 1;
        let c = self.rd(r_wr.slots + 8 * r_wr.offset_end as u64);
        self.wr(cp + 168 + 8 * end as u64, c);
        self.wr(cp + 224 + 8 * end as u64, r_wr.end_piv);
    }
    self.wr(cp + 216, l_wr.r_min); /* cp->min */
    let pe = self.rd(cp + 224 + 8 * end as u64);
    self.wr(cp + 240, pe); /* cp->max (union) */
    self.sb(cp, 248, end); /* cp->end */
}


pub fn cp_data_calc(
    &mut self,
    cp: u64,
    l_wr: &MaWrState,
    r_wr: &MaWrState,
    l_mas: &Mas,
    r_mas: &Mas,
) {
    let mut data = l_mas.offset;
    data = data.wrapping_add(self.b(cp, 248).wrapping_add(1)); /* + cp->end + 1 */
    data = data.wrapping_add(r_mas.end.wrapping_sub(r_wr.offset_end as u8));
    self.sb(cp, 252, data);
}


pub fn data_fits(&self, sib: &Mas, mas: &Mas, cp: u64) -> bool {
    let t = mte_node_type(mas.node);
    let space = (2 * MT_SLOTS[(t as usize).min(MT_SLOTS.len() - 1)]) as u8;
    let end = sib.end;
    let new_data = (end as u32 + 1 + self.b(cp, 252) as u32) as u8;
    if new_data > space {
        return false;
    }
    if new_data < space {
        return true;
    }
    false
}


pub fn push_data_sib(&mut self, cp: u64, mas: &Mas, sib: &mut Mas, parent: &Mas) {
    let mut no_push = false;
    if !self.mte_is_root(mas.node) {
        *sib = parent.clone();
        if sib.offset != 0 {
            sib.offset = sib.offset.wrapping_sub(1);
            self.mas_descend(sib);
            sib.end = self.mas_data_end(sib);
            if self.data_fits(sib, mas, cp) {
                return;
            }
            *sib = parent.clone();
        }
        if sib.offset >= sib.end {
            no_push = true;
        } else {
            sib.offset += 1;
            self.mas_descend(sib);
            sib.end = self.mas_data_end(sib);
            if self.data_fits(sib, mas, cp) {
                return;
            }
            no_push = true;
        }
    } else {
        no_push = true;
    }
    if no_push {
        sib.end = 0;
    }
}


pub fn rebalance_data(
    &mut self,
    cp: u64,
    wr: &MaWrState,
    wr_mas: &Mas,
    sib: &mut Mas,
    parent: &Mas,
) {
    self.cp_data_calc(cp, wr, wr, wr_mas, wr_mas);
    sib.end = 0;
    let mut use_sib = false;
    if self.b(cp, 252) as usize > MT_SLOTS[(wr.type_ as usize).min(MT_SLOTS.len() - 1)] {
        self.push_data_sib(cp, wr_mas, sib, parent);
        if sib.end != 0 {
            use_sib = true;
        }
    } else if self.b(cp, 252) as usize <= MT_MIN_SLOTS[(wr.type_ as usize).min(MT_MIN_SLOTS.len() - 1)] {
        if wr_mas.min != 0 || wr_mas.max != ULONG_MAX {
            self.rebalance_sib(parent, sib);
            use_sib = true;
        }
    }
    if use_sib {
        let d = self.b(cp, 252).wrapping_add(sib.end).wrapping_add(1);
        self.sb(cp, 252, d);
    }
}


pub fn spanning_data(
    &mut self,
    cp: u64,
    l_wr: &MaWrState,
    r_wr: &MaWrState,
    l_mas: &Mas,
    r_mas: &Mas,
    sib: &mut Mas,
) {
    self.cp_data_calc(cp, l_wr, r_wr, l_mas, r_mas);
    if (l_mas.min != 0 || r_mas.max != ULONG_MAX)
        && (self.b(cp, 252) as usize) <= MT_MIN_SLOTS[(l_wr.type_ as usize).min(MT_MIN_SLOTS.len() - 1)]
    {
        self.spanning_sib(l_mas, r_mas, sib);
        let d = self.b(cp, 252).wrapping_add(sib.end).wrapping_add(1);
        self.sb(cp, 252, d);
    } else {
        sib.end = 0;
    }
}


pub fn dst_setup(&mut self, cp: u64, mas: &mut Mas, mt: u8) {
    let mut goto_node_setup = false;
    let data = self.b(cp, 252);
    if data as usize <= MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] {
        self.sb(cp, 251, data.wrapping_sub(1)); /* split = data - 1 */
        self.sb(cp, 250, 1); /* d_count = 1 */
        goto_node_setup = true;
    }
    if !goto_node_setup {
        self.sb(cp, 251, data.wrapping_sub(1) / 2);
        self.sb(cp, 250, 2);
        if (data as usize) < MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] * 2 {
            goto_node_setup = true;
        }
    }
    if !goto_node_setup && data as usize == MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] * 2 {
        if ma_is_leaf(mt) {
            let mut off = self.b(cp, 251);
            let s_count = self.b(cp, 249);
            let mut s = 0u8;
            while s < s_count {
                let s_end = self.b(cp, cp_src_end(cp, s as usize));
                let s_start = self.b(cp, cp_src_start(cp, s as usize));
                let mut s_off = s_end.wrapping_sub(s_start);
                if s_off >= off {
                    break;
                }
                s_off += 1;
                off = off.wrapping_sub(s_off);
                s += 1;
            }
            let s_start = self.b(cp, cp_src_start(cp, s as usize));
            off = off.wrapping_add(s_start);
            let snode = self.rd(cp_src_node(cp, s as usize));
            let smt = self.b(cp, cp_src_mt(cp, s as usize));
            let slotv = self.rd(ma_slots(snode, smt) + 8 * off as u64);
            if slotv != 0 {
                goto_node_setup = true;
            } else {
                let mut split = self.b(cp, 251).wrapping_add(1);
                self.sb(cp, 251, split);
                if split < MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] as u8 {
                    goto_node_setup = true;
                } else {
                    split = split.wrapping_sub(2);
                    self.sb(cp, 251, split);
                    if (data - 2).wrapping_sub(split) < MT_SLOTS[(mt as usize).min(MT_SLOTS.len() - 1)] as u8 {
                        goto_node_setup = true;
                    }
                }
            }
        } else {
            goto_node_setup = true;
        }
    }
    if !goto_node_setup {
        self.sb(cp, 251, (data + 2) / 3);
        self.sb(cp, 250, 3);
    }
    /* node_setup: */
    let d_count = self.b(cp, 250);
    for i in 0..d_count as usize {
        self.sb(cp, (cp_dst_mt(cp, i) & 0xff) as usize, mt);
        let n = self.mas_pop_node(mas).unwrap_or(0);
        self.wr(cp_dst_node(cp, i), n);
    }
}


pub fn append_mas_cp(&mut self, cp: u64, mas: &Mas, start: u8, end: u8) {
    let count = self.b(cp, 249) as usize;
    let node = mas_mn(mas);
    let mt = mte_node_type(mas.node);
    self.wr(cp_src_node(cp, count), node);
    self.sb(cp, cp_src_mt(cp, count), mt);
    if mas.end <= end {
        self.wr(cp_src_max(cp, count), mas.max);
    } else {
        let v = self.rd(ma_pivots(node, mt) + 8 * end as u64);
        self.wr(cp_src_max(cp, count), v);
    }
    self.sb(cp, cp_src_start(cp, count), start);
    self.sb(cp, cp_src_end(cp, count), end);
    self.sb(cp, 249, (count as u8).wrapping_add(1));
}


pub fn append_wr_mas_cp(&mut self, cp: u64, wr: &MaWrState, wr_mas: &Mas, start: u8, end: u8) {
    let count = self.b(cp, 249) as usize;
    self.wr(cp_src_node(cp, count), wr.node);
    self.sb(cp, cp_src_mt(cp, count), wr.type_);
    if wr_mas.end <= end {
        self.wr(cp_src_max(cp, count), wr_mas.max);
    } else {
        let v = self.rd(wr.pivots + 8 * end as u64);
        self.wr(cp_src_max(cp, count), v);
    }
    self.sb(cp, cp_src_start(cp, count), start);
    self.sb(cp, cp_src_end(cp, count), end);
    self.sb(cp, 249, (count as u8).wrapping_add(1));
}


pub fn init_cp_src(&mut self, cp: u64) {
    let count = self.b(cp, 249) as usize;
    self.wr(cp_src_node(cp, count), cp); /* ma_mnode_ptr(cp) = node ptr */
    self.sb(cp, cp_src_mt(cp, count), MAPLE_COPY);
    let m = self.w(cp, 240 / 8);
    self.wr(cp_src_max(cp, count), m);
    self.sb(cp, cp_src_start(cp, count), 0);
    let e = self.b(cp, 248);
    self.sb(cp, cp_src_end(cp, count), e);
    self.sb(cp, 249, (count as u8).wrapping_add(1));
}


pub fn multi_src_setup(
    &mut self,
    cp: u64,
    l_wr: &MaWrState,
    r_wr: &MaWrState,
    l_mas: &Mas,
    r_mas: &Mas,
    sib: &Mas,
) {
    self.sb(cp, 249, 0); /* s_count = 0 */
    if sib.end != 0 && sib.max < l_mas.min {
        self.append_mas_cp(cp, sib, 0, sib.end);
    }
    if l_mas.offset != 0 {
        let off = l_mas.offset.wrapping_sub(1);
        self.append_wr_mas_cp(cp, l_wr, l_mas, 0, off);
        let cnt = self.b(cp, 249) as usize;
        let mn = self.w(cp, 216 / 8);
        self.wr(cp_src_max(cp, cnt - 1), mn - 1);
    }
    self.init_cp_src(cp);
    if r_mas.end != r_wr.offset_end as u8 {
        self.append_wr_mas_cp(
            cp,
            r_wr,
            r_mas,
            (r_wr.offset_end as u8).wrapping_add(1),
            r_mas.end,
        );
    }
    if sib.end != 0 && sib.min > r_mas.max {
        self.append_mas_cp(cp, sib, 0, sib.end);
    }
}


pub fn cp_data_write(&mut self, cp: u64, mas: &mut Mas) {
    let mut data_offset = 0u8;
    let mut s = 0u8;
    let mut d = 0u8;
    let mut src = self.rd(cp_src_node(cp, s as usize));
    let mut dst;
    let mut s_offset = self.b(cp, cp_src_start(cp, s as usize));
    let mut src_end = self.b(cp, cp_src_end(cp, s as usize));
    let mut split = self.b(cp, 251);
    let mut s_max = self.rd(cp_src_max(cp, s as usize));
    let mut s_mt = self.b(cp, cp_src_mt(cp, s as usize));
    let mut d_mt = self.b(cp, (cp_dst_mt(cp, d as usize) & 0xff) as usize);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        let mut dst_offset = 0u8;
        let mut d_max = 0u64;
        dst = self.rd(cp_dst_node(cp, d as usize));
        d_mt = self.b(cp, (cp_dst_mt(cp, d as usize) & 0xff) as usize);
        let mut dst_size = split + 1;
        while dst_size != 0 {
            let size = if src_end.wrapping_sub(s_offset) as i32 + 1 < dst_size as i32 {
                src_end.wrapping_sub(s_offset) + 1
            } else {
                dst_size
            };
            d_max = self.node_copy(mas, src, s_offset, size, s_max, s_mt, dst, dst_offset, d_mt,
            );
            dst_offset += size;
            s_offset += size;
            if s_offset > src_end {
                s += 1;
                if s >= self.b(cp, 249) {
                    self.wr(cp_dst_max(cp, d as usize), d_max);
                    self.node_finalise(dst, d_mt, dst_offset);
                    return;
                }
                src = self.rd(cp_src_node(cp, s as usize));
                s_offset = self.b(cp, cp_src_start(cp, s as usize));
                src_end = self.b(cp, cp_src_end(cp, s as usize));
                s_max = self.rd(cp_src_max(cp, s as usize));
                s_mt = self.b(cp, cp_src_mt(cp, s as usize));
            }
            dst_size -= size;
            data_offset += size;
        }
        split = self.b(cp, 251);
        self.wr(cp_dst_max(cp, d as usize), d_max);
        /* handle null entries */
        if d_max != ULONG_MAX && self.rd(ma_slots(dst, d_mt) + 8 * dst_offset.wrapping_sub(1) as u64) == 0 {
            if s_offset == self.b(cp, cp_src_start(cp, s as usize)) {
                s = s.wrapping_sub(1);
                src = self.rd(cp_src_node(cp, s as usize));
                src_end = self.b(cp, cp_src_end(cp, s as usize));
                s_max = self.rd(cp_src_max(cp, s as usize));
                s_mt = self.b(cp, cp_src_mt(cp, s as usize));
                s_offset = src_end;
            } else {
                s_offset = s_offset.wrapping_sub(1);
            }
            split += 1;
            data_offset = data_offset.wrapping_sub(1);
            dst_offset = dst_offset.wrapping_sub(1);
            let v = self.rd(ma_pivots(dst, d_mt) + 8 * dst_offset.wrapping_sub(1) as u64);
            self.wr(cp_dst_max(cp, d as usize), v);
        }
        self.node_finalise(dst, d_mt, dst_offset);
        d += 1;
        if d == self.b(cp, 250) - 1 {
            split = self.b(cp, 252) - data_offset;
        }
        if d >= self.b(cp, 250) {
            self.warn_on(data_offset < self.b(cp, 252));
            return;
        }
        if data_offset > self.b(cp, 252) {
            break;
        }
    }
}


pub fn cp_dst_to_slots(&mut self, cp: u64, min: u64, max: u64, mas: &Mas) {
    let mut slot_min = min;
    let d_count = self.b(cp, 250);
    for d in 0..d_count as usize {
        let mn = self.rd(cp_dst_node(cp, d));
        let mt = self.b(cp, (cp_dst_mt(cp, d) & 0xff) as usize);
        let slot_max = self.rd(cp_dst_max(cp, d));
        self.ma_init_slot(cp + 168 + 8 * d as u64, mn, mt);
        self.wr(cp + 224 + 8 * d as u64, slot_max);
        if self.mt_is_alloc(mas.tree) {
            if ma_is_leaf(mt) {
                let g = self.ma_leaf_max_gap(
                    mn,
                    mt,
                    slot_min,
                    slot_max,
                    ma_pivots(mn, mt),
                    ma_slots(mn, mt),
                );
                self.wr(cp + 192 + 8 * d as u64, g);
            } else {
                let gaps = ma_gaps(mn, mt);
                if gaps != 0 {
                    let gap_slot = self.ma_meta_gap(mn);
                    let g = self.rd(gaps + 8 * gap_slot as u64);
                    self.wr(cp + 192 + 8 * d as u64, g);
                }
            }
        }
        slot_min = slot_max.wrapping_add(1);
    }
    self.sb(cp, 248, d_count.wrapping_sub(1)); /* cp->end = d_count - 1 */
    self.wr(cp + 216, min); /* cp->min */
    self.wr(cp + 240, max); /* cp->max (union at pivot[2]) */
}


pub fn cp_is_new_root(&mut self, cp: u64, mas: &mut Mas) -> bool {
    if self.w(cp, 216 / 8) != 0 || self.w(cp, 240 / 8) != ULONG_MAX {
        return false;
    }
    if self.b(cp, 250) != 1 {
        let mt = if self.mt_is_alloc(mas.tree) {
            MAPLE_ARANGE_64
        } else {
            MAPLE_RANGE_64
        };
        let dc = self.b(cp, 250);
        self.sb(cp, 252, dc);
        self.sb(cp, 249, 0);
        self.dst_setup(cp, mas, mt);
        self.init_cp_src(cp);
        let s0 = self.rd(cp_src_node(cp, 0));
        let d0 = self.rd(cp_dst_node(cp, 0));
        let cd = self.b(cp, 252);
        let cmax = self.w(cp, 240 / 8);
        self.node_copy(mas, s0, 0, cd, cmax, MAPLE_COPY, d0, 0, mt);
        let e1 = self.b(cp, 248) + 1;
        self.node_finalise(d0, mt, e1);
        self.ma_init_slot(cp + 168, d0, mt);
        let h = self.b(cp, 253) + 1;
        self.sb(cp, 253, h);
    }
    let d0 = self.rd(cp_dst_node(cp, 0));
    let s0 = self.mt_slot_locked(mas.tree, cp + 168, 0);
    self.warn_on_once(2, d0 != mte_to_node(s0));
    let par = ma_parent_ptr(mas_tree_parent(mas));
    self.sw(d0, 0, par);
    mas.min = 0;
    mas.max = ULONG_MAX;
    mas.depth = 0;
    mas.node = self.mas_root_locked(mas);
    true
}


pub fn cp_converged(&mut self, cp: u64, mas: &Mas, sib: &Mas) -> bool {
    if self.b(cp, 250) != 1 || sib.end != 0 {
        return false;
    }
    let d0 = self.rd(cp_dst_node(cp, 0));
    let par = ma_parent_ptr(self.w(mas_mn(mas), 0));
    self.sw(d0, 0, par);
    true
}

/* spanning_ascend: l_wr_mas->mas IS `mas` in C — same object. */

pub fn spanning_ascend(
    &mut self,
    cp: u64,
    mas: &mut Mas,
    l_wr: &mut MaWrState,
    r_wr: &mut MaWrState,
    r_mas: &mut Mas,
    sib: &Mas,
) -> bool {
    if sib.end != 0 {
        if sib.max < mas.min {
            *mas = sib.clone();
        } else {
            *r_mas = sib.clone();
        }
    }
    let mn = mas.min;
    let mx = r_mas.max;
    self.cp_dst_to_slots(cp, mn, mx, mas);
    if self.cp_is_new_root(cp, mas) {
        return false;
    }
    if self.b(cp, 250) == 1 && mas.node == r_mas.node {
        let d0 = self.rd(cp_dst_node(cp, 0));
        let par = ma_parent_ptr(self.w(mas_mn(mas), 0));
        self.sw(d0, 0, par);
        return false;
    }
    let h = self.b(cp, 253) + 1;
    self.sb(cp, 253, h);
    self.wr_mas_ascend(l_wr, mas);
    self.wr_mas_ascend(r_wr, r_mas);
    true
}


pub fn rebalance_ascend(
    &mut self,
    cp: u64,
    wr: &mut MaWrState,
    mas: &mut Mas,
    sib: &Mas,
    parent: &mut Mas,
) -> bool {
    let min;
    let max;
    if sib.end == 0 {
        min = mas.min;
        max = mas.max;
    } else if sib.min > mas.max {
        min = mas.min;
        max = sib.max;
        wr.offset_end = parent.offset as u32 + 1;
    } else {
        min = sib.min;
        max = mas.max;
        wr.offset_end = parent.offset as u32;
        parent.offset = parent.offset.wrapping_sub(1);
    }
    self.cp_dst_to_slots(cp, min, max, mas);
    if self.cp_is_new_root(cp, mas) {
        return false;
    }
    if self.cp_converged(cp, mas, sib) {
        return false;
    }
    let h = self.b(cp, 253) + 1;
    self.sb(cp, 253, h);
    copy_tree_location(parent, mas);
    self.wr_mas_setup(wr, mas);
    true
}


pub fn mas_root_expand(&mut self, mas: &mut Mas, entry: u64) {
    let contents = self.mas_root_locked(mas);
    let t = MAPLE_LEAF_64;
    let node = match self.mas_pop_node(mas) {
        Some(n) => n,
        None => return, /* C would NULL-deref; unreachable on success paths */
    };
    let pivots = ma_pivots(node, t);
    let slots = ma_slots(node, t);
    let par = ma_parent_ptr(mas_tree_parent(mas));
    self.sw(node, 0, par);
    mas.node = mt_mk_node(node, t);
    mas.status = MA_ACTIVE;
    let mut slot = 0u8;
    if mas.index != 0 {
        if contents != 0 {
            self.wr(slots + 8 * slot as u64, contents);
            if mas.index > 1 {
                slot += 1;
            }
        }
        self.wr(pivots + 8 * slot as u64, mas.index.wrapping_sub(1));
        slot += 1;
    }
    self.wr(slots + 8 * slot as u64, entry);
    mas.offset = slot;
    self.wr(pivots + 8 * slot as u64, mas.last);
    if mas.last != ULONG_MAX {
        slot += 1;
        self.wr(pivots + 8 * slot as u64, ULONG_MAX);
    }
    self.mt_set_height(mas.tree, 1);
    self.ma_set_meta(node, MAPLE_LEAF_64, 0, slot);
    let r = mte_mk_root(mas.node);
    self.set_troot(mas.tree, r);
}


pub fn mas_store_root(&mut self, mas: &mut Mas, entry: u64) {
    if entry == 0 {
        if mas.index == 0 {
            self.set_troot(mas.tree, 0);
        }
    } else if mas.last != 0 || mas.index != 0 {
        self.mas_root_expand(mas, entry);
    } else if entry & 3 == 2 {
        self.mas_root_expand(mas, entry);
    } else {
        self.set_troot(mas.tree, entry);
        mas.status = MA_START;
    }
}


pub fn mas_is_span_wr(&self, wr: &MaWrState, mas: &Mas) -> bool {
    let mut max = wr.r_max;
    let last = mas.last;
    let t = wr.type_;
    let entry = wr.entry;
    if last < max {
        return false;
    }
    if ma_is_leaf(t) {
        max = mas.max;
        if last < max {
            return false;
        }
    }
    if last == max && (entry != 0 || last == ULONG_MAX) {
        return false;
    }
    let _ = self;
    true
}


pub fn mas_wr_walk_descend(&self, wr: &mut MaWrState, mas: &mut Mas) {
    wr.type_ = mte_node_type(mas.node);
    self.mas_wr_node_walk(wr, mas);
    wr.slots = ma_slots(wr.node, wr.type_);
}


pub fn mas_wr_walk(&mut self, wr: &mut MaWrState, mas: &mut Mas) -> bool {
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return false; }
        self.mas_wr_walk_descend(wr, mas);
        if self.mas_is_span_wr(wr, mas) {
            return false;
        }
        wr.content = self.mas_slot_locked(mas, wr.slots, mas.offset);
        if ma_is_leaf(wr.type_) {
            return true;
        }
        if (mas.end as usize) < MT_SLOTS[(wr.type_ as usize).min(MT_SLOTS.len() - 1)] - 1 {
            wr.vacant_height = mas.depth + 1;
        }
        if self.ma_is_root(mas_mn(mas)) {
            if mas.end > 2 {
                wr.sufficient_height = 1;
            }
        } else if (mas.end as usize) > MT_MIN_SLOTS[(wr.type_ as usize).min(MT_MIN_SLOTS.len() - 1)] + 1 {
            wr.sufficient_height = mas.depth + 1;
        }
        mas_wr_walk_traverse(wr, mas);
    }
}


pub fn mas_wr_walk_index(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        self.mas_wr_walk_descend(wr, mas);
        wr.content = self.mas_slot_locked(mas, wr.slots, mas.offset);
        if ma_is_leaf(wr.type_) {
            return;
        }
        mas_wr_walk_traverse(wr, mas);
    }
}


pub fn mas_extend_spanning_null(
    &self,
    l_wr: &mut MaWrState,
    r_wr: &mut MaWrState,
    l_mas: &mut Mas,
    r_mas: &mut Mas,
) {
    let l_slot = l_mas.offset;
    if l_wr.content == 0 {
        l_mas.index = l_wr.r_min;
    }
    if l_mas.index == l_wr.r_min
        && l_slot != 0
        && self.mas_slot_locked(l_mas, l_wr.slots, l_slot.wrapping_sub(1)) == 0
    {
        if l_slot > 1 {
            l_mas.index = self.rd(l_wr.pivots + 8 * (l_slot as u64 - 2)) + 1;
        } else {
            l_mas.index = l_mas.min;
        }
        l_mas.offset = l_slot.wrapping_sub(1);
        l_wr.r_min = l_mas.index;
    }
    if r_wr.content == 0 {
        if r_mas.last < r_wr.r_max {
            r_mas.last = r_wr.r_max;
        }
        r_mas.offset += 1;
    }
}


pub fn mas_state_walk(&mut self, mas: &mut Mas) -> u64 {
    let entry = self.mas_start(mas);
    if mas_is_none(mas) {
        return 0;
    }
    if mas_is_ptr(mas) {
        return entry;
    }
    self.mtree_range_walk(mas)
}


pub fn mtree_lookup_walk(&mut self, mas: &mut Mas) -> u64 {
    let mut next = mas.node;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        let node = mte_to_node(next);
        let t = mte_node_type(next);
        let pivots = ma_pivots(node, t);
        let end = MT_PIVOTS[(t as usize).min(MT_PIVOTS.len() - 1)] as u8;
        let mut offset = 0u8;
        let mut __lim = 0u64;
        loop {
            __lim += 1;
            if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
            if self.rd(pivots + 8 * offset as u64) >= mas.index {
                break;
            }
            offset += 1;
            if offset >= end {
                break;
            }
        }
        let slots = ma_slots(node, t);
        next = self.mt_slot(mas.tree, slots, offset);
        if self.ma_dead_node(node) {
            self.mas_reset(mas);
            return 0;
        }
        if ma_is_leaf(t) {
            break;
        }
    }
    next
}


pub fn mas_new_root(&mut self, mas: &mut Mas, entry: u64) {
    let root = self.mas_root_locked(mas);
    let t = MAPLE_LEAF_64;
    self.warn_on_once(3, mas.index != 0 || mas.last != ULONG_MAX);
    if entry == 0 {
        self.mt_set_height(mas.tree, 0);
        self.set_troot(mas.tree, entry);
        mas.status = MA_START;
    } else {
        let node = self.mas_pop_node(mas).unwrap_or(NULLN);
        let pivots = ma_pivots(node, t);
        let slots = ma_slots(node, t);
        let par = ma_parent_ptr(mas_tree_parent(mas));
        self.sw(node, 0, par);
        mas.node = mt_mk_node(node, t);
        mas.status = MA_ACTIVE;
        self.wr(slots, entry);
        self.wr(pivots, mas.last);
        self.mt_set_height(mas.tree, 1);
        let r = mte_mk_root(mas.node);
        self.set_troot(mas.tree, r);
    }
    if xa_is_node(root) {
        self.mte_destroy_walk(root, mas.tree);
    }
}


pub fn mas_wr_spanning_store(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let mut r_mas = mas.clone();
    let mut r_wr = MaWrState::new(&r_mas, wr.entry);
    if mas.index == 0 && mas.last == ULONG_MAX {
        self.mas_new_root(mas, wr.entry);
        return;
    }
    if r_mas.last.wrapping_add(1) != 0 {
        r_mas.last += 1;
    }
    r_mas.index = r_mas.last;
    self.mas_wr_walk_index(&mut r_wr, &mut r_mas);
    r_mas.last = mas.last;
    r_mas.index = mas.last;
    r_wr.end_piv = r_wr.r_max;
    self.mas_wr_walk_index(wr, mas);
    if wr.entry == 0 {
        self.mas_extend_spanning_null(wr, &mut r_wr, mas, &mut r_mas);
        mas.last = r_mas.last;
    }
    if mas.index == 0 && r_mas.last == ULONG_MAX {
        mas.index = 0;
        mas.last = ULONG_MAX; /* mas_set_range */
        self.mas_new_root(mas, wr.entry);
        return;
    }
    let cp = CPBASE;
    self.wzero(cp, 0, 32);
    let mut sib = Mas::new(mas.tree, 0, 0);
    self.cp_leaf_init(cp, mas, wr, &r_wr);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        self.spanning_data(cp, wr, &r_wr, mas, &r_mas, &mut sib);
        self.multi_src_setup(cp, wr, &r_wr, mas, &r_mas, &sib);
        self.dst_setup(cp, mas, wr.type_);
        self.cp_data_write(cp, mas);
        if !self.spanning_ascend(cp, mas, wr, &mut r_wr, &mut r_mas, &sib) {
            break;
        }
    }
    self.mas_wmb_replace(mas, cp);
}


pub fn mas_wr_node_store(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let in_rcu = self.mt_in_rcu(mas.tree);
    let mut offset_end = wr.offset_end as u8;
    let node_pivots = MT_PIVOTS[(wr.type_ as usize).min(MT_PIVOTS.len() - 1)] as u8;
    let node_slots = MT_SLOTS[(wr.type_ as usize).min(MT_SLOTS.len() - 1)] as u8;
    let mut new_end = mas
        .end
        .wrapping_add(1)
        .wrapping_sub(offset_end)
        .wrapping_add(mas.offset);
    if mas.last == wr.end_piv {
        offset_end += 1;
        new_end = new_end.wrapping_sub(1);
    }
    let newnode;
    if in_rcu {
        newnode = self.mas_pop_node(mas).unwrap_or(NULLN);
    } else {
        newnode = REUSE;
        self.wzero(REUSE, 0, 32);
    }
    let par = self.w(mas_mn(mas), 0);
    self.sw(newnode, 0, par);
    let dst_pivots = ma_pivots(newnode, wr.type_);
    let dst_slots = ma_slots(newnode, wr.type_);
    if mas.offset != 0 {
        self.rcopy(dst_pivots, wr.pivots, mas.offset as usize);
        self.rcopy(dst_slots, wr.slots, mas.offset as usize);
    }
    if wr.r_min < mas.index {
        self.wr(dst_slots + 8 * mas.offset as u64, wr.content);
        self.wr(dst_pivots + 8 * mas.offset as u64, mas.index.wrapping_sub(1));
        mas.offset += 1;
        new_end = new_end.wrapping_add(1);
    }
    if mas.offset < node_pivots {
        self.wr(dst_pivots + 8 * mas.offset as u64, mas.last);
    }
    self.wr(dst_slots + 8 * mas.offset as u64, wr.entry);
    if offset_end <= mas.end {
        let dst_offset = mas.offset.wrapping_add(1);
        let copy_size = mas.end.wrapping_sub(offset_end).wrapping_add(1);
        self.rcopy(
            dst_slots + 8 * dst_offset as u64,
            wr.slots + 8 * offset_end as u64,
            copy_size as usize,
        );
        self.rcopy(
            dst_pivots + 8 * dst_offset as u64,
            wr.pivots + 8 * offset_end as u64,
            (copy_size - 1) as usize,
        );
        if new_end < node_pivots {
            self.wr(dst_pivots + 8 * new_end as u64, mas.max);
        }
    }
    if !in_rcu && (new_end as u32 + 2) < node_slots as u32 {
        let clear_from = new_end + 1;
        self.rzero(
            dst_slots + 8 * clear_from as u64,
            (node_slots - clear_from) as usize,
        );
        if clear_from < node_pivots {
            self.rzero(
                dst_pivots + 8 * clear_from as u64,
                (node_pivots - clear_from) as usize,
            );
        }
    }
    self.mas_leaf_set_meta(newnode, wr.type_, new_end);
    if in_rcu {
        let old_enode = mas.node;
        mas.node = mt_mk_node(newnode, wr.type_);
        let h = self.mas_mt_height(mas) as u8;
        self.mas_replace_node(mas, old_enode, h);
    } else {
        self.wcopy(wr.node, newnode, 32);
    }
    self.mas_update_gap(mas);
    mas.end = new_end;
}


pub fn mas_wr_slot_store(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let offset = mas.offset;
    let slots = wr.slots;
    let mut gap = false;
    gap |= wr.content == 0;
    gap |= self.mt_slot_locked(mas.tree, slots, offset.wrapping_add(1)) == 0;
    if wr.offset_end as u8 - offset == 1 {
        if mas.index == wr.r_min {
            self.wr(slots + 8 * offset as u64, wr.entry);
            self.wr(wr.pivots + 8 * offset as u64, mas.last);
        } else {
            self.wr(slots + 8 * (offset as u64 + 1), wr.entry);
            self.wr(wr.pivots + 8 * offset as u64, mas.index.wrapping_sub(1));
            mas.offset += 1;
        }
    } else {
        self.warn_on_once(4, self.mt_in_rcu(mas.tree));
        gap |= self.mt_slot_locked(mas.tree, slots, offset + 2) == 0;
        self.wr(slots + 8 * (offset as u64 + 1), wr.entry);
        self.wr(wr.pivots + 8 * offset as u64, mas.index.wrapping_sub(1));
        self.wr(wr.pivots + 8 * (offset as u64 + 1), mas.last);
        mas.offset += 1;
    }
    if wr.entry == 0 || gap {
        self.mas_update_gap(mas);
    }
}


pub fn mas_wr_extend_null(&self, wr: &mut MaWrState, mas: &mut Mas) {
    if self.rd(wr.slots + 8 * wr.offset_end as u64) == 0 {
        mas.last = wr.end_piv;
    } else {
        if mas.last == wr.end_piv
            && mas.end != wr.offset_end as u8
            && self.rd(wr.slots + 8 * (wr.offset_end as u64 + 1)) == 0
        {
            wr.offset_end += 1;
            if wr.offset_end as u8 == mas.end {
                mas.last = mas.max;
            } else {
                mas.last = self.rd(wr.pivots + 8 * wr.offset_end as u64);
            }
            wr.end_piv = mas.last;
        }
    }
    if wr.content == 0 {
        mas.index = wr.r_min;
    } else {
        if mas.index == wr.r_min
            && mas.offset != 0
            && self.rd(wr.slots + 8u64.wrapping_mul((mas.offset as u64).wrapping_sub(1))) == 0
        {
            mas.offset = mas.offset.wrapping_sub(1);
            wr.r_min = self.mas_safe_min(mas, wr.pivots, mas.offset);
            mas.index = wr.r_min;
            wr.r_max = self.rd(wr.pivots + 8 * mas.offset as u64);
        }
    }
}


pub fn mas_wr_end_piv(&self, wr: &mut MaWrState, mas: &Mas) {
    while (wr.offset_end as u8) < mas.end && mas.last > self.rd(wr.pivots + 8 * wr.offset_end as u64) {
        wr.offset_end += 1;
    }
    if (wr.offset_end as u8) < mas.end {
        wr.end_piv = self.rd(wr.pivots + 8 * wr.offset_end as u64);
    } else {
        wr.end_piv = mas.max;
    }
}


pub fn mas_wr_append(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let end = mas.end;
    let new_end = mas_wr_new_end(wr, mas);
    if (new_end as usize) < MT_PIVOTS[(wr.type_ as usize).min(MT_PIVOTS.len() - 1)] {
        let v = self.rd(wr.pivots + 8 * end as u64);
        self.wr(wr.pivots + 8 * new_end as u64, v);
        self.ma_set_meta(wr.node, wr.type_, 0, new_end);
    }
    let slots = wr.slots;
    if new_end == end + 1 {
        if mas.last == wr.r_max {
            self.wr(slots + 8 * new_end as u64, wr.entry);
            self.wr(wr.pivots + 8 * end as u64, mas.index.wrapping_sub(1));
            mas.offset = new_end;
        } else {
            self.wr(slots + 8 * new_end as u64, wr.content);
            self.wr(wr.pivots + 8 * end as u64, mas.last);
            self.wr(slots + 8 * end as u64, wr.entry);
        }
    } else {
        self.wr(slots + 8 * new_end as u64, wr.content);
        self.wr(wr.pivots + 8 * (end as u64 + 1), mas.last);
        self.wr(slots + 8 * (end as u64 + 1), wr.entry);
        self.wr(wr.pivots + 8 * end as u64, mas.index.wrapping_sub(1));
        mas.offset = end + 1;
    }
    if wr.content == 0 || wr.entry == 0 {
        self.mas_update_gap(mas);
    }
    mas.end = new_end;
}


pub fn split_ascend(
    &mut self,
    cp: u64,
    wr: &mut MaWrState,
    mas: &mut Mas,
    sib: &Mas,
    parent: &mut Mas,
) -> bool {
    let mut min = mas.min;
    let mut max = mas.max;
    wr.offset_end = parent.offset as u32;
    if sib.end != 0 {
        if sib.max < mas.min {
            min = sib.min;
            parent.offset = parent.offset.wrapping_sub(1);
        } else {
            max = sib.max;
            wr.offset_end += 1;
        }
    }
    self.cp_dst_to_slots(cp, min, max, mas);
    if self.cp_is_new_root(cp, mas) {
        return false;
    }
    if self.cp_converged(cp, mas, sib) {
        return false;
    }
    let h = self.b(cp, 253) + 1;
    self.sb(cp, 253, h);
    copy_tree_location(parent, mas);
    self.wr_mas_setup(wr, mas);
    true
}


pub fn split_data(&mut self, cp: u64, wr: &MaWrState, mas: &Mas, sib: &mut Mas, parent: &Mas) {
    self.cp_data_calc(cp, wr, wr, mas, mas);
    if (self.b(cp, 252) as usize) <= MT_SLOTS[(wr.type_ as usize).min(MT_SLOTS.len() - 1)] {
        sib.end = 0;
        return;
    }
    self.push_data_sib(cp, mas, sib, parent);
    if sib.end != 0 {
        let d = self.b(cp, 252).wrapping_add(sib.end).wrapping_add(1);
        self.sb(cp, 252, d);
    }
}


pub fn mas_wr_split(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let mut parent = mas.clone();
    let mut sib = Mas::new(mas.tree, 0, 0);
    let cp = CPBASE;
    self.wzero(cp, 0, 32);
    self.cp_leaf_init(cp, mas, wr, wr);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        if !self.mte_is_root(parent.node) {
            self.mas_ascend(&mut parent);
            parent.end = self.mas_data_end(&parent);
        }
        self.split_data(cp, wr, mas, &mut sib, &parent);
        self.multi_src_setup(cp, wr, wr, mas, mas, &sib);
        self.dst_setup(cp, mas, wr.type_);
        self.cp_data_write(cp, mas);
        if !self.split_ascend(cp, wr, mas, &sib, &mut parent) {
            break;
        }
    }
    self.mas_wmb_replace(mas, cp);
}


pub fn mas_wr_rebalance(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let mut parent = mas.clone();
    let mut sib = Mas::new(mas.tree, 0, 0);
    let cp = CPBASE;
    self.wzero(cp, 0, 32);
    self.cp_leaf_init(cp, mas, wr, wr);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        if !self.mte_is_root(parent.node) {
            self.mas_ascend(&mut parent);
            parent.end = self.mas_data_end(&parent);
        }
        self.rebalance_data(cp, wr, mas, &mut sib, &parent);
        self.multi_src_setup(cp, wr, wr, mas, mas, &sib);
        self.dst_setup(cp, mas, wr.type_);
        self.cp_data_write(cp, mas);
        if !self.rebalance_ascend(cp, wr, mas, &sib, &mut parent) {
            break;
        }
    }
    self.mas_wmb_replace(mas, cp);
}


pub fn mas_wr_store_entry(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    match mas.store_type {
        WR_EXACT_FIT => {
            self.wr(wr.slots + 8 * mas.offset as u64, wr.entry);
            if (wr.entry != 0) != (wr.content != 0) {
                self.mas_update_gap(mas);
            }
        }
        WR_APPEND => self.mas_wr_append(wr, mas),
        WR_SLOT_STORE => self.mas_wr_slot_store(wr, mas),
        WR_NODE_STORE => self.mas_wr_node_store(wr, mas),
        WR_SPANNING_STORE => self.mas_wr_spanning_store(wr, mas),
        WR_SPLIT_STORE => self.mas_wr_split(wr, mas),
        WR_REBALANCE => self.mas_wr_rebalance(wr, mas),
        WR_NEW_ROOT => self.mas_new_root(mas, wr.entry),
        WR_STORE_ROOT => self.mas_store_root(mas, wr.entry),
        _ => {
            self.warn += 1; /* MT_BUG_ON wr_invalid */
        }
    }
}


pub fn mas_wr_prealloc_setup(&mut self, wr: &mut MaWrState, mas: &mut Mas) {
    let mut reset = false;
    if !mas_is_active(mas)
        && !mas_is_start(mas)
        && (mas_is_paused(mas) || mas_is_none(mas) || mas_is_overflow(mas) || mas_is_underflow(mas))
    {
        reset = true;
    }
    if !reset && mas_is_active(mas) {
        if mas.last > mas.max {
            reset = true;
        } else if wr.entry == 0 && mte_is_leaf(mas.node) && mas.last == mas.max {
            reset = true;
        }
    }
    if reset {
        self.mas_reset(mas);
    }
    wr.content = self.mas_start(mas);
}


pub fn mas_prealloc_calc(&mut self, wr: &MaWrState, mas: &mut Mas, entry: u64) {
    let height = self.mas_mt_height(mas) as u8;
    let mut ret: i32 = height as i32 * 3 + 1;
    let delta = height.wrapping_sub(wr.vacant_height) as i32;
    match mas.store_type {
        WR_EXACT_FIT | WR_APPEND | WR_SLOT_STORE => ret = 0,
        WR_SPANNING_STORE => {
            if wr.sufficient_height < wr.vacant_height {
                ret = (height - wr.sufficient_height) as i32 * 3 + 1;
            } else {
                ret = delta * 3 + 1;
            }
        }
        WR_SPLIT_STORE => ret = delta * 2 + 1,
        WR_REBALANCE => {
            if wr.sufficient_height < wr.vacant_height {
                ret = (height - wr.sufficient_height) as i32 * 2 + 1;
            } else {
                ret = delta * 2 + 1;
            }
        }
        WR_NODE_STORE => {
            ret = if self.mt_in_rcu(mas.tree) { 1 } else { 0 };
        }
        WR_NEW_ROOT => ret = 1,
        WR_STORE_ROOT => {
            if mas.last != 0 || mas.index != 0 {
                ret = 1;
            } else if entry & 3 == 2 {
                ret = 1;
            } else {
                ret = 0;
            }
        }
        _ => {
            self.warn_on_once(5, true);
        }
    }
    mas.node_request = ret as u8;
}


pub fn mas_wr_store_type(&mut self, wr: &mut MaWrState, mas: &mut Mas) -> u8 {
    if mas_is_none(mas) || mas_is_ptr(mas) {
        return WR_STORE_ROOT;
    }
    if !self.mas_wr_walk(wr, mas) {
        return WR_SPANNING_STORE;
    }
    self.mas_wr_end_piv(wr, mas);
    if wr.entry == 0 {
        self.mas_wr_extend_null(wr, mas);
    }
    if wr.r_min == mas.index && wr.r_max == mas.last {
        return WR_EXACT_FIT;
    }
    if mas.index == 0 && mas.last == ULONG_MAX {
        return WR_NEW_ROOT;
    }
    let new_end = mas_wr_new_end(wr, mas);
    let in_rcu = self.mt_in_rcu(mas.tree);
    let appending = mas.offset == mas.end;
    let one_slot = (wr.offset_end as u8) - mas.offset == 1;
    if (new_end as usize) < MT_MIN_SLOTS[(wr.type_ as usize).min(MT_MIN_SLOTS.len() - 1)] {
        if !self.mte_is_root(mas.node) {
            return WR_REBALANCE;
        }
        if !in_rcu {
            if appending {
                return WR_APPEND;
            } else if mas.end == new_end && one_slot {
                return WR_SLOT_STORE;
            }
        }
        return WR_NODE_STORE;
    }
    if (new_end as usize) >= MT_SLOTS[(wr.type_ as usize).min(MT_SLOTS.len() - 1)] {
        return WR_SPLIT_STORE;
    }
    if !in_rcu && appending {
        return WR_APPEND;
    }
    if new_end == mas.end && (!in_rcu || one_slot) {
        return WR_SLOT_STORE;
    }
    WR_NODE_STORE
}


pub fn mas_wr_preallocate(&mut self, wr: &mut MaWrState, mas: &mut Mas, entry: u64) {
    self.mas_wr_prealloc_setup(wr, mas);
    mas.store_type = self.mas_wr_store_type(wr, mas);
    self.mas_prealloc_calc(wr, mas, entry);
    if mas.node_request == 0 {
        return;
    }
    self.mas_alloc_nodes(mas, GFP_NOWAIT);
}


pub fn mas_insert(&mut self, mas: &mut Mas, entry: u64) -> u64 {
    let mut wr = MaWrState::new(mas, entry);
    wr.content = self.mas_start(mas);
    let mut exists = wr.content != 0;
    if !exists {
        self.mas_wr_preallocate(&mut wr, mas, entry);
        if mas_is_err(mas) {
            return 0;
        }
        if mas.store_type == WR_SPANNING_STORE {
            exists = true;
        } else {
            if mas.store_type != WR_NEW_ROOT && mas.store_type != WR_STORE_ROOT {
                wr.offset_end = mas.offset as u32;
                wr.end_piv = wr.r_max;
                if wr.content != 0 || mas.last > wr.r_max {
                    exists = true;
                }
            }
            if !exists {
                self.mas_wr_store_entry(&mut wr, mas);
                return wr.content;
            }
        }
    }
    let _ = exists;
    mas_set_err(mas, -EEXIST);
    wr.content
}

/* mas_set / mas_set_range / __mas_set_range (header inlines) */

pub fn __mas_set_range(&mut self, mas: &mut Mas, start: u64, last: u64) {
    self.warn_on(mas_is_active(mas) && (mas.index > start || mas.last < start));
    mas.index = start;
    mas.last = last;
}

pub fn mas_set_range(&mut self, mas: &mut Mas, start: u64, last: u64) {
    self.mas_reset(mas);
    self.__mas_set_range(mas, start, last);
}

pub fn mas_set(&mut self, mas: &mut Mas, index: u64) {
    self.mas_set_range(mas, index, index);
}


pub fn mas_rewalk(&mut self, mas: &mut Mas, index: u64) {
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        self.mas_set(mas, index);
        self.mas_state_walk(mas);
        if !mas_is_start(mas) {
            break;
        }
    }
}


pub fn mas_rewalk_if_dead(&mut self, mas: &mut Mas, node: u64, index: u64) -> bool {
    if self.ma_dead_node(node) {
        self.mas_rewalk(mas, index);
        return true;
    }
    false
}


pub fn mas_prev_node(&mut self, mas: &mut Mas, min: u64) -> i32 {
    let mut node = mas_mn(mas);
    if mas.min == 0 {
        if self.ma_dead_node(node) {
            return 1;
        }
        mas.status = MA_UNDERFLOW;
        return 0;
    }
    let max = mas.min.wrapping_sub(1);
    if max < min {
        if self.ma_dead_node(node) {
            return 1;
        }
        mas.status = MA_UNDERFLOW;
        return 0;
    }
    let mut level = 0;
    let mut offset;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        if self.ma_is_root(node) {
            if self.ma_dead_node(node) {
                return 1;
            }
            mas.status = MA_UNDERFLOW;
            return 0;
        }
        if self.mas_ascend(mas) != 0 {
            return 1;
        }
        offset = mas.offset;
        level += 1;
        node = mas_mn(mas);
        if offset != 0 {
            break;
        }
    }
    offset = offset.wrapping_sub(1);
    let mut mt = mte_node_type(mas.node);
    while level > 1 {
        level -= 1;
        let slots = ma_slots(node, mt);
        mas.node = self.mas_slot(mas, slots, offset);
        if self.ma_dead_node(node) {
            return 1;
        }
        mt = mte_node_type(mas.node);
        node = mas_mn(mas);
        let pivots = ma_pivots(node, mt);
        offset = self.ma_data_end(node, mt, pivots, max);
        if self.ma_dead_node(node) {
            return 1;
        }
    }
    let slots = ma_slots(node, mt);
    mas.node = self.mas_slot(mas, slots, offset);
    let pivots = ma_pivots(node, mt);
    if self.ma_dead_node(node) {
        return 1;
    }
    if offset != 0 {
        mas.min = self.rd(pivots + 8u64.wrapping_mul((offset as u64).wrapping_sub(1))) + 1;
    }
    mas.max = max;
    mas.offset = self.mas_data_end(mas);
    if self.mte_dead_node(mas.node) {
        return 1;
    }
    mas.end = mas.offset;
    0
}


pub fn mas_prev_slot(&mut self, mas: &mut Mas, min: u64, empty: bool) -> u64 {
    let save_point = mas.index;
    let mut __lim = 0u64;
    'retry: loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        let mut node = mas_mn(mas);
        let mut t = mte_node_type(mas.node);
        let mut pivots = ma_pivots(node, t);
        if self.mas_rewalk_if_dead(mas, node, save_point) {
            continue 'retry;
        }
        if mas.min <= min {
            let pivot = self.mas_safe_min(mas, pivots, mas.offset);
            if self.mas_rewalk_if_dead(mas, node, save_point) {
                continue 'retry;
            }
            if pivot <= min {
                mas.status = MA_UNDERFLOW;
                return 0;
            }
        }
        let mut __lim = 0u64;
        'again: loop {
            __lim += 1;
            if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
            let entry;
            if mas.offset != 0 {
                mas.offset = mas.offset.wrapping_sub(1);
                mas.last = mas.index.wrapping_sub(1);
                mas.index = self.mas_safe_min(mas, pivots, mas.offset);
            } else {
                if mas.index <= min {
                    mas.status = MA_UNDERFLOW;
                    return 0;
                }
                if self.mas_prev_node(mas, min) != 0 {
                    self.mas_rewalk(mas, save_point);
                    continue 'retry;
                }
                if mas_is_underflow(mas) {
                    self.warn_on_once(6, true);
                    return 0;
                }
                mas.last = mas.max;
                node = mas_mn(mas);
                t = mte_node_type(mas.node);
                pivots = ma_pivots(node, t);
                mas.index = self.rd(pivots + 8u64.wrapping_mul((mas.offset as u64).wrapping_sub(1))) + 1;
            }
            let slots = ma_slots(node, t);
            entry = self.mas_slot(mas, slots, mas.offset);
            if self.mas_rewalk_if_dead(mas, node, save_point) {
                continue 'retry;
            }
            if entry != 0 {
                return entry;
            }
            if !empty {
                if mas.index <= min {
                    mas.status = MA_UNDERFLOW;
                    return 0;
                }
                continue 'again;
            }
            return entry;
        }
    }
}


pub fn mas_next_node(&mut self, mas: &mut Mas, node0: u64, max: u64) -> i32 {
    let mut node = node0;
    if mas.max >= max {
        if self.ma_dead_node(node) {
            return 1;
        }
        mas.status = MA_OVERFLOW;
        return 0;
    }
    let min = mas.max.wrapping_add(1);
    let mut level = 0;
    let mut mt;
    let mut pivots;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        if self.ma_is_root(node) {
            if self.ma_dead_node(node) {
                return 1;
            }
            mas.status = MA_OVERFLOW;
            return 0;
        }
        if self.mas_ascend(mas) != 0 {
            return 1;
        }
        level += 1;
        node = mas_mn(mas);
        mt = mte_node_type(mas.node);
        pivots = ma_pivots(node, mt);
        let node_end = self.ma_data_end(node, mt, pivots, mas.max);
        if self.ma_dead_node(node) {
            return 1;
        }
        if mas.offset != node_end {
            break;
        }
    }
    mt = mte_node_type(mas.node);
    pivots = ma_pivots(node, mt);
    let slots = ma_slots(node, mt);
    mas.offset += 1;
    let mut enode = self.mas_slot(mas, slots, mas.offset);
    if self.ma_dead_node(node) {
        return 1;
    }
    if level > 1 {
        mas.offset = 0;
    }
    while level > 1 {
        level -= 1;
        mas.node = enode;
        node = mas_mn(mas);
        mt = mte_node_type(mas.node);
        let slots = ma_slots(node, mt);
        enode = self.mas_slot(mas, slots, 0);
        if self.ma_dead_node(node) {
            return 1;
        }
    }
    if mas.offset == 0 {
        pivots = ma_pivots(node, mt);
    }
    mas.max = self.mas_safe_pivot(mas, pivots, mas.offset, mt);
    let tmp = mte_to_node(enode);
    mt = mte_node_type(enode);
    pivots = ma_pivots(tmp, mt);
    mas.end = self.ma_data_end(tmp, mt, pivots, mas.max);
    if self.ma_dead_node(node) {
        return 1;
    }
    mas.node = enode;
    mas.min = min;
    0
}


pub fn mas_next_slot(&mut self, mas: &mut Mas, max: u64, empty: bool) -> u64 {
    let save_point = mas.last;
    let mut __lim = 0u64;
    'retry: loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        let mut node = mas_mn(mas);
        let mut t = mte_node_type(mas.node);
        let mut pivots = ma_pivots(node, t);
        if self.mas_rewalk_if_dead(mas, node, save_point) {
            continue 'retry;
        }
        if mas.max >= max {
            let pivot = if mas.offset < mas.end {
                self.rd(pivots + 8 * mas.offset as u64)
            } else {
                mas.max
            };
            if self.mas_rewalk_if_dead(mas, node, save_point) {
                continue 'retry;
            }
            if pivot >= max {
                mas.status = MA_OVERFLOW;
                return 0;
            }
        }
        let mut do_again;
        if mas.offset < mas.end {
            mas.index = self.rd(pivots + 8 * mas.offset as u64) + 1;
            do_again = true;
        } else {
            if mas.last >= max {
                mas.status = MA_OVERFLOW;
                return 0;
            }
            if self.mas_next_node(mas, node, max) != 0 {
                self.mas_rewalk(mas, save_point);
                continue 'retry;
            }
            if mas_is_overflow(mas) {
                self.warn_on_once(7, true);
                return 0;
            }
            mas.offset = 0;
            mas.index = mas.min;
            node = mas_mn(mas);
            t = mte_node_type(mas.node);
            pivots = ma_pivots(node, t);
            mas.last = self.rd(pivots);
            let slots = ma_slots(node, t);
            let entry = self.mt_slot(mas.tree, slots, mas.offset);
            if self.mas_rewalk_if_dead(mas, node, save_point) {
                continue 'retry;
            }
            if entry != 0 {
                return entry;
            }
            if !empty {
                if mas.last >= max {
                    mas.status = MA_OVERFLOW;
                    return 0;
                }
                mas.index = mas.last.wrapping_add(1);
                do_again = true;
            } else {
                return entry;
            }
        }
        if do_again {
            /* 'again:' in C — offset++/last update, entry check loop */
            let mut __lim = 0u64;
            loop {
                __lim += 1;
                if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
                mas.offset += 1;
                if mas.offset < mas.end {
                    mas.last = self.rd(pivots + 8 * mas.offset as u64);
                } else {
                    mas.last = mas.max;
                }
                let slots = ma_slots(node, t);
                let entry = self.mt_slot(mas.tree, slots, mas.offset);
                if self.mas_rewalk_if_dead(mas, node, save_point) {
                    continue 'retry;
                }
                if entry != 0 {
                    return entry;
                }
                if !empty {
                    if mas.last >= max {
                        mas.status = MA_OVERFLOW;
                        return 0;
                    }
                    mas.index = mas.last.wrapping_add(1);
                    continue;
                }
                return entry;
            }
        }
    }
}


pub fn mas_rev_awalk(
    &mut self,
    mas: &mut Mas,
    size: u64,
    gap_min: &mut u64,
    gap_max: &mut u64,
) -> bool {
    let t = mte_node_type(mas.node);
    let node = mas_mn(mas);
    if mas_is_err(mas) {
        return true;
    }
    if ma_is_dense(t) {
        mas.offset = (mas.index.wrapping_sub(mas.min)) as u8;
        return true;
    }
    let pivots = ma_pivots(node, t);
    let slots = ma_slots(node, t);
    let gaps = ma_gaps(node, t);
    let mut offset = mas.offset;
    let mut min = self.mas_safe_min(mas, pivots, offset);
    while mas.last < min {
        offset = offset.wrapping_sub(1);
        min = self.mas_safe_min(mas, pivots, offset);
    }
    let mut max = self.mas_safe_pivot(mas, pivots, offset, t);
    let mut gap = 0u64;
    let mut asc = false;
    while mas.index <= max {
        gap = 0;
        if gaps != 0 {
            gap = self.rd(gaps + 8 * offset as u64);
        } else if self.mas_slot(mas, slots, offset) == 0 {
            gap = max.wrapping_sub(min).wrapping_add(1);
        }
        if gap != 0 {
            if size <= gap && size <= mas.last.wrapping_sub(min).wrapping_add(1) {
                break;
            }
            if gaps == 0 {
                if offset < 2 {
                    asc = true;
                    break;
                }
                offset -= 2;
                max = self.rd(pivots + 8 * offset as u64);
                min = self.mas_safe_min(mas, pivots, offset);
                continue;
            }
        }
        if offset == 0 {
            asc = true;
            break;
        }
        offset = offset.wrapping_sub(1);
        max = min.wrapping_sub(1);
        min = self.mas_safe_min(mas, pivots, offset);
    }
    if !asc {
        if mas.index > max || size.wrapping_sub(1) > max.wrapping_sub(mas.index) {
            mas_set_err(mas, -EBUSY);
            return false;
        }
        if ma_is_leaf(t) {
            mas.offset = offset;
            *gap_min = min;
            *gap_max = min.wrapping_add(gap).wrapping_sub(1);
            return true;
        }
        mas.node = self.mas_slot(mas, slots, offset);
        mas.min = min;
        mas.max = max;
        mas.offset = self.mas_data_end(mas);
        return false;
    }
    /* ascend: */
    if !self.mte_is_root(mas.node) {
        return false;
    }
    mas_set_err(mas, -EBUSY);
    false
}


pub fn mas_anode_descend(&mut self, mas: &mut Mas, size: u64) -> bool {
    let t = mte_node_type(mas.node);
    if ma_is_dense(t) {
        mas.offset = (mas.index.wrapping_sub(mas.min)) as u8;
        return true;
    }
    let node = mas_mn(mas);
    let pivots = ma_pivots(node, t);
    let slots = ma_slots(node, t);
    let gaps = ma_gaps(node, t);
    let mut offset = mas.offset;
    let mut min = self.mas_safe_min(mas, pivots, offset);
    let data_end = self.ma_data_end(node, t, pivots, mas.max);
    let mut found = false;
    while offset <= data_end {
        let pivot = self.mas_safe_pivot(mas, pivots, offset, t);
        if mas.index > pivot {
            /* next_slot */
            min = pivot.wrapping_add(1);
            if mas.last <= pivot {
                mas_set_err(mas, -EBUSY);
                return true;
            }
            offset += 1;
            continue;
        }
        let gap;
        if gaps != 0 {
            gap = self.rd(gaps + 8 * offset as u64);
        } else if self.mas_slot(mas, slots, offset) == 0 {
            gap = pivot.min(mas.last).wrapping_sub(mas.index.max(min)).wrapping_add(1);
        } else {
            min = pivot.wrapping_add(1);
            if mas.last <= pivot {
                mas_set_err(mas, -EBUSY);
                return true;
            }
            offset += 1;
            continue;
        }
        if gap >= size {
            if ma_is_leaf(t) {
                found = true;
                break;
            }
            mas.node = self.mas_slot(mas, slots, offset);
            mas.min = min;
            mas.max = pivot;
            offset = 0;
            break;
        }
        min = pivot.wrapping_add(1);
        if mas.last <= pivot {
            mas_set_err(mas, -EBUSY);
            return true;
        }
        offset += 1;
    }
    mas.offset = offset;
    found
}


pub fn mas_walk(&mut self, mas: &mut Mas) -> u64 {
    mas_make_walkable(mas);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        let entry = self.mas_state_walk(mas);
        if mas_is_start(mas) {
            continue;
        } else if mas_is_none(mas) {
            mas.index = 0;
            mas.last = ULONG_MAX;
        } else if mas_is_ptr(mas) {
            if mas.index == 0 {
                mas.last = 0;
                return entry;
            }
            mas.index = 1;
            mas.last = ULONG_MAX;
            mas.status = MA_NONE;
            return 0;
        }
        return entry;
    }
}


pub fn mas_rewind_node(&mut self, mas: &mut Mas) -> bool {
    let mut slot;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return false; }
        if self.mte_is_root(mas.node) {
            slot = mas.offset;
            if slot == 0 {
                return false;
            }
            break;
        } else {
            self.mas_ascend(mas);
            slot = mas.offset;
            if slot != 0 {
                break;
            }
        }
    }
    mas.offset = slot.wrapping_sub(1);
    true
}


pub fn mas_skip_node(&mut self, mas: &mut Mas) -> bool {
    if mas_is_err(mas) {
        return false;
    }
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return false; }
        if self.mte_is_root(mas.node) {
            if mas.offset >= self.mas_data_end(mas) {
                mas_set_err(mas, -EBUSY);
                return false;
            }
        } else {
            self.mas_ascend(mas);
        }
        if mas.offset < self.mas_data_end(mas) {
            break;
        }
    }
    mas.offset += 1;
    true
}


pub fn mas_awalk(&mut self, mas: &mut Mas, size: u64) {
    let mut last = 0u64;
    let mut __limw = 0u64;
    while !mas_is_err(mas) && !self.mas_anode_descend(mas, size) {
        __limw += 1;
        if __limw > MAS_ITER_LIMIT { self.warn += 1; return; }
        if last == mas.node {
            self.mas_skip_node(mas);
        } else {
            last = mas.node;
        }
    }
}


pub fn mas_empty_area(&mut self, mas: &mut Mas, min: u64, max: u64, size: u64) -> i64 {
    if min > max {
        return -EINVAL;
    }
    if size == 0 || max.wrapping_sub(min) < size - 1 {
        return -EINVAL;
    }
    if mas_is_start(mas) {
        self.mas_start(mas);
    } else if mas.offset >= 2 {
        mas.offset -= 2;
    } else if !self.mas_skip_node(mas) {
        return -EBUSY;
    }
    if mas_is_none(mas) || mas_is_ptr(mas) {
        return mas_sparse_area(mas, min, max, size, true);
    }
    mas.index = min;
    mas.last = max;
    self.mas_awalk(mas, size);
    if mas_is_err(mas) {
        return xa_err(mas.node);
    }
    let offset = mas.offset;
    let node = mas_mn(mas);
    let mt = mte_node_type(mas.node);
    let pivots = ma_pivots(node, mt);
    let mn = self.mas_safe_min(mas, pivots, offset);
    if mas.index < mn {
        mas.index = mn;
    }
    mas.last = mas.index.wrapping_add(size).wrapping_sub(1);
    mas.end = self.ma_data_end(node, mt, pivots, mas.max);
    0
}


pub fn mas_empty_area_rev(&mut self, mas: &mut Mas, min: u64, max: u64, size: u64) -> i64 {
    let mut last = mas.node;
    if min > max {
        return -EINVAL;
    }
    if size == 0 || max.wrapping_sub(min) < size - 1 {
        return -EINVAL;
    }
    if mas_is_start(mas) {
        self.mas_start(mas);
    } else if mas.offset < 2 && !self.mas_rewind_node(mas) {
        return -EBUSY;
    }
    if mas_is_none(mas) || mas_is_ptr(mas) {
        return mas_sparse_area(mas, min, max, size, false);
    } else if mas.offset >= 2 {
        mas.offset -= 2;
    } else {
        mas.offset = self.mas_data_end(mas);
    }
    mas.index = min;
    mas.last = max;
    let mut gmin = min;
    let mut gmax = max;
    let mut __limw = 0u64;
    while !self.mas_rev_awalk(mas, size, &mut gmin, &mut gmax) {
        __limw += 1;
        if __limw > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        if last == mas.node {
            if !self.mas_rewind_node(mas) {
                return -EBUSY;
            }
        } else {
            last = mas.node;
        }
    }
    if mas_is_err(mas) {
        return xa_err(mas.node);
    }
    if mas.offset == MAPLE_NODE_SLOTS as u8 {
        return -EBUSY;
    }
    if gmax < mas.last {
        mas.last = gmax;
    }
    mas.index = mas.last.wrapping_sub(size).wrapping_add(1);
    mas.end = self.mas_data_end(mas);
    0
}

/* dead-view fields: piv_parent +24, parent_slot +32 (u32), type +36 (u8),
 * slot_len +40 (u8), ma_flags +44 */

pub fn nd_type(&self, node: u64) -> u8 {
    self.b(node, 36)
}

pub fn set_nd_type(&mut self, node: u64, v: u8) {
    self.sb(node, 36, v);
}

pub fn nd_slot_len(&self, node: u64) -> u8 {
    self.b(node, 40)
}

pub fn set_nd_slot_len(&mut self, node: u64, v: u8) {
    self.sb(node, 40, v);
}

pub fn nd_piv_parent(&self, node: u64) -> u64 {
    self.w(node, 3)
}

pub fn set_nd_piv_parent(&mut self, node: u64, v: u64) {
    self.sw(node, 3, v);
}

pub fn nd_parent_slot(&self, node: u64) -> u32 {
    self.g32(node, 32)
}

pub fn set_nd_parent_slot(&mut self, node: u64, v: u32) {
    self.s32(node, 32, v);
}

pub fn nd_ma_flags(&self, node: u64) -> u32 {
    self.g32(node, 44)
}

pub fn set_nd_ma_flags(&mut self, node: u64, v: u32) {
    self.s32(node, 44, v);
}


pub fn mte_dead_leaves(&mut self, enode: u64, mt: u64, slots: u64) -> u8 {
    let mut offset = 0usize;
    while offset < mt_slot_count(enode) {
        let entry = self.mt_slot(mt, slots, offset as u8);
        let t = mte_node_type(entry);
        let node = mte_to_node(entry);
        if node == 0 || t == 0 {
            break;
        }
        self.mte_set_node_dead(entry);
        self.set_nd_type(node, t);
        self.wr(slots + 8 * offset as u64, node);
        offset += 1;
    }
    offset as u8
}


pub fn mte_dead_walk(&mut self, enode: &mut u64, offset: u8) -> u64 {
    /* C: `next` is `struct maple_node *` holding the RAW slot value
    (encoding bits included); `next->type` reads byte next+36. */
    let mut next = mte_to_node(*enode);
    let mut offset = offset;
    let mut slots;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        *enode = ma_enode_ptr(next);
        let node = mte_to_node(*enode);
        slots = ma_slots(node, self.nd_type(node));
        next = self.rd(slots + 8 * offset as u64);
        offset = 0;
        if ma_is_leaf(self.b(next, 36 + (next & 0xff) as usize)) {
            break;
        }
    }
    slots
}


pub fn mt_free_walk(&mut self, node: u64) {
    if ma_is_leaf(self.nd_type(node)) {
        self.kfree(node);
        return;
    }
    let start = node;
    let mut enode = mt_mk_node(node, self.nd_type(node));
    let mut slots = self.mte_dead_walk(&mut enode, 0);
    let mut node = mte_to_node(enode);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        let sl = self.nd_slot_len(node) as usize;
        let mut tmp = [0u64; 32];
        for i in 0..sl.min(32) {
            tmp[i] = self.rd(slots + 8 * i as u64);
        }
        self.mt_free_bulk(sl.min(32), &tmp);
        let offset = self.nd_parent_slot(node) + 1;
        enode = self.nd_piv_parent(node);
        if mte_to_node(enode) == node {
            self.kfree(node);
            return;
        }
        let t = mte_node_type(enode);
        slots = ma_slots(mte_to_node(enode), t);
        if offset < self.nd_slot_len(mte_to_node(enode)) as u32 {
            slots = self.mte_dead_walk(&mut enode, offset as u8);
        }
        node = mte_to_node(enode);
        if node == start && (self.nd_slot_len(node) as u32) < offset {
            break;
        }
    }
    slots = ma_slots(node, self.nd_type(node));
    let sl = self.nd_slot_len(node) as usize;
    let mut tmp = [0u64; 32];
    for i in 0..sl.min(32) {
        tmp[i] = self.rd(slots + 8 * i as u64);
    }
    self.mt_free_bulk(sl.min(32), &tmp);
    self.kfree(node);
}


pub fn mte_destroy_descend(&mut self, enode: &mut u64, mt: u64, prev: u64, offset: u8) -> u64 {
    let mut next = *enode;
    let mut next_offset = 0u8;
    let mut offset = offset;
    let mut prev = prev;
    let mut slots;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        *enode = next;
        let node = mte_to_node(*enode);
        let t = mte_node_type(*enode);
        slots = ma_slots(node, t);
        next = self.mt_slot_locked(mt, slots, next_offset);
        if self.mte_dead_node(next) {
            next_offset += 1;
            next = self.mt_slot_locked(mt, slots, next_offset);
        }
        self.mte_set_node_dead(*enode);
        self.set_nd_type(node, t);
        self.set_nd_piv_parent(node, prev);
        self.set_nd_parent_slot(node, offset as u32);
        offset = next_offset;
        next_offset = 0;
        prev = *enode;
        if mte_is_leaf(next) {
            break;
        }
    }
    slots
}


pub fn mt_destroy_walk(&mut self, enode: u64, mt: u64, free: bool) {
    let mut node = mte_to_node(enode);
    let mut enode = enode;
    if mte_is_leaf(enode) {
        self.mte_set_node_dead(enode);
        self.set_nd_type(node, mte_node_type(enode));
        if free {
            self.kfree(node);
        }
        return;
    }
    let start = enode;
    let mut slots = self.mte_destroy_descend(&mut enode, mt, start, 0);
    node = mte_to_node(enode);
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        let sl = self.mte_dead_leaves(enode, mt, slots);
        self.set_nd_slot_len(node, sl);
        if free {
            let n = self.nd_slot_len(node) as usize;
            let mut tmp = [0u64; 32];
            for i in 0..n.min(32) {
                tmp[i] = self.rd(slots + 8 * i as u64);
            }
            self.mt_free_bulk(n.min(32), &tmp);
        }
        let offset = self.nd_parent_slot(node) + 1;
        enode = self.nd_piv_parent(node);
        if mte_to_node(enode) == node {
            if free {
                self.kfree(node);
            }
            return;
        }
        let t = mte_node_type(enode);
        slots = ma_slots(mte_to_node(enode), t);
        if offset < MT_SLOTS[(t as usize).min(MT_SLOTS.len() - 1)] as u32 {
            let tmp = self.mt_slot_locked(mt, slots, offset as u8);
            if mte_node_type(tmp) != 0 && mte_to_node(tmp) != 0 {
                let parent = enode;
                enode = tmp;
                slots = self.mte_destroy_descend(&mut enode, mt, parent, offset as u8);
            }
        }
        node = mte_to_node(enode);
        if start == enode {
            break;
        }
    }
    node = mte_to_node(enode);
    let sl = self.mte_dead_leaves(enode, mt, slots);
    self.set_nd_slot_len(node, sl);
    if free {
        let n = self.nd_slot_len(node) as usize;
        let mut tmp = [0u64; 32];
        for i in 0..n.min(32) {
            tmp[i] = self.rd(slots + 8 * i as u64);
        }
        self.mt_free_bulk(n.min(32), &tmp);
        self.kfree(node);
    }
}


pub fn mte_destroy_walk(&mut self, enode: u64, mt: u64) {
    let node = mte_to_node(enode);
    if self.mt_in_rcu(mt) {
        self.mt_destroy_walk(enode, mt, false);
        self.mt_free_walk(node);
    } else {
        self.mt_destroy_walk(enode, mt, true);
    }
}


pub fn mas_store(&mut self, mas: &mut Mas, entry: u64) -> u64 {
    let mut wr = MaWrState::new(mas, entry);
    if mas.index > mas.last {
        /* CONFIG_DEBUG_MAPLE_TREE is off in the shim; keep the check
        anyway since wrappers.c compiles upstream verbatim — upstream
        only warns/returns under debug. */
    }
    self.mas_wr_prealloc_setup(&mut wr, mas);
    mas.store_type = self.mas_wr_store_type(&mut wr, mas);
    if mas.mas_flags & MA_STATE_PREALLOC != 0 {
        self.mas_wr_store_entry(&mut wr, mas);
        if mas_is_err(mas) {
            self.warn += 1; /* MAS_WR_BUG_ON */
        }
        return wr.content;
    }
    self.mas_prealloc_calc(&wr, mas, entry);
    if mas.node_request != 0 {
        self.mas_alloc_nodes(mas, GFP_NOWAIT);
        if mas_is_err(mas) {
            return 0;
        }
    }
    self.mas_wr_store_entry(&mut wr, mas);
    self.mas_destroy(mas);
    wr.content
}


pub fn mas_store_gfp(&mut self, mas: &mut Mas, entry: u64, gfp: u32) -> i64 {
    let index = mas.index;
    let last = mas.last;
    let mut wr = MaWrState::new(mas, entry);
    let mut ret = 0i64;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        self.mas_wr_preallocate(&mut wr, mas, entry);
        if self.mas_nomem(mas, gfp) {
            if entry == 0 {
                mas.index = index;
                mas.last = last;
                mas.node = 0;
                mas.status = MA_START;
            }
            continue;
        }
        break;
    }
    if mas_is_err(mas) {
        ret = xa_err(mas.node);
    } else {
        self.mas_wr_store_entry(&mut wr, mas);
    }
    self.mas_destroy(mas);
    ret
}


pub fn mas_store_prealloc(&mut self, mas: &mut Mas, entry: u64) {
    let mut wr = MaWrState::new(mas, entry);
    if mas.store_type == WR_STORE_ROOT {
        self.mas_wr_prealloc_setup(&mut wr, mas);
    } else {
        self.mas_wr_walk_descend(&mut wr, mas);
        if mas.store_type != WR_SPANNING_STORE {
            wr.content = self.mas_slot_locked(mas, wr.slots, mas.offset);
            self.mas_wr_end_piv(&mut wr, mas);
        }
    }
    self.mas_wr_store_entry(&mut wr, mas);
    if mas_is_err(mas) {
        self.warn += 1;
    }
    self.mas_destroy(mas);
}


pub fn mas_preallocate(&mut self, mas: &mut Mas, entry: u64, gfp: u32) -> i64 {
    let mut wr = MaWrState::new(mas, entry);
    self.mas_wr_prealloc_setup(&mut wr, mas);
    mas.store_type = self.mas_wr_store_type(&mut wr, mas);
    self.mas_prealloc_calc(&wr, mas, entry);
    if mas.node_request != 0 {
        mas.mas_flags &= !MA_STATE_PREALLOC;
        self.mas_alloc_nodes(mas, gfp);
        if mas_is_err(mas) {
            let ret = xa_err(mas.node);
            mas.node_request = 0;
            self.mas_destroy(mas);
            self.mas_reset(mas);
            return ret;
        }
    }
    mas.mas_flags |= MA_STATE_PREALLOC;
    0
}


pub fn mas_destroy(&mut self, mas: &mut Mas) {
    mas.mas_flags &= !MA_STATE_PREALLOC;
    self.mas_empty_nodes(mas);
}


pub fn mas_nomem(&mut self, mas: &mut Mas, gfp: u32) -> bool {
    /* defined later in C; forward behaviour per upstream: */
    self.mas_nomem_impl(mas, gfp)
}


pub fn mas_may_activate(&mut self, mas: &mut Mas) {
    if mas.node == 0 {
        mas.status = MA_START;
    } else if mas.index > mas.max || mas.index < mas.min {
        mas.status = MA_START;
    } else {
        mas.status = MA_ACTIVE;
        let _ = self;
    }
}


pub fn mas_next_setup(&mut self, mas: &mut Mas, max: u64, entry: &mut u64) -> bool {
    let was_none = mas_is_none(mas);
    if mas.last >= max {
        mas.status = MA_OVERFLOW;
        return true;
    }
    match mas.status {
        MA_ACTIVE => return false,
        MA_NONE | MA_PAUSE | MA_START => {
            mas.status = MA_START;
            self.mas_walk(mas);
        }
        MA_OVERFLOW => {
            self.mas_may_activate(mas);
        }
        MA_UNDERFLOW => {
            self.mas_may_activate(mas);
            *entry = self.mas_walk(mas);
            if *entry != 0 {
                return true;
            }
        }
        MA_ROOT => {}
        MA_ERROR_S => return true,
        _ => {}
    }
    if mas_is_active(mas) {
        return false;
    }
    if mas_is_ptr(mas) {
        *entry = 0;
        if was_none && mas.index == 0 {
            mas.index = 0;
            mas.last = 0;
            return true;
        }
        mas.index = 1;
        mas.last = ULONG_MAX;
        mas.status = MA_NONE;
        return true;
    }
    if mas_is_none(mas) {
        return true;
    }
    false
}


pub fn mas_next(&mut self, mas: &mut Mas, max: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_next_setup(mas, max, &mut entry) {
        return entry;
    }
    self.mas_next_slot(mas, max, false)
}


pub fn mas_next_range(&mut self, mas: &mut Mas, max: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_next_setup(mas, max, &mut entry) {
        return entry;
    }
    self.mas_next_slot(mas, max, true)
}


pub fn mt_next(&mut self, mt: u64, index: u64, max: u64) -> u64 {
    let mut mas = Mas::new(mt, index, index);
    self.mas_next(&mut mas, max)
}


pub fn mas_prev_setup(&mut self, mas: &mut Mas, min: u64, entry: &mut u64) -> bool {
    if mas.index <= min {
        mas.status = MA_UNDERFLOW;
        return true;
    }
    match mas.status {
        MA_ACTIVE => return false,
        MA_START => {}
        MA_NONE | MA_PAUSE => {
            mas.status = MA_START;
        }
        MA_UNDERFLOW => {
            self.mas_may_activate(mas);
        }
        MA_OVERFLOW => {
            self.mas_may_activate(mas);
            *entry = self.mas_walk(mas);
            if *entry != 0 {
                return true;
            }
        }
        MA_ROOT => {}
        MA_ERROR_S => return true,
        _ => {}
    }
    if mas_is_start(mas) {
        self.mas_walk(mas);
    }
    if mas_is_ptr(mas) {
        if mas.index == 0 {
            mas.status = MA_NONE;
            return true;
        }
        mas.index = 0;
        mas.last = 0;
        *entry = self.mas_root(mas);
        return true;
    }
    if mas_is_none(mas) {
        if mas.index != 0 {
            mas.index = 0;
            mas.last = 0;
            mas.status = MA_ROOT;
            *entry = self.mas_root(mas);
            return true;
        }
        return true;
    }
    false
}


pub fn mas_prev(&mut self, mas: &mut Mas, min: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_prev_setup(mas, min, &mut entry) {
        return entry;
    }
    self.mas_prev_slot(mas, min, false)
}


pub fn mas_prev_range(&mut self, mas: &mut Mas, min: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_prev_setup(mas, min, &mut entry) {
        return entry;
    }
    self.mas_prev_slot(mas, min, true)
}


pub fn mt_prev(&mut self, mt: u64, index: u64, min: u64) -> u64 {
    let mut mas = Mas::new(mt, index, index);
    self.mas_prev(&mut mas, min)
}


pub fn mas_pause(&mut self, mas: &mut Mas) {
    mas.status = MA_PAUSE;
    mas.node = 0;
}


pub fn mas_find_setup(&mut self, mas: &mut Mas, max: u64, entry: &mut u64) -> bool {
    match mas.status {
        MA_ACTIVE => {
            if mas.last < max {
                return false;
            }
            return true;
        }
        MA_START => {}
        MA_PAUSE => {
            if mas.last >= max {
                return true;
            }
            mas.last += 1;
            mas.index = mas.last;
            mas.status = MA_START;
        }
        MA_NONE => {
            if mas.last >= max {
                return true;
            }
            mas.index = mas.last;
            mas.status = MA_START;
        }
        MA_UNDERFLOW => {
            if mas.index >= max {
                mas.status = MA_OVERFLOW;
                return true;
            }
            self.mas_may_activate(mas);
            *entry = self.mas_walk(mas);
            if *entry != 0 {
                return true;
            }
        }
        MA_OVERFLOW => {
            if mas.last >= max {
                return true;
            }
            self.mas_may_activate(mas);
            *entry = self.mas_walk(mas);
            if *entry != 0 {
                return true;
            }
        }
        MA_ROOT => {}
        MA_ERROR_S => return true,
        _ => {}
    }
    if mas_is_start(mas) {
        if mas.index > max {
            return true;
        }
        *entry = self.mas_walk(mas);
        if *entry != 0 {
            return true;
        }
    }
    if mas_is_ptr(mas) {
        mas.status = MA_NONE;
        mas.index = 1;
        mas.last = ULONG_MAX;
        return true;
    }
    if mas_is_none(mas) {
        return true;
    }
    if mas.index == max {
        return true;
    }
    false
}


pub fn mas_find(&mut self, mas: &mut Mas, max: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_find_setup(mas, max, &mut entry) {
        return entry;
    }
    entry = self.mas_next_slot(mas, max, false);
    if mas_is_overflow(mas) {
        mas.status = MA_ACTIVE;
    }
    entry
}


pub fn mas_find_range(&mut self, mas: &mut Mas, max: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_find_setup(mas, max, &mut entry) {
        return entry;
    }
    self.mas_next_slot(mas, max, true)
}


pub fn mas_find_rev_setup(&mut self, mas: &mut Mas, min: u64, entry: &mut u64) -> bool {
    match mas.status {
        MA_ACTIVE => {
            /* goto active */
            return mas.index < min;
        }
        MA_START => {}
        MA_PAUSE => {
            if mas.index <= min {
                mas.status = MA_UNDERFLOW;
                return true;
            }
            mas.index -= 1;
            mas.last = mas.index;
            mas.status = MA_START;
        }
        MA_NONE => {
            if mas.index <= min {
                mas.status = MA_NONE;
                return true;
            }
            mas.last = mas.index;
            mas.status = MA_START;
        }
        MA_OVERFLOW => {
            if mas.index <= min {
                mas.status = MA_UNDERFLOW;
                return true;
            }
            mas.status = MA_ACTIVE;
        }
        MA_UNDERFLOW => {
            if mas.index <= min {
                return true;
            }
            mas.status = MA_ACTIVE;
        }
        MA_ROOT => {}
        MA_ERROR_S => return true,
        _ => {}
    }
    if mas_is_start(mas) {
        if mas.index < min {
            return true;
        }
        *entry = self.mas_walk(mas);
        if *entry != 0 {
            return true;
        }
    }
    if mas_is_ptr(mas) {
        mas.status = MA_NONE;
        return true;
    }
    if mas_is_none(mas) {
        mas.index = 0;
        mas.last = 0;
        mas.status = MA_ROOT;
        *entry = self.mas_root(mas);
        return true;
    }
    /* active: */
    mas.index < min
}


pub fn mas_find_rev(&mut self, mas: &mut Mas, min: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_find_rev_setup(mas, min, &mut entry) {
        return entry;
    }
    self.mas_prev_slot(mas, min, false)
}


pub fn mas_find_range_rev(&mut self, mas: &mut Mas, min: u64) -> u64 {
    let mut entry = 0u64;
    if self.mas_find_rev_setup(mas, min, &mut entry) {
        return entry;
    }
    self.mas_prev_slot(mas, min, true)
}


pub fn mas_nomem_impl(&mut self, mas: &mut Mas, gfp: u32) -> bool {
    if mas.node != ma_error(-ENOMEM) {
        return false;
    }
    if !gfpflags_allow_blocking(gfp) && self.mt_external_lock(mas.tree) {
        self.warn_on_once(8, true);
    }
    self.mas_alloc_nodes(mas, gfp);
    if mas.sheaf == 0 && mas.alloc == 0 {
        return false;
    }
    self.mas_reset(mas);
    true
}


pub fn mas_nomem_nofail(&mut self, mas: &mut Mas, index: u64, last: u64) -> bool {
    if mas.node != ma_error(-ENOMEM) {
        return false;
    }
    let gfp = GFP_KERNEL | GFP_NOFAIL;
    self.mas_alloc_nodes(mas, gfp);
    mas.index = index;
    mas.last = last;
    mas.node = 0;
    mas.status = MA_START;
    true
}


pub fn mas_erase(&mut self, mas: &mut Mas) -> u64 {
    let index = mas.index;
    let mut wr = MaWrState::new(mas, 0);
    mas_make_walkable(mas);
    let mut retries = 0u32;
    let mut entry;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        entry = self.mas_state_walk(mas);
        if entry == 0 {
            break;
        }
        self.mas_reset(mas);
        self.mas_wr_preallocate(&mut wr, mas, 0);
        if self.mas_nomem_nofail(mas, index, index) {
            retries += 1;
            if retries > 1_000_000 {
                /* deliberate bound: C relies on __GFP_NOFAIL making
                progress; the pool allocator cannot guarantee that.
                See wrappers.c comment (same bound, same count). */
                self.warn += 1;
                break;
            }
            continue;
        }
        self.mas_wr_store_entry(&mut wr, mas);
        break;
    }
    self.mas_destroy(mas);
    entry
}


pub fn mtree_load(&mut self, mt: u64, index: u64) -> u64 {
    let mut mas = Mas::new(mt, index, index);
    let mut entry;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        entry = self.mas_start(&mut mas);
        if mas_is_none(&mas) {
            break;
        }
        if mas_is_ptr(&mas) {
            if index != 0 {
                entry = 0;
            }
            break;
        }
        entry = self.mtree_lookup_walk(&mut mas);
        if entry == 0 && mas_is_start(&mas) {
            continue;
        }
        break;
    }
    if xa_is_zero(entry) {
        return 0;
    }
    entry
}


pub fn mtree_store_range(
    &mut self,
    mt: u64,
    index: u64,
    last: u64,
    entry: u64,
    gfp: u32,
) -> i64 {
    let mut mas = Mas::new(mt, index, last);
    if self.warn_on_once(9, xa_is_advanced(entry)) {
        return -EINVAL;
    }
    if index > last {
        return -EINVAL;
    }
    self.mas_store_gfp(&mut mas, entry, gfp)
}


pub fn mtree_store(&mut self, mt: u64, index: u64, entry: u64, gfp: u32) -> i64 {
    self.mtree_store_range(mt, index, index, entry, gfp)
}


pub fn mtree_insert_range(
    &mut self,
    mt: u64,
    first: u64,
    last: u64,
    entry: u64,
    gfp: u32,
) -> i64 {
    let mut ms = Mas::new(mt, first, last);
    if self.warn_on_once(10, xa_is_advanced(entry)) {
        return -EINVAL;
    }
    if first > last {
        return -EINVAL;
    }
    let mut ret = 0i64;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        self.mas_insert(&mut ms, entry);
        if self.mas_nomem_impl(&mut ms, gfp) {
            continue;
        }
        break;
    }
    if mas_is_err(&ms) {
        ret = xa_err(ms.node);
    }
    self.mas_destroy(&mut ms);
    ret
}


pub fn mtree_insert(&mut self, mt: u64, index: u64, entry: u64, gfp: u32) -> i64 {
    self.mtree_insert_range(mt, index, index, entry, gfp)
}


pub fn mas_alloc_cyclic(
    &mut self,
    mas: &mut Mas,
    startp: &mut u64,
    entry: u64,
    range_lo: u64,
    range_hi: u64,
    next: &mut u64,
    gfp: u32,
) -> i64 {
    let min = range_lo;
    let mut range_lo = range_lo;
    let mut ret;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        range_lo = min.max(*next);
        ret = self.mas_empty_area(mas, range_lo, range_hi, 1);
        if ret < 0 && range_lo > min {
            self.mas_reset(mas);
            ret = self.mas_empty_area(mas, min, range_hi, 1);
            if ret == 0 {
                ret = 1;
            }
        }
        if ret < 0 {
            self.mas_destroy(mas);
            return ret;
        }
        self.mas_insert(mas, entry);
        if !self.mas_nomem_impl(mas, gfp) {
            break;
        }
    }
    if mas_is_err(mas) {
        ret = xa_err(mas.node);
        self.mas_destroy(mas);
        return ret;
    }
    let fl = self.tflags(mas.tree);
    if fl & 0x0800 != 0 && ret == 0 {
        self.set_tflags(mas.tree, fl & !0x0800);
        ret = 1;
    }
    *startp = mas.index;
    *next = startp.wrapping_add(1);
    if *next == 0 {
        self.set_tflags(mas.tree, self.tflags(mas.tree) | 0x0800);
    }
    self.mas_destroy(mas);
    ret
}


pub fn mtree_alloc_range(
    &mut self,
    mt: u64,
    startp: &mut u64,
    entry: u64,
    size: u64,
    min: u64,
    max: u64,
    gfp: u32,
) -> i64 {
    let mut mas = Mas::new(mt, 0, 0);
    if !self.mt_is_alloc(mt) {
        return -EINVAL;
    }
    if self.warn_on_once(11, mt_is_reserved(entry)) {
        return -EINVAL;
    }
    let mut ret = 0i64;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        ret = self.mas_empty_area(&mut mas, min, max, size);
        if ret != 0 {
            break;
        }
        self.mas_insert(&mut mas, entry);
        if self.mas_nomem_impl(&mut mas, gfp) {
            continue;
        }
        if mas_is_err(&mas) {
            ret = xa_err(mas.node);
        } else {
            *startp = mas.index;
        }
        break;
    }
    self.mas_destroy(&mut mas);
    ret
}


pub fn mtree_alloc_cyclic(
    &mut self,
    mt: u64,
    startp: &mut u64,
    entry: u64,
    range_lo: u64,
    range_hi: u64,
    next: &mut u64,
    gfp: u32,
) -> i64 {
    let mut mas = Mas::new(mt, 0, 0);
    if !self.mt_is_alloc(mt) {
        return -EINVAL;
    }
    if self.warn_on_once(12, mt_is_reserved(entry)) {
        return -EINVAL;
    }
    self.mas_alloc_cyclic(&mut mas, startp, entry, range_lo, range_hi, next, gfp)
}


pub fn mtree_alloc_rrange(
    &mut self,
    mt: u64,
    startp: &mut u64,
    entry: u64,
    size: u64,
    min: u64,
    max: u64,
    gfp: u32,
) -> i64 {
    let mut mas = Mas::new(mt, 0, 0);
    if !self.mt_is_alloc(mt) {
        return -EINVAL;
    }
    if self.warn_on_once(13, mt_is_reserved(entry)) {
        return -EINVAL;
    }
    let mut ret = 0i64;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        ret = self.mas_empty_area_rev(&mut mas, min, max, size);
        if ret != 0 {
            break;
        }
        self.mas_insert(&mut mas, entry);
        if self.mas_nomem_impl(&mut mas, gfp) {
            continue;
        }
        if mas_is_err(&mas) {
            ret = xa_err(mas.node);
        } else {
            *startp = mas.index;
        }
        break;
    }
    self.mas_destroy(&mut mas);
    ret
}


pub fn mtree_erase(&mut self, mt: u64, index: u64) -> u64 {
    let mut mas = Mas::new(mt, index, index);
    self.mas_erase(&mut mas)
}


pub fn mas_dup_free(&mut self, mas: &mut Mas) {
    if mas_is_none(mas) {
        return;
    }
    let mut __limw = 0u64;
    while !self.mte_is_root(mas.node) {
        __limw += 1;
        if __limw > MAS_ITER_LIMIT { self.warn += 1; return; }
        self.mas_ascend(mas);
        if mas.offset != 0 {
            mas.offset = mas.offset.wrapping_sub(1);
            let mut __lim = 0u64;
            loop {
                __lim += 1;
                if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
                self.mas_descend(mas);
                mas.offset = self.mas_data_end(mas);
                if mte_is_leaf(mas.node) {
                    break;
                }
            }
            self.mas_ascend(mas);
        }
        let node = mte_to_node(mas.node);
        let t = mte_node_type(mas.node);
        let slots = ma_slots(node, t);
        let count = self.mas_data_end(mas) as usize + 1;
        for i in 0..count.min(32) {
            let v = self.rd(slots + 8 * i as u64) & !MAPLE_NODE_MASK;
            self.wr(slots + 8 * i as u64, v);
        }
        let mut tmp = [0u64; 32];
        for i in 0..count.min(32) {
            tmp[i] = self.rd(slots + 8 * i as u64);
        }
        self.mt_free_bulk(count.min(32), &tmp);
    }
    let node = mte_to_node(mas.node);
    self.kfree(node);
}


pub fn mas_copy_node(&mut self, mas: &Mas, new_mas: &Mas, parent: u64) {
    let node = mte_to_node(mas.node);
    let new_node = mte_to_node(new_mas.node);
    self.wcopy(new_node, node, 32);
    let val = self.w(node, 0) & MAPLE_NODE_MASK;
    let np = ma_parent_ptr(val | parent);
    self.sw(new_node, 0, np);
}


pub fn mas_dup_alloc(&mut self, mas: &mut Mas, new_mas: &mut Mas, gfp: u32) {
    let node = mte_to_node(mas.node);
    let new_node = mte_to_node(new_mas.node);
    let t = mte_node_type(mas.node);
    let new_slots = ma_slots(new_node, t);
    let count = self.mas_data_end(mas) + 1;
    mas.node_request = count;
    self.mas_alloc_nodes(mas, gfp);
    if mas_is_err(mas) {
        return;
    }
    let slots = ma_slots(node, t);
    for i in 0..count {
        let val = self.mt_slot_locked(mas.tree, slots, i) & MAPLE_NODE_MASK;
        let nn = self.mas_pop_node(mas).unwrap_or(0);
        self.wr(new_slots + 8 * i as u64, nn | val);
    }
}


pub fn mas_dup_build(&mut self, mas: &mut Mas, new_mas: &mut Mas, gfp: u32) {
    let mut parent = 0u64;
    if self.mt_attr(mas.tree) != self.mt_attr(new_mas.tree) || self.troot(new_mas.tree) != 0 {
        mas_set_err(mas, -EINVAL);
        return;
    }
    let mut root = self.mas_start(mas);
    if mas_is_ptr(mas) || mas_is_none(mas) {
        /* set_new_tree */
        let f = self.tflags(mas.tree);
        self.set_tflags(new_mas.tree, f);
        self.set_troot(new_mas.tree, root);
        self.mas_empty_nodes(mas);
        return;
    }
    let node = match self.mt_alloc_one(gfp) {
        Some(n) => n,
        None => {
            new_mas.status = MA_NONE;
            mas_set_err(mas, -ENOMEM);
            return;
        }
    };
    let t = mte_node_type(mas.node);
    root = mt_mk_node(node, t);
    new_mas.node = root;
    new_mas.min = 0;
    new_mas.max = ULONG_MAX;
    root = mte_mk_root(root);
    let mut it = 0u32;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
        it += 1;
        if it > 100000 {
            panic!("dup spin");
        }
        self.mas_copy_node(mas, new_mas, parent);
        if !mte_is_leaf(mas.node) {
            self.mas_dup_alloc(mas, new_mas, gfp);
            if mas_is_err(mas) {
                self.mas_empty_nodes(mas);
                return;
            }
        } else {
            if mas.max == ULONG_MAX {
                break; /* done */
            }
            let mut asc = 0u32;
            let mut __lim = 0u64;
            loop {
                __lim += 1;
                if __lim > MAS_ITER_LIMIT { self.warn += 1; return; }
                asc += 1;
                self.mas_ascend(mas);
                self.mas_ascend(new_mas);
                if asc > 256 {
                    panic!("asc spin");
                }
                if mas.offset != self.mas_data_end(mas) {
                    break;
                }
            }
            mas.offset += 1;
            new_mas.offset += 1;
        }
        self.mas_descend(mas);
        parent = ma_parent_ptr(mte_to_node(new_mas.node));
        self.mas_descend(new_mas);
        mas.offset = 0;
        new_mas.offset = 0;
    }
    /* done: */
    let rn = mte_to_node(root);
    let par = ma_parent_ptr(mas_tree_parent(new_mas));
    self.sw(rn, 0, par);
    let f = self.tflags(mas.tree);
    self.set_tflags(new_mas.tree, f);
    self.set_troot(new_mas.tree, root);
    self.mas_empty_nodes(mas);
}


pub fn __mt_dup(&mut self, mt: u64, new: u64, gfp: u32) -> i64 {
    let mut mas = Mas::new(mt, 0, 0);
    let mut new_mas = Mas::new(new, 0, 0);
    self.mas_dup_build(&mut mas, &mut new_mas, gfp);
    let mut ret = 0i64;
    if mas_is_err(&mas) {
        ret = xa_err(mas.node);
        if ret == -ENOMEM {
            self.mas_dup_free(&mut new_mas);
        }
    }
    ret
}


pub fn mtree_dup(&mut self, mt: u64, new: u64, gfp: u32) -> i64 {
    self.__mt_dup(mt, new, gfp)
}


pub fn __mt_destroy(&mut self, mt: u64) {
    let root = self.mt_root_locked(mt);
    self.set_troot(mt, 0);
    if xa_is_node(root) {
        self.mte_destroy_walk(root, mt);
    }
    let f = self.mt_attr(mt);
    self.set_tflags(mt, f);
}


pub fn mtree_destroy(&mut self, mt: u64) {
    self.__mt_destroy(mt);
}


pub fn mt_find(&mut self, mt: u64, index: &mut u64, max: u64) -> u64 {
    let mut mas = Mas::new(mt, *index, *index);
    if *index > max {
        return 0;
    }
    let mut entry;
    let mut __lim = 0u64;
    loop {
        __lim += 1;
        if __lim > MAS_ITER_LIMIT { self.warn += 1; return 0; }
        entry = self.mas_state_walk(&mut mas);
        if !mas_is_start(&mas) {
            break;
        }
    }
    if xa_is_zero(entry) {
        entry = 0;
    }
    if entry == 0 {
        while mas_is_active(&mas) && mas.last < max {
            entry = self.mas_next_slot(&mut mas, max, false);
            if entry != 0 && !xa_is_zero(entry) {
                break;
            }
        }
        if xa_is_zero(entry) {
            entry = 0;
        }
    }
    if entry != 0 {
        *index = mas.last.wrapping_add(1);
    }
    entry
}


pub fn mt_find_after(&mut self, mt: u64, index: &mut u64, max: u64) -> u64 {
    if *index == 0 {
        return 0;
    }
    self.mt_find(mt, index, max)
}

}

#[cfg(test)]
mod tests {
    use super::*;
    include!("maple_tree_tests.inc");
}
