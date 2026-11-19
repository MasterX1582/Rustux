// klib::maple_tree tests — caller-owned node + sheaf arenas, canonical
// pointer model. Entries are plain aligned u64 "pointers" (low bits 00,
// outside the node/sheaf address spaces), exactly like the verified
// harness's external-pointer pool.

use std::vec;

const E0: u64 = 0x1000;
const E1: u64 = 0x1010;
const E2: u64 = 0x1020;
const E3: u64 = 0x1030;
const E4: u64 = 0x1040;
const E5: u64 = 0x1050;

fn nodes(n: usize) -> std::vec::Vec<Node> {
    vec![Node::new(); n]
}

fn sheafs(n: usize) -> std::vec::Vec<Sheaf> {
    vec![Sheaf::new(); n]
}

fn world(n: usize, s: usize, flags: u32) -> (std::vec::Vec<Node>, std::vec::Vec<Sheaf>, u32) {
    (nodes(n), sheafs(s), flags)
}

fn used_nodes(t: &MapleTree) -> usize {
    t.nodes.iter().filter(|n| n.used).count()
}

#[test]
fn constants_and_entry_helpers() {
    assert_eq!(MT_SLOTS[MAPLE_DENSE as usize], MAPLE_NODE_SLOTS);
    assert_eq!(MT_SLOTS[MAPLE_RANGE_64 as usize], MAPLE_RANGE64_SLOTS);
    assert_eq!(MT_SLOTS[MAPLE_ARANGE_64 as usize], MAPLE_ARANGE64_SLOTS);
    assert_eq!(MT_PIVOTS[MAPLE_ARANGE_64 as usize], MAPLE_ARANGE64_SLOTS - 1);
    /* error encoding round-trips like the kernel */
    assert!(xa_is_err(ma_error(-ENOMEM)));
    assert_eq!(xa_err(ma_error(-ENOMEM)), -ENOMEM);
    assert_eq!(xa_err(E0), 0);
    assert!(xa_is_internal(xa_mk_internal(300)));
    assert!(xa_is_zero(XA_ZERO_ENTRY));
    assert!(xa_is_advanced(XA_ZERO_ENTRY));
    assert!(!xa_is_advanced(E0));
    assert!(xa_is_node(NBASE | 2));
    assert_eq!(xa_to_node(NBASE | 2 | (3 << 3)), NBASE | (3 << 3));
    /* gfp blocking flag = __GFP_DIRECT_RECLAIM only */
    assert!(gfpflags_allow_blocking(GFP_KERNEL));
    assert!(!gfpflags_allow_blocking(GFP_NOFAIL));
    assert!(!gfpflags_allow_blocking(GFP_NOWAIT));
    assert_eq!(iabs(-7), 7);
    assert_eq!(iabs(9), 9);
    /* node type tagging: DENSE/LEAF_64 are leaf types, RANGE_64/ARANGE_64 not */
    let e = mt_mk_node(NBASE, MAPLE_LEAF_64);
    assert_eq!(mte_node_type(e), MAPLE_LEAF_64);
    assert!(mte_is_leaf(e));
    assert!(!mte_is_leaf(mt_mk_node(NBASE, MAPLE_RANGE_64)));
    assert!(!mte_is_leaf(mt_mk_node(NBASE, MAPLE_ARANGE_64)));
}

#[test]
fn store_load_single() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_load(MT_CANON, 0), 0);
    assert_eq!(t.mtree_store(MT_CANON, 0, E0, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 0), E0);
    /* overwrite returns 0 (mtree_store returns errno, not old entry) */
    assert_eq!(t.mtree_store(MT_CANON, 0, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 0), E1);
    /* a second index in the same leaf */
    assert_eq!(t.mtree_store(MT_CANON, 5, E2, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 5), E2);
    assert_eq!(t.mtree_load(MT_CANON, 0), E1);
    assert_eq!(t.mtree_load(MT_CANON, 4), 0);
    assert!(used_nodes(&t) > 0);
}

