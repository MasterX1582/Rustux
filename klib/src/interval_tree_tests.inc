// klib::interval_tree tests — parallel-slice node fields (`ItData`) over
// a caller-owned `&mut [RbNode]` pool, the interval_tree_generic.h
// instantiation on klib::rbtree's Augment callbacks.

use std::vec;
use std::vec::Vec;

fn nodes(n: usize) -> Vec<RbNode> {
    vec![RbNode::new(); n]
}

fn ins(nodes: &mut [RbNode], ctx: &mut ItData, i: usize, s: u64, l: u64, root: &mut RbRootCached) {
    ctx.start[i] = s;
    ctx.last[i] = l;
    interval_tree_insert(nodes, ctx, i, root);
}

/// Recompute `__subtree_last` for the whole subtree and check every node:
/// sub[i] == max(last[i], sub[left], sub[right]) with NIL counting as 0.
fn aug_check(nodes: &[RbNode], ctx: &ItData, i: usize) -> u64 {
    if i == NIL {
        return 0;
    }
    let l = aug_check(nodes, ctx, nodes[i].left);
    let r = aug_check(nodes, ctx, nodes[i].right);
    let expect = ctx.last[i].max(l).max(r);
    assert_eq!(ctx.sub[i], expect, "subtree_last wrong at node {}", i);
    expect
}

/// In-order traversal: starts must be non-decreasing (the tree's BST key).
fn inorder_starts(nodes: &[RbNode], ctx: &ItData, root: &RbRootCached) -> Vec<u64> {
    let mut out = Vec::new();
    let mut i = rb_first_cached(root);
    while i != NIL {
        out.push(ctx.start[i]);
        i = rb_next(nodes, i);
    }
    out
}

fn collect(nodes: &[RbNode], ctx: &ItData, root: &RbRootCached, qs: u64, ql: u64) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = interval_tree_iter_first(nodes, ctx, root, qs, ql);
    while i != NIL {
        out.push(i);
        i = interval_tree_iter_next(nodes, ctx, i, qs, ql);
    }
    out
}

#[test]
fn empty_tree() {
    let pool = nodes(4);
    let (mut s, mut l, mut sb) = ([0u64; 4], [0u64; 4], [0u64; 4]);
    let ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let root = RbRootCached::EMPTY;
    assert_eq!(root.root.rb_node, NIL);
    assert_eq!(root.rb_leftmost, NIL);
    assert_eq!(interval_tree_iter_first(&pool, &ctx, &root, 0, u64::MAX), NIL);
    assert_eq!(collect(&pool, &ctx, &root, 0, u64::MAX), Vec::<usize>::new());
}

#[test]
fn insert_and_query() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    ins(&mut pool, &mut ctx, 0, 10, 15, &mut root);
    ins(&mut pool, &mut ctx, 1, 0, 5, &mut root);
    ins(&mut pool, &mut ctx, 2, 20, 30, &mut root);
    aug_check(&pool, &ctx, root.root.rb_node);
    assert_eq!(root.rb_leftmost, 1);
    // Hits come back in non-decreasing start order.
    assert_eq!(collect(&pool, &ctx, &root, 4, 11), vec![1, 0]);
    assert_eq!(collect(&pool, &ctx, &root, 16, 19), Vec::<usize>::new());
    // Boundary endpoints are inclusive on both sides.
    assert_eq!(collect(&pool, &ctx, &root, 15, 15), vec![0]);
    assert_eq!(collect(&pool, &ctx, &root, 30, 30), vec![2]);
    assert_eq!(collect(&pool, &ctx, &root, 5, 5), vec![1]);
    assert_eq!(collect(&pool, &ctx, &root, 20, 20), vec![2]);
    assert_eq!(inorder_starts(&pool, &ctx, &root), vec![0, 10, 20]);
}

#[test]
fn overlapping_spans() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    ins(&mut pool, &mut ctx, 0, 0, 10, &mut root);
    ins(&mut pool, &mut ctx, 1, 5, 20, &mut root);
    ins(&mut pool, &mut ctx, 2, 8, 15, &mut root);
    ins(&mut pool, &mut ctx, 3, 3, 4, &mut root);
    ins(&mut pool, &mut ctx, 4, 30, 40, &mut root);
    aug_check(&pool, &ctx, root.root.rb_node);
    assert_eq!(collect(&pool, &ctx, &root, 9, 12), vec![0, 1, 2]);
    // Point inside all of 0/1/2's spans.
    assert_eq!(collect(&pool, &ctx, &root, 9, 9), vec![0, 1, 2]);
    // Fully covering query returns everything, ordered by start.
    assert_eq!(
        collect(&pool, &ctx, &root, 0, u64::MAX),
        vec![0, 3, 1, 2, 4]
    );
    // Gap between node 2's end (15) and node 4's start (30).
    assert_eq!(collect(&pool, &ctx, &root, 21, 29), Vec::<usize>::new());
    assert_eq!(collect(&pool, &ctx, &root, 16, 30), vec![1, 4]);
}

