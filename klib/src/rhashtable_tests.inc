// klib::rhashtable tests — tobj object model (48-byte slots at POOLV+i*48,
// rhash_head/rhlist_head at +8, u64 key at +24), matching the verified
// C harness layout.

use std::vec;

const O_HEAD: u64 = 8; /* offsetof(tobj, rhl/rhead) */
const O_KEY: u64 = 24; /* offsetof(tobj, key)      */

const NOBJ: usize = 256;
const OBJ_CELLS: usize = NOBJ * TOBJ_SZ as usize;
const MEM_CELLS: usize = OBJ_CELLS + (1 << 22); /* 4M-cell arena + iter */

fn params() -> Params {
    Params {
        key_len: 8,
        key_offset: O_KEY as u16,
        head_offset: O_HEAD as u16,
        ..Default::default()
    }
}

fn new_table(mem: &mut [u64]) -> Rhashtable<'_> {
    let mut t = Rhashtable::new(mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&params(), false), 0);
    t
}

/// Store `key` into slot `i` and return the rhash_head address callers
/// pass to the insert API (`&obj->rhead` in C).
fn mk_obj(t: &mut Rhashtable, i: usize, key: u64) -> u64 {
    let o = Rhashtable::obj(i);
    t.mem_w(o + O_KEY, key);
    o + O_HEAD
}

/// A canonical cell in object space holding `key`, for lookup calls.
fn key_cell(_t: &Rhashtable, i: usize) -> u64 {
    Rhashtable::obj(i) + O_KEY
}

/// Drive the deferred workqueue/irq_work model to quiescence.
fn settle(t: &mut Rhashtable) {
    let mut guard = 0;
    while t.work_pending() && guard < 64 {
        t.drain_work();
        guard += 1;
    }
}

#[test]
fn init_validates_params() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    /* key_len == 0 and no obj_hashfn -> -EINVAL */
    assert_eq!(t.rhashtable_init(&Params::default(), false), -22);
    /* obj_hashfn set without obj_cmpfn -> -EINVAL */
    let p = Params {
        key_len: 8,
        key_offset: O_KEY as u16,
        head_offset: O_HEAD as u16,
        obj_hashfn: Some(ObjHash::Good),
        ..Default::default()
    };
    assert_eq!(t.rhashtable_init(&p, false), -22);
    /* obj_hashfn + obj_cmpfn -> ok */
    let p = Params {
        obj_cmpfn: Some(ObjCmp::Eq),
        ..p
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
}

#[test]
fn init_applies_defaults() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&params(), false), 0);
    /* default table is HASH_DEFAULT_SIZE = 64 */
    assert_eq!(t.table_size(), 64);
    /* key_len 8 divisible by 4 -> jhash2 over 2 words */
    assert_eq!(t.ht.key_len, 2);
    assert!(t.ht.p.hashfn == Some(HashFn::Jhash2));
}

#[test]
fn insert_lookup_remove_roundtrip() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..8 {
        let he = mk_obj(&mut t, i, (i as u64 + 1) * 0x111);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    assert_eq!(t.nelems(), 8);
    for i in 0..8 {
        assert_eq!(t.rhashtable_lookup(key_cell(&t, i)), Rhashtable::obj(i));
    }
    /* missing key -> 0 */
    t.mem_w(key_cell(&t, 200), 0xdead);
    assert_eq!(t.rhashtable_lookup(key_cell(&t, 200)), 0);
    /* remove each object; lookup then misses */
    for i in 0..8 {
        let he = Rhashtable::obj(i) + O_HEAD;
        assert_eq!(t.rhashtable_remove_fast(he), 0);
        assert_eq!(t.rhashtable_lookup(key_cell(&t, i)), 0);
    }
    assert_eq!(t.nelems(), 0);
}

#[test]
fn insert_fast_no_dedup_lookup_insert_dedups() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let a = mk_obj(&mut t, 0, 7);
    let b = mk_obj(&mut t, 1, 7);
    /* insert_fast passes NULL key -> no dedup, both land */
    assert_eq!(t.rhashtable_insert_fast(a), 0);
    assert_eq!(t.rhashtable_insert_fast(b), 0);
    assert_eq!(t.nelems(), 2);
    /* lookup_insert_fast dedups against the existing chain */
    let c = mk_obj(&mut t, 2, 7);
    assert_eq!(t.rhashtable_lookup_insert_fast(c), -17 /* -EEXIST */);
    assert_eq!(t.nelems(), 2);
    /* a new key still inserts */
    let d = mk_obj(&mut t, 3, 8);
    assert_eq!(t.rhashtable_lookup_insert_fast(d), 0);
}