#[test]
fn store_range_and_load() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store_range(MT_CANON, 100, 109, E3, GFP_KERNEL), 0);
    for i in 100..=109 {
        assert_eq!(t.mtree_load(MT_CANON, i), E3, "index {i}");
    }
    assert_eq!(t.mtree_load(MT_CANON, 99), 0);
    assert_eq!(t.mtree_load(MT_CANON, 110), 0);
    /* index > last is -EINVAL; xa_is_advanced entries WARN + -EINVAL */
    assert_eq!(t.mtree_store_range(MT_CANON, 50, 40, E0, GFP_KERNEL), -EINVAL);
    assert_eq!(t.mtree_store(MT_CANON, 7, XA_ZERO_ENTRY, GFP_KERNEL), -EINVAL);
}

#[test]
fn many_sparse_indexes() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    let idxs = [
        0u64,
        1,
        2,
        63,
        64,
        65,
        255,
        256,
        4095,
        4096,
        1 << 20,
        (1 << 40) + 7,
        u64::MAX - 1,
        u64::MAX,
    ];
    for (i, &ix) in idxs.iter().enumerate() {
        assert_eq!(t.mtree_store(MT_CANON, ix, E0 + 0x10 * i as u64, GFP_KERNEL), 0, "index {ix}");
    }
    for (i, &ix) in idxs.iter().enumerate() {
        assert_eq!(t.mtree_load(MT_CANON, ix), E0 + 0x10 * i as u64, "index {ix}");
    }
    /* unwritten neighbours stay NULL */
    assert_eq!(t.mtree_load(MT_CANON, 66), 0);
    assert_eq!(t.mtree_load(MT_CANON, (1 << 40) + 6), 0);
    assert_eq!(t.mtree_load(MT_CANON, u64::MAX - 2), 0);
}

#[test]
fn mas_store_and_erase() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    /* cursor-level store then erase returns the old entry */
    let mut mas = Mas::new(MT_CANON, 42, 42);
    assert_eq!(t.mas_store_gfp(&mut mas, E4, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 42), E4);
    let mut mas = Mas::new(MT_CANON, 42, 42);
    assert_eq!(t.mas_erase(&mut mas), E4);
    assert_eq!(t.mtree_load(MT_CANON, 42), 0);
    /* mtree_erase convenience wrapper */
    assert_eq!(t.mtree_store(MT_CANON, 9, E5, GFP_KERNEL), 0);
    assert_eq!(t.mtree_erase(MT_CANON, 9), E5);
    assert_eq!(t.mtree_erase(MT_CANON, 9), 0);
}

#[test]
fn insert_eexist() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_insert(MT_CANON, 3, E0, GFP_KERNEL), 0);
    assert_eq!(t.mtree_insert(MT_CANON, 3, E1, GFP_KERNEL), -EEXIST);
    assert_eq!(t.mtree_load(MT_CANON, 3), E0);
    /* insert_range overlapping an occupied range fails too */
    assert_eq!(t.mtree_store_range(MT_CANON, 10, 20, E2, GFP_KERNEL), 0);
    assert_eq!(t.mtree_insert_range(MT_CANON, 15, 25, E3, GFP_KERNEL), -EEXIST);
    assert_eq!(t.mtree_insert_range(MT_CANON, 21, 25, E3, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 23), E3);
}

#[test]
fn mt_find_iteration() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store(MT_CANON, 3, E0, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store_range(MT_CANON, 10, 12, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store(MT_CANON, 40, E2, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store(MT_CANON, 900, E3, GFP_KERNEL), 0);
    let mut got = std::vec::Vec::new();
    let mut i = 0u64;
    loop {
        let e = t.mt_find(MT_CANON, &mut i, u64::MAX);
        if e == 0 {
            break;
        }
        got.push(e);
    }
    /* [10,12] is one range -> one entry */
    assert_eq!(got, vec![E0, E1, E2, E3]);
    /* mt_find_after requires a nonzero start */
    assert_eq!(t.mt_find_after(MT_CANON, &mut 0u64, u64::MAX), 0);
}

#[test]
fn mas_next_prev_walk() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    for (i, &ix) in [5u64, 6, 7, 30, 31, 80].iter().enumerate() {
        assert_eq!(t.mtree_store(MT_CANON, ix, E0 + i as u64 * 0x10, GFP_KERNEL), 0);
    }
    /* forward walk from the bottom */
    let mut mas = Mas::new(MT_CANON, 0, 0);
    let e1 = t.mas_next(&mut mas, u64::MAX);
    assert_eq!(e1, E0);
    let e2 = t.mas_next(&mut mas, u64::MAX);
    assert_eq!(e2, E0 + 0x10); /* distinct ranges: [5,7] is one range entry */
    /* backward walk from the top */
    let mut mas = Mas::new(MT_CANON, u64::MAX, u64::MAX);
    let last = t.mas_prev(&mut mas, 0);
    assert_eq!(last, E0 + 5 * 0x10);
    /* mt_next/mt_prev convenience wrappers */
    assert_eq!(t.mt_next(MT_CANON, 0, u64::MAX), E0);
    assert_eq!(t.mt_prev(MT_CANON, u64::MAX, 0), E0 + 5 * 0x10);
}