#[test]
fn boundary_adjacency() {
    let mut pool = nodes(4);
    let (mut s, mut l, mut sb) = ([0u64; 4], [0u64; 4], [0u64; 4]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    // [0,5] and [6,10] are adjacent but disjoint; [11,15] touches [6,10].
    ins(&mut pool, &mut ctx, 0, 0, 5, &mut root);
    ins(&mut pool, &mut ctx, 1, 6, 10, &mut root);
    ins(&mut pool, &mut ctx, 2, 11, 15, &mut root);
    // [5,6] reaches both sides of the adjacency.
    assert_eq!(collect(&pool, &ctx, &root, 5, 6), vec![0, 1]);
    // But each endpoint alone belongs to exactly one interval.
    assert_eq!(collect(&pool, &ctx, &root, 5, 5), vec![0]);
    assert_eq!(collect(&pool, &ctx, &root, 6, 6), vec![1]);
    assert_eq!(collect(&pool, &ctx, &root, 10, 11), vec![1, 2]);
    // subtree_search invariant applies at the root: query below all
    // starts still scans correctly.
    assert_eq!(interval_tree_subtree_search(&pool, &ctx, root.root.rb_node, 0, 4), 0);
}

#[test]
fn remove_keeps_augment_and_queries() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    let spans = [
        (5u64, 10u64),
        (15, 20),
        (7, 9),
        (0, 3),
        (12, 18),
        (25, 30),
        (1, 2),
    ];
    for (i, &(st, la)) in spans.iter().enumerate() {
        ins(&mut pool, &mut ctx, i, st, la, &mut root);
    }
    aug_check(&pool, &ctx, root.root.rb_node);
    assert_eq!(collect(&pool, &ctx, &root, 8, 16), vec![0, 2, 4, 1]);

    // Remove the highest-`last` node: subtree_last must shrink again.
    interval_tree_remove(&mut pool, &mut ctx, 4, &mut root);
    aug_check(&pool, &ctx, root.root.rb_node);
    assert_eq!(collect(&pool, &ctx, &root, 8, 16), vec![0, 2, 1]);

    // Remove the leftmost node (start 0); the cached rb_leftmost must
    // follow to node 6 (start 1).
    assert_eq!(root.rb_leftmost, 3);
    interval_tree_remove(&mut pool, &mut ctx, 3, &mut root);
    assert_eq!(root.rb_leftmost, 6);
    aug_check(&pool, &ctx, root.root.rb_node);
    assert_eq!(collect(&pool, &ctx, &root, 0, 4), vec![6]);

    // Remove the rest one by one; invariant holds throughout, queries
    // track exactly the remaining set.
    for &i in &[0usize, 6, 1, 5] {
        interval_tree_remove(&mut pool, &mut ctx, i, &mut root);
        aug_check(&pool, &ctx, root.root.rb_node);
    }
    interval_tree_remove(&mut pool, &mut ctx, 2, &mut root);
    assert_eq!(root.root.rb_node, NIL);
    assert_eq!(root.rb_leftmost, NIL);
    assert_eq!(collect(&pool, &ctx, &root, 0, u64::MAX), Vec::<usize>::new());
}

