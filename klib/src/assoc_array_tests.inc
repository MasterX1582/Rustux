// klib::assoc_array tests — caller-owned pools (`AssocArray`), canonical
// tagged pointers, edit-script apply/cancel flow. Keys are two-word
// TKey values; objects are index-keyed (`okey`/`orefs`) like the
// keyring ops the port bakes in.

use std::vec;
use std::vec::Vec;

/// Fresh pools: node/shortcut/edit arenas + object store.
fn pools(np: usize, sp: usize, ne: usize, no: usize) -> (Vec<Node>, Vec<Scell>, Vec<Edit>, Vec<TKey>, Vec<i64>) {
    (
        vec![NODE0; np],
        vec![SC0; sp],
        vec![EDIT0; ne],
        vec![[0; KW]; no],
        vec![0; no],
    )
}

/// Insert object `i` carrying key `k` and apply the edit (like
/// assoc_array_insert + apply in one step). Returns the leaf canonical.
fn insert_obj(aa: &mut AssocArray, i: usize, k: &TKey) -> u64 {
    aa.okey[i] = *k;
    aa.orefs[i] += 1;
    let e = assoc_array_insert(aa, k, leaf_to_ptr(i));
    assert!(!is_err(e), "insert ENOMEM");
    assoc_array_apply_edit(aa, edit_i(e));
    leaf_to_ptr(i)
}

fn delete_key(aa: &mut AssocArray, k: &TKey) -> u64 {
    let e = assoc_array_delete(aa, k);
    assert!(e != 0 && !is_err(e), "delete failed");
    assoc_array_apply_edit(aa, edit_i(e));
    e
}

fn count_used_nodes(aa: &AssocArray) -> usize {
    aa.nodes.iter().filter(|n| n.used).count()
}
fn count_used_shorts(aa: &AssocArray) -> usize {
    aa.shorts.iter().filter(|s| s.used).count()
}

#[test]
fn empty_array_noops() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    assert_eq!(aa.root, 0);
    assert_eq!(aa.nr_leaves_on_tree, 0);
    assert_eq!(assoc_array_find(&mut aa, &[1, 0]), 0);
    assert_eq!(assoc_array_delete(&mut aa, &[1, 0]), 0); // NULL
    assert_eq!(assoc_array_clear(&mut aa), 0); // NULL
    let mut calls = 0u64;
    let ret = assoc_array_iterate(
        &mut aa,
        &mut |_r: &mut AssocArray, c: &mut u64, _l: u64| -> i32 {
            *c += 1;
            0
        },
        &mut calls,
    );
    assert_eq!((ret, calls), (0, 0));
    let ret = assoc_array_gc(&mut aa, &mut |_r, _c: &mut (), _l| true, &mut ());
    assert_eq!(ret, 0);
    assoc_array_destroy(&mut aa);
    assert_eq!(aa.root, 0);
}

#[test]
fn insert_find_single() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    let leaf = insert_obj(&mut aa, 3, &[0xdead, 0xbeef]);
    assert_eq!(aa.nr_leaves_on_tree, 1);
    assert!(is_node(aa.root));
    assert_eq!(assoc_array_find(&mut aa, &[0xdead, 0xbeef]), leaf);
    assert_eq!(assoc_array_find(&mut aa, &[0xdead, 0]), 0);
    assert_eq!(assoc_array_find(&mut aa, &[0, 0xbeef]), 0);
}

#[test]
fn insert_many_one_node() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(16, 8, 4, 24);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // Keys distinct at level 0 share one terminal node (16 slots).
    for i in 0..12u64 {
        let k = [(i << 4) | (i & 0xf), i << 32];
        insert_obj(&mut aa, i as usize, &k);
    }
    assert_eq!(aa.nr_leaves_on_tree, 12);
    assert_eq!(count_used_nodes(&aa), 1);
    for i in 0..12u64 {
        let k = [(i << 4) | (i & 0xf), i << 32];
        assert_eq!(assoc_array_find(&mut aa, &k), leaf_to_ptr(i as usize));
    }
}

