// klib::xarray tests — caller-owned node arena, canonical-address model.
// Entries used below are plain aligned u64 "pointers" (low bits 00, outside
// the node address space) or xa_mk_value() tagged values.

use std::vec;

const E0: u64 = 0x1000;
const E1: u64 = 0x1010;
const E2: u64 = 0x1020;
const E3: u64 = 0x1030;

fn arena() -> std::vec::Vec<Node> {
    vec![Node::new(); NP]
}

#[test]
fn entry_encodings() {
    for v in [0u64, 1, 42, u64::MAX >> 1] {
        let e = xa_mk_value(v);
        assert!(xa_is_value(e));
        assert!(!xa_is_internal(e));
        assert_eq!(xa_to_value(e), v);
    }
    assert!(xa_is_internal(xa_mk_internal(300)));
    assert_eq!(xa_to_internal(xa_mk_internal(300)), 300);
    assert!(xa_is_node(xa_mk_node(7)));
    assert_eq!(xa_to_node(xa_mk_node(7)), 7);
    assert_eq!(nidx_ptr(node_ptr(9)), 9);
    assert!(xa_is_sibling(xa_mk_sibling(62)));
    /* boundary: offset-63 sibling tag is not classified as a sibling */
    assert!(!xa_is_sibling(xa_mk_sibling(63)));
    assert_eq!(xa_to_sibling(xa_mk_sibling(21)), 21);
    assert!(xa_is_retry(XA_RETRY_ENTRY));
    assert!(xa_is_zero(XA_ZERO_ENTRY));
    assert!(xa_is_advanced(XA_RETRY_ENTRY));
    assert!(xa_is_err(xa_error(-ENOMEM)));
    assert_eq!(xa_err(xa_error(-ENOMEM)), -ENOMEM);
    assert_eq!(xa_err(xa_mk_value(9)), 0);
    assert_eq!(xa_zero_to_null(XA_ZERO_ENTRY), 0);
    assert_eq!(xa_zero_to_null(E0), E0);
}

#[test]
fn store_load_erase() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    assert_eq!(xa.xa_load(0), 0);
    assert_eq!(xa.xa_store(0, E0), 0);
    assert_eq!(xa.xa_load(0), E0);
    assert_eq!(xa.xa_store(0, E1), E0); /* returns old */
    assert_eq!(xa.xa_load(0), E1);
    assert_eq!(xa.xa_erase(0), E1);
    assert_eq!(xa.xa_load(0), 0);
    assert_eq!(xa.xa_erase(0), 0);
}

#[test]
fn many_indexes() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    let idxs = [0u64, 1, 63, 64, 65, 255, 4095, 4096, 1 << 20, (1 << 26) + 7];
    for (i, &ix) in idxs.iter().enumerate() {
        assert_eq!(xa.xa_store(ix, E0 + 0x10 * i as u64), 0);
    }
    for (i, &ix) in idxs.iter().enumerate() {
        assert_eq!(xa.xa_load(ix), E0 + 0x10 * i as u64, "index {}", ix);
    }
    /* an unwritten index between written ones stays NULL */
    assert_eq!(xa.xa_load(66), 0);
    /* nodes were allocated for the deep indexes */
    assert!(xa.nodes.iter().any(|n| n.used));
}

#[test]
fn value_and_internal_entries() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    let v = xa_mk_value(0xdead);
    assert_eq!(xa.xa_store(10, v), 0);
    assert_eq!(xa.xa_load(10), v);
    /* XA_ZERO_ENTRY loads as NULL, like the kernel */
    assert_eq!(xa.xa_store(11, XA_ZERO_ENTRY), 0);
    assert_eq!(xa.xa_load(11), 0);
    assert_eq!(xa.xas_load(&mut Xas::new(11)), XA_ZERO_ENTRY);
}

#[test]
fn multi_index_order_store() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* order-4 store covering [0x40..0x50) */
    let mut xas = Xas::new_order(0x40, 4);
    assert_eq!(xa.xas_store(&mut xas, E2), 0);
    for i in 0x40..0x50u64 {
        assert_eq!(xa.xa_load(i), E2, "index {}", i);
    }
    assert_eq!(xa.xa_load(0x3f), 0);
    assert_eq!(xa.xa_load(0x50), 0);
    assert_eq!(xa.xa_get_order(0x40), 4);
    assert_eq!(xa.xa_get_order(0x44), 4);
    /* order-0 store over a sibling slot: the walk follows the sibling to
     * the master slot, so the whole range takes the new entry (faithful
     * xas_descend behaviour — siblings resolve to the master). */
    assert_eq!(xa.xa_store(0x44, E3), E2);
    for i in 0x40..0x50u64 {
        assert_eq!(xa.xa_load(i), E3, "index {}", i);
    }
}

