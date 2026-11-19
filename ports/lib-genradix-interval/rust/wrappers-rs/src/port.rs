//! Rust port of `wrappers.c` — seed-driven scalar drivers over faithful
//! ports of two Linux library units:
//!
//!  * `lib/generic-radix-tree.c` + `include/linux/generic-radix-tree.h`:
//!    a page-granular sparse array. The C macro API is instantiated twice
//!    (u64 and a 24-byte struct, i.e. power-of-2 and non-power-of-2 object
//!    sizes); the model keeps the byte-offset core (`__genradix_*`) and
//!    parameterizes the object-size wrappers exactly like the C macros do.
//!    Nodes live in a caller-owned arena (`GAlloc`) standing in for the
//!    page allocator — including its deterministic failure hook.
//!
//!  * `lib/interval_tree.c` + `include/linux/interval_tree_generic.h`:
//!    an augmented red-black tree. In C the INTERVAL_TREE_DEFINE template
//!    is generic over the node struct, the rb field, the endpoint type and
//!    the augmented field; the port mirrors that by being generic over the
//!    *index model*: nodes are `&mut [RbNode]` pool entries, and the
//!    `start`/`last`/`__subtree_last` "fields" are parallel `Vec<u64>`
//!    arrays in `ItData` — the index-model analogue of ITSTART/ITLAST/
//!    ITSUBTREE selection. The rb machinery is the already-verified
//!    index-arena port of lib/rbtree.c (`klib::rbtree`).
//!
//! The two models live in the sibling `genradix-interval-model` crate;
//! this file keeps the harness plumbing and the wrap_* drivers.
//! Every C pointer becomes a `usize` pool index (`NIL`/`GNIL` for NULL);
//! every `size_t` value is a `u64`. Where C unsigned arithmetic wraps,
//! `wrapping_*` ops reproduce it.
#![forbid(unsafe_code)]

// ================= harness shared =================

const ITN: usize = 64;
const NILX: u64 = 0xffff;
const ITKEY: u64 = 4 * ITN as u64;
const ITSPAN: u64 = 96;

fn xs64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(2685821657736338717)
}

fn iabs(v: i64) -> u64 {
    if v < 0 {
        0u64.wrapping_sub(v as u64)
    } else {
        v as u64
    }
}

fn fold(h: &mut u64, v: u64) {
    *h = (*h ^ v).wrapping_mul(0x100000001b3);
}

use genradix_interval_model::genradix::*;
use genradix_interval_model::interval_tree::*;
use klib::rbtree::*;
// ---- concrete instantiations (the GENRADIX(_type) macro layer) ----
//
// GENRADIX(uint64_t):   obj_size 8,  objs_per_page 64, remainder 0
// GENRADIX(struct gobj): obj_size 24, objs_per_page 21, remainder 8
//   struct gobj { u64 a; u64 b; u32 c; } — field offsets 0, 8, 16.

const U64_OBJ: u64 = 8;
const U64_OPP: u64 = 64;
const GOBJ_OBJ: u64 = 24;
const GOBJ_OPP: u64 = 21;

fn genradix_walk_fwd_u64(g: &Genradix, a: &GAlloc, start: u64, cap: i32, h: &mut u64) {
    let mut it = genradix_iter_init(start, U64_OBJ);
    fold(h, it.offset);
    let mut i = 0;
    let mut p_some = false;
    while i < cap {
        match genradix_iter_peek(&mut it, g, a, U64_OPP) {
            Some(s) => {
                p_some = true;
                fold(h, it.pos);
                fold(h, slot_u64(a, s));
                genradix_iter_advance(&mut it, U64_OBJ);
            }
            None => {
                p_some = false;
                break;
            }
        }
        i += 1;
    }
    fold(h, p_some as u64);
    fold(h, i as u64);
}

fn genradix_walk_rev_u64(g: &Genradix, a: &GAlloc, cap: i32, h: &mut u64) {
    let mut it = genradix_iter_init(genradix_last_pos(U64_OPP), U64_OBJ);
    let mut i = 0;
    let mut p_some = false;
    while i < cap {
        match genradix_iter_peek_prev(&mut it, g, a, U64_OPP, U64_OBJ) {
            Some(s) => {
                p_some = true;
                fold(h, it.pos);
                fold(h, slot_u64(a, s));
                genradix_iter_rewind(&mut it, U64_OBJ);
            }
            None => {
                p_some = false;
                break;
            }
        }
        i += 1;
    }
    fold(h, p_some as u64);
    fold(h, i as u64);
}

