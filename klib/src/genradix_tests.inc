// klib::genradix tests — caller-owned node arena (`GAlloc`), index model.
// Objects are u64s stored in leaf slots via slot_u64/slot_u64_set;
// obj_size 8 => 64 objects per leaf page, so idx >= 64 forces growth.

use std::vec;
use std::vec::Vec;

fn arena(n: usize) -> Vec<GNode> {
    vec![GNode::zeroed(); n]
}

/// Store `v` at object `idx` (creating pages as needed) and read it back.
fn put(g: &mut Genradix, a: &mut GAlloc, idx: u64, v: u64, obj_size: u64) -> Slot {
    let s = genradix_ptr_alloc(g, a, idx, &mut None, obj_size).expect("alloc failed");
    slot_u64_set(a, s, v);
    s
}

fn get(g: &Genradix, a: &GAlloc, idx: u64, obj_size: u64) -> Option<u64> {
    genradix_ptr(g, a, idx, obj_size).map(|s| slot_u64(a, s))
}

#[test]
fn empty_tree_lookup_misses() {
    let mut ar = arena(4);
    let a = &mut GAlloc::new(&mut ar);
    let g = Genradix::new();
    assert_eq!(g.root, 0);
    assert_eq!(genradix_ptr(&g, a, 0, 8), None);
    assert_eq!(genradix_ptr(&g, a, 1, 8), None);
    assert_eq!(genradix_prealloc(&mut Genradix::new(), a, 512), 0);
}

#[test]
fn insert_lookup_single_level() {
    let mut ar = arena(4);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // obj_size 8: indices 0..63 share leaf page 0.
    put(&mut g, a, 0, 0xdead, 8);
    put(&mut g, a, 5, 0xbeef, 8);
    put(&mut g, a, 63, 0xf00d, 8);
    assert_eq!(root_to_depth(g.root), 0); // still a single leaf
    assert_eq!(a.used, 1);
    assert_eq!(get(&g, a, 0, 8), Some(0xdead));
    assert_eq!(get(&g, a, 5, 8), Some(0xbeef));
    assert_eq!(get(&g, a, 63, 8), Some(0xf00d));
    // Same page as an allocated leaf -> valid (zeroed) slot, like C.
    assert_eq!(get(&g, a, 1, 8), Some(0));
    // Beyond the leaf's coverage -> miss.
    assert_eq!(get(&g, a, 64, 8), None);
}

#[test]
fn growth_to_multi_level() {
    let mut ar = arena(16);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    put(&mut g, a, 3, 0x1111, 8);
    assert_eq!(root_to_depth(g.root), 0);
    // idx 100 -> offset 800 > 511: tree must grow to depth 1.
    put(&mut g, a, 100, 0x3333, 8);
    assert_eq!(root_to_depth(g.root), 1);
    // idx 4096 -> offset 32768 = 64 * 512: grows to depth 2.
    put(&mut g, a, 4096, 0x2222, 8);
    assert_eq!(root_to_depth(g.root), 2);
    assert_eq!(a.used, 6); // leaf + grow + leaf + grow + interior + leaf
    assert_eq!(get(&g, a, 3, 8), Some(0x1111));
    assert_eq!(get(&g, a, 100, 8), Some(0x3333));
    assert_eq!(get(&g, a, 4096, 8), Some(0x2222));
    // Sparse interior: idx 8192 -> offset 65536 -> root child[2] absent.
    assert_eq!(get(&g, a, 8192, 8), None);
    // Same depth-1 subtree as idx 100 but absent leaf page -> miss.
    assert_eq!(get(&g, a, 200, 8), None);
}

#[test]
fn iter_round_trip_sorted() {
    let mut ar = arena(16);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // Pages 0, 1 and 64 allocated; iteration visits every object slot in
    // every allocated leaf page (zeroed ones included) — the C API is
    // page-granular, not object-granular.
    let keys = [3u64, 100, 4096, 7, 64];
    for &k in &keys {
        put(&mut g, a, k, k * 16 + 1, 8);
    }
    let objs_per_page = GENRADIX_NODE_SIZE / 8;
    let mut it = genradix_iter_init(0, 8);
    let mut seen = Vec::new();
    while let Some(s) = genradix_iter_peek(&mut it, &g, a, objs_per_page) {
        seen.push((it.pos, slot_u64(a, s)));
        genradix_iter_advance(&mut it, 8);
    }
    let mut expect: Vec<(u64, u64)> = (0u64..128)
        .chain(4096..4160)
        .map(|pos| (pos, if keys.contains(&pos) { pos * 16 + 1 } else { 0 }))
        .collect();
    expect.sort_unstable();
    assert_eq!(seen, expect);
}