#[test]
fn marks_set_find() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    xa.xa_store(5, E0);
    xa.xa_store(50, E1);
    xa.xa_store(500, E2);
    assert!(!xa.xa_get_mark(5, XA_MARK_0 as usize));
    xa.xa_set_mark(5, XA_MARK_0 as usize);
    xa.xa_set_mark(500, XA_MARK_0 as usize);
    assert!(xa.xa_get_mark(5, XA_MARK_0 as usize));
    assert!(!xa.xa_get_mark(50, XA_MARK_0 as usize));
    let mut idx = 0;
    let e = xa.xa_find(&mut idx, u64::MAX, XA_MARK_0);
    assert_eq!(e, E0);
    assert_eq!(idx, 5);
    let e = xa.xa_find_after(&mut idx, u64::MAX, XA_MARK_0);
    assert_eq!(e, E2);
    assert_eq!(idx, 500);
    xa.xa_clear_mark(5, XA_MARK_0 as usize);
    assert!(!xa.xa_get_mark(5, XA_MARK_0 as usize));
}

#[test]
fn find_and_find_after() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    for &ix in &[5u64, 100, 200, 999] {
        xa.xa_store(ix, E0 + ix);
    }
    let mut idx = 0;
    assert_eq!(xa.xa_find(&mut idx, u64::MAX, XA_PRESENT), E0 + 5);
    assert_eq!(idx, 5);
    let e = xa.xa_find_after(&mut idx, u64::MAX, XA_PRESENT);
    assert_eq!(e, E0 + 100);
    assert_eq!(idx, 100);
    /* bound `max` */
    let mut idx2 = 0;
    assert_eq!(xa.xa_find(&mut idx2, 150, XA_PRESENT), E0 + 5);
    idx2 = 5;
    assert_eq!(xa.xa_find_after(&mut idx2, 150, XA_PRESENT), E0 + 100);
    let e = xa.xa_find_after(&mut idx2, 150, XA_PRESENT);
    assert_eq!(e, 0);
}

#[test]
fn insert_reserve_release() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    assert_eq!(xa.xa_insert(7, E0), 0);
    assert_eq!(xa.xa_load(7), E0);
    assert_eq!(xa.xa_insert(7, E1), -EBUSY); /* occupied */
    assert_eq!(xa.xa_reserve(9), 0);
    assert_eq!(xa.xa_load(9), 0); /* zero entry reads as NULL */
    assert_eq!(xa.xa_insert(9, E2), -EBUSY);
    xa.xa_release(9);
    assert_eq!(xa.xa_insert(9, E2), 0);
    assert_eq!(xa.xa_load(9), E2);
}

#[test]
fn cmpxchg() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* NULL old: stores */
    assert_eq!(xa.xa_cmpxchg(3, 0, E0), 0);
    assert_eq!(xa.xa_load(3), E0);
    /* wrong old: returns current, no store */
    assert_eq!(xa.xa_cmpxchg(3, E1, E2), E0);
    assert_eq!(xa.xa_load(3), E0);
    /* matching old: swaps */
    assert_eq!(xa.xa_cmpxchg(3, E0, E2), E0);
    assert_eq!(xa.xa_load(3), E2);
}

#[test]
fn alloc_lowest_free() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, XA_FLAGS_ALLOC);
    let mut id = u32::MAX;
    assert_eq!(xa.xa_alloc(&mut id, E0, 0, 10), 0);
    assert_eq!(id, 0);
    assert_eq!(xa.xa_alloc(&mut id, E1, 0, 10), 0);
    assert_eq!(id, 1);
    /* min bound honoured */
    assert_eq!(xa.xa_alloc(&mut id, E2, 4, 10), 0);
    assert_eq!(id, 4);
    /* fill the rest of [4..10], then -EBUSY */
    for i in 5..=10u32 {
        assert_eq!(xa.xa_alloc(&mut id, E3, 5, 10), 0);
        assert_eq!(id, i);
    }
    assert_eq!(xa.xa_alloc(&mut id, E3, 4, 10), -EBUSY);
    /* alloc'd entries are loadable */
    assert_eq!(xa.xa_load(0), E0);
    /* allocated index 2,3 still free */
    assert_eq!(xa.xa_alloc(&mut id, E3, 2, 3), 0);
    assert_eq!(id, 2);
}

