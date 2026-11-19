//! Wrappers.c layer of the lib-maple-tree port: the canonical model
//! (`Pool`, `Mas`, all `mas_*`/`mtree_*` semantics) lives in the
//! `maple-model` crate; this file ports the wrapper op loops
//! (`gen_*`, `op_*`, `dump_tree`, `fold_mas`) and the `wrap_mt_*` drivers.

#![forbid(unsafe_code)]
#![allow(clippy::all)]
#![allow(dead_code)]

use maple_model::*;

/* ================= wrappers.c translation ================= */

/* efold is the identity on canonical values (raw values can't reach the
 * canonical bases: generated indices/pivots stay < 2^46 or near
 * ULONG_MAX). */
fn efold(e: u64) -> u64 {
    e
}
fn ncanon(c: u64) -> u64 {
    if c == 0 { 0 } else { c & !0xff }
}
fn scanon(c: u64) -> u64 {
    if c == 0 { 0 } else { c & !0xff }
}

fn gen_idx(r: u64) -> u64 {
    match r % 16 {
        0 | 1 | 2 => (r >> 8) % 64,
        3 | 4 => (r >> 8) % 1024,
        5 => 60 + (r >> 8) % 9,
        6 => (r >> 8) % 65536,
        7 => 4090 + (r >> 8) % 12,
        8 => ((r >> 8) % (1 << 16)) << 4,
        9 => (r >> 8) % (1 << 20),
        10 => ((r >> 8) % (1 << 8)) << 12,
        11 => ((r >> 8) % (1 << 12)) << 16,
        12 => ((r >> 8) % (1 << 10)) << 24,
        13 => (1u64 << 32) + (r >> 8) % (1 << 20),
        14 => ULONG_MAX - (r >> 8) % 2048,
        _ => (ULONG_MAX >> 1) + (r >> 8) % 256,
    }
}

fn gen_range(r: u64) -> (u64, u64) {
    let s = gen_idx(r);
    let mut len = match (r >> 3) % 12 {
        0 | 1 | 2 => (r >> 11) % 4,
        3 | 4 => (r >> 11) % 20,
        5 => 14 + (r >> 11) % 4,
        6 | 7 => (r >> 11) % 200,
        8 => (r >> 11) % 4096,
        9 => (r >> 11) % (1 << 16),
        10 => (r >> 11) % (1 << 20),
        _ => {
            if s > ULONG_MAX / 2 {
                (r >> 11) % 64
            } else {
                ULONG_MAX / 4 - (r >> 11) % 64
            }
        }
    };
    if len > ULONG_MAX - s {
        len = ULONG_MAX - s;
    }
    (s, s + len)
}

/* entries are canonical epool pointers, tags preserved */
fn gen_entry(r: u64) -> u64 {
    match r % 12 {
        0 | 1 => 0,
        2 | 3 | 4 | 5 | 6 | 7 => EBASE + ((r >> 8) % EP as u64) * 16,
        8 => EBASE + ((r >> 8) % EP as u64) * 16 + 1,
        9 => EBASE + ((r >> 8) % EP as u64) * 16 + 3,
        _ => EBASE + ((r >> 8) % EP as u64) * 16,
    }
}
fn gen_entry_nonnull(r: u64) -> u64 {
    let e = gen_entry(r);
    if e == 0 {
        EBASE + ((r >> 16) % EP as u64) * 16
    } else {
        e
    }
}
fn gen_gfp(r: u64) -> u32 {
    match r % 4 {
        0 => GFP_KERNEL,
        1 => GFP_ATOMIC,
        2 => GFP_NOWAIT,
        _ => 0,
    }
}

/*
 * Semantic node dump — mirrors wrappers.c dump_tree.  Upstream's stack
 * "reuse" buffer in mas_wr_node_store copies uninitialised dead bytes
 * into live nodes; those bytes are never read semantically.  Node types
 * are discovered by walking from the root enode and only the words that
 * are semantic for that type's layout are folded:
 *   dense   (0): w0 + slots w1..w30, then w31 (see dump_last_word:
 *                last slot pointer or meta.end + dead bytes)
 *   leaf64  (1): w0 + pivots w1..w15 + slot0 w16, then w31 (same
 *                slot[15]-or-meta union tail as range64); slots 1..14
 *                (w17..w30) are not folded: past-data_end slots may hold
 *                stale reuse-buffer bytes
 *   range64 (2): w0 + pivots w1..w15 + slots w16..w30, then w31
 *   arange64(3): w0 + pivots w1..w9 + slots w10..w19 + gaps w20..w29
 *                (only gaps 0..meta.end), then meta in w30 low bytes;
 *                w30 high bytes and w31 are padding
 *   other/unlinked: index + 0xff sentinel only
 * meta.end is the low byte of w31 for mr64 types, of w30 for arange64.
 */