#[test]
fn iter_rewind_and_peek_prev() {
    let mut ar = arena(16);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // Sparse pages 0, 64 and 128.
    for &k in &[10u64, 4096, 8192] {
        put(&mut g, a, k, k * 2, 8);
    }
    let objs_per_page = GENRADIX_NODE_SIZE / 8;
    let rem = GENRADIX_NODE_SIZE % 8;
    let prev = |it: &mut GIter| genradix_iter_peek_prev(it, &g, a, objs_per_page, 8 + rem);
    // Starting inside an allocated page yields that object.
    let mut it = genradix_iter_init(8192, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (8192, 16384));
    // One step back lands in the gap: prev skips to the last object of
    // the previous allocated page (page 64 -> idx 4159, zeroed).
    genradix_iter_rewind(&mut it, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (4159, 0));
    // Same skip from the top of page 64 down to the tail of page 0.
    let mut it = genradix_iter_init(4096, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (4096, 8192));
    genradix_iter_rewind(&mut it, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (63, 0));
    // Stored object in page 0; pos 0 rewinds to the end sentinel.
    let mut it = genradix_iter_init(10, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (10, 20));
    let mut it = genradix_iter_init(0, 8);
    let s = prev(&mut it).unwrap();
    assert_eq!((it.pos, slot_u64(a, s)), (0, 0));
    genradix_iter_rewind(&mut it, 8);
    assert_eq!(it.offset, u64::MAX);
    assert_eq!(prev(&mut it), None);
}

#[test]
fn non_pow2_obj_size() {
    let mut ar = arena(16);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // obj_size 24: 21 objects per page (512/24); idx 21 starts page 1.
    assert_eq!(idx_to_offset(0, 24), 0);
    assert_eq!(idx_to_offset(20, 24), 480);
    assert_eq!(idx_to_offset(21, 24), 512);
    assert_eq!(idx_to_offset(42, 24), 1024);
    for &k in &[0u64, 20, 21, 42] {
        put(&mut g, a, k, k + 0x100, 24);
    }
    assert_eq!(get(&g, a, 0, 24), Some(0x100));
    assert_eq!(get(&g, a, 20, 24), Some(0x114));
    assert_eq!(get(&g, a, 21, 24), Some(0x115));
    assert_eq!(get(&g, a, 42, 24), Some(0x12a));
    // Iteration must skip the page-remainder padding between idx 20 and
    // 21 but still yields every object slot of each allocated page —
    // pages 0, 1, 2 are allocated, so positions run contiguously 0..63.
    let objs_per_page = GENRADIX_NODE_SIZE / 24;
    let mut it = genradix_iter_init(0, 24);
    let mut seen = Vec::new();
    while genradix_iter_peek(&mut it, &g, a, objs_per_page).is_some() {
        seen.push(it.pos);
        genradix_iter_advance(&mut it, 24);
    }
    let expect: Vec<u64> = (0..63).collect();
    assert_eq!(seen, expect);
}

#[test]
fn alloc_failure_budget() {
    let mut ar = arena(8);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // budget 0: every alloc_node fails.
    a.budget = 0;
    assert_eq!(genradix_ptr_alloc(&mut g, a, 5, &mut None, 8), None);
    // budget 1: the leaf-alloc succeeds, growing past it fails, and the
    // tree stays consistent (C semantics: root leaf linked, offset out of
    // coverage -> miss but no corruption).
    a.budget = 1;
    assert_eq!(genradix_ptr_alloc(&mut g, a, 100, &mut None, 8), None);
    assert_eq!(a.used, 1);
    a.budget = -1;
    // Retry now succeeds; the earlier leaf is reused as the depth-0 child.
    let s = genradix_ptr_alloc(&mut g, a, 100, &mut None, 8).unwrap();
    slot_u64_set(a, s, 0x42);
    assert_eq!(get(&g, a, 100, 8), Some(0x42));
}