fn genradix_walk_fwd_gobj(g: &Genradix, a: &GAlloc, start: u64, cap: i32, h: &mut u64) {
    let mut it = genradix_iter_init(start, GOBJ_OBJ);
    fold(h, it.offset);
    let mut i = 0;
    let mut p_some = false;
    while i < cap {
        match genradix_iter_peek(&mut it, g, a, GOBJ_OPP) {
            Some(s) => {
                p_some = true;
                fold(h, it.pos);
                fold(h, slot_u64_at(a, s, 0));
                fold(h, slot_u64_at(a, s, 8));
                fold(h, slot_u32_at(a, s, 16) as u64);
                genradix_iter_advance(&mut it, GOBJ_OBJ);
            }
            None => {
                p_some = false;
                break;
            }
        }
        i += 1;
    }
    fold(h, p_some as u64);
    fold(h, i as u64);
}

fn genradix_walk_rev_gobj(g: &Genradix, a: &GAlloc, cap: i32, h: &mut u64) {
    let mut it = genradix_iter_init(genradix_last_pos(GOBJ_OPP), GOBJ_OBJ);
    let mut i = 0;
    let mut p_some = false;
    while i < cap {
        match genradix_iter_peek_prev(&mut it, g, a, GOBJ_OPP, GOBJ_OBJ + 8) {
            Some(s) => {
                p_some = true;
                fold(h, it.pos);
                fold(h, slot_u64_at(a, s, 0));
                fold(h, slot_u64_at(a, s, 8));
                fold(h, slot_u32_at(a, s, 16) as u64);
                genradix_iter_rewind(&mut it, GOBJ_OBJ);
            }
            None => {
                p_some = false;
                break;
            }
        }
        i += 1;
    }
    fold(h, p_some as u64);
    fold(h, i as u64);
}

/// `genradix_ops` in wrappers.c — one mixed op sequence on one of the
/// two genradixes.
#[allow(clippy::too_many_arguments)]
fn genradix_ops(
    nops: i64,
    variant: usize,
    g64: &mut Genradix,
    gnz: &mut Genradix,
    a: &mut GAlloc,
    rng: &mut u64,
    h: &mut u64,
) {
    let idxspace: u64 = match variant % 3 {
        0 => 150,
        1 => 5000,
        _ => 1 << 17,
    };
    let use_gobj = variant >= 3;

    for i in 0..nops {
        let r = xs64(rng);
        let t = (r % 100) as i64;
        let idx = xs64(rng) % idxspace;

        if t < 45 {
            /* allocate + write */
            if use_gobj {
                let p = genradix_ptr_alloc(gnz, a, idx, &mut None, GOBJ_OBJ);
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    slot_u64_at_set(a, s, 0, idx);
                    slot_u64_at_set(a, s, 8, idx ^ 0xabcdef);
                    slot_u32_at_set(a, s, 16, i as u32);
                }
            } else {
                let p = genradix_ptr_alloc(g64, a, idx, &mut None, U64_OBJ);
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    slot_u64_set(a, s, idx ^ (*rng & 0xffffffff));
                }
            }
        } else if t < 65 {
            /* lookup */
            if use_gobj {
                let p = genradix_ptr(gnz, a, idx, GOBJ_OBJ);
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    fold(h, slot_u64_at(a, s, 0));
                    fold(h, slot_u64_at(a, s, 8));
                    fold(h, slot_u32_at(a, s, 16) as u64);
                }
            } else {
                let p = genradix_ptr(g64, a, idx, U64_OBJ);
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    fold(h, slot_u64(a, s));
                }
            }
        } else if t < 75 {
            /* forward iterate from idx */
            if use_gobj {
                genradix_walk_fwd_gobj(gnz, a, idx, 48, h);
            } else {
                genradix_walk_fwd_u64(g64, a, idx, 48, h);
            }
        } else if t < 80 {
            /* reverse iterate whole tree */
            if use_gobj {
                genradix_walk_rev_gobj(gnz, a, 48, h);
            } else {
                genradix_walk_rev_u64(g64, a, 48, h);
            }
        } else if t < 85 {
            /* prealloc */
            let nr = xs64(rng) % 48;
            if use_gobj {
                let ret = genradix_prealloc(gnz, a, idx_to_offset(nr + 1, GOBJ_OBJ));
                fold(h, ret as i64 as u64);
            } else {
                let ret = genradix_prealloc(g64, a, idx_to_offset(nr + 1, U64_OBJ));
                fold(h, ret as i64 as u64);
            }
        } else if t < 90 {
            /* preallocated node path */
            let mut nn = a.alloc_node();
            fold(h, nn.is_some() as u64);
            if use_gobj {
                let p = genradix_ptr_alloc(gnz, a, idx, &mut nn, GOBJ_OBJ);
                fold(h, p.is_some() as u64);
                fold(h, nn.is_some() as u64); /* always consumed */
                if let Some(s) = p {
                    slot_u64_at_set(a, s, 0, idx);
                    slot_u64_at_set(a, s, 8, idx);
                    slot_u32_at_set(a, s, 16, 7);
                }
            } else {
                let p = genradix_ptr_alloc(g64, a, idx, &mut nn, U64_OBJ);
                fold(h, p.is_some() as u64);
                fold(h, nn.is_some() as u64);
                if let Some(s) = p {
                    slot_u64_set(a, s, idx);
                }
            }
        } else if t < 95 {
            /* inlined fast-path alloc */
            if use_gobj {
                let p = match genradix_ptr_inlined(gnz, a, idx_to_offset(idx, GOBJ_OBJ)) {
                    Some(s) => Some(s),
                    None => genradix_ptr_alloc(gnz, a, idx, &mut None, GOBJ_OBJ),
                };
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    slot_u64_at_set(a, s, 0, idx);
                    slot_u64_at_set(a, s, 8, !idx);
                    slot_u32_at_set(a, s, 16, 3);
                }
            } else {
                let p = match genradix_ptr_inlined(g64, a, idx_to_offset(idx, U64_OBJ)) {
                    Some(s) => Some(s),
                    None => genradix_ptr_alloc(g64, a, idx, &mut None, U64_OBJ),
                };
                fold(h, p.is_some() as u64);
                if let Some(s) = p {
                    slot_u64_set(a, s, idx + 1);
                }
            }
        } else {
            /* free + reinit mid-run */
            if use_gobj {
                genradix_free(gnz, a);
                *gnz = Genradix::new();
            } else {
                genradix_free(g64, a);
                *g64 = Genradix::new();
            }
            fold(h, 0xdead);
        }
    }
}