#[test]
fn lookup_get_insert_returns_existing() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let a = mk_obj(&mut t, 0, 42);
    assert_eq!(t.rhashtable_insert_fast(a), 0);
    /* duplicate key: get_insert returns the existing rhash_head, not err */
    let b = mk_obj(&mut t, 1, 42);
    let ret = t.rhashtable_lookup_get_insert_fast(b);
    assert_eq!(ret, Rhashtable::obj(0));
    assert_eq!(t.nelems(), 1);
}

#[test]
fn replace_fast_swaps_object() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let old = mk_obj(&mut t, 0, 9);
    let new = mk_obj(&mut t, 1, 9);
    assert_eq!(t.rhashtable_insert_fast(old), 0);
    assert_eq!(t.rhashtable_replace_fast(old, new), 0);
    assert_eq!(t.rhashtable_lookup(key_cell(&t, 1)), Rhashtable::obj(1));
    /* different keys -> -EINVAL like C */
    let wrong_key = mk_obj(&mut t, 2, 77);
    assert_eq!(t.rhashtable_replace_fast(wrong_key, new), -22);
    /* same key but obj_old never inserted -> -ENOENT */
    let missing = mk_obj(&mut t, 3, 9);
    let new2 = mk_obj(&mut t, 4, 9);
    assert_eq!(t.rhashtable_replace_fast(missing, new2), -2);
}

#[test]
fn grow_rehash_preserves_lookups() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..96 {
        let he = mk_obj(&mut t, i, i as u64 + 1000);
        assert_eq!(t.rhashtable_insert_fast(he), 0, "insert {i}");
    }
    settle(&mut t);
    assert!(t.table_size() >= 128, "size {}", t.table_size());
    assert!(!t.rehash_in_progress());
    for i in 0..96 {
        assert_eq!(t.rhashtable_lookup(key_cell(&t, i)), Rhashtable::obj(i));
    }
}

#[test]
fn automatic_shrinking() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    let p = Params {
        min_size: 4,
        automatic_shrinking: true,
        ..params()
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
    for i in 0..64 {
        let he = mk_obj(&mut t, i, i as u64 + 5000);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    settle(&mut t);
    let big = t.table_size();
    assert!(big >= 64);
    for i in 0..64 {
        assert_eq!(t.rhashtable_remove_fast(Rhashtable::obj(i) + O_HEAD), 0);
    }
    settle(&mut t);
    assert_eq!(t.table_size(), 4, "shrunk to min_size");
}

#[test]
fn max_size_caps_growth_e2big() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    let p = Params {
        max_size: 8,
        insecure_elasticity: true, /* keep chains long rather than ENOMEM */
        ..params()
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
    /* max_elems = max_size*2 = 16: the 17th insert hits -E2BIG */
    let mut big = false;
    for i in 0..24 {
        let he = mk_obj(&mut t, i, i as u64);
        let r = t.rhashtable_insert_fast(he);
        if r == -7 {
            big = true;
            break;
        }
        assert_eq!(r, 0);
    }
    assert!(big, "expected -E2BIG once table is full");
}

#[test]
fn rhlist_chains_duplicates() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&params(), true), 0);
    let a = mk_obj(&mut t, 0, 5);
    let b = mk_obj(&mut t, 1, 5);
    let c = mk_obj(&mut t, 2, 6);
    /* rhltable_insert allows dup keys: chains them via rhl.next */
    assert_eq!(t.rhltable_insert(a), 0);
    assert_eq!(t.rhltable_insert(b), 0);
    assert_eq!(t.rhltable_insert(c), 0);
    assert_eq!(t.nelems(), 2); /* one chain head per key */
    /* lookup returns the chain head's rhlist_head (= last-inserted) */
    let head = t.rhltable_lookup(key_cell(&t, 0));
    assert_eq!(head, b);
    /* follow rhl.next (+8) to the older member */
    assert_eq!(t.mem_r(head + 8), a);
    assert_eq!(t.mem_r(a + 8), 0);
    /* removing the head promotes the next member */
    assert_eq!(t.rhltable_remove(b), 0);
    assert_eq!(t.rhltable_lookup(key_cell(&t, 0)), a);
    assert_eq!(t.rhltable_remove(a), 0);
    assert_eq!(t.rhltable_lookup(key_cell(&t, 0)), 0);
}