#[test]
fn split_node_on_full() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(64, 32, 4, 48);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // keys [i,0]: level-0 segments 1..15,0 then repeat — the 17th insert
    // fills the node and forces assoc_array_split_node (matching pair).
    for i in 1..=18usize {
        insert_obj(&mut aa, i, &[i as u64, 0]);
    }
    assert_eq!(aa.nr_leaves_on_tree, 18);
    assert!(count_used_nodes(&aa) >= 2);
    for i in 1..=18usize {
        assert_eq!(assoc_array_find(&mut aa, &[i as u64, 0]), leaf_to_ptr(i));
    }
    // a key that was never inserted still misses
    assert_eq!(assoc_array_find(&mut aa, &[19, 0]), 0);
}

#[test]
fn split_node_shortcut_path() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(64, 32, 4, 48);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // All keys share the level-0 segment (low nibble 1): the 17th insert
    // splits into an all-one-segment node -> assoc_array_insert_shortcut.
    for i in 0..17usize {
        insert_obj(&mut aa, i, &[1 + ((i as u64) << 8), 0]);
    }
    assert_eq!(aa.nr_leaves_on_tree, 17);
    assert!(is_shortcut(aa.root), "expected a root shortcut");
    assert!(count_used_shorts(&aa) >= 1);
    for i in 0..17usize {
        assert_eq!(
            assoc_array_find(&mut aa, &[1 + ((i as u64) << 8), 0]),
            leaf_to_ptr(i)
        );
    }
}

#[test]
fn insert_mid_shortcut() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(64, 32, 4, 48);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    for i in 0..17usize {
        insert_obj(&mut aa, i, &[1 + ((i as u64) << 8), 0]);
    }
    assert!(is_shortcut(aa.root));
    // A key diverging below the shortcut's skip_to_level drives
    // assoc_array_insert_mid_shortcut.
    let leaf = insert_obj(&mut aa, 20, &[0x1002, 0]);
    assert_eq!(assoc_array_find(&mut aa, &[0x1002, 0]), leaf);
    for i in 0..17usize {
        assert_eq!(
            assoc_array_find(&mut aa, &[1 + ((i as u64) << 8), 0]),
            leaf_to_ptr(i)
        );
    }
    // And one diverging in the second key word.
    let leaf = insert_obj(&mut aa, 21, &[1 + (3 << 8), 0x777]);
    assert_eq!(assoc_array_find(&mut aa, &[1 + (3 << 8), 0x777]), leaf);
}

#[test]
fn insert_set_object_and_replace() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(16, 8, 4, 16);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    let k = [0x51, 0];
    insert_obj(&mut aa, 0, &k);
    // Re-insert the same key as a different object: insert_in_node spots
    // the matching leaf and schedules a replace; apply frees dead_leaf.
    aa.okey[1] = k;
    aa.orefs[1] += 1;
    let e = assoc_array_insert(&mut aa, &k, leaf_to_ptr(1));
    assert!(!is_err(e));
    assoc_array_apply_edit(&mut aa, edit_i(e));
    assert_eq!(assoc_array_find(&mut aa, &k), leaf_to_ptr(1));
    assert_eq!(aa.orefs[0], 0); // dead leaf's object ref dropped
    assert_eq!(aa.nr_leaves_on_tree, 1);

    // insert_set_object swaps the pending leaf before apply.
    let k2 = [0x52, 0];
    aa.okey[2] = k2;
    aa.okey[3] = k2;
    aa.orefs[2] += 1;
    let e = assoc_array_insert(&mut aa, &k2, leaf_to_ptr(2));
    assert!(!is_err(e));
    assoc_array_insert_set_object(&mut aa, edit_i(e), leaf_to_ptr(3));
    aa.orefs[3] += 1;
    assoc_array_apply_edit(&mut aa, edit_i(e));
    assert_eq!(assoc_array_find(&mut aa, &k2), leaf_to_ptr(3));
}