fn dump_t_slots(t: u8) -> usize {
    match t {
        MAPLE_RANGE_64 => 15,
        MAPLE_ARANGE_64 => 10,
        MAPLE_LEAF_64 => 16,
        MAPLE_DENSE => 30,
        _ => 0,
    }
}
fn dump_t_slot_base(t: u8) -> usize {
    match t {
        MAPLE_RANGE_64 => 16,
        MAPLE_ARANGE_64 => 10,
        MAPLE_LEAF_64 | MAPLE_DENSE => 1,
        _ => 0,
    }
}

fn dump_walk(p: &Pool, e: u64, ntype: &mut [u8; NP]) {
    if e == 0 {
        return;
    }
    let base = e & !0xff;
    if base < NBASE || base >= NBASE + (NP as u64) * 256 {
        return; /* direct entry / NULL */
    }
    let idx = ((base - NBASE) >> 8) as usize;
    if idx >= NP || ntype[idx] != 0xff {
        return;
    }
    let t = ((e >> 3) & 0x0f) as u8;
    ntype[idx] = t;
    if t != MAPLE_RANGE_64 && t != MAPLE_ARANGE_64 {
        return; /* leaf: no children */
    }
    let nb = idx * 32;
    /* meta.end: mr64 types keep it in w31 low byte; arange64 in w30. */
    let end = (p.w[nb + if t == MAPLE_ARANGE_64 { 30 } else { 31 }] & 0xff) as usize;
    let sb = dump_t_slot_base(t);
    let ns = dump_t_slots(t);
    for i in 0..ns {
        if i > end {
            break;
        }
        dump_walk(p, p.w[nb + sb + i], ntype);
    }
}

/* meta.end is consulted by ma_data_end only when pivot[14]==0 (word 15
 * == 0 for leaf64/range64), unconditionally for arange64; leaf64 under
 * ALLOC_RANGE also uses meta via mas_leaf_max_gap.  Otherwise the two
 * metadata bytes are advisory/stale scratch — fold 0. */
fn dump_meta_live(p: &Pool, ti: usize, t: u8, nb: usize) -> bool {
    if t == MAPLE_ARANGE_64 {
        return true;
    }
    if t == MAPLE_DENSE {
        return false;
    }
    if t == MAPLE_LEAF_64 && (p.flags[ti] & MT_FLAGS_ALLOC_RANGE) != 0 {
        return true;
    }
    p.w[nb + 15] == 0
}

/* Mirrors efold()'s pointer domain on the canonical value space: a word
 * that lies inside a canonical pool/tree range is a pointer, anything
 * else is a scalar (pivot, gap, meta, MA_ERROR, stale junk). */
fn is_canonical_ptr(v: u64) -> bool {
    (v >= NBASE && v < NBASE + (NP as u64) * 256)
        || (v >= EBASE && v < EBASE + (EP as u64) * 16)
        || (v >= MT_CANON && v < MT2_CANON + 0x1000)
        || (v >= SHBASE && v < SHBASE + (NSP as u64) * 256)
}

/* Word 31 of the mr64-based layouts (dense/leaf64/range64) is a union
 * tail: either the last slot pointer (dense slot[30], leaf64/range64
 * slot[15]) or metadata {end,gap} + 6 dead bytes.  Fold the canonical
 * pointer when the slot is live; otherwise only meta.end (byte 0) is
 * ever read upstream for these types (ma_meta_gap() uses the arange64
 * offset at byte 241), so the rest is dead. */
fn dump_last_word(p: &mut Pool, ti: usize, t: u8, nb: usize) {
    let w = p.w[nb + 31];
    if is_canonical_ptr(w) {
        p.fold(w);
        return;
    }
    let m = if dump_meta_live(p, ti, t, nb) {
        w & 0xff
    } else {
        0
    };
    p.fold(m);
}