#[test]
fn walk_enumerates_all() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let n = 40;
    for i in 0..n {
        let he = mk_obj(&mut t, i, i as u64 + 9000);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    settle(&mut t);
    let mut seen = [false; NOBJ];
    let mut count = 0;
    t.walk_enter(0);
    t.walk_start(0);
    loop {
        let r = t.walk_next(0);
        if r == 0 {
            break;
        }
        if is_err(r) {
            /* -EAGAIN: rehash boundary crossed, keep walking */
            continue;
        }
        let i = t.tidx(r);
        assert_ne!(i, NILX, "walked non-object {r:#x}");
        assert!(!seen[i as usize], "object {i} walked twice");
        seen[i as usize] = true;
        count += 1;
    }
    t.walk_stop(0);
    t.walk_exit(0);
    assert_eq!(count, n);
}

#[test]
fn walk_peek_returns_next() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..4 {
        let he = mk_obj(&mut t, i, i as u64 + 1);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    t.walk_enter(0);
    t.walk_start(0);
    let peeked = t.walk_peek(0);
    let peeked2 = t.walk_peek(0);
    assert_eq!(peeked, peeked2, "peek does not advance the cursor");
    assert_ne!(peeked, 0);
    t.walk_stop(0);
    t.walk_exit(0);
}

#[test]
fn next_key_iterates() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..6 {
        let he = mk_obj(&mut t, i, i as u64 + 42);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    settle(&mut t);
    let mut found = [false; NOBJ];
    let mut prev = 0u64;
    let mut count = 0;
    loop {
        let he = t.rhashtable_next_key(prev);
        if he == 0 || is_err(he) {
            break;
        }
        let i = t.tidx(he);
        assert_ne!(i, NILX);
        found[i as usize] = true;
        prev = Rhashtable::obj(i as usize) + O_KEY;
        count += 1;
        if count > NOBJ {
            panic!("next_key looped");
        }
    }
    assert_eq!(count, 6);
}

#[test]
fn fail_nth_fallback_min_table() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    t.fail_nth = 1; /* first kalloc fails; NOFAIL fallback succeeds */
    let p = Params {
        nelem_hint: 512,
        ..params()
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
    /* fell back to the min-size table instead of the hinted size */
    assert_eq!(t.table_size(), 4);
}

#[test]
fn free_and_destroy_visits_all() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..5 {
        let he = mk_obj(&mut t, i, i as u64 + 1);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    settle(&mut t);
    let mut count = 0u64;
    let mut or_keys = 0u64;
    t.free_and_destroy(|t, o| {
        count += 1;
        or_keys |= t.mem_r(o + O_KEY);
    });
    assert_eq!(count, 5);
    assert_eq!(or_keys, 1 | 2 | 3 | 4 | 5);
}

#[test]
fn destroy_without_freefn() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    for i in 0..3 {
        let he = mk_obj(&mut t, i, i as u64 + 1);
        assert_eq!(t.rhashtable_insert_fast(he), 0);
    }
    t.destroy(); /* must not panic, no callback arm taken */
}