#[test]
fn alloc_cyclic_wraps() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, XA_FLAGS_ALLOC);
    let mut next = 0u32;
    let mut id = u32::MAX;
    assert_eq!(xa.xa_alloc_cyclic(&mut id, E0, 0, 3, &mut next), 0);
    assert_eq!(id, 0);
    assert_eq!(next, 1);
    xa.xa_alloc_cyclic(&mut id, E1, 0, 3, &mut next);
    xa.xa_alloc_cyclic(&mut id, E2, 0, 3, &mut next);
    xa.xa_alloc_cyclic(&mut id, E3, 0, 3, &mut next);
    assert_eq!(id, 3);
    /* range exhausted */
    assert_eq!(xa.xa_alloc_cyclic(&mut id, E3, 0, 3, &mut next), -EBUSY);
    /* free index 1, cyclic starts at next=4 -> wraps to 1 */
    xa.xa_erase(1);
    assert_eq!(xa.xa_alloc_cyclic(&mut id, E0, 0, 3, &mut next), 0);
    assert_eq!(id, 1);
}

#[test]
fn store_range_covers() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    xa.xa_store_range(10, 20, E0);
    assert_eq!(xa.xa_load(10), E0);
    assert_eq!(xa.xa_load(15), E0);
    assert_eq!(xa.xa_load(20), E0);
    assert_eq!(xa.xa_load(9), 0);
    assert_eq!(xa.xa_load(21), 0);
    /* internal entries rejected (values are fine, internals are not) */
    assert_eq!(xa.xa_store_range(30, 40, xa_mk_internal(5)), xa_error(-EINVAL));
}

#[test]
fn extract_entries() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    xa.xa_store(1, E0);
    xa.xa_store(3, E1);
    xa.xa_store(8, E2);
    let mut dst = [0u64; 8];
    let n = xa.xa_extract(&mut dst, 0, u64::MAX, 8, XA_PRESENT);
    assert_eq!(n, 3);
    assert_eq!(&dst[..3], &[E0, E1, E2]);
}

#[test]
fn destroy_frees_nodes() {
    let mut nodes = arena();
    {
        let mut xa = Xarray::new(&mut nodes, 0);
        xa.xa_store(1, E0);
        xa.xa_store(1000, E1);
        xa.xa_store(1 << 20, E2);
        assert!(xa.nodes.iter().any(|n| n.used));
        xa.xa_destroy();
        assert_eq!(xa.xa_head, 0);
    }
    assert!(nodes.iter().all(|n| !n.used));
}

#[test]
fn enomem_injection() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* fail every node allocation: the xas_nomem retry loop keeps asking,
     * so a single-bit mask only delays success — exhaust all 64 tries */
    xa.fail_mask = u64::MAX;
    let r = xa.xa_store(64, E0); /* index 64 needs a child node */
    assert_eq!(xa_err(r), -ENOMEM);
    /* recovery: clear mask, retry succeeds */
    xa.fail_mask = 0;
    assert_eq!(xa.xa_store(64, E0), 0);
    assert_eq!(xa.xa_load(64), E0);
}

#[test]
fn update_shadow_list() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    let mut xas = Xas::new(64);
    xas.update = true;
    assert_eq!(xa.xas_store(&mut xas, E0), 0);
    assert!(xa.nodes.iter().any(|n| n.on_list));
    /* erase removes the node from the shadow list on free */
    let mut xas = Xas::new(64);
    xas.update = true;
    xa.xas_store(&mut xas, 0);
    assert!(xa.nodes.iter().all(|n| !n.on_list));
}