fn dump_tree(p: &mut Pool, ti: usize) {
    p.fold(p.flags[ti] as u64);
    p.fold(efold(p.root[ti]));
    let mut nused = 0u64;
    for i in 0..NP {
        if p.used[i] {
            nused += 1;
        }
    }
    p.fold(nused);
    let mut ntype = [0xffu8; NP];
    dump_walk(p, p.root[ti], &mut ntype);
    for i in 0..NP {
        if !p.used[i] {
            continue;
        }
        p.fold(i as u64);
        let t = ntype[i];
        p.fold(t as u64);
        let nb = i * 32;
        match t {
            MAPLE_DENSE => {
                for w in 0..=30usize {
                    p.fold(efold(p.w[nb + w]));
                }
                dump_last_word(p, ti, t, nb);
            }
            MAPLE_LEAF_64 => {
                for w in 0..=16usize {
                    p.fold(efold(p.w[nb + w]));
                }
                dump_last_word(p, ti, t, nb);
            }
            MAPLE_RANGE_64 => {
                for w in 0..=30usize {
                    p.fold(efold(p.w[nb + w]));
                }
                dump_last_word(p, ti, t, nb);
            }
            MAPLE_ARANGE_64 => {
                /* ma64: pivots w1..9, slots w10..19, gaps w20..29,
                 * meta at w30 low bytes; w30 high + w31 are padding. */
                let end = p.w[nb + 30] & 0xff;
                for w in 0..=19usize {
                    p.fold(efold(p.w[nb + w]));
                }
                for w in 20..=29usize {
                    let v = if (w - 20) as u64 <= end {
                        efold(p.w[nb + w])
                    } else {
                        0
                    };
                    p.fold(v);
                }
                p.fold(p.w[nb + 30] & 0xffff);
            }
            _ => {}
        }
    }
    for i in 0..NSP {
        if !p.sheaf_used[i] {
            continue;
        }
        p.fold(i as u64 | 0x4000);
        p.fold(p.sheaf[i].size as u64);
        for j in 0..p.sheaf[i].size as usize {
            let v = p.sheaf[i].objs[j];
            p.fold(efold(v));
        }
    }
    p.fold(NILX);
}
fn dump_both(p: &mut Pool) {
    dump_tree(p, 0);
    dump_tree(p, 1);
}
fn fold_mas(p: &mut Pool, mas: &Mas) {
    p.fold(mas.index);
    p.fold(mas.last);
    p.fold(mas.min);
    p.fold(mas.max);
    p.fold(mas.status as u64);
    p.fold(mas.depth as u64);
    p.fold(mas.offset as u64);
    p.fold(mas.end as u64);
    p.fold(mas.mas_flags as u64);
    p.fold(mas.node_request as u64);
    p.fold(mas.store_type as u64);
    p.fold(efold(mas.node));
    p.fold(ncanon(mas.alloc));
    p.fold(scanon(mas.sheaf));
}

const FLAG_TAB: [u32; 6] = [
    0,
    MT_FLAGS_ALLOC_RANGE,
    MT_FLAGS_USE_RCU,
    MT_FLAGS_USE_RCU | MT_FLAGS_ALLOC_RANGE,
    MT_FLAGS_LOCK_EXTERN,
    MT_FLAGS_USE_RCU | MT_FLAGS_ALLOC_RANGE | MT_FLAGS_LOCK_EXTERN,
];

fn hot_note(p: &mut Pool, s: u64, l: u64) {
    for i in 0..16 {
        if p.hot[i].2 && p.hot[i].0 == s && p.hot[i].1 == l {
            return;
        }
    }
    for i in 0..16 {
        if !p.hot[i].2 {
            p.hot[i] = (s, l, true);
            return;
        }
    }
    p.hot[15] = (s, l, true);
}
fn hot_kill(p: &mut Pool, s: u64, l: u64) {
    for i in 0..16 {
        if !p.hot[i].2 {
            continue;
        }
        if p.hot[i].0 <= l && s <= p.hot[i].1 {
            p.hot[i].2 = false;
        }
    }
}
fn hot_pick(p: &Pool, r: u64, s: &mut u64, l: &mut u64) -> bool {
    let mut nth = (r % 3) as i32;
    for i in 0..16 {
        if p.hot[i].2 {
            if nth == 0 {
                *s = p.hot[i].0;
                *l = p.hot[i].1;
                return true;
            }
            nth -= 1;
        }
    }
    for i in 0..16 {
        if p.hot[i].2 {
            *s = p.hot[i].0;
            *l = p.hot[i].1;
            return true;
        }
    }
    false
}

fn op_store(p: &mut Pool, r: u64) {
    let e = gen_entry(r >> 20);
    let (mut s, mut l) = (0u64, 0u64);
    if (r >> 6 & 1) != 0 && hot_pick(p, r >> 10, &mut s, &mut l) {
        if r & 0x40 != 0 {
            l = s + (l - s) / 2;
        } else if r & 0x80 != 0 {
            l = s;
        }
    } else {
        let g = gen_range(r);
        s = g.0;
        l = g.1;
    }
    let mut mas = Mas::new(MT_CANON, 0, 0);
    mas_set_range(p, &mut mas, s, l);
    let ret = mas_store_gfp(p, &mut mas, e, gen_gfp(r >> 40));
    p.fold(ret as u64);
    fold_mas(p, &mas);
    if e != 0 {
        hot_note(p, s, l);
    } else {
        hot_kill(p, s, l);
    }
}