/// `genradix_dump` — full observable dump: forward then reverse
/// traversal, allocs off.
fn genradix_dump(use_gobj: bool, g64: &Genradix, gnz: &Genradix, a: &mut GAlloc, h: &mut u64) {
    let saved = a.budget;
    a.budget = -1; /* traversals never allocate anyway */
    if use_gobj {
        genradix_walk_fwd_gobj(gnz, a, 0, 400, h);
        genradix_walk_rev_gobj(gnz, a, 400, h);
    } else {
        genradix_walk_fwd_u64(g64, a, 0, 400, h);
        genradix_walk_rev_u64(g64, a, 400, h);
    }
    a.budget = saved;
}

pub fn wrap_genradix_run(seed: u64, nops_i: i64, variant_i: i64, budget_i: i64) -> u64 {
    let variant = (iabs(variant_i) % 6) as usize;
    let nops = (iabs(nops_i) % 200) as i64;
    let nb = iabs(budget_i) % 160;
    let use_gobj = variant >= 3;

    let mut h = 0xcbf29ce484222325u64;
    let mut rng = seed | 1;
    let mut g64 = Genradix::new();
    let mut gnz = Genradix::new();
    let mut a = GAlloc::new();
    a.budget = if nb < 80 { -1 } else { (nb - 80) as i64 };
    fold(&mut h, variant as u64);

    genradix_ops(nops, variant, &mut g64, &mut gnz, &mut a, &mut rng, &mut h);
    genradix_dump(use_gobj, &g64, &gnz, &mut a, &mut h);
    fold(&mut h, rng);

    genradix_free(&mut g64, &mut a);
    genradix_free(&mut gnz, &mut a);
    h
}