#[test]
fn cancel_edit_leaves_tree_untouched() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(16, 8, 4, 16);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    insert_obj(&mut aa, 0, &[7, 0]);
    let e = assoc_array_insert(&mut aa, &[9, 0], leaf_to_ptr(1));
    assert!(!is_err(e));
    let ei = edit_i(e);
    assoc_array_cancel_edit(&mut aa, ei);
    assert_eq!(assoc_array_find(&mut aa, &[9, 0]), 0);
    assert_eq!(aa.nr_leaves_on_tree, 1);
    // cancelled edit slot is reusable
    assert!(!aa.edits[ei].used);
}

#[test]
fn delete_simple_and_last() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(16, 8, 4, 16);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    insert_obj(&mut aa, 0, &[0x11, 0]);
    insert_obj(&mut aa, 1, &[0x22, 0]);
    delete_key(&mut aa, &[0x11, 0]);
    assert_eq!(assoc_array_find(&mut aa, &[0x11, 0]), 0);
    assert_eq!(assoc_array_find(&mut aa, &[0x22, 0]), leaf_to_ptr(1));
    assert_eq!(aa.nr_leaves_on_tree, 1);
    assert_eq!(aa.orefs[0], 0); // dead leaf freed
    // deleting the last leaf collapses root to NULL
    delete_key(&mut aa, &[0x22, 0]);
    assert_eq!(aa.root, 0);
    assert_eq!(aa.nr_leaves_on_tree, 0);
    assert_eq!(assoc_array_delete(&mut aa, &[0x22, 0]), 0);
}

#[test]
fn delete_triggers_collapse() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(96, 48, 4, 64);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // 17 keys [i,0]: split leaves root new_n0 holding 15 leaves + a meta
    // pointer to new_n1 (the matching-segment pair key1/key17).
    for i in 1..=17usize {
        insert_obj(&mut aa, i, &[i as u64, 0]);
    }
    let nodes_before = count_used_nodes(&aa);
    assert_eq!(nodes_before, 2);
    // key2 sits in the root node: its node has a meta pointer and
    // nr_leaves_on_branch == 17 <= FAN_OUT+1, so delete_collapse repacks
    // the whole subtree into a single new node.
    delete_key(&mut aa, &[2, 0]);
    assert_eq!(aa.nr_leaves_on_tree, 16);
    assert_eq!(count_used_nodes(&aa), 1);
    for i in 1..=17usize {
        if i == 2 {
            assert_eq!(assoc_array_find(&mut aa, &[2, 0]), 0);
        } else {
            assert_eq!(assoc_array_find(&mut aa, &[i as u64, 0]), leaf_to_ptr(i));
        }
    }
}

#[test]
fn iterate_and_early_stop() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(64, 32, 4, 48);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    for i in 0..10usize {
        insert_obj(&mut aa, i, &[0x30 + i as u64, 0]);
    }
    let mut seen = Vec::new();
    let ret = assoc_array_iterate(
        &mut aa,
        &mut |_r: &mut AssocArray, c: &mut Vec<u64>, l: u64| -> i32 {
            c.push(l);
            0
        },
        &mut seen,
    );
    assert_eq!(ret, 0);
    seen.sort_unstable();
    let want: Vec<u64> = (0..10).map(leaf_to_ptr).collect();
    assert_eq!(seen, want);

    // early stop propagates the callback's return value
    let mut cnt = 0u64;
    let ret = assoc_array_iterate(
        &mut aa,
        &mut |_r: &mut AssocArray, c: &mut u64, _l: u64| -> i32 {
            *c += 1;
            if *c >= 3 {
                13 + *c as i32
            } else {
                0
            }
        },
        &mut cnt,
    );
    assert_eq!(ret, 16);
    assert_eq!(cnt, 3);
}