fn op_store_range(p: &mut Pool, r: u64) {
    let e = gen_entry(r >> 24);
    let (mut s, mut l) = gen_range(r);
    if r & 0x2000 != 0 {
        std::mem::swap(&mut s, &mut l);
    }
    let ret = mtree_store_range(p, MT_CANON, s, l, e, gen_gfp(r >> 44));
    p.fold(ret as u64);
    if e != 0 {
        hot_note(p, s, l);
    } else if s <= l {
        hot_kill(p, s, l);
    }
}

fn op_insert(p: &mut Pool, r: u64) {
    let e = gen_entry_nonnull(r >> 20);
    let mut s = 0u64;
    let mut l = 0u64;
    if !((r >> 5 & 1) != 0 && hot_pick(p, r >> 9, &mut s, &mut l)) {
        let g = gen_range(r);
        s = g.0;
        l = g.1;
    }
    let ret;
    if r & 0x400 != 0 {
        if s == l || r & 0x800 != 0 {
            ret = mtree_insert(p, MT_CANON, s, e, gen_gfp(r >> 30));
        } else {
            ret = mtree_insert_range(p, MT_CANON, s, l, e, gen_gfp(r >> 30));
        }
    } else {
        ret = mtree_insert_range(p, MT_CANON, s, l, e, gen_gfp(r >> 30));
    }
    p.fold(ret as u64);
}

fn op_load(p: &mut Pool, r: u64) {
    let mut idx = 0u64;
    let mut dummy = 0u64;
    /* C passes &idx for BOTH out-params: the last write (l) wins */
    if (r >> 4 & 1) != 0 && hot_pick(p, r >> 9, &mut dummy, &mut idx) {
        if r & 0x200 != 0 {
            idx += (r >> 20) % 8;
        }
    } else {
        idx = gen_idx(r);
    }
    if r & 0x8000 != 0 {
        let mut mas = Mas::new(MT_CANON, idx, idx);
        let e = mas_walk(p, &mut mas);
        p.fold(efold(e));
        fold_mas(p, &mas);
    } else {
        let e = mtree_load(p, MT_CANON, idx);
        p.fold(efold(e));
    }
}

fn op_erase(p: &mut Pool, r: u64) {
    let mut idx = 0u64;
    let mut dummy = 0u64;
    /* C passes &idx for BOTH out-params: the last write (l) wins */
    if (r >> 4 & 1) != 0 && hot_pick(p, r >> 9, &mut dummy, &mut idx) {
        if r & 0x200 != 0 {
            idx += (r >> 20) % 8;
        }
    } else {
        idx = gen_idx(r);
    }
    if r & 0x4000 != 0 {
        let mut mas = Mas::new(MT_CANON, idx, idx);
        let e = mas_erase(p, &mut mas);
        p.fold(efold(e));
        fold_mas(p, &mas);
    } else {
        let e = mtree_erase(p, MT_CANON, idx);
        p.fold(efold(e));
    }
    hot_kill(p, idx, idx);
}

fn op_walk(p: &mut Pool, r: u64) {
    let mut mas = Mas::new(MT_CANON, 0, 0);
    let steps = 4 + ((r >> 33) % 8) as usize;
    let idx;
    let mut d = 0u64;
    let mut di = 0u64;
    /* C passes &idx twice: the last write (l) wins */
    if (r >> 4 & 1) != 0 && hot_pick(p, r >> 9, &mut d, &mut di) {
        idx = di;
    } else {
        idx = gen_idx(r);
    }
    mas_set(p, &mut mas, idx);
    for _ in 0..steps {
        let q = p.xs64();
        let e = match q % 8 {
            0 => mas_walk(p, &mut mas),
            1 => mas_next(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    ULONG_MAX
                } else {
                    gen_idx(q >> 12)
                },
            ),
            2 => mas_prev(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    0
                } else {
                    gen_idx(q >> 12) % 256
                },
            ),
            3 => mas_find(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    ULONG_MAX
                } else {
                    idx.wrapping_add((q >> 10) % 4096)
                },
            ),
            4 => mas_find_rev(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    0
                } else {
                    gen_idx(q >> 12) % 256
                },
            ),
            5 => mas_next_range(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    ULONG_MAX
                } else {
                    idx.wrapping_add((q >> 10) % 4096)
                },
            ),
            6 => mas_prev_range(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    0
                } else {
                    gen_idx(q >> 12) % 256
                },
            ),
            _ => mas_find_range(
                p,
                &mut mas,
                if q & 0x80 != 0 {
                    ULONG_MAX
                } else {
                    idx.wrapping_add((q >> 10) % 4096)
                },
            ),
        };
        p.fold(efold(e));
        fold_mas(p, &mas);
        if r & 0x40000 != 0 {
            mas_pause(p, &mut mas);
        } else if r & 0x80000 != 0 {
            mas_reset(p, &mut mas);
        }
        if mas.status == MA_OVERFLOW || mas.status == MA_UNDERFLOW {
            let g = gen_idx(q >> 20);
            mas_set(p, &mut mas, g);
        }
    }
    mas_destroy(p, &mut mas);
}