#[test]
fn xas_retry_and_state() {
    let mut xas = Xas::new(0);
    assert!(xas_retry(&mut xas, XA_RETRY_ENTRY));
    assert_eq!(xas.node, XAS_RESTART);
    assert!(!xas_retry(&mut xas, E0));
    let mut xo = Xas::new_order(0x43, 3);
    assert_eq!(xo.index & !7, 0x40);
    assert_eq!(xo.sibs, 7);
    xas_set(&mut xo, 0x77);
    assert_eq!(xo.index, 0x77);
}

#[test]
fn exhausted_pool_returns_enomem() {
    /* a two-node arena can only hold a shallow tree */
    let mut nodes = vec![Node::new(); 2];
    let mut xa = Xarray::new(&mut nodes, 0);
    let mut last = 0u64;
    for i in 0..32u64 {
        let r = xa.xa_store(i * 64, E0 + i * 16);
        if xa_err(r) != 0 {
            assert_eq!(xa_err(r), -ENOMEM);
            last = r;
            break;
        }
    }
    assert_ne!(last, 0, "pool never exhausted");
    /* stored entries before exhaustion still load */
    assert_eq!(xa.xa_load(0), E0);
}

#[test]
fn extract_caps_at_dst_len() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    for i in 0..12u64 {
        xa.xa_store(i * 4, E0 + i * 16);
    }
    let mut dst = [0u64; 8];
    /* n > dst.len() is clamped: no OOB write, returns dst.len() */
    let n = xa.xa_extract(&mut dst, 0, u64::MAX, 12, XA_PRESENT);
    assert_eq!(n, 8);
    for i in 0..8u64 {
        assert_eq!(dst[i as usize], E0 + i * 16);
    }
}

#[test]
fn node_class_entry_warns() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    let w0 = xa.warn;
    /* storing a node-pointer-class entry is a contract violation */
    xa.xa_store(5, xa_mk_node(3));
    assert_eq!(xa.warn, w0 + 1);
}

#[test]
fn split_without_alloc_warns_not_panics() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* deep store so xa_head holds a real (shift > 0) root node */
    xa.xa_store(1 << 16, E0);
    assert!(xa_is_node(xa.xa_head));
    let ni = xa_to_node(xa.xa_head);
    /* xas_split without xas_split_alloc: contract violation, warn+return */
    let mut xas = Xas::new(0);
    xas.node = node_ptr(ni);
    xas.offset = 0;
    xas.shift = 0;
    let w0 = xa.warn;
    xa.xas_split(&mut xas, E1, 9);
    assert!(xa.warn >= w0 + 1);
}

#[test]
fn sibling_check_wrapping_shift() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* deep store so xa_head holds a real root node, not a plain entry */
    xa.xa_store(1 << 16, E0);
    assert!(xa_is_node(xa.xa_head));
    let ni = xa_to_node(xa.xa_head);
    /* caller-corrupted shift >= 60: wraps like C, no underflow panic */
    xa.nodes[ni].shift = 60;
    let mut xas = Xas::new(0);
    xas.node = node_ptr(ni);
    let _ = xa.xas_sibling_check(&xas);
    xa.nodes[ni].shift = 0;
}

#[test]
fn store_range_max_no_hang() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    /* [0, u64::MAX] makes xas_size wrap to 0 (upstream livelock);
     * bounded model must break out instead */
    xa.xa_store_range(0, u64::MAX, E0);
    assert_eq!(xa.xa_load(0), E0);
}

#[test]
fn extract_zero_n() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    xa.xa_store(1, E0);
    let mut dst = [0u64; 8];
    /* direct xas_extract_* with n=0 must not write dst */
    let mut xas = Xas::new(0);
    assert_eq!(xa.xas_extract_present(&mut xas, &mut dst, u64::MAX, 0), 0);
    assert_eq!(dst[0], 0);
    let mut xas = Xas::new(0);
    assert_eq!(xa.xas_extract_marked(&mut xas, &mut dst, u64::MAX, 0, 0), 0);
}

#[test]
fn xas_reload_fresh_state() {
    let mut nodes = arena();
    let mut xa = Xarray::new(&mut nodes, 0);
    xa.xa_store(1, E0);
    /* fresh/reset cursors hold sentinel node values, not a node ptr */
    let mut xas = Xas::new(1);
    assert_eq!(xa.xas_reload(&mut xas), 0);
    let mut xas2 = Xas::new(1);
    xa.xas_set_offset(&mut xas2);
    xa.xas_move_index(&mut xas2, 3);
}
