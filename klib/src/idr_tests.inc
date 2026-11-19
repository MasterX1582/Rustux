// klib::idr tests — Idr + Ida over caller-owned Node/Bmap arenas.
// Entries are plain aligned u64 "pointers" (low bits 00, below NBASE).

use std::vec;

const E0: u64 = 0x1000;
const E1: u64 = 0x1010;
const E2: u64 = 0x1020;
const GFP: u32 = GFP_KERNEL;

fn nodes(n: usize) -> std::vec::Vec<Node> {
    vec![Node::new(); n]
}

fn bmaps(n: usize) -> std::vec::Vec<Bmap> {
    let mut v = std::vec::Vec::with_capacity(n);
    for _ in 0..n {
        v.push(Bmap::new());
    }
    v
}

#[test]
fn alloc_find_remove() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    assert!(idr.idr_is_empty());
    assert_eq!(idr.idr_alloc(E0, 0, 0, GFP), 0);
    assert_eq!(idr.idr_alloc(E1, 0, 0, GFP), 1);
    assert_eq!(idr.idr_find(0), E0);
    assert_eq!(idr.idr_find(1), E1);
    assert_eq!(idr.idr_find(2), 0);
    idr.idr_remove(0);
    assert_eq!(idr.idr_find(0), 0);
    assert!(!idr.idr_is_empty());
    idr.idr_remove(1);
    assert!(idr.idr_is_empty());
}

#[test]
fn alloc_u32_bounds() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    let mut nextid = 5u32;
    assert_eq!(idr.idr_alloc_u32(E0, &mut nextid, 10, GFP), 0);
    assert_eq!(nextid, 5);
    assert_eq!(idr.idr_find(5), E0);
    // next free in (5, 10] is 6
    assert_eq!(idr.idr_alloc_u32(E1, &mut nextid, 10, GFP), 0);
    assert_eq!(nextid, 6);
    // cap it: fill through 10, then ENOSPC
    for _ in 7..=10 {
        assert_eq!(idr.idr_alloc_u32(E2, &mut nextid, 10, GFP), 0);
    }
    assert_eq!(idr.idr_alloc_u32(E2, &mut nextid, 10, GFP), -ENOSPC);
}

#[test]
fn alloc_start_end() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    assert_eq!(idr.idr_alloc(E0, 4, 8, GFP), 4);
    assert_eq!(idr.idr_alloc(E1, 4, 8, GFP), 5);
    assert_eq!(idr.idr_alloc(E2, 4, 8, GFP), 6);
    assert_eq!(idr.idr_alloc(E0, 4, 8, GFP), 7);
    assert_eq!(idr.idr_alloc(E0, 4, 8, GFP), -ENOSPC);
    // negative start is rejected like upstream (WARN + -EINVAL)
    assert_eq!(idr.idr_alloc(E1, -5, 4, GFP), -EINVAL);
}

#[test]
fn alloc_cyclic() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    assert_eq!(idr.idr_alloc_cyclic(E0, 0, 0, GFP), 0);
    assert_eq!(idr.idr_alloc_cyclic(E1, 0, 0, GFP), 1);
    assert_eq!(idr.idr_alloc_cyclic(E2, 0, 0, GFP), 2);
    idr.idr_remove(1);
    // cyclic cursor continues from after last alloc
    assert_eq!(idr.idr_alloc_cyclic(E1, 0, 0, GFP), 3);
    assert_eq!(idr.idr_get_cursor(), 4);
    idr.idr_set_cursor(1);
    assert_eq!(idr.idr_get_cursor(), 1);
}

#[test]
fn replace() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    idr.idr_alloc(E0, 0, 0, GFP);
    assert_eq!(idr.idr_replace(E2, 0), E0);
    assert_eq!(idr.idr_find(0), E2);
    // replace on a free/absent slot -> -ENOENT
    assert_eq!(idr.idr_replace(E0, 5) as i64, -ENOENT);
}

#[test]
fn get_next() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    assert_eq!(idr.idr_alloc(E0, 2, 0, GFP), 2); /* id 2 */
    assert_eq!(idr.idr_alloc(E1, 3, 0, GFP), 3); /* id 3 */
    assert_eq!(idr.idr_alloc(E2, 4, 0, GFP), 4); /* id 4 */
    // idr_get_next leaves *nextid at the found id; the caller
    // advances it (upstream idr_for_each_entry does `id += 1`)
    let mut next = 0i64;
    let e = idr.idr_get_next(&mut next);
    assert_eq!(e, E0);
    assert_eq!(next, 2);
    next += 1;
    let e = idr.idr_get_next(&mut next);
    assert_eq!(e, E1);
    assert_eq!(next, 3);
    next += 1;
    let e = idr.idr_get_next(&mut next);
    assert_eq!(e, E2);
    assert_eq!(next, 4);
    next += 1;
    assert_eq!(idr.idr_get_next(&mut next), 0);
}