fn op_for_each(p: &mut Pool, r: u64) {
    let mut mas = Mas::new(MT_CANON, 0, 0);
    let mut k = 0u64;
    if r & 0x20 != 0 {
        let idx = gen_idx(r);
        let lim = if r & 0x40 != 0 {
            ULONG_MAX
        } else {
            idx + (r >> 16) % 8192
        };
        mas_set(p, &mut mas, idx);
        loop {
            let e = mas_find(p, &mut mas, lim);
            if e == 0 {
                break;
            }
            p.fold(efold(e));
            p.fold(mas.index);
            p.fold(mas.last);
            k += 1;
            if k > 48 {
                break;
            }
        }
        p.fold(NILX);
        p.fold(k);
    } else {
        let idx = gen_idx(r);
        let lim = if r & 0x40 != 0 { 0 } else { idx % 512 };
        mas_set(p, &mut mas, idx);
        loop {
            let e = mas_find_rev(p, &mut mas, lim);
            if e == 0 {
                break;
            }
            p.fold(efold(e));
            p.fold(mas.index);
            p.fold(mas.last);
            k += 1;
            if k > 48 {
                break;
            }
        }
        p.fold(NILX);
        p.fold(k);
    }
    mas_destroy(p, &mut mas);
}

fn op_empty_area(p: &mut Pool, r: u64) {
    let (min, max) = gen_range(r);
    let mut size = 1 + ((r >> 21) % 48);
    if r & 0x400 != 0 {
        size = 1 + ((r >> 25) % 1024);
    }
    let mut mas = Mas::new(MT_CANON, 0, 0);
    let ret = if r & 0x2000 != 0 {
        mas_empty_area_rev(p, &mut mas, min, max, size)
    } else {
        mas_empty_area(p, &mut mas, min, max, size)
    };
    p.fold(ret as u64);
    fold_mas(p, &mas);
    mas_destroy(p, &mut mas);
}

fn op_prealloc(p: &mut Pool, r: u64) {
    let e = gen_entry_nonnull(r >> 20);
    let (s, l) = gen_range(r);
    let mut mas = Mas::new(MT_CANON, 0, 0);
    mas_set_range(p, &mut mas, s, l);
    let ret = mas_preallocate(p, &mut mas, e, gen_gfp(r >> 40));
    p.fold(ret as u64);
    fold_mas(p, &mas);
    if ret == 0 && (r & 0x800) == 0 {
        mas_store_prealloc(p, &mut mas, e);
        fold_mas(p, &mas);
        hot_note(p, s, l);
    } else {
        mas_destroy(p, &mut mas);
        p.fold(NILX);
    }
}

fn mt_init_flags(p: &mut Pool, mt: u64, flags: u32) {
    p.set_tflags(mt, flags);
    p.set_troot(mt, 0);
}

fn op_dup(p: &mut Pool, r: u64, flags: u32) {
    mt_init_flags(p, MT2_CANON, flags);
    let ret = if r & 0x100 != 0 {
        __mt_dup(p, MT_CANON, MT2_CANON, gen_gfp(r >> 20))
    } else {
        mtree_dup(p, MT_CANON, MT2_CANON, gen_gfp(r >> 20))
    };
    p.fold(ret as u64);
    dump_both(p);
    if r & 0x200 != 0 {
        let idx = gen_idx(r >> 10);
        let e = mtree_load(p, MT2_CANON, idx);
        p.fold(efold(e));
        let e2 = mtree_erase(p, MT2_CANON, idx);
        p.fold(efold(e2));
    }
    if r & 0x400 != 0 {
        let e3 = mtree_erase(p, MT2_CANON, gen_idx(r >> 14));
        p.fold(efold(e3));
    }
    dump_both(p);
    mtree_destroy(p, MT2_CANON);
    p.fold(efold(p.troot(MT2_CANON)));
    mt_init_flags(p, MT2_CANON, flags);
}

fn op_destroy(p: &mut Pool, flags: u32) {
    mtree_destroy(p, MT_CANON);
    p.fold(efold(p.troot(MT_CANON)));
    p.fold(p.tflags(MT_CANON) as u64);
    p.hot = [(0, 0, false); 16];
    mt_init_flags(p, MT_CANON, flags);
    p.fold(flags as u64);
}