#[test]
fn mas_find_and_find_rev() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store_range(MT_CANON, 100, 109, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store_range(MT_CANON, 200, 209, E2, GFP_KERNEL), 0);
    /* mas_find lands on the first entry >= index */
    let mut mas = Mas::new(MT_CANON, 0, 0);
    assert_eq!(t.mas_find(&mut mas, u64::MAX), E1);
    assert_eq!(mas.index, 100);
    assert_eq!(mas.last, 109);
    assert_eq!(t.mas_find(&mut mas, u64::MAX), E2);
    assert_eq!(mas.index, 200);
    assert_eq!(t.mas_find(&mut mas, u64::MAX), 0);
    /* reverse search finds the last entry <= index */
    let mut mas = Mas::new(MT_CANON, u64::MAX, u64::MAX);
    assert_eq!(t.mas_find_rev(&mut mas, 0), E2);
    assert_eq!(mas.index, 200);
    assert_eq!(mas.last, 209);
    let mut mas = Mas::new(MT_CANON, 150, 150);
    assert_eq!(t.mas_find_rev(&mut mas, 0), E1);
}

#[test]
fn empty_area_search() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store_range(MT_CANON, 100, 109, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store_range(MT_CANON, 200, 209, E2, GFP_KERNEL), 0);
    /* gap [110,199] is 90 slots: forward search takes the low end */
    let mut mas = Mas::new(MT_CANON, 0, 0);
    assert_eq!(t.mas_empty_area(&mut mas, 110, 199, 50), 0);
    assert_eq!(mas.index, 110);
    assert_eq!(mas.last, 159);
    /* reverse takes the high end */
    let mut mas = Mas::new(MT_CANON, u64::MAX, u64::MAX);
    assert_eq!(t.mas_empty_area_rev(&mut mas, 110, 199, 50), 0);
    assert_eq!(mas.index, 150);
    assert_eq!(mas.last, 199);
    /* a request larger than the window is -EINVAL ... */
    let mut mas = Mas::new(MT_CANON, 0, 0);
    assert_eq!(t.mas_empty_area(&mut mas, 100, 209, 200), -EINVAL);
    /* ... and a window that is fully occupied is -EBUSY */
    assert_eq!(t.mtree_store_range(MT_CANON, 300, 319, E4, GFP_KERNEL), 0);
    let mut mas = Mas::new(MT_CANON, 0, 0);
    assert_eq!(t.mas_empty_area(&mut mas, 300, 319, 10), -EBUSY);
}

#[test]
fn alloc_range_tree() {
    let (mut n, mut s, f) = world(NP, NSP, MT_FLAGS_ALLOC_RANGE);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    let mut start = 0u64;
    /* alloc requires MT_FLAGS_ALLOC_RANGE; finds lowest free range */
    assert_eq!(t.mtree_alloc_range(MT_CANON, &mut start, E0, 10, 0, 999, GFP_KERNEL), 0);
    assert_eq!(start, 0);
    assert_eq!(t.mtree_load(MT_CANON, 5), E0);
    let mut start2 = 0u64;
    assert_eq!(t.mtree_alloc_range(MT_CANON, &mut start2, E1, 10, 0, 999, GFP_KERNEL), 0);
    assert_eq!(start2, 10);
    /* non-alloc tree rejects alloc */
    let (mut n2, mut s2, f2) = world(NP, NSP, 0);
    let mut t2 = MapleTree::new(&mut n2, &mut s2, f2);
    let mut x = 0u64;
    assert_eq!(t2.mtree_alloc_range(MT_CANON, &mut x, E0, 4, 0, 99, GFP_KERNEL), -EINVAL);
}