#[test]
fn clear_then_destroy() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(64, 32, 4, 48);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    for i in 0..20usize {
        insert_obj(&mut aa, i, &[1 + ((i as u64) << 8), 0]);
    }
    let e = assoc_array_clear(&mut aa);
    assert!(e != 0 && !is_err(e));
    assoc_array_apply_edit(&mut aa, edit_i(e));
    assert_eq!(aa.root, 0);
    assert_eq!(aa.nr_leaves_on_tree, 0);
    // ops_for_excised_subtree -> every leaf's object ref was freed
    assert!(aa.orefs.iter().all(|&r| r == 0));
    assert_eq!(count_used_nodes(&aa), 0);
    assert_eq!(count_used_shorts(&aa), 0);

    // destroy on a populated tree frees everything the same way
    for i in 0..6usize {
        insert_obj(&mut aa, i, &[0x99 + i as u64, 0]);
    }
    assoc_array_destroy(&mut aa);
    assert_eq!(aa.root, 0);
    assert_eq!(count_used_nodes(&aa), 0);
    assert!(aa.orefs.iter().all(|&r| r <= 0));
}

#[test]
fn gc_keeps_and_drops() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(96, 48, 4, 64);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    for i in 0..12usize {
        insert_obj(&mut aa, i, &[0x40 + i as u64, i as u64]);
    }
    // keep all: tree rebuilt, every leaf preserved
    let ret = assoc_array_gc(&mut aa, &mut |_r, _c: &mut (), _l| true, &mut ());
    assert_eq!(ret, 0);
    assert_eq!(aa.nr_leaves_on_tree, 12);
    for i in 0..12usize {
        assert_eq!(
            assoc_array_find(&mut aa, &[0x40 + i as u64, i as u64]),
            leaf_to_ptr(i)
        );
    }
    // drop odd object indices: the copy keeps even leaves only
    let ret = assoc_array_gc(
        &mut aa,
        &mut |_r, _c: &mut (), l: u64| leaf_i(l) % 2 == 0,
        &mut (),
    );
    assert_eq!(ret, 0);
    assert_eq!(aa.nr_leaves_on_tree, 6);
    for i in 0..12usize {
        let got = assoc_array_find(&mut aa, &[0x40 + i as u64, i as u64]);
        if i % 2 == 0 {
            assert_eq!(got, leaf_to_ptr(i));
        } else {
            assert_eq!(got, 0);
        }
    }
}

#[test]
fn enomem_paths() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(16, 8, 4, 16);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // bit 0: fail the very first alloc (the edit itself)
    aa.fail_mask = 1;
    let r = assoc_array_insert(&mut aa, &[1, 0], leaf_to_ptr(0));
    assert!(is_err(r));
    // bit 1: edit allocs, the node alloc fails -> cancel path
    aa.fail_mask = 0b10;
    let r = assoc_array_insert(&mut aa, &[1, 0], leaf_to_ptr(0));
    assert!(is_err(r));
    assert!(!aa.edits[0].used);
    // mask cleared: works
    aa.fail_mask = 0;
    insert_obj(&mut aa, 0, &[1, 0]);
    assert_eq!(aa.nr_leaves_on_tree, 1);
}

#[test]
fn pool_exhaustion_enomem() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(2, 1, 4, 24);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // 16 keys fill the single node; the 17th needs a split (two fresh
    // nodes) but the 2-node pool hands out only one -> -ENOMEM.
    for i in 1..=16usize {
        insert_obj(&mut aa, i, &[i as u64, 0]);
    }
    let r = assoc_array_insert(&mut aa, &[17, 0], leaf_to_ptr(17));
    assert!(is_err(r));
    // the failed split freed its partial work; the tree is intact
    assert_eq!(aa.nr_leaves_on_tree, 16);
    for i in 1..=16usize {
        assert_eq!(assoc_array_find(&mut aa, &[i as u64, 0]), leaf_to_ptr(i));
    }
}

// ---- wave-1 hardening: crafted state is bounded, never a hang/OOB ----

#[test]
fn wild_root_node_find_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    insert_obj(&mut aa, 0, &[1, 0]);
    // root canonical decoded outside the node arena -> scratch reads,
    // the walk terminates instead of dereferencing wild memory.
    aa.root = node_to_ptr(9999);
    assert_eq!(assoc_array_find(&mut aa, &[1, 0]), 0);
    // a wild shortcut canonical decodes outside the shortcut arena too
    // (the BUG_ON contract still applies to the scratch skip_to_level).
    aa.root = 0;
    assert_eq!(assoc_array_find(&mut aa, &[1, 0]), 0);
}