fn op_mt_find(p: &mut Pool, r: u64) {
    let mut idx;
    let mut d = 0u64;
    let mut di = 0u64;
    /* C passes &idx twice: the last write (l) wins */
    if (r >> 4 & 1) != 0 && hot_pick(p, r >> 9, &mut d, &mut di) {
        idx = di;
    } else {
        idx = gen_idx(r);
    }
    match (r >> 3) % 4 {
        0 => {
            let max = if r & 0x1000 != 0 {
                ULONG_MAX
            } else {
                idx + (r >> 20) % 4096
            };
            let e = mt_find(p, MT_CANON, &mut idx, max);
            p.fold(efold(e));
            p.fold(idx);
        }
        1 => {
            idx += 1;
            if idx == 0 {
                p.fold(NILX);
            }
            let max = if r & 0x1000 != 0 {
                ULONG_MAX
            } else {
                idx + (r >> 20) % 4096
            };
            let e = mt_find_after(p, MT_CANON, &mut idx, max);
            p.fold(efold(e));
            p.fold(idx);
        }
        2 => {
            let e = mt_next(
                p,
                MT_CANON,
                idx,
                if r & 0x1000 != 0 {
                    ULONG_MAX
                } else {
                    idx + (r >> 20) % 4096
                },
            );
            p.fold(efold(e));
        }
        _ => {
            let e = mt_prev(
                p,
                MT_CANON,
                idx,
                if r & 0x1000 != 0 { 0 } else { idx % 256 },
            );
            p.fold(efold(e));
        }
    }
}

pub fn wrap_mt_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    const MIX: [i32; 8] = [22, 10, 12, 8, 6, 14, 16, 12];
    let nops = (iabs(nops_i) % 180) as usize;
    let flags = FLAG_TAB[(iabs(seed2) % 6) as usize];
    let dump_every = 4 + (iabs(seed2) % 7) as usize;
    let mut p = Pool::new();
    p.reset_all(flags);
    p.hot = [(0, 0, false); 16];
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = !0;
    }
    for i in 0..nops {
        let r = p.xs64();
        if p.trace {
            eprintln!("OP {} r={:x}", i, r);
        }
        let t = (r % 100) as i32;
        let mut c = 0i32;
        if t < {
            c += MIX[0];
            c
        } {
            match (r >> 4) % 4 {
                0 | 1 => op_store(&mut p, r),
                2 => op_store_range(&mut p, r),
                _ => op_insert(&mut p, r),
            }
        } else if t < {
            c += MIX[1];
            c
        } {
            op_load(&mut p, r);
        } else if t < {
            c += MIX[2];
            c
        } {
            op_erase(&mut p, r);
        } else if t < {
            c += MIX[3];
            c
        } {
            op_prealloc(&mut p, r);
        } else if t < {
            c += MIX[4];
            c
        } {
            op_mt_find(&mut p, r);
        } else if t < {
            c += MIX[5];
            c
        } {
            op_walk(&mut p, r);
        } else if t < {
            c += MIX[6];
            c
        } {
            match (r >> 4) % 4 {
                0 => op_for_each(&mut p, r),
                1 => op_empty_area(&mut p, r),
                2 => {
                    let (lo, hi) = gen_range(r);
                    let mut startp = 0xbadu64;
                    if r & 0x80 != 0 {
                        let mut next = lo + (r >> 16) % 64;
                        let ret = mtree_alloc_cyclic(
                            &mut p,
                            MT_CANON,
                            &mut startp,
                            gen_entry_nonnull(r >> 30),
                            lo,
                            hi,
                            &mut next,
                            gen_gfp(r >> 40),
                        );
                        p.fold(ret as u64);
                        p.fold(next);
                    } else if r & 0x100 != 0 {
                        let ret = mtree_alloc_rrange(
                            &mut p,
                            MT_CANON,
                            &mut startp,
                            gen_entry_nonnull(r >> 30),
                            1 + (r >> 12) % 32,
                            lo,
                            hi,
                            gen_gfp(r >> 40),
                        );
                        p.fold(ret as u64);
                    } else {
                        let ret = mtree_alloc_range(
                            &mut p,
                            MT_CANON,
                            &mut startp,
                            gen_entry_nonnull(r >> 30),
                            1 + (r >> 12) % 32,
                            lo,
                            hi,
                            gen_gfp(r >> 40),
                        );
                        p.fold(ret as u64);
                    }
                    p.fold(startp);
                }
                _ => op_walk(&mut p, r.wrapping_add(0x9e3779b9)),
            }
        } else {
            match (r >> 4) % 4 {
                0 => op_dup(&mut p, r, flags),
                1 => op_destroy(&mut p, flags),
                2 => op_for_each(&mut p, r ^ 0x5a5a5a5a),
                _ => op_empty_area(&mut p, r ^ 0x1234567),
            }
        }
        if p.trace {
            eprintln!("DONE {}", i);
        }
        if i % dump_every == dump_every - 1 {
            dump_both(&mut p);
        }
    }
    dump_both(&mut p);
    let w = p.warn;
    p.fold(w);
    p.h
}

