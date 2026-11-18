//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/llist.c and the include/linux/llist.h inline
//! layer, driven through the same seeded index-pool harness as the C
//! wrappers.  `struct llist_node *` becomes a `usize` pool index and
//! NULL is `NIL` (`usize::MAX`; folded into checksums as 0xffff, like
//! the C `NILX`).  `container_of`/`llist_entry` is the identity on
//! indices — the C side does real offsetof arithmetic because `node`
//! sits behind `val` in `struct tnode`, but it always recovers the same
//! pool slot, so folding indices is equivalent.
//!
//! The lockless atomics are sequential: try_cmpxchg/xchg always succeed
//! on the first pass, matching the single-threaded shim in linux/atomic.h.
//! `init_llist_node`'s self-pointer becomes `next[i] = i`, so
//! `llist_on_list(i)` is `next[i] != i`.

#![forbid(unsafe_code)]
// pub fn names mirror the wrappers.c export names verbatim.
#![allow(non_snake_case)]

const N: usize = 32;
const NIL: usize = usize::MAX;
const NILX: u64 = 0xffff;
const TEND: u64 = 0xfff0;

/// The harness pools: `next[i]` is `pool[i].node.next`, `first` is
/// `head.first`, `val`/`on_list` are the `struct tnode` payload and the
/// harness's bookkeeping flag.
struct Rig {
    next: [usize; N],
    first: usize,
    val: [u64; N],
    on_list: [bool; N],
    rng: u64,
    h: u64,
}

/// C `reset_all()`: pools are memset to 0 — the next pointers read NULL
/// (NIL in the index model, not slot 0!), val 0, on_list 0.
fn rig_new() -> Rig {
    Rig {
        next: [NIL; N],
        first: NIL,
        val: [0; N],
        on_list: [false; N],
        rng: 0,
        h: 0x9e3779b97f4a7c15,
    }
}

fn xs64(r: &mut u64) -> u64 {
    let mut x = *r;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *r = x;
    x.wrapping_mul(2685821657736338717)
}

fn iabs(v: i64) -> u64 {
    v.unsigned_abs()
}

fn fold(h: &mut u64, v: u64) {
    (*h) = ((*h) ^ v).wrapping_mul(0x100000001b3);
}

fn nidx(p: usize) -> u64 {
    if p == NIL { NILX } else { p as u64 }
}

fn free_count(r: &Rig, npool: usize) -> usize {
    r.on_list[..npool].iter().filter(|&&x| !x).count()
}

fn kth_free(r: &Rig, npool: usize, k: usize) -> i64 {
    let mut k = k;
    for (i, &on) in r.on_list[..npool].iter().enumerate() {
        if !on {
            if k == 0 {
                return i as i64;
            }
            k -= 1;
        }
    }
    -1
}

// ---- lib/llist.c + include/linux/llist.h, index-modelled ------------------

/// C `init_llist_node`: `WRITE_ONCE(node->next, node)` — self-pointer.
fn init_llist_node(r: &mut Rig, i: usize) {
    r.next[i] = i;
}

/// C `llist_on_list`: `READ_ONCE(node->next) != node`.
fn llist_on_list(r: &Rig, i: usize) -> bool {
    r.next[i] != i
}

/// C `llist_empty`: `READ_ONCE(head->first) == NULL`.
fn llist_empty(r: &Rig) -> bool {
    r.first == NIL
}

/// C `llist_next`: `READ_ONCE(node->next)`.
fn llist_next(r: &Rig, i: usize) -> usize {
    r.next[i]
}

/// C `llist_del_first`: try_cmpxchg always succeeds on the first pass
/// in the sequential model, so the do/while runs exactly once.
fn llist_del_first(r: &mut Rig) -> usize {
    let entry = r.first; // smp_load_acquire(&head->first)
    if entry == NIL {
        return NIL;
    }
    let next = r.next[entry]; // READ_ONCE(entry->next)
    r.first = next; // try_cmpxchg(&head->first, &entry, next) == true
    entry
}

/// C `llist_del_first_this`.
fn llist_del_first_this(r: &mut Rig, this: usize) -> bool {
    let entry = r.first;
    if entry != this {
        return false;
    }
    let next = r.next[entry];
    r.first = next;
    true
}