#[test]
fn prealloc_reports_enomem() {
    let mut ar = arena(8);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    a.budget = 1;
    // size 1024 needs two pages -> second alloc fails -> -ENOMEM.
    assert_eq!(genradix_prealloc(&mut g, a, 2 * GENRADIX_NODE_SIZE), -ENOMEM);
    a.budget = -1;
    let mut g2 = Genradix::new();
    assert_eq!(genradix_prealloc(&mut g2, a, 2 * GENRADIX_NODE_SIZE), 0);
}

#[test]
fn preallocated_node_is_consumed() {
    let mut ar = arena(8);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    // Caller pre-allocates a node and hands it over; it must be linked or
    // freed, never leaked back through `preallocated`.
    let mut pre = Some(a.alloc_node().unwrap());
    put_pre(&mut g, a, 5, 0x77, &mut pre);
    assert_eq!(pre, None);
    assert_eq!(get(&g, a, 5, 8), Some(0x77));
    // When no new node is needed (slot exists in same page), the spare is
    // freed back to the arena's free list.
    let mut pre = Some(a.alloc_node().unwrap());
    let head_before = a.free_head;
    put_pre(&mut g, a, 6, 0x78, &mut pre);
    assert_eq!(pre, None);
    assert_ne!(a.free_head, head_before); // spare went back on the free list
    assert_eq!(get(&g, a, 6, 8), Some(0x78));
}

fn put_pre(g: &mut Genradix, a: &mut GAlloc, idx: u64, v: u64, pre: &mut Option<usize>) {
    let s = genradix_ptr_alloc(g, a, idx, pre, 8).expect("alloc failed");
    slot_u64_set(a, s, v);
}

#[test]
fn free_releases_all_nodes() {
    let mut ar = arena(16);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    for &k in &[3u64, 100, 4096, 7] {
        put(&mut g, a, k, k, 8);
    }
    let used = a.used;
    assert!(used > 1);
    genradix_free(&mut g, a);
    assert_eq!(g.root, 0);
    assert_eq!(genradix_ptr(&g, a, 3, 8), None);
    // Every allocated node is on the intrusive free list: reuse `used`
    // times without the high-water mark moving.
    for _ in 0..used {
        assert!(a.alloc_node().is_some());
    }
    assert_eq!(a.used, used);
    assert_eq!(a.free_head, GNIL);
    // Free list drained: the next allocation grows the arena.
    assert_eq!(a.alloc_node(), Some(used));
    assert_eq!(a.used, used + 1);
}

#[test]
fn reuse_after_free_round_trip() {
    let mut ar = arena(8);
    let a = &mut GAlloc::new(&mut ar);
    let mut g = Genradix::new();
    put(&mut g, a, 100, 0xaaaa, 8);
    genradix_free(&mut g, a);
    // Rebuild on the freed nodes: same indices, no arena growth.
    let used = a.used;
    put(&mut g, a, 7, 0xbbbb, 8);
    put(&mut g, a, 100, 0xcccc, 8);
    assert_eq!(a.used, used);
    assert_eq!(get(&g, a, 7, 8), Some(0xbbbb));
    assert_eq!(get(&g, a, 100, 8), Some(0xcccc));
}

#[test]
fn helpers_match_c_semantics() {
    assert_eq!(ilog2(0), -1);
    assert_eq!(ilog2(1), 0);
    assert_eq!(ilog2(511), 8);
    assert_eq!(ilog2(512), 9);
    assert!(is_power_of_2(512));
    assert!(!is_power_of_2(0));
    assert!(!is_power_of_2(24));
    assert_eq!(round_up(513, 512), 1024);
    assert_eq!(round_down(513, 512), 512);
    assert_eq!(genradix_depth_size(0), 512);
    assert_eq!(genradix_depth_size(1), 512 * 64);
    assert_eq!(mk_root(0, 0), 16);
    assert_eq!(root_to_node(mk_root(5, 2)), 5);
    assert_eq!(root_to_depth(mk_root(5, 2)), 2);
}