pub fn wrap_mt_area(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    let nops = (iabs(nops_i) % 160) as usize;
    let flags = MT_FLAGS_ALLOC_RANGE
        | if (iabs(seed2) & 3) == 1 {
            MT_FLAGS_USE_RCU
        } else {
            0
        }
        | if (iabs(seed2) & 7) == 3 {
            MT_FLAGS_LOCK_EXTERN
        } else {
            0
        };
    let dump_every = 4 + (iabs(seed2) % 6) as usize;
    let mut p = Pool::new();
    p.reset_all(flags);
    p.hot = [(0, 0, false); 16];
    p.r = seed | 1;
    p.fail_mask = if iabs(seed2) != 0 {
        iabs(seed2).wrapping_mul(0x9e3779b97f4a7c15)
    } else {
        0
    };
    if iabs(seed2) % 5 == 1 {
        p.fail_mask = !0;
    }
    for i in 0..nops {
        let r = p.xs64();
        match (r % 20) as u32 {
            0 | 1 | 2 | 3 => {
                let (lo, hi) = gen_range(r);
                let mut startp = 0xbadu64;
                let ret = mtree_alloc_range(
                    &mut p,
                    MT_CANON,
                    &mut startp,
                    gen_entry_nonnull(r >> 30),
                    1 + (r >> 12) % 32,
                    lo,
                    hi,
                    gen_gfp(r >> 40),
                );
                p.fold(ret as u64);
                p.fold(startp);
                hot_note(&mut p, startp, startp + (r >> 12) % 32);
            }
            4 | 5 => {
                let (lo, hi) = gen_range(r);
                let mut startp = 0xbadu64;
                let ret = mtree_alloc_rrange(
                    &mut p,
                    MT_CANON,
                    &mut startp,
                    gen_entry_nonnull(r >> 30),
                    1 + (r >> 12) % 32,
                    lo,
                    hi,
                    gen_gfp(r >> 40),
                );
                p.fold(ret as u64);
                p.fold(startp);
                hot_note(&mut p, startp, startp + (r >> 12) % 32);
            }
            6 | 7 => {
                let (lo, hi) = gen_range(r);
                let mut startp = 0xbadu64;
                let mut next = (r >> 16) % 128;
                let ret = mtree_alloc_cyclic(
                    &mut p,
                    MT_CANON,
                    &mut startp,
                    gen_entry_nonnull(r >> 30),
                    lo,
                    hi,
                    &mut next,
                    gen_gfp(r >> 40),
                );
                p.fold(ret as u64);
                p.fold(startp);
                p.fold(next);
                hot_note(&mut p, startp, startp);
            }
            8 | 9 => {
                let (lo, hi) = gen_range(r);
                let mut mas = Mas::new(MT_CANON, 0, 0);
                let ret = mas_empty_area(&mut p, &mut mas, lo, hi, 1 + (r >> 12) % 64);
                p.fold(ret as u64);
                p.fold(mas.index);
                p.fold(mas.last);
                mas_destroy(&mut p, &mut mas);
            }
            10 | 11 => {
                let (lo, hi) = gen_range(r);
                let mut mas = Mas::new(MT_CANON, 0, 0);
                let ret = mas_empty_area_rev(&mut p, &mut mas, lo, hi, 1 + (r >> 12) % 64);
                p.fold(ret as u64);
                p.fold(mas.index);
                p.fold(mas.last);
                mas_destroy(&mut p, &mut mas);
            }
            12 | 13 => op_store(&mut p, r),
            14 => op_erase(&mut p, r),
            15 => op_load(&mut p, r),
            16 => op_for_each(&mut p, r),
            17 => op_destroy(&mut p, flags),
            18 => {
                let (lo, hi0) = gen_range(r);
                let hi = lo + (hi0 - lo) % 40;
                let ret = mtree_store_range(
                    &mut p,
                    MT_CANON,
                    lo,
                    hi,
                    gen_entry_nonnull(r >> 30),
                    gen_gfp(r >> 40),
                );
                p.fold(ret as u64);
                if r & 0x4000 != 0 {
                    let e = mtree_erase(&mut p, MT_CANON, lo + (r >> 20) % 40);
                    p.fold(efold(e));
                }
                hot_note(&mut p, lo, hi);
            }
            _ => op_walk(&mut p, r),
        }
        if i % dump_every == dump_every - 1 {
            dump_both(&mut p);
        }
    }
    dump_both(&mut p);
    let w = p.warn;
    p.fold(w);
    p.h
}