/// `wrap_genradix_seq` — sequential bulk insert + traversals + free.
pub fn wrap_genradix_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    let variant = (iabs(variant_i) % 6) as usize;
    let n = (iabs(n_i) % 200) as i64;
    let use_gobj = variant >= 3;

    let mut h = 0xcbf29ce484222325u64;
    let mut rng = seed | 1;
    let mut g64 = Genradix::new();
    let mut gnz = Genradix::new();
    let mut a = GAlloc::new();
    fold(&mut h, variant as u64);

    for i in 0..n {
        let idx = (i as u64) * ((xs64(&mut rng) % 3) + 1);
        if use_gobj {
            let p = genradix_ptr_alloc(&mut gnz, &mut a, idx, &mut None, GOBJ_OBJ);
            fold(&mut h, p.is_some() as u64);
            if let Some(s) = p {
                slot_u64_at_set(&mut a, s, 0, idx);
                slot_u64_at_set(&mut a, s, 8, idx * 3);
                slot_u32_at_set(&mut a, s, 16, i as u32);
            }
        } else {
            let p = genradix_ptr_alloc(&mut g64, &mut a, idx, &mut None, U64_OBJ);
            fold(&mut h, p.is_some() as u64);
            if let Some(s) = p {
                slot_u64_set(&mut a, s, idx + rng);
            }
        }
    }
    genradix_dump(use_gobj, &g64, &gnz, &mut a, &mut h);
    /* lookups of every slot we may have touched */
    for i in 0..n.min(64) {
        if use_gobj {
            let p = genradix_ptr(&gnz, &a, i as u64, GOBJ_OBJ);
            fold(&mut h, p.is_some() as u64);
        } else {
            let p = genradix_ptr(&g64, &a, i as u64, U64_OBJ);
            fold(&mut h, p.is_some() as u64);
        }
    }
    genradix_free(&mut g64, &mut a);
    genradix_free(&mut gnz, &mut a);
    h
}

// ---- interval harness (wrappers.c) ----

struct ItPool {
    nodes: Vec<RbNode>,
    data: ItData,
    present: Vec<bool>,
}

impl ItPool {
    fn new() -> Self {
        Self {
            nodes: vec![RbNode::new(); ITN],
            data: ItData {
                start: vec![0; ITN],
                last: vec![0; ITN],
                sub: vec![0; ITN],
            },
            present: vec![false; ITN],
        }
    }
}

fn it_pick_victim(p: &ItPool, r: u64) -> i64 {
    let off = (r % ITN as u64) as usize;
    for i in 0..ITN {
        let idx = (off + i) % ITN;
        if p.present[idx] {
            return idx as i64;
        }
    }
    -1
}

fn itidx(n: usize) -> u64 {
    if n == NIL { NILX } else { n as u64 }
}

fn it_query_first(p: &ItPool, itree: &RbRootCached, qs: u64, ql: u64, walk_cap: i32, h: &mut u64) {
    let mut node = interval_tree_iter_first(&p.nodes, &p.data, itree, qs, ql);
    let mut i = 0;
    while node != NIL && i < walk_cap {
        fold(h, itidx(node));
        fold(h, p.data.start[node]);
        fold(h, p.data.last[node]);
        node = interval_tree_iter_next(&p.nodes, &p.data, node, qs, ql);
        i += 1;
    }
    fold(h, (node != NIL) as u64);
    fold(h, i as u64);
}

fn it_span_run(p: &ItPool, itree: &RbRootCached, first: u64, last: u64, h: &mut u64) {
    let mut span = SpanIter {
        nodes: [NIL; 2],
        first_index: 0,
        last_index: 0,
        span_start: 0,
        span_last: 0,
        is_hole: 0,
    };
    span_iter_first(&p.nodes, &p.data, itree, &mut span, first, last);
    while span.is_hole != -1 {
        fold(h, (span.is_hole + 2) as u64);
        fold(h, span.span_start);
        fold(h, span.span_last);
        span_iter_next(&p.nodes, &p.data, &mut span);
    }
    fold(h, NILX);
}

fn it_span_advance(p: &ItPool, itree: &RbRootCached, first: u64, last: u64, pos: u64, h: &mut u64) {
    let mut span = SpanIter {
        nodes: [NIL; 2],
        first_index: 0,
        last_index: 0,
        span_start: 0,
        span_last: 0,
        is_hole: 0,
    };
    span_iter_first(&p.nodes, &p.data, itree, &mut span, first, last);
    fold(h, (span.is_hole + 2) as u64);
    for j in 0..3u64 {
        span_iter_advance(&p.nodes, &p.data, itree, &mut span, pos + j);
        fold(h, (span.is_hole + 2) as u64);
        if span.is_hole != -1 {
            fold(h, span.span_start);
            fold(h, span.span_last);
        }
    }
    fold(h, NILX);
}