#[test]
fn alloc_cyclic_wraps() {
    let (mut n, mut s, f) = world(NP, NSP, MT_FLAGS_ALLOC_RANGE);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store_range(MT_CANON, 0, 9, E0, GFP_KERNEL), 0);
    let mut start = 0u64;
    let mut next = 0u64;
    assert_eq!(t.mtree_alloc_cyclic(MT_CANON, &mut start, E1, 0, 19, &mut next, GFP_KERNEL), 0);
    assert_eq!(start, 10);
    assert_eq!(next, 11);
    /* next=15 exceeds hi=14 -> wraps to the window start; returns 1 */
    let mut start = 0u64;
    let mut next = 15u64;
    assert_eq!(t.mtree_alloc_cyclic(MT_CANON, &mut start, E2, 0, 14, &mut next, GFP_KERNEL), 1);
    assert_eq!(start, 11); /* first free index after [0,10] */
    assert_eq!(next, 12);
}

#[test]
fn enomem_injection() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    /* the first store into an empty tree needs no allocation (new root) */
    assert_eq!(t.mtree_store(MT_CANON, 0, E0, GFP_KERNEL), 0);
    let base = t.alloc_ctr;
    /* fail the next TWO allocation attempts: upstream mas_nomem retries
     * the alloc once before propagating, so a single failed attempt is
     * absorbed even under GFP_NOWAIT */
    t.fail_mask = 0b11 << base;
    assert_eq!(t.mtree_store(MT_CANON, 5, E1, GFP_NOWAIT), -ENOMEM);
    assert_eq!(t.alloc_ctr, base + 2); /* both masked attempts counted */
    /* clear the mask: the same store now succeeds */
    t.fail_mask = 0;
    assert_eq!(t.mtree_store(MT_CANON, 5, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 5), E1);
    assert_eq!(t.mtree_load(MT_CANON, 0), E0);
    /* the failed attempt left no half-inserted state */
    assert_eq!(t.mtree_load(MT_CANON, 6), 0);
}

