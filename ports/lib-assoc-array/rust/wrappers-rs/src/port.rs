//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/assoc_array.c plus the include/linux/{assoc_array,
//! assoc_array_priv}.h inline layer, driven through the same seeded pool
//! harness as the C wrappers.  Every C pointer becomes a canonical u64
//! "address" — the same encodings the C harness folds into its checksums:
//!
//!   tagged assoc_array_ptr values:
//!     NULL              -> 0
//!     leaf i            -> LCANON + i*16        (tag 0b00)
//!     node i            -> NCANON + i*0x1000    (tag 0b01)
//!     shortcut j        -> SCANON + j*0x800     (tag 0b11)
//!   raw object pointers used by the edit script:
//!     edit k            -> EDCANON + k*0x800
//!   struct assoc_array_ptr ** slot addresses:
//!     &array.root                  -> PC_ROOT
//!     &node[i].slots[s]            -> PC_NSLOT + i*64 + s
//!     &node[i].back_pointer        -> PC_NBACK + i
//!     &sc[j].next_node             -> PC_SNEXT + j
//!     &sc[j].back_pointer          -> PC_SBACK + j
//!   u8 * parent_slot addresses:    -> PC_NPSLT + i
//!
//! The pools are fixed arrays inside `Rig`: nodes, shortcuts (with a
//! SCFLEX-word index_key tail), edit scripts and leaf objects.  The
//! allocator mirrors the C pool_alloc exactly: one shared alloc ordinal
//! stream, the same seeded fail_mask, lowest-free-slot-first policy.

#![forbid(unsafe_code)]
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

const NP: usize = 320;
const SP: usize = 200;
const NE: usize = 8;
const NO: usize = 96;
const KW: usize = 2;
const SCFLEX: usize = 4;
const NILX: u64 = 0xffff;

const LCANON: u64 = 0x10000;
const NCANON: u64 = 0x40000;
const SCANON: u64 = 0x200000;
const EDCANON: u64 = 0x600000;
const PC_ROOT: u64 = 0x810000;
const PC_NSLOT: u64 = 0xB10000;
const PC_NBACK: u64 = 0xB20000;
const PC_NPSLT: u64 = 0xB30000;
const PC_SNEXT: u64 = 0xB40000;
const PC_SBACK: u64 = 0xB50000;

const ENOMEM: i64 = 12;
const FAN_OUT: usize = 16;
const FAN_MASK: u64 = 15;
const LEVEL_STEP: i32 = 4;
const LEVEL_STEP_MASK: i32 = 3;
const KEY_CHUNK_SIZE: i32 = 64;
const KEY_CHUNK_MASK: i32 = 63;
const KEY_CHUNK_SHIFT: i32 = 6;

// ---- tagged pointer helpers (assoc_array_priv.h inline layer) ---------------

fn is_meta(p: u64) -> bool {
    p & 1 == 1
}
fn is_leaf(p: u64) -> bool {
    !is_meta(p)
}
fn is_shortcut(p: u64) -> bool {
    p & 2 == 2
}
fn is_node(p: u64) -> bool {
    !is_shortcut(p)
}

/// `assoc_array_ptr_to_leaf`: strip the type tag; result is a leaf object
/// canonical (LCANON + i*16).
fn ptr_to_leaf(p: u64) -> u64 {
    p & !1
}
/// `assoc_array_ptr_to_node`: raw node canonical.
fn node_i(p: u64) -> usize {
    ((p & !3).wrapping_sub(NCANON) / 0x1000) as usize
}
/// `assoc_array_ptr_to_shortcut`: raw shortcut canonical.
fn sc_i(p: u64) -> usize {
    ((p & !3).wrapping_sub(SCANON) / 0x800) as usize
}
/// raw leaf canonical -> object pool index.
fn leaf_i(p: u64) -> usize {
    (ptr_to_leaf(p).wrapping_sub(LCANON) / 16) as usize
}
fn edit_i(p: u64) -> usize {
    (p.wrapping_sub(EDCANON) / 0x800) as usize
}

fn leaf_to_ptr(i: usize) -> u64 {
    LCANON + i as u64 * 16
}
fn node_to_ptr(i: usize) -> u64 {
    NCANON + i as u64 * 0x1000 + 1
}
fn sc_to_ptr(j: usize) -> u64 {
    SCANON + j as u64 * 0x800 + 3
}
fn edit_canon(i: usize) -> u64 {
    EDCANON + i as u64 * 0x800
}

// ---- structures ------------------------------------------------------------

type TKey = [u64; KW];

/// `struct assoc_array_node`.
#[derive(Clone, Copy)]
struct Node {
    used: bool,
    back_pointer: u64, // tagged ptr
    parent_slot: u8,
    slots: [u64; FAN_OUT],
    nr_leaves_on_branch: u64,
}

/// `struct assoc_array_shortcut` + SCFLEX words of trailing index_key[].
#[derive(Clone, Copy)]
struct Scell {
    used: bool,
    back_pointer: u64, // tagged ptr
    parent_slot: i32,
    skip_to_level: i32,
    next_node: u64, // tagged ptr
    index_key: [u64; SCFLEX],
}

/// `struct assoc_array_edit`.  `ops`/`array` are always set in the harness
/// and folded as booleans; `ops_for_excised_subtree` decides whether a
/// destroyed excised subtree frees its leaves.
#[derive(Clone, Copy)]
struct Edit {
    used: bool,
    ops_for_excised_subtree: bool,
    leaf: u64,
    leaf_p: u64, // slot address canonical, 0 = NULL
    dead_leaf: u64,
    new_meta: [u64; 3],
    excised_meta: [u64; 2],
    excised_subtree: u64,
    set_backpointers: [u64; FAN_OUT],
    set_backpointers_to: u64,
    adjust_count_on: u64, // raw node canonical, 0 = NULL
    adjust_count_by: i64,
    set_ptr: [u64; 2], // slot address canonicals
    set_to: [u64; 2],  // tagged ptrs
    sps_p: u64,        // parent_slot (u8*) address canonical
    sps_to: u8,
    segment_cache: [u8; FAN_OUT + 1],
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

struct Rig {
    nodes: [Node; NP],
    shorts: [Scell; SP],
    edits: [Edit; NE],
    okey: [TKey; NO],
    orefs: [i64; NO],
    root: u64,              // array.root (tagged ptr)
    nr_leaves_on_tree: u64, // array.nr_leaves_on_tree
    alloc_ctr: u64,
    fail_mask: u64,
    rng_state: u64,
    g_h: u64,
}

const NODE0: Node = Node {
    used: false,
    back_pointer: 0,
    parent_slot: 0,
    slots: [0; FAN_OUT],
    nr_leaves_on_branch: 0,
};
const SC0: Scell = Scell {
    used: false,
    back_pointer: 0,
    parent_slot: 0,
    skip_to_level: 0,
    next_node: 0,
    index_key: [0; SCFLEX],
};
const EDIT0: Edit = Edit {
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

fn rig_new() -> Rig {
    Rig {
        nodes: [NODE0; NP],
        shorts: [SC0; SP],
        edits: [EDIT0; NE],
        okey: [[0; KW]; NO],
        orefs: [0; NO],
        root: 0,
        nr_leaves_on_tree: 0,
        alloc_ctr: 0,
        fail_mask: 0,
        rng_state: 0,
        g_h: 0x9e3779b97f4a7c15,
    }
}

fn bug_on(c: bool) {
    // The C contract makes these unreachable; a trip is a port bug.
    assert!(!c, "BUG_ON");
}

/// IS_ERR_VALUE: v in the top 4095 addresses (small negative errno).
fn is_err(v: u64) -> bool {
    v >= (-4095i64) as u64
}

// ---- slot-address access -----------------------------------------------------

fn store_p(r: &mut Rig, a: u64, v: u64) {
    if a == PC_ROOT {
        r.root = v;
        return;
    }
    if (PC_NSLOT..PC_NSLOT + (NP * 64) as u64).contains(&a) {
        let d = (a - PC_NSLOT) as usize;
        r.nodes[d / 64].slots[d % 64] = v;
        return;
    }
    if (PC_NBACK..PC_NBACK + NP as u64).contains(&a) {
        r.nodes[(a - PC_NBACK) as usize].back_pointer = v;
        return;
    }
    if (PC_SNEXT..PC_SNEXT + SP as u64).contains(&a) {
        r.shorts[(a - PC_SNEXT) as usize].next_node = v;
        return;
    }
    if (PC_SBACK..PC_SBACK + SP as u64).contains(&a) {
        r.shorts[(a - PC_SBACK) as usize].back_pointer = v;
        return;
    }
    bug_on(true);
}

fn store8_p(r: &mut Rig, a: u64, v: u8) {
    if (PC_NPSLT..PC_NPSLT + NP as u64).contains(&a) {
        r.nodes[(a - PC_NPSLT) as usize].parent_slot = v;
        return;
    }
    bug_on(true);
}

// ---- deterministic allocator ------------------------------------------------

fn alloc_fail_check(r: &mut Rig) -> bool {
    r.alloc_ctr += 1;
    r.alloc_ctr <= 64 && (r.fail_mask >> (r.alloc_ctr - 1)) & 1 == 1
}

/// kzalloc(sizeof(struct assoc_array_node)): raw node canonical or 0.
fn kzalloc_node(r: &mut Rig) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    for i in 0..NP {
        if !r.nodes[i].used {
            r.nodes[i] = NODE0;
            r.nodes[i].used = true;
            return i;
        }
    }
    NIL
}

/// kzalloc/kmalloc(struct_size(shortcut, index_key, keylen)).
fn alloc_sc(r: &mut Rig, keylen: usize) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    bug_on(keylen > SCFLEX);
    for j in 0..SP {
        if !r.shorts[j].used {
            r.shorts[j] = SC0;
            r.shorts[j].used = true;
            return j;
        }
    }
    NIL
}