/// C `llist_del_first_init`: del_first + init_llist_node on the result.
fn llist_del_first_init(r: &mut Rig) -> usize {
    let n = llist_del_first(r);
    if n != NIL {
        init_llist_node(r, n);
    }
    n
}

/// C `llist_del_all` (`xchg(&head->first, NULL)`).
fn llist_del_all(r: &mut Rig) -> usize {
    let old = r.first;
    r.first = NIL;
    old
}

/// C `__llist_del_all`.
fn __llist_del_all(r: &mut Rig) -> usize {
    let first = r.first;
    r.first = NIL;
    first
}

/// C `llist_reverse_order`.
fn llist_reverse_order(r: &mut Rig, mut head: usize) -> usize {
    let mut new_head = NIL;
    while head != NIL {
        let tmp = head;
        head = r.next[head];
        r.next[tmp] = new_head;
        new_head = tmp;
    }
    new_head
}

/// C `llist_add_batch`: single-threaded -> the cmpxchg loop is one pass.
fn llist_add_batch(r: &mut Rig, new_first: usize, new_last: usize) -> bool {
    let first = r.first; // READ_ONCE(head->first)
    r.next[new_last] = first;
    r.first = new_first; // try_cmpxchg succeeds
    first == NIL
}

/// C `__llist_add_batch`.
fn __llist_add_batch(r: &mut Rig, new_first: usize, new_last: usize) -> bool {
    r.next[new_last] = r.first;
    r.first = new_first;
    r.next[new_last] == NIL
}

/// C `llist_add`.
fn llist_add(r: &mut Rig, new: usize) -> bool {
    llist_add_batch(r, new, new)
}

/// C `__llist_add`.
fn __llist_add(r: &mut Rig, new: usize) -> bool {
    __llist_add_batch(r, new, new)
}

// ---- fold helpers ---------------------------------------------------------

fn fold_chain(r: &mut Rig, first: usize) {
    let mut p = first;
    let mut c: u64 = 0;
    while p != NIL {
        fold(&mut r.h, nidx(p));
        let v = r.val[p];
        fold(&mut r.h, v);
        c += 1;
        p = r.next[p];
    }
    fold(&mut r.h, TEND);
    fold(&mut r.h, c);
}

/// Same walk as fold_chain — in the index model llist_for_each_entry is
/// identical to llist_for_each (container_of is the identity on slots).
fn fold_chain_entry(r: &mut Rig, first: usize) {
    fold_chain(r, first);
}

fn fold_chain_entry_safe(r: &mut Rig, first: usize) {
    let mut p = first;
    let mut c: u64 = 0;
    while p != NIL {
        let n = r.next[p]; // n = llist_entry(pos->member.next, ...)
        fold(&mut r.h, p as u64);
        let v = r.val[p];
        fold(&mut r.h, v);
        // member_address_is_nonnull(n, node): the fake pointer produced
        // by llist_entry(NULL) — NIL here — fails the check.
        fold(&mut r.h, (n != NIL) as u64);
        c += 1;
        p = n;
    }
    fold(&mut r.h, TEND);
    fold(&mut r.h, c);
}

// ---- exported wrappers ----------------------------------------------------