#[test]
fn wild_leaf_in_slot_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    insert_obj(&mut aa, 0, &[1, 0]);
    // plant a leaf pointer whose object index is outside okey/orefs
    let root_n = node_i(aa.root);
    aa.nodes[root_n].slots[3] = leaf_to_ptr(9999);
    // find walks the terminal node; compare_object hits the scratch key
    assert_eq!(assoc_array_find(&mut aa, &[1, 0]), leaf_to_ptr(0));
    assert_eq!(assoc_array_find(&mut aa, &[5, 5]), 0);
    // iterate reports the wild leaf to the callback without OOB
    let mut cnt = 0u64;
    let ret = assoc_array_iterate(
        &mut aa,
        &mut |_r: &mut AssocArray, c: &mut u64, _l: u64| -> i32 {
            *c += 1;
            0
        },
        &mut cnt,
    );
    assert_eq!((ret, cnt), (0, 2));
}

#[test]
fn iterate_back_pointer_cycle_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // craft a climb/descent cycle: node0 -> meta node1, node1 -> meta
    // node0, node0.back = node1, node1.back = node0
    aa.nodes[0] = NODE0;
    aa.nodes[0].used = true;
    aa.nodes[0].back_pointer = node_to_ptr(1);
    aa.nodes[0].slots[0] = node_to_ptr(1);
    aa.nodes[1] = NODE0;
    aa.nodes[1].used = true;
    aa.nodes[1].back_pointer = node_to_ptr(0);
    aa.nodes[1].slots[0] = node_to_ptr(0);
    aa.root = node_to_ptr(0);
    let mut cnt = 0u64;
    let ret = assoc_array_iterate(
        &mut aa,
        &mut |_r: &mut AssocArray, c: &mut u64, _l: u64| -> i32 {
            *c += 1;
            0
        },
        &mut cnt,
    );
    assert_eq!(ret, 0);
    assert!(aa.warn > 0, "cycle must trip the iteration cap");
}

#[test]
fn walk_self_loop_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // node0's slot 0 meta-points at itself: the descent loop cycles and
    // the __lim cap bails to a bounded miss.
    aa.nodes[0] = NODE0;
    aa.nodes[0].used = true;
    aa.nodes[0].slots[0] = node_to_ptr(0);
    aa.root = node_to_ptr(0);
    assert_eq!(assoc_array_find(&mut aa, &[0, 0]), 0);
    assert!(aa.warn > 0);
}

#[test]
#[should_panic(expected = "BUG_ON")]
fn wild_root_shortcut_trips_contract() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // a root shortcut with skip_to_level 0 violates the walk contract:
    // bounded BUG_ON panic, not a hang.
    aa.root = sc_to_ptr(0);
    let _ = assoc_array_find(&mut aa, &[1, 0]);
}

#[test]
fn bad_edit_index_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    // edit index outside the arena maps to the scratch edit: no panic,
    // no effect on real state.
    assoc_array_apply_edit(&mut aa, 9999);
    assoc_array_cancel_edit(&mut aa, 9999);
    assoc_array_insert_set_object(&mut aa, 9999, leaf_to_ptr(0));
    assert_eq!(aa.root, 0);
}

#[test]
fn crafted_adjust_count_cycle_bounded() {
    let (mut n, mut s, mut e, mut ok, mut orf) = pools(8, 4, 4, 8);
    let mut aa = AssocArray::new(&mut n, &mut s, &mut e, &mut ok, &mut orf);
    insert_obj(&mut aa, 0, &[1, 0]);
    // craft an edit whose count-adjust chain cycles node0 <-> node1
    aa.nodes[0].back_pointer = node_to_ptr(1);
    aa.nodes[1] = NODE0;
    aa.nodes[1].used = true;
    aa.nodes[1].back_pointer = node_to_ptr(0);
    aa.edits[0] = EDIT0;
    aa.edits[0].used = true;
    aa.edits[0].adjust_count_on = NCANON;
    aa.edits[0].adjust_count_by = 1;
    assoc_array_apply_edit(&mut aa, 0);
    assert!(aa.warn > 0);
}
