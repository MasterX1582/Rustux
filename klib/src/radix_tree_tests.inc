// klib::radix_tree tests — caller-owned node arena, canonical-address model.
// Entries are plain aligned u64 "pointers" (low bits 00, below NBASE).

use std::vec;

const E0: u64 = 0x1000;
const E1: u64 = 0x1010;
const E2: u64 = 0x1020;
const E3: u64 = 0x1030;
const GFP: u32 = GFP_KERNEL;

fn nodes(n: usize) -> std::vec::Vec<Node> {
    vec![Node::new(); n]
}

#[test]
fn insert_lookup_delete() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    assert_eq!(rt.radix_tree_lookup(5), 0);
    assert_eq!(rt.radix_tree_insert(5, E0), 0);
    assert_eq!(rt.radix_tree_lookup(5), E0);
    // duplicate insert -> -EEXIST
    assert_eq!(rt.radix_tree_insert(5, E1), -EEXIST);
    assert_eq!(rt.radix_tree_lookup(5), E0);
    assert_eq!(rt.radix_tree_lookup_slot(5), Some(SlotL::Node(0, 5)));
    assert_eq!(rt.radix_tree_delete(5), E0);
    assert_eq!(rt.radix_tree_lookup(5), 0);
    assert_eq!(rt.radix_tree_lookup_slot(5), None);
}

#[test]
fn delete_item() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(7, E1);
    // item mismatch: entry stays
    assert_eq!(rt.radix_tree_delete_item(7, E2), 0);
    assert_eq!(rt.radix_tree_lookup(7), E1);
    assert_eq!(rt.radix_tree_delete_item(7, E1), E1);
    assert_eq!(rt.radix_tree_lookup(7), 0);
}

#[test]
fn lookup_slot_and_replace() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(3, E0);
    let slot = rt.radix_tree_lookup_slot(3).unwrap();
    rt.radix_tree_replace_slot(slot, E2);
    assert_eq!(rt.radix_tree_lookup(3), E2);
}

#[test]
fn deep_index_multi_level() {
    let mut arena = nodes(16);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(1 << 14, E0); /* needs level >= 2 */
    rt.radix_tree_insert(3, E1);
    assert_eq!(rt.radix_tree_lookup(1 << 14), E0);
    assert_eq!(rt.radix_tree_lookup(3), E1);
    assert_eq!(rt.radix_tree_lookup(1 << 13), 0);
}

#[test]
fn tags() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(1, E0);
    assert_eq!(rt.radix_tree_tag_get(1, 0), 0);
    rt.radix_tree_tag_set(1, 0);
    assert_eq!(rt.radix_tree_tag_get(1, 0), 1);
    assert_ne!(rt.radix_tree_tagged(0), 0);
    assert_eq!(rt.radix_tree_tagged(1), 0);
    rt.radix_tree_tag_clear(1, 0);
    assert_eq!(rt.radix_tree_tag_get(1, 0), 0);
}

#[test]
fn gang_lookups() {
    let mut arena = nodes(16);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(1, E0);
    rt.radix_tree_insert(5, E1);
    rt.radix_tree_insert(9, E2);
    rt.radix_tree_tag_set(9, 1);
    let mut out = [0u64; 8];
    let n = rt.radix_tree_gang_lookup(&mut out, 0);
    assert_eq!(&out[..n as usize], &[E0, E1, E2]);
    let n = rt.radix_tree_gang_lookup(&mut out, 4);
    assert_eq!(&out[..n as usize], &[E1, E2]);
    let n = rt.radix_tree_gang_lookup_tag(&mut out, 0, 1);
    assert_eq!(&out[..n as usize], &[E2]);
    let mut slots = [SlotL::Root; 8];
    let n = rt.radix_tree_gang_lookup_tag_slot(&mut slots, 0, 1);
    assert_eq!(n, 1);
    assert_eq!(slots[0], SlotL::Node(0, 9));
    let n = rt.radix_tree_gang_lookup_slot(&mut slots, 0);
    assert_eq!(n, 3);
}

#[test]
fn iteration() {
    let mut arena = nodes(16);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(2, E0);
    rt.radix_tree_insert(4, E1);
    rt.radix_tree_insert(70, E2);
    let mut iter = RIter::new();
    let slot = radix_tree_iter_init(&mut iter, 0);
    assert!(slot.is_none());
    let s = rt.radix_tree_next_chunk(&mut iter, 0).unwrap();
    assert_eq!(iter.index, 2);
    let mut order = std::vec::Vec::new();
    let mut s = Some(s);
    while let Some(sl) = s {
        order.push(rt.slot_get(sl));
        // next_slot walks within the chunk; next_chunk crosses leaf
        // boundaries (faithful upstream iteration protocol)
        s = rt
            .radix_tree_next_slot(Some(sl), &mut iter, 0)
            .or_else(|| rt.radix_tree_next_chunk(&mut iter, 0));
    }
    assert_eq!(order, vec![E0, E1, E2]);
}