fn kzalloc_edit(r: &mut Rig) -> usize {
    if alloc_fail_check(r) {
        return NIL;
    }
    for k in 0..NE {
        if !r.edits[k].used {
            r.edits[k] = EDIT0;
            r.edits[k].used = true;
            return k;
        }
    }
    NIL
}

/// kfree() on a raw pool canonical (untagged node/shortcut/edit base).
fn kfree(r: &mut Rig, p: u64) {
    if p == 0 {
        return;
    }
    if p >= NCANON && p < NCANON + (NP * 0x1000) as u64 && (p - NCANON) % 0x1000 == 0 {
        let i = ((p - NCANON) / 0x1000) as usize;
        bug_on(!r.nodes[i].used);
        r.nodes[i].used = false;
        return;
    }
    if p >= SCANON && p < SCANON + (SP * 0x800) as u64 && (p - SCANON) % 0x800 == 0 {
        let j = ((p - SCANON) / 0x800) as usize;
        bug_on(!r.shorts[j].used);
        r.shorts[j].used = false;
        return;
    }
    if p >= EDCANON && p < EDCANON + (NE * 0x800) as u64 && (p - EDCANON) % 0x800 == 0 {
        let k = ((p - EDCANON) / 0x800) as usize;
        bug_on(!r.edits[k].used);
        r.edits[k].used = false;
        return;
    }
    bug_on(true);
}

// ---- the ops table (keyring_assoc_array_ops semantics) -----------------------

fn get_key_chunk(k: &TKey, level: i32) -> u64 {
    let w = level as u64 / KEY_CHUNK_SIZE as u64;
    if (w as usize) < KW { k[w as usize] } else { 0 }
}

fn get_object_key_chunk(r: &Rig, leaf: u64, level: i32) -> u64 {
    let w = level as u64 / KEY_CHUNK_SIZE as u64;
    if (w as usize) < KW {
        r.okey[leaf_i(leaf)][w as usize]
    } else {
        0
    }
}

fn compare_object(r: &Rig, leaf: u64, k: &TKey) -> bool {
    r.okey[leaf_i(leaf)] == *k
}

fn diff_objects(r: &Rig, leaf: u64, k: &TKey) -> i64 {
    let o = &r.okey[leaf_i(leaf)];
    for i in 0..KW {
        let d = o[i] ^ k[i];
        if d != 0 {
            return i as i64 * 64 + d.trailing_zeros() as i64;
        }
    }
    -1
}

fn free_object(r: &mut Rig, leaf: u64) {
    let i = leaf_i(leaf);
    fold(r, 0xfee1);
    fold(r, i as u64);
    r.orefs[i] -= 1;
    fold(r, r.orefs[i] as u64);
}

// ---- rng / checksum -----------------------------------------------------------

fn xs64(r: &mut Rig) -> u64 {
    let mut x = r.rng_state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    r.rng_state = x;
    x.wrapping_mul(2685821657736338717)
}

fn iabs(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}

fn fold(r: &mut Rig, v: u64) {
    r.g_h = (r.g_h ^ v).wrapping_mul(0x100000001b3);
}

// =================== lib/assoc_array.c ===================