pub fn wrap_mt_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    let n = (iabs(n_i) % 120) as usize;
    let walk = (iabs(seed2) % 4) as usize;
    let flags = FLAG_TAB[(iabs(seed2) % 6) as usize];
    let mut p = Pool::new();
    p.reset_all(flags);
    p.hot = [(0, 0, false); 16];
    p.r = seed | 1;
    p.fail_mask = 0;
    for _ in 0..n {
        let r = p.xs64();
        if r % 4 == 0 {
            let (s, l) = gen_range(r);
            let ret = mtree_store_range(
                &mut p,
                MT_CANON,
                s,
                l,
                gen_entry_nonnull(r >> 30),
                GFP_KERNEL,
            );
            p.fold(ret as u64);
            hot_note(&mut p, s, l);
        } else {
            let ret = mtree_store(
                &mut p,
                MT_CANON,
                gen_idx(r),
                gen_entry_nonnull(r >> 9),
                GFP_KERNEL,
            );
            p.fold(ret as u64);
        }
        if r % 11 == 0 {
            let e = mtree_erase(&mut p, MT_CANON, gen_idx(r >> 25));
            p.fold(efold(e));
        }
    }
    dump_both(&mut p);
    match walk {
        0 => {
            let mut index = 0u64;
            let max = if seed2 & 0x100 != 0 {
                ULONG_MAX
            } else {
                gen_idx(p.xs64()) % (1 << 24)
            };
            let mut steps = 0u64;
            let mut entry = mt_find(&mut p, MT_CANON, &mut index, max);
            while entry != 0 {
                p.fold(index);
                p.fold(efold(entry));
                steps += 1;
                if steps > 80 || index == ULONG_MAX {
                    break;
                }
                index += 1;
                entry = mt_find_after(&mut p, MT_CANON, &mut index, max);
            }
            p.fold(NILX);
            p.fold((steps > 80) as u64);
        }
        1 => {
            let mut mas = Mas::new(MT_CANON, 0, 0);
            let st = gen_idx(p.xs64()) % 4096;
            mas_set(&mut p, &mut mas, st);
            let max = if seed2 & 0x200 != 0 {
                ULONG_MAX
            } else {
                1 << 20
            };
            let mut steps = 0u64;
            loop {
                let entry = mas_find(&mut p, &mut mas, max);
                if entry == 0 {
                    break;
                }
                p.fold(efold(entry));
                p.fold(mas.index);
                p.fold(mas.last);
                steps += 1;
                if steps % 3 == 0 {
                    mas_pause(&mut p, &mut mas);
                }
                if steps > 60 {
                    break;
                }
            }
            p.fold(NILX);
            p.fold((steps > 60) as u64);
            mas_destroy(&mut p, &mut mas);
        }
        2 => {
            let mut mas = Mas::new(MT_CANON, 0, 0);
            let start = gen_idx(p.xs64()) % 512;
            mas_set(&mut p, &mut mas, start);
            for _ in 0..90 {
                let e = mas_next(&mut p, &mut mas, ULONG_MAX);
                p.fold(efold(e));
                p.fold(mas.index);
                if mas.status == MA_OVERFLOW {
                    break;
                }
            }
            p.fold(NILX);
            let mi = mas.index;
            mas_set(&mut p, &mut mas, mi);
            for _ in 0..90 {
                let e = mas_prev(&mut p, &mut mas, 0);
                p.fold(efold(e));
                p.fold(mas.index);
                if mas.status == MA_UNDERFLOW {
                    break;
                }
            }
            p.fold(NILX);
            mas_destroy(&mut p, &mut mas);
        }
        _ => {
            let mut mas = Mas::new(MT_CANON, 0, 0);
            let mut k = 0u64;
            mas_set(&mut p, &mut mas, 0);
            loop {
                let entry = mas_find(&mut p, &mut mas, ULONG_MAX);
                if entry == 0 {
                    break;
                }
                p.fold(mas.index);
                p.fold(mas.last);
                if k % 2 == 0 {
                    let mut em = Mas::new(MT_CANON, mas.index, mas.index);
                    let e = mas_erase(&mut p, &mut em);
                    p.fold(efold(e));
                }
                k += 1;
                if k > 120 || mas.index == ULONG_MAX {
                    break;
                }
                let mi = mas.index + 1;
                mas_set(&mut p, &mut mas, mi);
            }
            p.fold(NILX);
            mas_destroy(&mut p, &mut mas);
        }
    }
    dump_both(&mut p);
    let w = p.warn;
    p.fold(w);
    p.h
}

/* ==== APPEND POINT ==== */