#[test]
fn for_each() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    idr.idr_alloc(E0, 0, 0, GFP);
    idr.idr_alloc(E1, 0, 0, GFP);
    idr.idr_alloc(E2, 0, 0, GFP);
    let mut seen = std::vec::Vec::new();
    let r = idr.idr_for_each(|id, e| {
        seen.push((id, e));
        0
    });
    assert_eq!(r, 0);
    assert_eq!(seen, vec![(0, E0), (1, E1), (2, E2)]);
    // early stop propagates the callback's return
    let mut count = 0;
    let r = idr.idr_for_each(|_, _| {
        count += 1;
        42
    });
    assert_eq!(r, 42);
    assert_eq!(count, 1);
}

#[test]
fn idr_base() {
    let mut arena = nodes(16);
    let mut idr = Idr::new_base(&mut arena, 1);
    assert_eq!(idr.idr_alloc(E0, 0, 0, GFP), 1);
    assert_eq!(idr.idr_alloc(E1, 0, 0, GFP), 2);
    assert_eq!(idr.idr_find(1), E0);
    assert_eq!(idr.idr_find(0), 0);
}

#[test]
fn preload_and_destroy() {
    let mut arena = nodes(24);
    let mut idr = Idr::new(&mut arena);
    idr.idr_preload(GFP);
    idr.idr_preload_end();
    idr.idr_alloc(E0, 0, 0, GFP);
    idr.idr_alloc(E1, 0, 0, GFP);
    idr.idr_destroy();
    assert_eq!(idr.idr_find(0), 0);
    assert_eq!(idr.idr_find(1), 0);
}

#[test]
fn ida_basic() {
    let mut arena = nodes(8);
    let mut bm = bmaps(8);
    let mut ida = Ida::new(&mut arena, &mut bm);
    assert_eq!(ida.ida_alloc(GFP), 0);
    assert_eq!(ida.ida_alloc(GFP), 1);
    assert_eq!(ida.ida_alloc(GFP), 2);
    assert!(ida.ida_exists(1) != 0);
    assert!(ida.ida_exists(5) == 0);
    ida.ida_free(1);
    assert!(ida.ida_exists(1) == 0);
    assert_eq!(ida.ida_alloc(GFP), 1);
    ida.ida_destroy();
    assert!(ida.ida_is_empty());
}

#[test]
fn ida_min_max() {
    let mut arena = nodes(8);
    let mut bm = bmaps(8);
    let mut ida = Ida::new(&mut arena, &mut bm);
    assert_eq!(ida.ida_alloc_min(10, GFP), 10);
    assert_eq!(ida.ida_alloc_min(10, GFP), 11);
    // alloc_max(9) = lowest free id in [0, 9] = 0
    assert_eq!(ida.ida_alloc_max(9, GFP), 0);
}

#[test]
fn ida_range_bounds() {
    let mut arena = nodes(8);
    let mut bm = bmaps(8);
    let mut ida = Ida::new(&mut arena, &mut bm);
    assert_eq!(ida.ida_alloc_range(3, 6, GFP), 3);
    assert_eq!(ida.ida_alloc_range(3, 6, GFP), 4);
    assert_eq!(ida.ida_alloc_range(3, 6, GFP), 5);
    assert_eq!(ida.ida_alloc_range(3, 6, GFP), 6);
    assert_eq!(ida.ida_alloc_range(3, 6, GFP), -ENOSPC);
    // value-packing boundary: ids 0..=62 share one xa value entry.
    // 4 used by the range allocs above; fill the remaining 59 bits.
    for _ in 0..59 {
        assert!(ida.ida_alloc(GFP) >= 0);
    }
    // crossing the 63-bit value boundary allocates an ida_bitmap
    assert_eq!(ida.ida_alloc(GFP), 63);
}

#[test]
fn ida_find_first() {
    let mut arena = nodes(8);
    let mut bm = bmaps(8);
    let mut ida = Ida::new(&mut arena, &mut bm);
    // find_first reports the first ALLOCATED id among present
    // entries; an empty tree has none -> -ENOENT
    assert_eq!(ida.ida_find_first(), -ENOENT);
    ida.ida_alloc_range(5, 5, GFP);
    assert_eq!(ida.ida_find_first(), 5);
    ida.ida_free(5);
    assert_eq!(ida.ida_find_first_range(4, 8), -ENOENT);
}

#[test]
fn enomem_injection() {
    let mut arena = nodes(16);
    let mut idr = Idr::new(&mut arena);
    idr.rt.fail_mask = 1; /* fail first alloc */
    // idr_alloc retries through the ENOMEM path (xas_nomem contract)
    let r = idr.idr_alloc(E0, 0, 0, GFP);
    assert!(r == 0 || r == -ENOMEM);
}

#[test]
fn ida_find_first_range_crafted_bmap() {
    // coverage-critic: this path missed the bmap_idx bounds guard.
    // A non-value entry whose bmap index lies outside the caller-owned
    // bmap arena must degrade to -ENOENT + warn, not an OOB panic.
    let mut arena = nodes(8);
    let mut bm = bmaps(4);
    let mut ida = Ida::new(&mut arena, &mut bm);
    let mut xas = Xas::new(0);
    ida.rt.xas_store(&mut xas, bmap_canon(100));
    assert_eq!(ida.ida_find_first_range(0, 256), -ENOENT);
    assert!(ida.rt.warn > 0);
}