/// assoc_array_subtree_iterate: iterative walk; `stop` is the tagged parent
/// pointer that ends the climb (NULL for a full iterate, the node's back
/// pointer for a subtree collapse).  `it` is invoked on each leaf's tagged
/// pointer.
fn subtree_iterate<C>(
    r: &mut Rig,
    root: u64,
    stop: u64,
    it: &mut dyn FnMut(&mut Rig, &mut C, u64) -> i32,
    ctx: &mut C,
) -> i32 {
    let mut cursor = root;
    let mut node;
    let mut slot: i32;

    'begin_node: loop {
        if is_shortcut(cursor) {
            cursor = r.shorts[sc_i(cursor)].next_node;
        }
        node = node_i(cursor);
        slot = 0;

        let mut has_meta = 0u64;
        while slot < FAN_OUT as i32 {
            let ptr = r.nodes[node].slots[slot as usize];
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
            'continue_node: loop {
                node = node_i(cursor);
                while slot < FAN_OUT as i32 {
                    let ptr = r.nodes[node].slots[slot as usize];
                    if is_meta(ptr) {
                        cursor = ptr;
                        continue 'begin_node;
                    }
                    slot += 1;
                }
                break 'continue_node;
            }
        }

        // finished_node
        loop {
            let mut parent = r.nodes[node].back_pointer;
            slot = r.nodes[node].parent_slot as i32;
            if parent == stop {
                return 0;
            }
            if is_shortcut(parent) {
                let s = sc_i(parent);
                parent = r.shorts[s].back_pointer;
                slot = r.shorts[s].parent_slot;
                if parent == stop {
                    return 0;
                }
            }
            cursor = parent;
            slot += 1;
            // continue_node
            node = node_i(cursor);
            while slot < FAN_OUT as i32 {
                let ptr = r.nodes[node].slots[slot as usize];
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
fn assoc_array_iterate<C>(
    r: &mut Rig,
    it: &mut dyn FnMut(&mut Rig, &mut C, u64) -> i32,
    ctx: &mut C,
) -> i32 {
    if r.root == 0 {
        return 0;
    }
    subtree_iterate(r, r.root, 0, it, ctx)
}

fn assoc_array_walk(r: &Rig, index_key: &TKey, result: &mut WalkResult) -> u8 {
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
            if is_shortcut(cursor) {
                // follow_shortcut
                let shortcut = sc_i(cursor);
                sc_level = level + LEVEL_STEP;
                bug_on(sc_level > r.shorts[shortcut].skip_to_level);
                let next_sc;
                loop {
                    if sc_level & KEY_CHUNK_MASK == 0 {
                        segments = get_key_chunk(index_key, sc_level);
                    }
                    let sc_segments =
                        r.shorts[shortcut].index_key[(sc_level >> KEY_CHUNK_SHIFT) as usize];
                    let mut dissimilarity = segments ^ sc_segments;
                    let next_sc_level;
                    if r.shorts[shortcut].skip_to_level
                        < round_down_i32(sc_level, KEY_CHUNK_SIZE) + KEY_CHUNK_SIZE
                    {
                        let shift = (r.shorts[shortcut].skip_to_level & KEY_CHUNK_MASK) as u32;
                        dissimilarity &= !(u64::MAX << shift);
                        next_sc_level = r.shorts[shortcut].skip_to_level;
                    } else {
                        next_sc_level = round_down_i32(sc_level + KEY_CHUNK_SIZE, KEY_CHUNK_SIZE);
                    }
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
                    if sc_level >= r.shorts[shortcut].skip_to_level {
                        break;
                    }
                }
                next_sc = r.shorts[shortcut].next_node;
                cursor = next_sc;
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
            let ptr = r.nodes[node].slots[slot as usize];
            if !is_meta(ptr) {
                result.tn_node = node;
                result.tn_level = level;
                result.tn_slot = slot as i32;
                result.status = 1;
                return 1;
            }
            if is_node(ptr) {
                cursor = ptr;
                level += LEVEL_STEP;
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
fn round_up_i32(x: i32, y: i32) -> i32 {
    (x + y - 1) & !(y - 1)
}

/// assoc_array_find.
fn assoc_array_find(r: &Rig, index_key: &TKey) -> u64 {
    let mut result = WalkResult::default();
    if assoc_array_walk(r, index_key, &mut result) != 1 {
        return 0;
    }
    let node = result.tn_node;
    for slot in 0..FAN_OUT {
        let ptr = r.nodes[node].slots[slot];
        if ptr != 0 && is_leaf(ptr) && compare_object(r, ptr, index_key) {
            return ptr_to_leaf(ptr);
        }
    }
    0
}

/// assoc_array_destroy_subtree.  `free` is whether the ops table's
/// free_object runs on the leaves (the C `ops` parameter: real ops free,
/// NULL ops only reap metadata).
fn destroy_subtree(r: &mut Rig, root: u64, free: bool) {
    let mut cursor = root;
    if cursor == 0 {
        return;
    }
    let mut parent: u64 = 0;
    let mut slot: i32 = -1;

    'move_to_meta: loop {
        if is_shortcut(cursor) {
            let s = sc_i(cursor);
            bug_on(r.shorts[s].back_pointer != parent);
            bug_on(slot != -1 && r.shorts[s].parent_slot != slot);
            parent = cursor;
            cursor = r.shorts[s].next_node;
            slot = -1;
            bug_on(!is_node(cursor));
        }

        let mut node = node_i(cursor);
        bug_on(r.nodes[node].back_pointer != parent);
        bug_on(slot != -1 && r.nodes[node].parent_slot as i32 != slot);
        slot = 0;

        // continue_node / finished_node / ascend loop
        loop {
            while slot < FAN_OUT as i32 {
                let ptr = r.nodes[node].slots[slot as usize];
                slot += 1;
                if ptr == 0 {
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
            }

            // finished_node
            parent = r.nodes[node].back_pointer;
            slot = r.nodes[node].parent_slot as i32;
            kfree(r, NCANON + node as u64 * 0x1000);
            if parent == 0 {
                return;
            }
            if is_shortcut(parent) {
                let s = sc_i(parent);
                bug_on(r.shorts[s].next_node != cursor);
                parent = r.shorts[s].back_pointer;
                slot = r.shorts[s].parent_slot;
                kfree(r, SCANON + s as u64 * 0x800);
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
fn assoc_array_destroy(r: &mut Rig) {
    destroy_subtree(r, r.root, true);
    r.root = 0;
}

fn assoc_array_insert_in_empty_tree(r: &mut Rig, e: usize) -> bool {
    let new_n0 = kzalloc_node(r);
    if new_n0 == NIL {
        return false;
    }
    r.edits[e].new_meta[0] = node_to_ptr(new_n0);
    r.edits[e].leaf_p = PC_NSLOT + new_n0 as u64 * 64;
    r.edits[e].adjust_count_on = NCANON + new_n0 as u64 * 0x1000;
    r.edits[e].set_ptr[0] = PC_ROOT;
    r.edits[e].set_to[0] = node_to_ptr(new_n0);
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

fn assoc_array_insert_in_node(r: &mut Rig, e: usize, index_key: &TKey, node: usize) -> bool {
    let mut free_slot: i32 = -1;

    for i in 0..FAN_OUT {
        let ptr = r.nodes[node].slots[i];
        if ptr == 0 {
            free_slot = i as i32;
            continue;
        }
        if is_leaf(ptr) && compare_object(r, ptr, index_key) {
            r.edits[e].leaf_p = PC_NSLOT + node as u64 * 64 + i as u64;
            r.edits[e].dead_leaf = ptr;
            return true;
        }
    }

    if free_slot >= 0 {
        r.edits[e].leaf_p = PC_NSLOT + node as u64 * 64 + free_slot as u64;
        r.edits[e].adjust_count_on = NCANON + node as u64 * 0x1000;
        return true;
    }
    false
}

fn assoc_array_fill_segment_cache(r: &mut Rig, e: usize, node: usize, level: i32) -> bool {
    let mut have_meta = false;
    let mut ed = r.edits[e];
    for i in 0..FAN_OUT {
        let ptr = r.nodes[node].slots[i];
        if is_meta(ptr) {
            ed.segment_cache[i] = 0xff;
            have_meta = true;
            continue;
        }
        let mut base_seg = get_object_key_chunk(r, ptr, level);
        base_seg >>= (level & KEY_CHUNK_MASK) as u32;
        ed.segment_cache[i] = (base_seg & FAN_MASK) as u8;
    }
    r.edits[e] = ed;
    have_meta
}

fn assoc_array_all_one_segment(r: &Rig, e: usize) -> bool {
    for i in 1..FAN_OUT + 1 {
        if r.edits[e].segment_cache[i] != r.edits[e].segment_cache[0] {
            return false;
        }
    }
    true
}

fn assoc_array_split_node(r: &mut Rig, e: usize, node: usize, new_n0: usize, new_n1: usize) {
    r.nodes[new_n1].back_pointer = node_to_ptr(new_n0);
    r.nodes[new_n0].nr_leaves_on_branch = r.nodes[node].nr_leaves_on_branch;
    r.nodes[new_n1].nr_leaves_on_branch = 0;

    // find two matching leaf segments
    let mut slot = 0xffi32;
    'found: {
        for i in 0..FAN_OUT {
            slot = r.edits[e].segment_cache[i] as i32;
            if slot != 0xff {
                for j in i + 1..FAN_OUT + 1 {
                    if r.edits[e].segment_cache[j] as i32 == slot {
                        break 'found;
                    }
                }
            }
        }
        // C falls through to the label with i == FAN_OUT
        bug_on(true);
    }
    bug_on(slot >= FAN_OUT as i32);

    r.nodes[new_n1].parent_slot = slot as u8;

    for i in 0..FAN_OUT {
        if is_meta(r.nodes[node].slots[i]) {
            r.nodes[new_n0].slots[i] = r.nodes[node].slots[i];
        } else {
            r.nodes[new_n0].slots[i] = 0;
        }
    }
    bug_on(r.nodes[new_n0].slots[slot as usize] != 0);
    r.nodes[new_n0].slots[slot as usize] = node_to_ptr(new_n1);

    let mut free_slot: i32 = -1;
    let mut next_slot = 0usize;
    for i in 0..FAN_OUT {
        if is_meta(r.nodes[node].slots[i]) {
            continue;
        }
        if r.edits[e].segment_cache[i] as i32 == slot {
            r.nodes[new_n1].slots[next_slot] = r.nodes[node].slots[i];
            next_slot += 1;
            r.nodes[new_n1].nr_leaves_on_branch += 1;
        } else {
            loop {
                free_slot += 1;
                if r.nodes[new_n0].slots[free_slot as usize] == 0 {
                    break;
                }
            }
            r.nodes[new_n0].slots[free_slot as usize] = r.nodes[node].slots[i];
        }
    }

    if r.edits[e].segment_cache[FAN_OUT] as i32 != slot {
        loop {
            free_slot += 1;
            if r.nodes[new_n0].slots[free_slot as usize] == 0 {
                break;
            }
        }
        r.edits[e].leaf_p = PC_NSLOT + new_n0 as u64 * 64 + free_slot as u64;
        r.edits[e].adjust_count_on = NCANON + new_n0 as u64 * 0x1000;
    } else {
        r.edits[e].leaf_p = PC_NSLOT + new_n1 as u64 * 64 + next_slot as u64;
        r.edits[e].adjust_count_on = NCANON + new_n1 as u64 * 0x1000;
    }

    bug_on(next_slot <= 1);

    r.edits[e].set_backpointers_to = node_to_ptr(new_n0);
    for i in 0..FAN_OUT {
        if r.edits[e].segment_cache[i] == 0xff {
            let ptr = r.nodes[node].slots[i];
            bug_on(is_leaf(ptr));
            if is_node(ptr) {
                r.edits[e].set_backpointers[i] = PC_NBACK + node_i(ptr) as u64;
            } else {
                r.edits[e].set_backpointers[i] = PC_SBACK + sc_i(ptr) as u64;
            }
        }
    }
}

/// assoc_array_insert_shortcut: returns the raw shortcut index or NIL.
fn assoc_array_insert_shortcut(
    r: &mut Rig,
    e: usize,
    index_key: &TKey,
    node: usize,
    level: i32,
    new_n0: usize,
) -> usize {
    // find where the leaves start diverging (min over diff_objects)
    let mut diff: i64 = i64::MAX;
    for i in 0..FAN_OUT {
        let x = diff_objects(r, ptr_to_leaf(r.nodes[node].slots[i]), index_key);
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
    r.edits[e].new_meta[2] = sc_to_ptr(new_s0);

    r.shorts[new_s0].back_pointer = r.nodes[node].back_pointer;
    r.shorts[new_s0].parent_slot = r.nodes[node].parent_slot as i32;

    if r.nodes[node].back_pointer != 0 && is_shortcut(r.nodes[node].back_pointer) {
        let above = sc_i(r.nodes[node].back_pointer);
        r.shorts[new_s0].back_pointer = r.shorts[above].back_pointer;
        r.shorts[new_s0].parent_slot = r.shorts[above].parent_slot;
        r.edits[e].excised_meta[1] = r.nodes[node].back_pointer;
    }
    r.shorts[new_s0].next_node = node_to_ptr(new_n0);
    r.nodes[new_n0].back_pointer = sc_to_ptr(new_s0);
    r.nodes[new_n0].parent_slot = 0;

    let level = diff as i32 & !LEVEL_STEP_MASK;
    r.shorts[new_s0].skip_to_level = level;
    bug_on(level <= 0);

    for i in 0..keylen {
        r.shorts[new_s0].index_key[i] = get_key_chunk(index_key, i as i32 * KEY_CHUNK_SIZE);
    }

    if level & KEY_CHUNK_MASK != 0 {
        let blank = u64::MAX << (level & KEY_CHUNK_MASK) as u32;
        r.shorts[new_s0].index_key[keylen - 1] &= !blank;
    }

    assoc_array_fill_segment_cache(r, e, node, level);
    let mut base_seg = get_key_chunk(index_key, level);
    base_seg >>= (level & KEY_CHUNK_MASK) as u32;
    r.edits[e].segment_cache[FAN_OUT] = (base_seg & FAN_MASK) as u8;
    new_s0
}

fn assoc_array_insert_into_terminal_node(
    r: &mut Rig,
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
    r.edits[e].new_meta[0] = node_to_ptr(new_n0);
    let new_n1 = kzalloc_node(r);
    if new_n1 == NIL {
        return false;
    }
    r.edits[e].new_meta[1] = node_to_ptr(new_n1);

    let have_meta = assoc_array_fill_segment_cache(r, e, node, level);
    r.edits[e].segment_cache[FAN_OUT] = result.tn_slot as u8;

    if !have_meta && assoc_array_all_one_segment(r, e) {
        let new_s0 = assoc_array_insert_shortcut(r, e, index_key, node, level, new_n0);
        if new_s0 == NIL {
            return false;
        }
        r.edits[e].set_ptr[0] =
            parent_link(r.shorts[new_s0].back_pointer, r.shorts[new_s0].parent_slot);
        r.edits[e].set_to[0] = sc_to_ptr(new_s0);
    } else {
        r.nodes[new_n0].back_pointer = r.nodes[node].back_pointer;
        r.nodes[new_n0].parent_slot = r.nodes[node].parent_slot;
        r.edits[e].set_ptr[0] = parent_link(
            r.nodes[new_n0].back_pointer,
            r.nodes[new_n0].parent_slot as i32,
        );
        r.edits[e].set_to[0] = node_to_ptr(new_n0);
    }

    assoc_array_split_node(r, e, node, new_n0, new_n1);
    r.edits[e].excised_meta[0] = node_to_ptr(node);
    true
}

fn assoc_array_insert_mid_shortcut(r: &mut Rig, e: usize, result: &WalkResult) -> bool {
    let shortcut = result.ws_shortcut;
    let level = result.ws_level;
    let sc_level = result.ws_sc_level;
    let sc_segments = result.ws_sc_segments;
    let dissimilarity = result.ws_dissimilarity;

    let mut diff = dissimilarity.trailing_zeros() as i32; // __ffs
    diff &= !LEVEL_STEP_MASK;
    diff += sc_level & !KEY_CHUNK_MASK;

    bug_on(r.shorts[shortcut].back_pointer != 0 && !is_node(r.shorts[shortcut].back_pointer));
    r.edits[e].set_ptr[0] = parent_link(
        r.shorts[shortcut].back_pointer,
        r.shorts[shortcut].parent_slot,
    );

    r.edits[e].excised_meta[0] = sc_to_ptr(shortcut);

    let new_n0 = kzalloc_node(r);
    if new_n0 == NIL {
        return false;
    }
    r.edits[e].new_meta[0] = node_to_ptr(new_n0);
    r.edits[e].adjust_count_on = NCANON + new_n0 as u64 * 0x1000;

    let mut level = level;
    if r.shorts[shortcut].back_pointer != 0 {
        level += LEVEL_STEP;
    }
    if diff > level {
        let keylen = (round_up_i32(diff, KEY_CHUNK_SIZE) >> KEY_CHUNK_SHIFT) as usize;
        let new_s0 = alloc_sc(r, keylen);
        if new_s0 == NIL {
            return false;
        }
        r.edits[e].new_meta[1] = sc_to_ptr(new_s0);
        r.edits[e].set_to[0] = sc_to_ptr(new_s0);
        r.shorts[new_s0].back_pointer = r.shorts[shortcut].back_pointer;
        r.shorts[new_s0].parent_slot = r.shorts[shortcut].parent_slot;
        r.shorts[new_s0].next_node = node_to_ptr(new_n0);
        r.shorts[new_s0].skip_to_level = diff;

        r.nodes[new_n0].back_pointer = sc_to_ptr(new_s0);
        r.nodes[new_n0].parent_slot = 0;

        for i in 0..keylen {
            r.shorts[new_s0].index_key[i] = r.shorts[shortcut].index_key[i];
        }

        if diff & KEY_CHUNK_MASK != 0 {
            let blank = u64::MAX << (diff & KEY_CHUNK_MASK) as u32;
            r.shorts[new_s0].index_key[keylen - 1] &= !blank;
        }
    } else {
        r.edits[e].set_to[0] = node_to_ptr(new_n0);
        r.nodes[new_n0].back_pointer = r.shorts[shortcut].back_pointer;
        r.nodes[new_n0].parent_slot = r.shorts[shortcut].parent_slot as u8;
    }

    let side = node_i(r.shorts[shortcut].next_node);
    r.nodes[new_n0].nr_leaves_on_branch = r.nodes[side].nr_leaves_on_branch;

    let sc_slot = (sc_segments >> (diff & KEY_CHUNK_MASK) as u32) & FAN_MASK;
    let sc_slot_i = sc_slot as usize;

    let level2 = diff + LEVEL_STEP;
    if level2 < r.shorts[shortcut].skip_to_level {
        let keylen = (round_up_i32(r.shorts[shortcut].skip_to_level, KEY_CHUNK_SIZE)
            >> KEY_CHUNK_SHIFT) as usize;
        let new_s1 = alloc_sc(r, keylen);
        if new_s1 == NIL {
            return false;
        }
        r.edits[e].new_meta[2] = sc_to_ptr(new_s1);

        r.shorts[new_s1].back_pointer = node_to_ptr(new_n0);
        r.shorts[new_s1].parent_slot = sc_slot_i as i32;
        r.shorts[new_s1].next_node = r.shorts[shortcut].next_node;
        r.shorts[new_s1].skip_to_level = r.shorts[shortcut].skip_to_level;

        r.nodes[new_n0].slots[sc_slot_i] = sc_to_ptr(new_s1);

        for i in 0..keylen {
            r.shorts[new_s1].index_key[i] = r.shorts[shortcut].index_key[i];
        }

        r.edits[e].set_ptr[1] = PC_NBACK + side as u64;
        r.edits[e].set_to[1] = sc_to_ptr(new_s1);
    } else {
        r.nodes[new_n0].slots[sc_slot_i] = r.shorts[shortcut].next_node;
        r.edits[e].sps_p = PC_NPSLT + side as u64;
        r.edits[e].sps_to = sc_slot_i as u8;
        r.edits[e].set_ptr[1] = PC_NBACK + side as u64;
        r.edits[e].set_to[1] = node_to_ptr(new_n0);
    }

    if sc_slot_i == 0 {
        r.edits[e].leaf_p = PC_NSLOT + new_n0 as u64 * 64 + 1;
    } else {
        r.edits[e].leaf_p = PC_NSLOT + new_n0 as u64 * 64;
    }
    true
}

/// assoc_array_insert: returns an edit canonical or ERR_PTR(-ENOMEM).
fn assoc_array_insert(r: &mut Rig, index_key: &TKey, object: u64) -> u64 {
    bug_on(is_meta(object));

    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edits[e].leaf = object;
    r.edits[e].adjust_count_by = 1;

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
fn assoc_array_insert_set_object(r: &mut Rig, e: usize, object: u64) {
    bug_on(object == 0 || is_meta(object));
    // object is a leaf canonical already tagged
    r.edits[e].leaf = object;
}

fn collapse_iterator(r: &mut Rig, ctx: &mut Collapse, leaf: u64) -> i32 {
    if leaf == ctx.skip_leaf {
        return 0;
    }
    bug_on(ctx.slot >= FAN_OUT as i32);
    let n = ctx.node;
    r.nodes[n].slots[ctx.slot as usize] = leaf;
    ctx.slot += 1;
    0
}

fn assoc_array_delete_collapse(r: &mut Rig, e: usize, node0: usize) -> bool {
    let mut has_meta = false;
    for i in 0..FAN_OUT {
        if is_meta(r.nodes[node0].slots[i]) {
            has_meta = true;
            break;
        }
    }

    let mut parent = node0;
    loop {
        let mut ptr = r.nodes[parent].back_pointer;
        if ptr != 0 && is_shortcut(ptr) {
            ptr = r.shorts[sc_i(ptr)].back_pointer;
        }
        if ptr == 0 {
            break;
        }
        let grandparent = node_i(ptr);
        if r.nodes[grandparent].nr_leaves_on_branch > (FAN_OUT + 1) as u64 {
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
        r.edits[e].new_meta[0] = node_to_ptr(new_n0);

        r.nodes[new_n0].back_pointer = r.nodes[node].back_pointer;
        r.nodes[new_n0].parent_slot = r.nodes[node].parent_slot;
        r.nodes[new_n0].nr_leaves_on_branch = r.nodes[node].nr_leaves_on_branch;
        r.edits[e].adjust_count_on = NCANON + new_n0 as u64 * 0x1000;

        let mut collapse = Collapse {
            node: new_n0,
            skip_leaf: ptr_to_leaf(r.edits[e].dead_leaf),
            slot: 0,
        };
        subtree_iterate(
            r,
            node_to_ptr(node),
            r.nodes[node].back_pointer,
            &mut collapse_iterator,
            &mut collapse,
        );
        bug_on(collapse.slot as u64 != r.nodes[new_n0].nr_leaves_on_branch - 1);

        bug_on(r.nodes[node].back_pointer != 0 && is_leaf(r.nodes[node].back_pointer));
        r.edits[e].set_ptr[1] =
            parent_link(r.nodes[node].back_pointer, r.nodes[node].parent_slot as i32);
        r.edits[e].set_to[1] = node_to_ptr(new_n0);
        r.edits[e].excised_subtree = node_to_ptr(node);
    }
    true
}

/// assoc_array_delete: edit canonical, 0 (not found) or ERR_PTR(-ENOMEM).
fn assoc_array_delete(r: &mut Rig, index_key: &TKey) -> u64 {
    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edits[e].adjust_count_by = -1;

    let mut node: usize = 0;
    let mut slot: usize = FAN_OUT;
    let mut result = WalkResult::default();
    if assoc_array_walk(r, index_key, &mut result) == 1 {
        node = result.tn_node;
        for s in 0..FAN_OUT {
            let ptr = r.nodes[node].slots[s];
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

    r.edits[e].dead_leaf = r.nodes[node].slots[slot];
    r.edits[e].set_ptr[0] = PC_NSLOT + node as u64 * 64 + slot as u64;
    r.edits[e].set_to[0] = 0;
    r.edits[e].adjust_count_on = NCANON + node as u64 * 0x1000;

    if r.nr_leaves_on_tree == 1 {
        r.edits[e].set_ptr[1] = PC_ROOT;
        r.edits[e].set_to[1] = 0;
        r.edits[e].adjust_count_on = 0;
        r.edits[e].excised_subtree = r.root;
        return edit_canon(e);
    }

    if r.nodes[node].nr_leaves_on_branch <= (FAN_OUT + 1) as u64
        && !assoc_array_delete_collapse(r, e, node)
    {
        assoc_array_cancel_edit(r, e);
        return (-ENOMEM) as u64;
    }

    edit_canon(e)
}

/// assoc_array_clear: edit canonical, 0 (empty) or ERR_PTR(-ENOMEM).
fn assoc_array_clear(r: &mut Rig) -> u64 {
    if r.root == 0 {
        return 0;
    }
    let e = kzalloc_edit(r);
    if e == NIL {
        return (-ENOMEM) as u64;
    }
    r.edits[e].set_ptr[1] = PC_ROOT;
    r.edits[e].set_to[1] = 0;
    r.edits[e].excised_subtree = r.root;
    r.edits[e].ops_for_excised_subtree = true;
    edit_canon(e)
}

/// assoc_array_rcu_cleanup — runs synchronously, like the call_rcu shim.
fn rcu_cleanup(r: &mut Rig, e: usize) {
    if r.edits[e].dead_leaf != 0 {
        let leaf = ptr_to_leaf(r.edits[e].dead_leaf);
        free_object(r, leaf);
    }
    for i in 0..2 {
        let m = r.edits[e].excised_meta[i];
        if m != 0 {
            kfree(r, m & !3);
        }
    }

    if r.edits[e].excised_subtree != 0 {
        bug_on(is_leaf(r.edits[e].excised_subtree));
        if is_node(r.edits[e].excised_subtree) {
            let n = node_i(r.edits[e].excised_subtree);
            r.nodes[n].back_pointer = 0;
        } else {
            let s = sc_i(r.edits[e].excised_subtree);
            r.shorts[s].back_pointer = 0;
        }
        let sub = r.edits[e].excised_subtree;
        let free = r.edits[e].ops_for_excised_subtree;
        destroy_subtree(r, sub, free);
    }

    r.edits[e].used = false;
}

/// assoc_array_apply_edit.
fn assoc_array_apply_edit(r: &mut Rig, e: usize) {
    if r.edits[e].leaf_p != 0 {
        let (a, v) = (r.edits[e].leaf_p, r.edits[e].leaf);
        store_p(r, a, v);
    }
    if r.edits[e].sps_p != 0 {
        let (a, v) = (r.edits[e].sps_p, r.edits[e].sps_to);
        store8_p(r, a, v);
    }
    for i in 0..FAN_OUT {
        if r.edits[e].set_backpointers[i] != 0 {
            let (a, v) = (
                r.edits[e].set_backpointers[i],
                r.edits[e].set_backpointers_to,
            );
            store_p(r, a, v);
        }
    }
    for i in 0..2 {
        if r.edits[e].set_ptr[i] != 0 {
            let (a, v) = (r.edits[e].set_ptr[i], r.edits[e].set_to[i]);
            store_p(r, a, v);
        }
    }

    if r.root == 0 {
        r.nr_leaves_on_tree = 0;
    } else if r.edits[e].adjust_count_on != 0 {
        let mut node = node_i(r.edits[e].adjust_count_on);
        loop {
            r.nodes[node].nr_leaves_on_branch = r.nodes[node]
                .nr_leaves_on_branch
                .wrapping_add(r.edits[e].adjust_count_by as u64);
            let mut ptr = r.nodes[node].back_pointer;
            if ptr == 0 {
                break;
            }
            if is_shortcut(ptr) {
                ptr = r.shorts[sc_i(ptr)].back_pointer;
                if ptr == 0 {
                    break;
                }
            }
            bug_on(!is_node(ptr));
            node = node_i(ptr);
        }
        r.nr_leaves_on_tree = r
            .nr_leaves_on_tree
            .wrapping_add(r.edits[e].adjust_count_by as u64);
    }

    rcu_cleanup(r, e);
}

/// assoc_array_cancel_edit.
fn assoc_array_cancel_edit(r: &mut Rig, e: usize) {
    for i in 0..3 {
        let ptr = r.edits[e].new_meta[i];
        if ptr != 0 {
            if is_node(ptr) {
                kfree(r, ptr & !3);
            } else {
                kfree(r, ptr & !3);
            }
        }
    }
    r.edits[e].used = false;
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

fn gc_write_link(r: &mut Rig, w: &mut GcWalk, link: GcLink, v: u64) {
    match link {
        GcLink::WRoot => w.new_root = v,
        GcLink::Slot(n, s) => r.nodes[n].slots[s] = v,
        GcLink::ScNext(s) => r.shorts[s].next_node = v,
    }
}

fn assoc_array_gc_copy(r: &mut Rig, w: &mut GcWalk, cursor0: u64, link0: GcLink) -> bool {
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
        let keylen = (round_up_i32(r.shorts[shortcut].skip_to_level, KEY_CHUNK_SIZE)
            >> KEY_CHUNK_SHIFT) as usize;
        let new_s = alloc_sc(r, keylen);
        if new_s == NIL {
            kfree(r, NCANON + new_n as u64 * 0x1000);
            return false;
        }
        // memcpy(new_s, shortcut, struct_size) then patch
        let src = r.shorts[shortcut];
        r.shorts[new_s] = src;
        r.shorts[new_s].used = true;
        r.shorts[new_s].back_pointer = new_parent;
        r.shorts[new_s].parent_slot = src.parent_slot;
        gc_write_link(r, w, link, sc_to_ptr(new_s));
        new_parent = sc_to_ptr(new_s);
        link = GcLink::ScNext(new_s);
        cursor = src.next_node;
    }

    w.node = node_i(cursor);
    r.nodes[new_n].back_pointer = new_parent;
    r.nodes[new_n].parent_slot = r.nodes[w.node].parent_slot;
    gc_write_link(r, w, link, node_to_ptr(new_n));
    w.new_n = new_n;
    w.slot = 0;
    true
}

fn assoc_array_gc_next<C>(
    r: &mut Rig,
    w: &mut GcWalk,
    it: &mut dyn FnMut(&mut Rig, &mut C, u64) -> bool,
    ctx: &mut C,
) -> u64 {
    while w.slot < FAN_OUT as i32 {
        let ptr = r.nodes[w.node].slots[w.slot as usize];
        if ptr == 0 {
            w.slot += 1;
            continue;
        }
        if is_leaf(ptr) {
            if it(r, ctx, ptr_to_leaf(ptr)) {
                let nn = w.new_n;
                r.nodes[nn].slots[w.slot as usize] = ptr;
            }
            w.slot += 1;
            continue;
        }
        return ptr;
    }
    0
}

fn assoc_array_gc_fold(r: &mut Rig, new_n: usize) -> i32 {
    'retry_compress: loop {
        r.nodes[new_n].nr_leaves_on_branch = 0;
        let mut nr_free: i32 = 0;
        for slot in 0..FAN_OUT {
            let ptr = r.nodes[new_n].slots[slot];
            if ptr == 0 {
                nr_free += 1;
            } else if is_leaf(ptr) {
                r.nodes[new_n].nr_leaves_on_branch += 1;
            }
        }

        let mut retained = false;
        let mut next_slot = 0usize;
        for slot in 0..FAN_OUT {
            let mut ptr = r.nodes[new_n].slots[slot];
            if ptr == 0 || is_leaf(ptr) {
                continue;
            }
            let mut have_s = false;
            if is_shortcut(ptr) {
                have_s = true;
                ptr = r.shorts[sc_i(ptr)].next_node;
            }
            let child = node_i(ptr);
            r.nodes[new_n].nr_leaves_on_branch += r.nodes[child].nr_leaves_on_branch;

            if r.nodes[child].nr_leaves_on_branch <= (nr_free + 1) as u64 {
                // fold the child node into this one
                bug_on(have_s);
                r.nodes[new_n].slots[slot] = 0;
                nr_free += 1;
                if slot < next_slot {
                    next_slot = slot;
                }
                for i in 0..FAN_OUT {
                    let p = r.nodes[child].slots[i];
                    if p == 0 {
                        continue;
                    }
                    bug_on(is_meta(p));
                    while r.nodes[new_n].slots[next_slot] != 0 {
                        next_slot += 1;
                    }
                    bug_on(next_slot >= FAN_OUT);
                    r.nodes[new_n].slots[next_slot] = p;
                    next_slot += 1;
                    nr_free -= 1;
                }
                r.nodes[child].used = false;
            } else {
                retained = true;
            }
        }

        if retained && r.nodes[new_n].nr_leaves_on_branch <= FAN_OUT as u64 {
            continue 'retry_compress;
        }
        return nr_free;
    }
}

/// assoc_array_gc_pack: returns the copy's parent tagged ptr, or 0 at root.
fn assoc_array_gc_pack(r: &mut Rig, w: &mut GcWalk) -> u64 {
    let new_n = w.new_n;
    let nr_free = assoc_array_gc_fold(r, new_n);
    w.nr_leaves_on_tree = r.nodes[new_n].nr_leaves_on_branch;

    if nr_free == (FAN_OUT - 1) as i32 {
        let mut slot0 = 0usize;
        let mut ptr = 0u64;
        for slot in 0..FAN_OUT {
            ptr = r.nodes[new_n].slots[slot];
            if ptr != 0 {
                slot0 = slot;
                break;
            }
        }
        if is_meta(ptr) && is_shortcut(ptr) {
            let new_s = sc_i(ptr);
            let mut new_parent = r.nodes[new_n].back_pointer;
            let mut slot = r.nodes[new_n].parent_slot as i32;
            r.nodes[new_n].used = false;
            if new_parent == 0 {
                r.shorts[new_s].back_pointer = 0;
                r.shorts[new_s].parent_slot = 0;
                w.new_root = ptr;
                return 0;
            }
            if is_shortcut(new_parent) {
                let s = sc_i(new_parent);
                new_parent = r.shorts[s].back_pointer;
                slot = r.shorts[s].parent_slot;
                r.shorts[new_s].back_pointer = new_parent;
                r.shorts[new_s].parent_slot = slot;
                r.shorts[s].used = false;
                if new_parent == 0 {
                    r.shorts[new_s].back_pointer = 0;
                    r.shorts[new_s].parent_slot = 0;
                    w.new_root = ptr;
                    return 0;
                }
            }
            r.shorts[new_s].back_pointer = new_parent;
            r.shorts[new_s].parent_slot = slot;
            r.nodes[node_i(new_parent)].slots[slot as usize] = ptr;
            return new_parent;
        }
        let _ = slot0;
    }

    let ptr = r.nodes[new_n].back_pointer;
    if ptr == 0 {
        return 0;
    }
    if !is_shortcut(ptr) {
        return ptr;
    }

    let new_s = sc_i(ptr);
    let new_parent = r.shorts[new_s].back_pointer;
    let slot = r.shorts[new_s].parent_slot;

    if r.nodes[new_n].nr_leaves_on_branch <= FAN_OUT as u64 {
        r.nodes[new_n].back_pointer = new_parent;
        r.nodes[new_n].parent_slot = slot as u8;
        r.shorts[new_s].used = false;
        if new_parent == 0 {
            w.new_root = node_to_ptr(new_n);
            return 0;
        }
        r.nodes[node_i(new_parent)].slots[slot as usize] = node_to_ptr(new_n);
    }
    new_parent
}

fn assoc_array_gc_ascend(r: &Rig, w: &mut GcWalk, parent: u64) {
    w.new_n = node_i(parent);

    let ptr = r.nodes[w.node].back_pointer;
    let slot;
    let cursor;
    if is_shortcut(ptr) {
        let s = sc_i(ptr);
        slot = r.shorts[s].parent_slot;
        cursor = r.shorts[s].back_pointer;
    } else {
        slot = r.nodes[w.node].parent_slot as i32;
        cursor = ptr;
    }
    bug_on(cursor == 0);
    w.node = node_i(cursor);
    w.slot = slot + 1;
}

/// assoc_array_gc.
fn assoc_array_gc<C>(
    r: &mut Rig,
    it: &mut dyn FnMut(&mut Rig, &mut C, u64) -> bool,
    ctx: &mut C,
) -> i32 {
    if r.root == 0 {
        return 0;
    }
    let e = kzalloc_edit(r);
    if e == NIL {
        return -ENOMEM as i32;
    }
    r.edits[e].ops_for_excised_subtree = true;
    r.edits[e].set_ptr[0] = PC_ROOT;
    r.edits[e].excised_subtree = r.root;

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
        if !assoc_array_gc_copy(r, &mut w, ptr, link) {
            // enomem: destroy the partially built copy, free the edit
            let nr = w.new_root;
            destroy_subtree(r, nr, true);
            r.edits[e].used = false;
            return -ENOMEM as i32;
        }
        loop {
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

    r.edits[e].set_to[0] = w.new_root;
    assoc_array_apply_edit(r, e);
    r.nr_leaves_on_tree = w.nr_leaves_on_tree;
    0
}

// =================== harness (wrappers.c) ===================

fn gen_key(r: u64) -> TKey {
    match r % 8 {
        0 | 1 => [(r >> 8) % 48, 0],
        2 => [(r >> 8) % 65536, 0],
        3 => [0x12345 | (((r >> 8) % 256) << 32), (r >> 16) % 4],
        4 => [0xABCDEF0123 | (((r >> 8) % 1024) << 40), (r >> 24) % 8],
        5 => [0x777777777777777, (r >> 8) % 24],
        6 => [1 + (r >> 8) % 4, 0],
        _ => [(r >> 8) & 0xffffffff, (r >> 40) & 0xffffff],
    }
}

fn cluster_key(fam: u64, v: u64) -> TKey {
    match fam & 3 {
        0 => [0x20 | ((v % (1 << 24)) << 20), v % 64],
        1 => [0x777 | ((v % (1 << 16)) << 32), v % 32],
        2 => [0x777777777777777, v % 96],
        _ => [0xABCDEF0123 | ((v % 256) << 40), v % 16],
    }
}

fn live_key(r: &Rig, rv: u64) -> Option<TKey> {
    let mut want = 1 + (rv >> 13) % 8;
    for i in 0..NO {
        if r.orefs[i] > 0 {
            want -= 1;
            if want == 0 {
                return Some(r.okey[i]);
            }
        }
    }
    None
}

fn pick_free_obj(r: &Rig, rv: u64) -> i32 {
    let start = ((rv >> 8) % NO as u64) as usize;
    for i in 0..NO {
        let k = (start + i) % NO;
        if r.orefs[k] == 0 {
            return k as i32;
        }
    }
    -1
}

fn fold_edit(r: &mut Rig, e: usize) {
    let ed = r.edits[e];
    fold(r, edit_canon(e));
    fold(r, 1); // ops != NULL
    fold(r, ed.ops_for_excised_subtree as u64);
    fold(r, ed.leaf);
    fold(r, ed.leaf_p);
    fold(r, ed.dead_leaf);
    for i in 0..3 {
        fold(r, ed.new_meta[i]);
    }
    for i in 0..2 {
        fold(r, ed.excised_meta[i]);
    }
    fold(r, ed.excised_subtree);
    for i in 0..FAN_OUT {
        fold(r, ed.set_backpointers[i]);
    }
    fold(r, ed.set_backpointers_to);
    fold(r, ed.adjust_count_on);
    fold(r, ed.adjust_count_by as u64);
    for i in 0..2 {
        fold(r, ed.set_ptr[i]);
        fold(r, ed.set_to[i]);
    }
    fold(r, ed.sps_p);
    fold(r, ed.sps_to as u64);
    for i in 0..FAN_OUT + 1 {
        fold(r, ed.segment_cache[i] as u64);
    }
}

fn dump_state(r: &mut Rig) {
    fold(r, r.root);
    fold(r, r.nr_leaves_on_tree);

    let mut nused = 0u64;
    for i in 0..NP {
        if r.nodes[i].used {
            nused += 1;
        }
    }
    fold(r, nused);
    for i in 0..NP {
        if !r.nodes[i].used {
            continue;
        }
        let n = r.nodes[i];
        fold(r, i as u64);
        fold(r, n.parent_slot as u64);
        fold(r, n.back_pointer);
        fold(r, n.nr_leaves_on_branch);
        for s in 0..FAN_OUT {
            fold(r, n.slots[s]);
        }
    }
    fold(r, NILX);

    let mut sused = 0u64;
    for j in 0..SP {
        if r.shorts[j].used {
            sused += 1;
        }
    }
    fold(r, sused);
    for j in 0..SP {
        if !r.shorts[j].used {
            continue;
        }
        let sc = r.shorts[j];
        fold(r, j as u64);
        fold(r, sc.parent_slot as u64);
        fold(r, sc.skip_to_level as u64);
        fold(r, sc.back_pointer);
        fold(r, sc.next_node);
        let mut kl = (round_up_i32(sc.skip_to_level, KEY_CHUNK_SIZE) >> KEY_CHUNK_SHIFT) as usize;
        if kl > SCFLEX {
            kl = SCFLEX;
        }
        for s in 0..kl {
            fold(r, sc.index_key[s]);
        }
    }
    fold(r, NILX);

    let mut live = 0u64;
    for i in 0..NO {
        if r.orefs[i] > 0 {
            live += 1;
        }
    }
    fold(r, live);
    for i in 0..NO {
        if r.orefs[i] <= 0 {
            continue;
        }
        fold(r, i as u64);
        fold(r, r.orefs[i] as u64);
        fold(r, r.okey[i][0]);
        fold(r, r.okey[i][1]);
    }
    fold(r, NILX);
}

fn reset_all() -> Rig {
    rig_new()
}

fn op_insert_key(r: &mut Rig, k: &TKey, rv: u64) {
    let oi = pick_free_obj(r, rv >> 28);
    if oi < 0 {
        fold(r, NILX);
        return;
    }
    let oi = oi as usize;
    let mut held = oi;
    r.okey[oi] = *k;
    r.orefs[oi] += 1;

    let edit = assoc_array_insert(r, k, leaf_to_ptr(oi));
    if is_err(edit) {
        fold(r, edit);
        r.orefs[held] -= 1;
        return;
    }
    fold(r, 0xe1);
    let e = edit_i(edit);
    fold_edit(r, e);

    if rv & 0x8000 != 0 {
        let oi2 = pick_free_obj(r, rv >> 40);
        fold(r, if oi2 < 0 { NILX } else { oi2 as u64 });
        if oi2 >= 0 {
            let oi2 = oi2 as usize;
            r.okey[oi2] = *k;
            r.orefs[oi2] += 1;
            assoc_array_insert_set_object(r, e, leaf_to_ptr(oi2));
            r.orefs[oi] -= 1;
            held = oi2;
            fold(r, r.edits[e].leaf);
        }
    }

    if rv & 0x10000 != 0 {
        fold(r, 0xca9ce1);
        assoc_array_cancel_edit(r, e);
        r.orefs[held] -= 1;
    } else {
        fold(r, 0xa991e);
        assoc_array_apply_edit(r, e);
    }
}

fn op_insert(r: &mut Rig, rv: u64) {
    let k = gen_key(rv >> 4);
    op_insert_key(r, &k, rv);
}

fn op_insert_burst(r: &mut Rig, rv: u64) {
    let n = 1 + (rv >> 10) % 4;
    for _ in 0..n {
        let r2 = xs64(r);
        let k = gen_key(r2 >> 4);
        op_insert_key(r, &k, r2 & !0x18000u64);
    }
}

fn op_cluster(r: &mut Rig, rv: u64) {
    let fam = (rv >> 4) % 4;
    let base_v = rv >> 12;
    let n = 4 + (rv >> 24) % 9;
    for i in 0..n {
        let r2 = xs64(r);
        let k = cluster_key(fam, base_v.wrapping_add(i.wrapping_mul(0x9e3779b9)));
        op_insert_key(r, &k, r2 & !0x18000u64);
    }
}

fn op_delete(r: &mut Rig, rv: u64) {
    // C: if (!(r & 0x40000) || !live_key(&k, r)) gen_key(&k, r >> 4);
    let k = if rv & 0x40000 != 0 {
        match live_key(r, rv) {
            Some(k) => k,
            None => gen_key(rv >> 4),
        }
    } else {
        gen_key(rv >> 4)
    };
    let edit = assoc_array_delete(r, &k);
    if edit == 0 {
        fold(r, NILX);
        return;
    }
    if is_err(edit) {
        fold(r, edit);
        return;
    }
    fold(r, 0xde1);
    let e = edit_i(edit);
    fold_edit(r, e);
    if rv & 0x20000 != 0 {
        fold(r, 0xca9);
        assoc_array_cancel_edit(r, e);
    } else {
        assoc_array_apply_edit(r, e);
    }
}

fn op_find(r: &mut Rig, rv: u64) {
    let k = if rv & 0x40000 != 0 {
        match live_key(r, rv) {
            Some(k) => k,
            None => gen_key(rv >> 4),
        }
    } else {
        gen_key(rv >> 4)
    };
    let obj = assoc_array_find(r, &k);
    fold(r, obj);
}

struct IterCtx {
    stop_after: i32,
    n: i32,
}

fn iter_cb(r: &mut Rig, c: &mut IterCtx, leaf: u64) -> i32 {
    fold(r, 0x17e7);
    fold(r, leaf);
    c.n += 1;
    if c.n >= c.stop_after { 13 + c.n } else { 0 }
}

fn op_iterate(r: &mut Rig, rv: u64) {
    let mut c = IterCtx {
        stop_after: if rv & 0x8000 != 0 {
            1 + ((rv >> 16) % 5) as i32
        } else {
            1000000
        },
        n: 0,
    };
    fold(r, 0x17aa);
    let ret = assoc_array_iterate(r, &mut iter_cb, &mut c);
    fold(r, ret as i64 as u64);
    fold(r, c.n as u64);
}

fn op_clear(r: &mut Rig, rv: u64) {
    let edit = assoc_array_clear(r);
    if edit == 0 {
        fold(r, NILX);
        return;
    }
    if is_err(edit) {
        fold(r, edit);
        return;
    }
    fold(r, 0xc1ea);
    let e = edit_i(edit);
    fold_edit(r, e);
    if rv & 0x20000 != 0 {
        fold(r, 0xca9);
        assoc_array_cancel_edit(r, e);
    } else {
        assoc_array_apply_edit(r, e);
    }
}

struct GcCtx {
    mode: u64,
}

fn gc_cb(r: &mut Rig, c: &mut GcCtx, leaf: u64) -> bool {
    let i = leaf_i(leaf);
    let keep = match c.mode % 4 {
        0 => true,
        1 => i & 1 == 1,
        2 => r.okey[i][0] & 1 == 1,
        _ => i % 4 != 0,
    };
    fold(r, keep as u64);
    if keep {
        r.orefs[i] += 1;
    }
    keep
}

fn op_gc(r: &mut Rig, rv: u64) {
    let mut c = GcCtx { mode: rv >> 8 };
    fold(r, 0x9c);
    let ret = assoc_array_gc(r, &mut gc_cb, &mut c);
    fold(r, ret as i64 as u64);
}

fn op_destroy(r: &mut Rig, rv: u64) {
    fold(r, 0xde57);
    assoc_array_destroy(r);
    fold(r, r.root);
    fold(r, r.nr_leaves_on_tree);
    if rv & 0x80000 != 0 {
        r.root = 0;
        r.nr_leaves_on_tree = 0;
    }
    fold(r, r.nr_leaves_on_tree);
}

fn op_misc(r: &mut Rig, rv: u64) {
    match rv % 4 {
        0 => fold(r, r.nr_leaves_on_tree),
        1 => {
            let mut c = IterCtx {
                stop_after: 1000000,
                n: 0,
            };
            let ret = assoc_array_iterate(r, &mut iter_cb, &mut c);
            fold(r, ret as i64 as u64);
        }
        2 => {
            let k = gen_key(rv >> 8);
            let obj = assoc_array_find(r, &k);
            fold(r, obj);
            if obj != 0 {
                let e = assoc_array_delete(r, &k);
                let ok = e != 0 && !is_err(e);
                fold(r, ok as u64);
                if ok {
                    let ei = edit_i(e);
                    fold_edit(r, ei);
                    assoc_array_apply_edit(r, ei);
                }
            }
        }
        _ => {
            let k = gen_key(rv >> 8);
            op_insert_key(r, &k, (rv >> 8) & !0x18000u64);
        }
    }
}

/// `uint64_t wrap_assoc_seq(uint64_t seed, int64_t nops_i, int64_t seed2)`
pub fn wrap_assoc_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let mut r = reset_all();
    let nops = (iabs(nops_i) % 190) as i32;
    let dump_every = 5 + (iabs(seed2) % 7) as i32;
    r.rng_state = seed | 1;
    r.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        r.fail_mask = !0;
    }

    for i in 0..nops {
        let rv = xs64(&mut r);
        match rv % 16 {
            0 | 1 => op_insert(&mut r, rv),
            2 | 3 => op_cluster(&mut r, rv),
            4 => op_insert_burst(&mut r, rv),
            5 | 6 | 7 => op_delete(&mut r, rv),
            8 | 9 => op_find(&mut r, rv),
            10 => op_iterate(&mut r, rv),
            11 => op_gc(&mut r, rv),
            12 => op_clear(&mut r, rv),
            13 => op_destroy(&mut r, rv),
            14 => op_misc(&mut r, rv),
            _ => op_insert(&mut r, rv),
        }
        if i % dump_every == dump_every - 1 {
            dump_state(&mut r);
        }
    }
    dump_state(&mut r);
    let ac = r.alloc_ctr;
    fold(&mut r, ac);
    r.g_h
}