#[test]
fn obj_hashfn_must_agree_with_key_hashfn() {
    /* Kernel contract: when obj_hashfn disagrees with the key hashfn,
     * insert/remove (obj_hashfn) still work but rhashtable_lookup
     * (key_hashfn) legitimately misses — the objects land in buckets
     * the key hash never probes. */
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    let p = Params {
        obj_hashfn: Some(ObjHash::Bad), /* key & 0xf != jhash bucket */
        obj_cmpfn: Some(ObjCmp::Eq),
        ..params()
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
    for i in 0..32 {
        let he = mk_obj(&mut t, i, i as u64);
        assert_eq!(t.rhashtable_insert_fast(he), 0, "insert {i}");
    }
    assert_eq!(t.nelems(), 32);
    /* documented miss: key hashfn probes different buckets */
    assert_eq!(t.rhashtable_lookup(key_cell(&t, 0)), 0);
    /* removal hashes the object consistently -> still works */
    for i in 0..32 {
        assert_eq!(t.rhashtable_remove_fast(Rhashtable::obj(i) + O_HEAD), 0);
    }
    assert_eq!(t.nelems(), 0);
}

#[test]
fn constant_obj_hashfn_reports_ebusy() {
    /* A pathological constant obj_hashfn fills one chain past
     * RHT_ELASTICITY; once a rehash is attached, slow-path inserts
     * legitimately surface -EBUSY (the C model does the same — the
     * corpus recorded these transient failures verbatim). */
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    let p = Params {
        min_size: 4,
        max_size: 2048,
        obj_hashfn: Some(ObjHash::Bad),
        obj_cmpfn: Some(ObjCmp::Eq),
        ..params()
    };
    assert_eq!(t.rhashtable_init(&p, false), 0);
    let mut busy = false;
    for i in 0..96 {
        let he = mk_obj(&mut t, i, (i * 16) as u64); /* all keys &0xf == 0 */
        match t.rhashtable_insert_fast(he) {
            0 => {}
            -16 => {
                /* -EBUSY: rehash machinery busy; retry after drain */
                busy = true;
                settle(&mut t);
                let r = t.rhashtable_insert_fast(he);
                assert!(r == 0 || r == -16, "insert {i}: {r}");
            }
            r => panic!("insert {i}: {r}"),
        }
    }
    settle(&mut t);
    assert!(busy, "expected at least one -EBUSY under a constant hash");
    /* nelems counts landed objects regardless of the lookup contract */
    assert!(t.nelems() > 16);
}

#[test]
fn nested_table_via_alloc_failure_fallback() {
    /* Nested bucket_tables engage only when a non-blocking flat alloc
     * fails with nbuckets >= 1024 — the corpus covered this through
     * fail_i injection into the GFP_ATOMIC rehash path; sweep the same
     * knob here until a large-table alloc is the one that fails. */
    let mut saw_nested = false;
    'outer: for f in 1..16u64 {
        const AOBJ: usize = 1100;
        let mut mem = vec![0u64; AOBJ * TOBJ_SZ as usize + (1 << 19)];
        let mut t = Rhashtable::new(&mut mem, AOBJ * TOBJ_SZ as usize);
        t.fail_nth = f;
        let p = Params {
            min_size: 4,
            max_size: 4096,
            ..params()
        };
        assert_eq!(t.rhashtable_init(&p, false), 0);
        for i in 0..AOBJ {
            let he = mk_obj(&mut t, i, i as u64 + 1);
            match t.rhashtable_insert_fast(he) {
                0 => {}
                -16 | -12 => {
                    /* -EBUSY/-ENOMEM while rehash machinery struggles:
                     * drain the deferred retry, then retry the insert */
                    settle(&mut t);
                    let r2 = t.rhashtable_insert_fast(he);
                    if r2 != 0 && r2 != -16 && r2 != -12 {
                        panic!("insert {i}: {r2}");
                    }
                    if r2 != 0 {
                        continue;
                    }
                }
                r => panic!("insert {i}: {r}"),
            }
            /* once a nested table attaches and the rehash drains it
             * becomes ht.tbl — check after every insert AND at settle */
            let mut tbl = t.ht.tbl;
            for _ in 0..8 {
                if tbl == 0 {
                    break;
                }
                if t.tbl_nest(tbl) != 0 {
                    saw_nested = true;
                    break 'outer;
                }
                tbl = t.tbl_future(tbl);
            }
        }
        settle(&mut t);
        if t.tbl_nest(t.ht.tbl) != 0 {
            saw_nested = true;
            break;
        }
    }
    assert!(saw_nested, "no alloc ordinal produced a nested table");
}

#[test]
fn init_enomem_on_exhausted_arena() {
    /* arena too small for even the min table: init must fail, and the
     * API must degrade gracefully (no panics, no phantom inserts). */
    /* obj cells + iter cells are mandatory; arena tail < min table */
    let mut mem = vec![0u64; OBJ_CELLS + 64 + 8];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&params(), false), -12);
    assert_eq!(t.rhashtable_lookup(key_cell(&t, 0)), 0);
    let o = mk_obj(&mut t, 0, 7);
    assert_eq!(t.rhashtable_insert_fast(o), -22);
    assert_eq!(t.rhashtable_remove_fast(o), -2);
    let o2 = mk_obj(&mut t, 1, 8);
    assert_eq!(t.rhashtable_replace_fast(o, o2), -2);
    assert_eq!(t.rhashtable_next_key(0), 0);
    t.deferred_worker(); /* no-op, must not panic */
    t.destroy();
}

#[test]
fn ops_on_uninitialized_table_degrade_gracefully() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    let o = Rhashtable::obj(0) + O_HEAD;
    t.mem_w(Rhashtable::obj(0) + O_KEY, 9);
    assert_eq!(t.rhashtable_lookup(key_cell(&t, 0)), 0);
    assert_eq!(t.rhashtable_insert_fast(o), -22);
    assert_eq!(t.nelems(), 0); /* no phantom insert */
}