/// Port of `wrap_llist_seq`: seeded op-mix driver.
pub fn wrap_llist_seq(seed: u64, npool_i: i64, nops_i: i64) -> u64 {
    let npool = (iabs(npool_i) % (N as u64 + 1)) as usize;
    let nops = (iabs(nops_i) % 200) as usize;

    let mut r = rig_new();
    r.rng = seed | 1;
    for i in 0..npool {
        r.val[i] = xs64(&mut r.rng);
    }

    for _ in 0..nops {
        let x = xs64(&mut r.rng);
        let op = (x % 16) as u32;

        match op {
            0..=2 => {
                // llist_add / __llist_add
                let fc = free_count(&r, npool);
                let idx = kth_free(
                    &r,
                    npool,
                    ((x >> 8) % (fc | (fc == 0) as usize) as u64) as usize,
                );
                if idx < 0 {
                    fold(&mut r.h, 0xbad0);
                    continue;
                }
                let idx = idx as usize;
                let b = if x & 0x80 != 0 {
                    llist_add(&mut r, idx)
                } else {
                    __llist_add(&mut r, idx)
                };
                fold(&mut r.h, b as u64);
                r.on_list[idx] = true;
                fold(&mut r.h, idx as u64);
            }
            3..=4 => {
                // llist_add_batch / __llist_add_batch
                let fc = free_count(&r, npool);
                let mut k = 1 + ((x >> 8) % 4) as usize;
                let mut first: i64 = -1;
                let mut last: i64 = -1;
                let mut prev: i64 = -1;
                if k > fc {
                    k = fc;
                }
                if k == 0 {
                    fold(&mut r.h, 0xbad1);
                    continue;
                }
                for j in 0..k {
                    let idx = kth_free(&r, npool, 0);
                    if j == 0 {
                        first = idx;
                    } else {
                        r.next[prev as usize] = idx as usize;
                    }
                    r.on_list[idx as usize] = true;
                    prev = idx;
                    last = idx;
                }
                r.next[last as usize] = NIL;
                let b = if x & 0x80 != 0 {
                    llist_add_batch(&mut r, first as usize, last as usize)
                } else {
                    __llist_add_batch(&mut r, first as usize, last as usize)
                };
                fold(&mut r.h, b as u64);
                fold(&mut r.h, first as u64);
                fold(&mut r.h, k as u64);
            }
            5..=6 => {
                // llist_del_first / _init
                if x & 0x80 != 0 {
                    let p = llist_del_first_init(&mut r);
                    fold(&mut r.h, nidx(p));
                    let ol = if p != NIL {
                        llist_on_list(&r, p) as u64
                    } else {
                        7
                    };
                    fold(&mut r.h, ol);
                    if p != NIL {
                        r.on_list[p] = false;
                    }
                } else {
                    let p = llist_del_first(&mut r);
                    fold(&mut r.h, nidx(p));
                    if p != NIL {
                        r.on_list[p] = false;
                    }
                }
            }
            7 => {
                // llist_del_first_this
                let idx: i64 = if !llist_empty(&r) && (x & 0x80 != 0) {
                    r.first as i64 // hit
                } else if x & 0x40 != 0 {
                    let fc = free_count(&r, npool);
                    kth_free(
                        &r,
                        npool,
                        ((x >> 8) % (fc | (fc == 0) as usize) as u64) as usize,
                    )
                } else {
                    ((x >> 8) % (npool | (npool == 0) as usize) as u64) as i64
                };
                if idx < 0 {
                    fold(&mut r.h, 0xbad2);
                    continue;
                }
                let idx = idx as usize;
                let ret = llist_del_first_this(&mut r, idx);
                fold(&mut r.h, ret as u64);
                if ret {
                    r.on_list[idx] = false;
                } else if !r.on_list[idx] {
                    fold(&mut r.h, 0xe11);
                } else {
                    fold(&mut r.h, 0xe12);
                }
                fold(&mut r.h, idx as u64);
            }
            8 => {
                // llist_del_all / __llist_del_all
                let p = if x & 0x80 != 0 {
                    llist_del_all(&mut r)
                } else {
                    __llist_del_all(&mut r)
                };
                fold(&mut r.h, nidx(p));
                let mut c: u64 = 0;
                let mut q = p;
                while q != NIL {
                    let n2 = r.next[q];
                    fold(&mut r.h, nidx(q));
                    r.on_list[q] = false;
                    c += 1;
                    q = n2;
                }
                fold(&mut r.h, TEND);
                fold(&mut r.h, c);
            }
            9 => {
                // llist_reverse_order on the detached chain
                let p = __llist_del_all(&mut r);
                fold(&mut r.h, nidx(p));
                let p = llist_reverse_order(&mut r, p);
                r.first = p;
                fold(&mut r.h, nidx(p));
                let mut q = p;
                while q != NIL {
                    let n2 = r.next[q];
                    fold(&mut r.h, nidx(q));
                    q = n2;
                }
                fold(&mut r.h, TEND);
            }
            10 => {
                // traversal variants
                let e = llist_empty(&r) as u64;
                fold(&mut r.h, e);
                let f = r.first;
                match (x >> 8) % 3 {
                    0 => fold_chain(&mut r, f),
                    1 => fold_chain_entry(&mut r, f),
                    _ => fold_chain_entry_safe(&mut r, f),
                }
            }
            11 => {
                // llist_next / peek head
                let p = r.first;
                fold(&mut r.h, nidx(p));
                let nx = if p != NIL { llist_next(&r, p) } else { NIL };
                fold(&mut r.h, nidx(nx));
            }
            12 => {
                // init_llist_node / llist_on_list
                let idx = ((x >> 8) % (npool | (npool == 0) as usize) as u64) as usize;
                fold(&mut r.h, idx as u64);
                let ol = llist_on_list(&r, idx) as u64;
                fold(&mut r.h, ol);
                if !r.on_list[idx] {
                    init_llist_node(&mut r, idx);
                    let ol = llist_on_list(&r, idx) as u64;
                    fold(&mut r.h, ol);
                }
            }
            13 => {
                let e = llist_empty(&r) as u64;
                fold(&mut r.h, e);
                let f = nidx(r.first);
                fold(&mut r.h, f);
            }
            _ => {
                // 14,15: re-fold state
                let f = r.first;
                fold_chain(&mut r, f);
            }
        }
    }
    let f = r.first;
    fold_chain(&mut r, f);
    r.h
}