#[test]
fn randomized_ops_match_brute_force() {
    // Deterministic LCG: insert/remove a churning set of intervals and
    // cross-check every query against the brute-force intersection set.
    const N: usize = 24;
    let mut pool = nodes(N);
    let (mut s, mut l, mut sb) = ([0u64; N], [0u64; N], [0u64; N]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    let mut live = [false; N];
    let mut x = 0x9e3779b97f4a7c15u64;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..200 {
        let i = (next() as usize) % N;
        if live[i] {
            interval_tree_remove(&mut pool, &mut ctx, i, &mut root);
            live[i] = false;
        } else {
            let st = next() % 128;
            let la = st + next() % 32;
            ins(&mut pool, &mut ctx, i, st, la, &mut root);
            live[i] = true;
        }
        aug_check(&pool, &ctx, root.root.rb_node);
        let qs = next() % 128;
        let ql = qs + next() % 64;
        let got = collect(&pool, &ctx, &root, qs, ql);
        let mut want: Vec<usize> = (0..N)
            .filter(|&i| live[i] && ctx.start[i] <= ql && qs <= ctx.last[i])
            .collect();
        want.sort_unstable();
        // Same node set; visit order is non-decreasing start (equal
        // starts have no stable order under rbtree rotations).
        let mut got_sorted = got.clone();
        got_sorted.sort_unstable();
        assert_eq!(got_sorted, want, "query [{qs},{ql}] set");
        assert!(
            got.windows(2).all(|w| ctx.start[w[0]] <= ctx.start[w[1]]),
            "query [{qs},{ql}] order: {got:?}"
        );
    }
}

#[test]
fn iter_next_walks_only_matches() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    // Interleaved: even indices in [0,50], odd in [100,150].
    for i in 0..6usize {
        let (st, la) = if i % 2 == 0 { (i as u64, 50) } else { (100, 150) };
        ins(&mut pool, &mut ctx, i, st, la, &mut root);
    }
    aug_check(&pool, &ctx, root.root.rb_node);
    // Narrow query inside the low band must skip all high nodes even
    // though they sit between them in start order? (starts are distinct:
    // 0,100,2,100,4,100 — iter order follows start, so hits interleave.)
    let got = collect(&pool, &ctx, &root, 10, 40);
    assert_eq!(got, vec![0, 2, 4]);
    // Equal starts (100) have no stable visit order — compare as a set.
    let mut got = collect(&pool, &ctx, &root, 60, 200);
    got.sort_unstable();
    assert_eq!(got, vec![1, 3, 5]);
}

#[test]
fn span_iter_holes_and_used() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    ins(&mut pool, &mut ctx, 0, 2, 5, &mut root);
    ins(&mut pool, &mut ctx, 1, 10, 15, &mut root);
    ins(&mut pool, &mut ctx, 2, 14, 18, &mut root); // merges with node 1's span
    let mut st = SpanIter {
        nodes: [NIL; 2],
        first_index: 0,
        last_index: 0,
        span_start: 0,
        span_last: 0,
        is_hole: 0,
    };
    let mut spans = Vec::new();
    span_iter_first(&pool, &ctx, &root, &mut st, 0, 20);
    while st.is_hole != -1 {
        spans.push((st.span_start, st.span_last, st.is_hole));
        span_iter_next(&pool, &ctx, &mut st);
    }
    assert_eq!(
        spans,
        vec![
            (0, 1, 1),   // leading hole
            (2, 5, 0),   // node 0
            (6, 9, 1),   // interior hole
            (10, 18, 0), // nodes 1+2 merged (adjacent/overlapping)
            (19, 20, 1), // trailing hole
        ]
    );
}

#[test]
fn span_iter_advance_repositions() {
    let mut pool = nodes(8);
    let (mut s, mut l, mut sb) = ([0u64; 8], [0u64; 8], [0u64; 8]);
    let mut ctx = ItData {
        start: &mut s,
        last: &mut l,
        sub: &mut sb,
    };
    let mut root = RbRootCached::EMPTY;
    ins(&mut pool, &mut ctx, 0, 10, 20, &mut root);
    let mut st = SpanIter {
        nodes: [NIL; 2],
        first_index: 0,
        last_index: 0,
        span_start: 0,
        span_last: 0,
        is_hole: 0,
    };
    span_iter_first(&pool, &ctx, &root, &mut st, 0, 30);
    assert_eq!((st.span_start, st.span_last, st.is_hole), (0, 9, 1));
    // Advance into the used span: recomputed "starting inside used" —
    // the union-aliased span_start becomes the new index.
    span_iter_advance(&pool, &ctx, &root, &mut st, 15);
    assert_eq!((st.span_start, st.span_last, st.is_hole), (15, 20, 0));
    // Advance past the end: recomputed hole runs to last_index.
    span_iter_advance(&pool, &ctx, &root, &mut st, 25);
    assert_eq!((st.span_start, st.span_last, st.is_hole), (25, 30, 1));
    // Advance beyond last_index terminates the iteration.
    span_iter_advance(&pool, &ctx, &root, &mut st, 31);
    assert_eq!(st.is_hole, -1);
}