#[test]
#[should_panic(expected = "walk slot out of range")]
fn walk_slot_out_of_range_panics() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    t.walk_enter(NI); /* only slots 0..NI exist */
}

#[test]
fn rehash_alloc_rejects_oversized_table() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    assert_eq!(t.rehash_alloc(t.ht.tbl, 1u64 << 61), -12);
    /* failed alloc must not burn arena: a normal grow still works */
    assert_eq!(t.rehash_alloc(t.ht.tbl, 128), 0);
    t.destroy();
}

#[test]
fn corrupt_chain_next_terminates_walks() {
    /* scribble a NULL next into a chain — C would NULL-deref, the model
     * must terminate rather than hang. */
    let mut mem = vec![0u64; MEM_CELLS];
    let p = Params {
        obj_hashfn: Some(ObjHash::Bad),
        obj_cmpfn: Some(ObjCmp::Eq),
        ..params()
    };
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&p, false), 0);
    for i in 0..4 {
        let o = mk_obj(&mut t, i, i as u64); /* Bad hash: all same bucket */
        let r = t.rhashtable_insert_fast(o);
        assert!(r == 0 || r == -11, "insert {i}: {r}");
    }
    settle(&mut t);
    t.mem_w(Rhashtable::obj(1) + O_HEAD, 0); /* corrupt obj1->next = NULL */
    let kc = key_cell(&t, 0);
    let _ = t.rhashtable_lookup(kc); /* must terminate */
    let _ = t.rhashtable_next_key(0);
    t.destroy();
}

#[test]
fn walk_terminates_on_corrupt_chain() {
    /* N1: a NULL-terminated (corrupt) chain must not yield garbage or
     * livelock the walker. */
    let mut mem = vec![0u64; MEM_CELLS];
    let p = Params {
        obj_hashfn: Some(ObjHash::Bad),
        obj_cmpfn: Some(ObjCmp::Eq),
        ..params()
    };
    let mut t = Rhashtable::new(&mut mem, OBJ_CELLS);
    assert_eq!(t.rhashtable_init(&p, false), 0);
    for i in 0..4 {
        let o = mk_obj(&mut t, i, i as u64);
        let r = t.rhashtable_insert_fast(o);
        assert!(r == 0 || r == -11, "insert {i}: {r}");
    }
    settle(&mut t);
    t.mem_w(Rhashtable::obj(0) + O_HEAD, 0); /* corrupt obj0->next = NULL */
    t.walk_enter(0);
    t.walk_start(0);
    let mut n = 0;
    for _ in 0..64 {
        let obj = t.walk_next(0);
        if obj == 0 || obj == u64::MAX - 9 {
            break; /* end or error ptr */
        }
        assert!(Rhashtable::tidx(&t, obj) != NILX, "walk yielded garbage {obj:#x}");
        n += 1;
    }
    t.walk_stop(0);
    t.walk_exit(0);
    t.destroy();
}

#[test]
#[should_panic(expected = "cyclic chain")]
fn cyclic_chain_panics_not_hangs() {
    /* N2: inserting the same rhash_head twice self-cycles the bucket;
     * a miss lookup must panic (bounded), not hang. */
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let o = mk_obj(&mut t, 0, 1);
    assert_eq!(t.rhashtable_insert_fast(o), 0);
    let _ = t.rhashtable_insert_fast(o); /* same head again -> cycle */
    let kc = key_cell(&t, 1);
    let _ = t.rhashtable_lookup(kc); /* key 999? need a miss */
}

#[test]
fn rehash_alloc_zero_size_rejected() {
    /* N3: a size-0 future table wedges the slow path (-EAGAIN loop);
     * reject it outright. */
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    let o = mk_obj(&mut t, 0, 5);
    assert_eq!(t.rhashtable_insert_fast(o), 0);
    assert_eq!(t.rehash_alloc(t.ht.tbl, 0), -22);
    let o2 = mk_obj(&mut t, 1, 6);
    assert_eq!(t.rhashtable_insert_fast(o2), 0);
    t.destroy();
}

#[test]
fn rehash_alloc_rejects_null_old_tbl() {
    let mut mem = vec![0u64; MEM_CELLS];
    let mut t = new_table(&mut mem);
    assert_eq!(t.rehash_alloc(0, 64), -22);
    t.destroy();
}