#[test]
fn exhausted_node_pool() {
    /* a 6-node arena cannot hold a deep tree; inserts eventually ENOMEM */
    let (mut n, mut s, f) = world(6, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    let mut ok = 0;
    let mut nomem = false;
    for i in 0..64u64 {
        match t.mtree_store(MT_CANON, i * 1000, E0 + i * 0x10, GFP_KERNEL) {
            0 => ok += 1,
            e if e == -ENOMEM => {
                nomem = true;
                break;
            }
            e => panic!("unexpected store result {e}"),
        }
    }
    assert!(nomem, "small arena should exhaust");
    /* every successful store still loads correctly */
    for i in 0..ok {
        assert_eq!(t.mtree_load(MT_CANON, i * 1000), E0 + i * 0x10);
    }
}

#[test]
fn sheaf_pool_semantics() {
    /* no sheaf objects: prefill/get_sheaf fail, and a forced multi-node
     * request lands the cursor in MA_ERROR (single-node requests still
     * take the mt_alloc_one path, matching upstream) */
    let (mut n, mut s, f) = world(NP, 0, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert!(t.kmem_cache_prefill_sheaf(4).is_none());
    assert!(t.mt_get_sheaf(GFP_KERNEL, 4).is_none());
    let mut mas = Mas::new(MT_CANON, 500, 500);
    mas.node_request = 4;
    t.mas_alloc_nodes(&mut mas, GFP_KERNEL);
    assert!(mas_is_err(&mas));
    assert_eq!(xa_err(mas.node), -ENOMEM);

    /* with sheafs present they fill, return, and exhaust deterministically */
    let (mut n2, mut s2, f2) = world(NP, 2, 0);
    let mut t2 = MapleTree::new(&mut n2, &mut s2, f2);
    let sh = t2.kmem_cache_prefill_sheaf(4).unwrap();
    assert_eq!(t2.sheaf_size(sh), 4);
    t2.kmem_cache_return_sheaf(sh);
    let a = t2.kmem_cache_prefill_sheaf(2).unwrap();
    let b = t2.kmem_cache_prefill_sheaf(2).unwrap();
    assert!(t2.kmem_cache_prefill_sheaf(1).is_none()); /* pool exhausted */
    t2.kmem_cache_return_sheaf(a);
    assert!(t2.kmem_cache_prefill_sheaf(1).is_some());
    t2.kmem_cache_return_sheaf(b);
}

#[test]
fn mas_state_transitions() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    let mut mas = Mas::new(MT_CANON, 7, 7);
    assert!(mas_is_start(&mas));
    assert!(!mas_is_active(&mas));
    /* first store moves through the state machine */
    assert_eq!(t.mas_store_gfp(&mut mas, E0, GFP_KERNEL), 0);
    /* mas_set re-arms the cursor at a new index */
    t.mas_set(&mut mas, 99);
    assert!(mas_is_start(&mas));
    assert_eq!(mas.index, 99);
    assert_eq!(mas.last, 99);
    /* walk pauses the cursor */
    let mut mas = Mas::new(MT_CANON, 0, 0);
    let _ = t.mas_next(&mut mas, u64::MAX);
    t.mas_pause(&mut mas);
    assert!(mas_is_paused(&mas));
    /* an injected allocation failure lands the cursor in MA_ERROR */
    let base = t.alloc_ctr;
    t.fail_mask = 0b11 << base;
    let mut mas = Mas::new(MT_CANON, 200000, 200000);
    mas.node_request = 2; /* force the sheaf alloc path deterministically */
    t.mas_alloc_nodes(&mut mas, GFP_NOWAIT);
    assert!(mas_is_err(&mas));
    assert_eq!(xa_err(mas.node), -ENOMEM);
}

#[test]
fn preallocate_path() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    let mut mas = Mas::new(MT_CANON, 33, 40);
    assert_eq!(t.mas_preallocate(&mut mas, E2, GFP_KERNEL), 0);
    assert!(mas.node_request == 0 || mas.alloc != 0 || mas.sheaf != 0);
    t.mas_store_prealloc(&mut mas, E2);
    for i in 33..=40 {
        assert_eq!(t.mtree_load(MT_CANON, i), E2, "index {i}");
    }
    /* prealloc store without a preceding preallocate still works via mas_store */
    let mut mas = Mas::new(MT_CANON, 60, 60);
    assert_eq!(t.mas_store(&mut mas, E3), 0);
    assert_eq!(t.mtree_load(MT_CANON, 60), E3);
}

#[test]
fn large_index_wraparound() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store(MT_CANON, u64::MAX, E0, GFP_KERNEL), 0);
    assert_eq!(t.mtree_store(MT_CANON, u64::MAX - 1, E1, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, u64::MAX), E0);
    assert_eq!(t.mtree_load(MT_CANON, u64::MAX - 1), E1);
    assert_eq!(t.mtree_load(MT_CANON, u64::MAX - 2), 0);
    /* empty-area search at the top of the address space */
    let mut mas = Mas::new(MT_CANON, u64::MAX - 10, u64::MAX - 10);
    let r = t.mas_empty_area(&mut mas, u64::MAX - 10, u64::MAX, 5);
    assert!(r == 0 || r == -EBUSY);
    if r == 0 {
        assert!(mas.index >= u64::MAX - 10);
        assert!(mas.last <= u64::MAX);
        assert!(!(mas.index..=mas.last).contains(&(u64::MAX - 1)));
        assert!(!(mas.index..=mas.last).contains(&u64::MAX));
    }
    /* find walks off the end without wrapping */
    let mut i = u64::MAX;
    assert_eq!(t.mt_find(MT_CANON, &mut i, u64::MAX), E0);
}

#[test]
fn destroy_frees_all() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    for i in 0..40u64 {
        assert_eq!(t.mtree_store(MT_CANON, i * 3, E0 + i * 0x10, GFP_KERNEL), 0);
    }
    assert!(used_nodes(&t) > 0);
    assert_ne!(t.troot(MT_CANON), 0);
    t.mtree_destroy(MT_CANON);
    assert_eq!(t.troot(MT_CANON), 0);
    assert_eq!(used_nodes(&t), 0);
    assert_eq!(t.mtree_load(MT_CANON, 0), 0);
    /* the arena is reusable after destroy */
    assert_eq!(t.mtree_store(MT_CANON, 1, E4, GFP_KERNEL), 0);
    assert_eq!(t.mtree_load(MT_CANON, 1), E4);
}