/// `it_dump` — in-order dump: index, start, last, __subtree_last per
/// node + cached leftmost.
fn it_dump(p: &ItPool, itree: &RbRootCached, h: &mut u64) {
    fold(h, itidx(itree.rb_leftmost));
    let root = itree.root.rb_node;
    if root == NIL {
        fold(h, NILX);
        return;
    }
    let mut node = root;
    while p.nodes[node].left != NIL {
        node = p.nodes[node].left;
    }
    let mut cnt = 0u64;
    let mut cur = Some(node);
    while let Some(n) = cur {
        fold(h, n as u64);
        fold(h, p.data.start[n]);
        fold(h, p.data.last[n]);
        fold(h, p.data.sub[n]);
        cnt += 1;
        let nx = rb_next(&p.nodes, n);
        cur = if nx == NIL { None } else { Some(nx) };
    }
    fold(h, cnt);
}

/// `wrap_interval_run` — mixed insert/remove/query ops.
pub fn wrap_interval_run(seed: u64, nops_i: i64, variant_i: i64) -> u64 {
    const MIX: [[i64; 2]; 4] = [[45, 25], [60, 25], [35, 35], [50, 20]];
    let m = MIX[(iabs(variant_i) % 4) as usize];
    let nops = (iabs(nops_i) % 200) as i64;

    let mut h = 0xcbf29ce484222325u64;
    let mut rng = seed | 1;
    let mut p = ItPool::new();
    let mut itree = RbRootCached::EMPTY;
    fold(&mut h, iabs(variant_i) % 4);

    for _ in 0..nops {
        let r = xs64(&mut rng);
        let t = (r % 100) as i64;

        if t < m[0] {
            let idx = (xs64(&mut rng) % ITN as u64) as usize;
            if !p.present[idx] {
                p.data.start[idx] = xs64(&mut rng) % ITKEY;
                p.data.last[idx] = p.data.start[idx] + xs64(&mut rng) % ITSPAN;
                interval_tree_insert(&mut p.nodes, &mut p.data, idx, &mut itree);
                p.present[idx] = true;
                fold(&mut h, idx as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else if t < m[0] + m[1] {
            let idx = it_pick_victim(&p, xs64(&mut rng));
            if idx >= 0 {
                let idx = idx as usize;
                interval_tree_remove(&mut p.nodes, &mut p.data, idx, &mut itree);
                p.present[idx] = false;
                fold(&mut h, idx as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else {
            let q = xs64(&mut rng) % 4;
            let qs = xs64(&mut rng) % ITKEY;
            let ql = qs + xs64(&mut rng) % (2 * ITSPAN);
            match q {
                0 => it_query_first(&p, &itree, qs, ql, 64, &mut h),
                1 => {
                    let node = interval_tree_iter_first(&p.nodes, &p.data, &itree, qs, ql);
                    fold(&mut h, itidx(node));
                    if node != NIL {
                        let nx = interval_tree_iter_next(&p.nodes, &p.data, node, qs, ql);
                        fold(&mut h, itidx(nx));
                    }
                }
                2 => it_span_run(&p, &itree, qs, ql, &mut h),
                _ => it_span_advance(&p, &itree, qs, ql, xs64(&mut rng) % ITKEY, &mut h),
            }
        }
    }
    it_dump(&p, &itree, &mut h);
    fold(&mut h, rng);
    h
}

/// `wrap_interval_seq` — n inserts, a query sweep, then erase all.
pub fn wrap_interval_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    let n = (iabs(n_i) % (ITN as u64 + 1)) as usize;
    let step = ((iabs(variant_i) % 5) + 1) as usize;

    let mut h = 0xcbf29ce484222325u64;
    let mut rng = seed | 1;
    let mut p = ItPool::new();
    let mut itree = RbRootCached::EMPTY;
    fold(&mut h, step as u64);

    for i in 0..n {
        p.data.start[i] = ((i as u64) * (step as u64) + xs64(&mut rng) % 3) % ITKEY;
        p.data.last[i] = p.data.start[i] + xs64(&mut rng) % ITSPAN;
        interval_tree_insert(&mut p.nodes, &mut p.data, i, &mut itree);
        p.present[i] = true;
        fold(&mut h, i as u64);
    }
    it_dump(&p, &itree, &mut h);
    for _ in 0..8 {
        let qs = xs64(&mut rng) % ITKEY;
        let ql = qs + xs64(&mut rng) % (2 * ITSPAN);
        it_query_first(&p, &itree, qs, ql, 64, &mut h);
    }
    it_span_run(&p, &itree, 0, ITKEY - 1, &mut h);
    let mut i = 0;
    while i < n {
        if p.present[i] {
            interval_tree_remove(&mut p.nodes, &mut p.data, i, &mut itree);
            p.present[i] = false;
            fold(&mut h, i as u64);
        }
        i += step;
    }
    it_dump(&p, &itree, &mut h);
    fold(&mut h, rng);
    h
}