/// Port of `wrap_llist_reverse`: build n, detach, reverse, reverse again.
pub fn wrap_llist_reverse(seed: u64, n_i: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 + 1)) as usize;

    let mut r = rig_new();
    r.rng = seed | 1;
    for i in 0..n {
        r.val[i] = xs64(&mut r.rng);
    }

    let mode = (xs64(&mut r.rng) % 2) as u32;
    for i in 0..n {
        let b = if mode != 0 {
            llist_add(&mut r, i)
        } else {
            __llist_add(&mut r, i)
        };
        fold(&mut r.h, b as u64);
        r.on_list[i] = true;
    }

    let p = llist_del_all(&mut r);
    fold(&mut r.h, nidx(p));
    let p = llist_reverse_order(&mut r, p);
    r.first = p;
    let f = r.first;
    fold_chain_entry(&mut r, f);

    // reverse again -> original order
    let p = __llist_del_all(&mut r);
    let p = llist_reverse_order(&mut r, p);
    r.first = p;
    let f = r.first;
    fold_chain(&mut r, f);
    r.h
}

/// Port of `wrap_llist_batch`: batch of k then batch of m.
pub fn wrap_llist_batch(seed: u64, k_i: i64) -> u64 {
    let k = (iabs(k_i) % (N as u64 + 1)) as usize;

    let mut r = rig_new();
    r.rng = seed | 1;
    for i in 0..N {
        r.val[i] = xs64(&mut r.rng);
    }

    // chain 0..k-1, add in one batch (empty -> true)
    let mut first: i64 = -1;
    let mut prev: i64 = -1;
    for i in 0..k {
        if i != 0 {
            r.next[prev as usize] = i;
        } else {
            first = i as i64;
        }
        r.on_list[i] = true;
        prev = i as i64;
    }
    if k != 0 {
        r.next[k - 1] = NIL;
        let b = llist_add_batch(&mut r, first as usize, k - 1);
        fold(&mut r.h, b as u64);
    } else {
        fold(&mut r.h, 0xbad3);
    }
    let f = r.first;
    fold_chain(&mut r, f);

    // second batch over the rest (non-empty -> false)
    let mut m = N - k;
    if m > 8 {
        m = 8;
    }
    let mut first: i64 = -1;
    let mut prev: i64 = -1;
    for i in k..(k + m) {
        if i == k {
            first = i as i64;
        } else {
            r.next[prev as usize] = i;
        }
        r.on_list[i] = true;
        prev = i as i64;
    }
    if m != 0 {
        r.next[k + m - 1] = NIL;
        let x = xs64(&mut r.rng);
        let b = if x & 1 != 0 {
            llist_add_batch(&mut r, first as usize, k + m - 1)
        } else {
            __llist_add_batch(&mut r, first as usize, k + m - 1)
        };
        fold(&mut r.h, b as u64);
    } else {
        fold(&mut r.h, 0xbad4);
    }
    let f = r.first;
    fold_chain_entry_safe(&mut r, f);
    r.h
}