#[test]
fn dup_copies_tree() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    for (i, &ix) in [0u64, 5, 9, 100, 101, 102, 5000].iter().enumerate() {
        assert_eq!(t.mtree_store(MT_CANON, ix, E0 + i as u64 * 0x10, GFP_KERNEL), 0);
    }
    assert_eq!(t.mtree_dup(MT_CANON, MT2_CANON, GFP_KERNEL), 0);
    assert_ne!(t.troot(MT2_CANON), 0);
    for (i, &ix) in [0u64, 5, 9, 100, 101, 102, 5000].iter().enumerate() {
        assert_eq!(t.mtree_load(MT2_CANON, ix), E0 + i as u64 * 0x10, "index {ix}");
    }
    /* independent: erasing in the copy does not touch the original */
    assert_eq!(t.mtree_erase(MT2_CANON, 5), E0 + 0x10);
    assert_eq!(t.mtree_load(MT_CANON, 5), E0 + 0x10);
    assert_eq!(t.mtree_load(MT2_CANON, 5), 0);
}

#[test]
fn reset_all_reinitializes() {
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store(MT_CANON, 8, E0, GFP_KERNEL), 0);
    t.fail_mask = 0xdead;
    t.reset_all(MT_FLAGS_ALLOC_RANGE);
    assert_eq!(t.mtree_load(MT_CANON, 8), 0);
    assert_eq!(t.tflags(MT_CANON), MT_FLAGS_ALLOC_RANGE);
    assert_eq!(t.fail_mask, 0xdead); /* persists, like the harness */
    assert_eq!(t.alloc_ctr, 0);
    assert_eq!(used_nodes(&t), 0);
}

#[test]
fn crafted_node_type_table_clamps() {
    // mte_node_type masks to 0..15 but the MT_* tables have 5 rows;
    // crafted enode types must clamp to the last row, not index OOB.
    let enode = mt_mk_node(NBASE, 15);
    assert_eq!(mt_slot_count(enode), 3); /* clamped to the copy row */
    let _ = ma_pivots(NBASE, 15);
    let _ = ma_slots(NBASE, 15);
}

#[test]
fn wild_root_ptr_bounded() {
    // a tree head holding a wild pointer: traversal hits the NULLN
    // scratch via nidx and returns bounded, not a hang/panic.
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    t.set_troot(MT_CANON, NBASE + 0x7fff00);
    let _ = t.mtree_load(MT_CANON, 5);
    t.set_troot(MT_CANON, mt_mk_node(NBASE + 0x3000, 15));
    let _ = t.mtree_load(MT_CANON, 5);
}

#[test]
fn w_sw_word_index_bounded() {
    // k >= 32 routes to the scratch node instead of indexing w[32];
    // copy/zero helpers clamp n at the 32-word node bound.
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.w(NBASE, 0xffff), 0);
    t.sw(NBASE, 0xffff, 0xdead_beef);
    assert_eq!(t.w(NBASE, 0xffff), 0xdead_beef); /* scratch read-back */
    t.wcopy(NBASE, NBASE + 256, 64);
    t.wzero(NBASE, 0, 100);
    t.rcopy(NBASE, NBASE + 8, 64);
    t.rzero(NBASE, 100);
}

#[test]
fn crafted_mas_offset_no_panic() {
    // caller-controlled Mas fields feed u8 arithmetic and slot indices;
    // extreme values must wrap/clamp like C, not panic.
    let (mut n, mut s, f) = world(NP, NSP, 0);
    let mut t = MapleTree::new(&mut n, &mut s, f);
    assert_eq!(t.mtree_store(MT_CANON, 1, E0, GFP_KERNEL), 0);
    let mut mas = Mas::new(MT_CANON, 0, 0);
    mas.offset = u8::MAX;
    mas.end = u8::MAX;
    let _ = t.mas_find(&mut mas, 10);
}