#[test]
fn iter_delete_and_tag_clear() {
    let mut arena = nodes(16);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(2, E0);
    rt.radix_tree_insert(4, E1);
    rt.radix_tree_tag_set(4, 2);
    let mut iter = RIter::new();
    let slot = rt.radix_tree_iter_lookup(&mut iter, 4).unwrap();
    rt.radix_tree_iter_tag_clear(&mut iter, 2);
    assert_eq!(rt.radix_tree_tag_get(4, 2), 0);
    rt.radix_tree_iter_delete(&mut iter, slot);
    assert_eq!(rt.radix_tree_lookup(4), 0);
}

#[test]
fn preload_path() {
    let mut arena = nodes(24);
    let mut rt = RadixTree::new(&mut arena, GFP);
    assert_eq!(rt.radix_tree_preload(GFP), 0);
    assert_eq!(rt.radix_tree_maybe_preload(GFP), 0);
    // GFP_ATOMIC insert draws from the preload list on alloc failure
    rt.fail_mask = 1; /* fail the first slab alloc */
    assert_eq!(rt.radix_tree_insert(1, E0), 0);
    assert_eq!(rt.radix_tree_lookup(1), E0);
    rt.radix_tree_preload_end();
}

#[test]
fn enomem() {
    let mut arena = nodes(2);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.radix_tree_insert(1, E0); /* one node allocated */
    // second deep insert needs 2 fresh nodes; only 1 left in a 2-arena? -> ENOMEM
    let r = rt.radix_tree_insert(1 << 14, E1);
    assert!(r == 0 || r == -ENOMEM);
    if r == 0 {
        assert_eq!(rt.radix_tree_lookup(1 << 14), E1);
    }
}

#[test]
fn empty_and_shrink() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    assert!(rt.radix_tree_empty());
    rt.radix_tree_insert(9, E0);
    assert!(!rt.radix_tree_empty());
    rt.radix_tree_delete(9);
    assert!(rt.radix_tree_empty());
}

#[test]
fn contiguous_iter_flag() {
    let mut arena = nodes(8);
    let mut rt = RadixTree::new(&mut arena, GFP);
    for i in 0..6u64 {
        rt.radix_tree_insert(i, E0 + i * 8);
    }
    let mut iter = RIter::new();
    let mut idxs = std::vec::Vec::new();
    let mut slot = None;
    loop {
        let s = match slot {
            Some(s) => s,
            None => match rt.radix_tree_next_chunk(&mut iter, ITER_CONTIG) {
                None => break,
                Some(s) => s,
            },
        };
        idxs.push(iter.index);
        slot = rt.radix_tree_next_slot(Some(s), &mut iter, ITER_CONTIG);
    }
    assert_eq!(idxs, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn crafted_root_entry_no_panic() {
    // an internal-class root entry whose node index underflows the
    // arena (entry_to_node wraps) must hit the scratch node via nat(),
    // not panic on an OOB index.
    let mut arena = nodes(4);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.root.head = xa_mk_internal(0x100); /* internal, far below NBASE */
    assert_eq!(rt.radix_tree_lookup(0), 0);
    assert_eq!(rt.radix_tree_tag_get(0, 0), 0);
}

#[test]
fn crafted_node_fields_bounded() {
    // a real node pointer to a node with absurd shift/count: member
    // reads clamp (.min(63)/CHUNK_SIZE-1), descent returns bounded.
    let mut arena = nodes(4);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.nodes[0].used = true;
    rt.nodes[0].shift = 200;
    rt.nodes[0].count = 63;
    rt.nodes[0].nr_values = 63;
    rt.nodes[0].offset = 63;
    rt.root.head = node_ptr(0);
    let _ = rt.radix_tree_lookup(0);
    let _ = rt.radix_tree_lookup_slot(0);
}

#[test]
fn persistent_retry_entry_bounded() {
    // XA_RETRY_ENTRY in a leaf slot spins xa_find's xas_retry loop in C
    // (and in the unbounded klib cut); the 256-try bound now returns.
    let mut arena = nodes(4);
    let mut rt = RadixTree::new(&mut arena, GFP);
    rt.nodes[0].used = true;
    rt.nodes[0].shift = 0;
    rt.nodes[0].count = 1;
    rt.nodes[0].slots[0] = XA_RETRY_ENTRY;
    rt.root.head = node_ptr(0);
    let _ = rt.radix_tree_lookup(0); /* bounded: must return, not spin */
}

#[test]
fn riter_wild_node_index_bounded() {
    // a caller-crafted iterator pointing outside the arena indexes via
    // nat() scratch instead of panicking.
    let mut arena = nodes(4);
    let mut rt = RadixTree::new(&mut arena, GFP);
    assert_eq!(rt.radix_tree_insert(0, E0), 0);
    let mut it = RIter::new();
    it.node = Some(usize::MAX);
    let _ = rt.radix_tree_next_chunk(&mut it, 0);
}
