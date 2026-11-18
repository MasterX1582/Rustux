//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! The real sort()/list_sort() algorithms from lib/sort.c and lib/list_sort.c
//! are ported as private helpers over byte offsets / node indices; the public
//! functions mirror the C wrappers that drive them on seeded inputs.

#![forbid(unsafe_code)]

const N: usize = 64;
const NIL: usize = usize::MAX;

/* xorshift64: identical stream to the C reference. */
fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn checksum_u32(a: &[u32]) -> u64 {
    let mut h = 0x9e3779b97f4a7c15u64;
    for v in a {
        h = (h ^ *v as u64).wrapping_mul(0x100000001b3);
    }
    h
}

// ---- lib/sort.c --------------------------------------------------------

/// `__sort_r` in element-index units: the C works on byte offsets, but every
/// offset is `k * size`, so all arithmetic maps to element indices 1:1 —
/// including `parent(i) == (i/size - 1)/2` for ANY size (the `lsbit` trick is
/// just division-free index math), `c = 2b+1`, `d = c+1`, `a -= 1<<shift`.
/// `cmp` compares two elements; the C's swap granularity (words vs bytes)
/// doesn't affect results.
fn sort_slice<T>(arr: &mut [T], cmp: &mut dyn FnMut(&T, &T) -> i32) {
    let mut n = arr.len();
    let mut a = arr.len() / 2;
    let mut shift = 0usize;

    if a == 0 {
        return;
    }

    loop {
        let mut c;
        let mut d;
        if a != 0 {
            a -= 1 << shift;
        } else if n > 3 {
            n -= 1;
            arr.swap(0, n);
            shift = usize::from(cmp(&arr[1], &arr[2]) <= 0);
            a = 1 << shift;
            n -= 1;
            arr.swap(a, n);
        } else {
            break;
        }

        let mut b = a;
        loop {
            c = 2 * b + 1;
            d = c + 1;
            if d >= n {
                break;
            }
            b = if cmp(&arr[c], &arr[d]) > 0 { c } else { d };
        }
        if d == n {
            b = c;
        }

        while b != a && cmp(&arr[a], &arr[b]) >= 0 {
            b = (b - 1) / 2;
        }
        c = b;
        while b != a {
            b = (b - 1) / 2;
            arr.swap(b, c);
        }
    }

    n -= 1;
    arr.swap(0, n);
    if n == 2 && cmp(&arr[0], &arr[1]) > 0 {
        arr.swap(0, 1);
    }
}

fn sort_u32_slice(arr: &mut [u32]) {
    sort_slice(arr, &mut |x, y| i32::from(x > y) - i32::from(x < y));
}

fn sort_u64_slice(arr: &mut [u64]) {
    sort_slice(arr, &mut |x, y| i32::from(x > y) - i32::from(x < y));
}

fn sort_u8_slice(arr: &mut [u8]) {
    sort_slice(arr, &mut |x, y| i32::from(x > y) - i32::from(x < y));
}

// ---- lib/list_sort.c ---------------------------------------------------

#[derive(Clone)]
struct Node {
    key: u32,
    next: usize,
    prev: usize,
}

/// `merge()`: combine two null-terminated singly-linked runs (stability:
/// takes 'a' on equal keys). `**tail` is emulated by a slot: `None` means
/// the local `head` variable, `Some(i)` means `nodes[i].next`.
fn merge(nodes: &mut [Node], mut a: usize, mut b: usize) -> usize {
    let mut head = NIL;
    let mut slot: Option<usize> = None;
    macro_rules! put {
        ($v:expr) => {
            match slot {
                None => head = $v,
                Some(i) => nodes[i].next = $v,
            }
        };
    }

    loop {
        let na = nodes[a].key;
        let nb = nodes[b].key;
        if na <= nb {
            put!(a);
            slot = Some(a);
            a = nodes[a].next;
            if a == NIL {
                put!(b);
                break;
            }
        } else {
            put!(b);
            slot = Some(b);
            b = nodes[b].next;
            if b == NIL {
                put!(a);
                break;
            }
        }
    }
    head
}

/// `merge_final()`: final merge that also restores prev links and closes
/// the circular list through the sentinel `head` node.
fn merge_final(nodes: &mut [Node], head: usize, mut a: usize, mut b: usize) {
    let mut tail = head;

    loop {
        let na = nodes[a].key;
        let nb = nodes[b].key;
        if na <= nb {
            nodes[tail].next = a;
            nodes[a].prev = tail;
            tail = a;
            a = nodes[a].next;
            if a == NIL {
                break;
            }
        } else {
            nodes[tail].next = b;
            nodes[b].prev = tail;
            tail = b;
            b = nodes[b].next;
            if b == NIL {
                b = a;
                break;
            }
        }
    }

    nodes[tail].next = b;
    while b != NIL {
        nodes[b].prev = tail;
        tail = b;
        b = nodes[b].next;
    }
    nodes[tail].next = head;
    nodes[head].prev = tail;
}

/// `list_sort()` over index-linked nodes; `head` is the sentinel index.
/// `cmp` compares node keys (wrapper supplies cmp_node's semantics).
fn list_sort_idx(nodes: &mut [Node], head: usize) {
    let mut list = nodes[head].next;
    let mut pending = NIL;
    let mut count = 0u64;

    if list == nodes[head].prev {
        return;
    }
    let last = nodes[head].prev;
    nodes[last].next = NIL;

    // A "**tail" slot is either the `pending` variable or node[i].prev.
    #[derive(Clone, Copy)]
    enum Slot {
        Pending,
        Prev(usize),
    }
    fn slot_get(nodes: &[Node], slot: Slot, pending: usize) -> usize {
        match slot {
            Slot::Pending => pending,
            Slot::Prev(i) => nodes[i].prev,
        }
    }
    fn slot_set(nodes: &mut [Node], slot: Slot, pending: &mut usize, v: usize) {
        match slot {
            Slot::Pending => *pending = v,
            Slot::Prev(i) => nodes[i].prev = v,
        }
    }

    loop {
        // Find the least-significant clear bit in count, walking tail down
        // the pending list-of-lists.
        let mut tail = Slot::Pending;
        let mut bits = count;
        while bits & 1 != 0 {
            bits >>= 1;
            tail = Slot::Prev(slot_get(nodes, tail, pending));
        }
        if bits != 0 {
            let a = slot_get(nodes, tail, pending);
            let b = nodes[a].prev;
            let m = merge(nodes, b, a);
            nodes[m].prev = nodes[b].prev;
            slot_set(nodes, tail, &mut pending, m);
        }

        nodes[list].prev = pending;
        pending = list;
        list = nodes[list].next;
        nodes[pending].next = NIL;
        count += 1;
        if list == NIL {
            break;
        }
    }

    list = pending;
    pending = nodes[pending].prev;
    loop {
        let next = nodes[pending].prev;
        if next == NIL {
            break;
        }
        list = merge(nodes, pending, list);
        pending = next;
    }
    merge_final(nodes, head, pending, list);
}

// ---- wrapper functions (mirror wrappers.c) -----------------------------

fn fill32(seed: u64) -> [u32; N] {
    let mut s = seed | 1;
    let mut a = [0u32; N];
    for v in &mut a {
        *v = xorshift(&mut s) as u32;
    }
    a
}

fn idx_mod(idx: i64) -> usize {
    let n = N as i64;
    (idx % n + if idx % n < 0 { n } else { 0 }) as usize
}

/// Port of `uint64_t sort_u32_sum(uint64_t seed)` (`wrappers.c`).
pub fn sort_u32_sum(seed: u64) -> u64 {
    let mut a = fill32(seed);
    sort_u32_slice(&mut a);
    checksum_u32(&a)
}

/// Port of `uint32_t sort_u32_elem(uint64_t seed, int64_t idx)` (`wrappers.c`).
pub fn sort_u32_elem(seed: u64, idx: i64) -> u32 {
    let mut a = fill32(seed);
    sort_u32_slice(&mut a);
    a[idx_mod(idx)]
}

/// Port of `uint64_t sort_r_u32_sum(uint64_t seed)` (`wrappers.c`). The C
/// exercises the cmp_r entry point; `sort_slice` is the shared __sort_r,
/// so the same path is verified.
pub fn sort_r_u32_sum(seed: u64) -> u64 {
    let mut a = fill32(seed);
    sort_u32_slice(&mut a);
    checksum_u32(&a)
}

/// Port of `uint64_t sort_u64_elems(uint64_t seed)` (`wrappers.c`).
pub fn sort_u64_elems(seed: u64) -> u64 {
    let mut s = seed | 1;
    let mut a = [0u64; N];
    for v in &mut a {
        *v = xorshift(&mut s);
    }
    sort_u64_slice(&mut a);
    let mut h = 0x9e3779b97f4a7c15u64;
    for v in &a {
        h = (h ^ *v).wrapping_mul(0x100000001b3);
    }
    h
}

/// Port of `uint64_t sort_u8_elems(uint64_t seed)` (`wrappers.c`).
pub fn sort_u8_elems(seed: u64) -> u64 {
    let mut s = seed | 1;
    let mut a = [0u8; N];
    for v in &mut a {
        *v = xorshift(&mut s) as u8;
    }
    sort_u8_slice(&mut a);
    let mut h = 0x9e3779b97f4a7c15u64;
    for v in &a {
        h = (h ^ *v as u64).wrapping_mul(0x100000001b3);
    }
    h
}

/// Port of `uint64_t sort_u32_partial(uint64_t seed, int64_t n)` (`wrappers.c`).
pub fn sort_u32_partial(seed: u64, n: i64) -> u64 {
    let cnt = ((if n < 0 { n.wrapping_neg() } else { n }) as usize) % (N + 1);
    let mut a = fill32(seed);
    sort_u32_slice(&mut a[..cnt]);
    checksum_u32(&a)
}

fn fill_list(seed: u64, modulus: u32) -> Vec<Node> {
    let mut s = seed | 1;
    let mut nodes = Vec::with_capacity(N + 1);
    let mut keys = Vec::with_capacity(N);
    for _ in 0..N {
        let mut k = xorshift(&mut s) as u32;
        if modulus != 0 {
            k %= modulus;
        }
        keys.push(k);
    }
    // nodes[i].next/prev form a circular list; index N is the sentinel head.
    for (i, &key) in keys.iter().enumerate() {
        nodes.push(Node {
            key,
            next: i + 1,
            prev: if i == 0 { N } else { i - 1 },
        });
    }
    nodes.push(Node {
        key: 0,
        next: 0,
        prev: N - 1,
    });
    nodes[N - 1].next = N; // last element -> sentinel
    nodes
}

fn list_checksum(nodes: &[Node]) -> u64 {
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut p = nodes[N].next;
    while p != N {
        h = (h ^ nodes[p].key as u64).wrapping_mul(0x100000001b3);
        p = nodes[p].next;
    }
    h
}

/// Port of `uint64_t listsort_sum(uint64_t seed)` (`wrappers.c`).
pub fn listsort_sum(seed: u64) -> u64 {
    let mut nodes = fill_list(seed, 0);
    list_sort_idx(&mut nodes, N);
    list_checksum(&nodes)
}

/// Port of `uint64_t listsort_ties_sum(uint64_t seed)` (`wrappers.c`).
/// key%8 collisions: only a STABLE sort produces this checksum.
pub fn listsort_ties_sum(seed: u64) -> u64 {
    let mut nodes = fill_list(seed, 8);
    list_sort_idx(&mut nodes, N);
    list_checksum(&nodes)
}

/// Port of `uint64_t listsort_small(uint64_t seed, int64_t n)` (`wrappers.c`).
/// Drives list_sort on 0..7 elements — covers the empty/singleton early
/// return that the fixed-N wrappers never reach.
pub fn listsort_small(seed: u64, n: i64) -> u64 {
    let cnt = ((if n < 0 { n.wrapping_neg() } else { n }) as usize) % 8;
    let mut s = seed | 1;
    let mut nodes = Vec::with_capacity(cnt + 1);
    for i in 0..cnt {
        nodes.push(Node {
            key: xorshift(&mut s) as u32,
            next: i + 1,
            prev: if i == 0 { cnt } else { i - 1 },
        });
    }
    // Sentinel head at index `cnt`.
    nodes.push(Node {
        key: 0,
        next: 0,
        prev: if cnt == 0 { 0 } else { cnt - 1 },
    });
    if cnt == 0 {
        nodes[0].next = 0;
    } else {
        nodes[cnt - 1].next = cnt;
    }
    list_sort_idx(&mut nodes, cnt);
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut p = nodes[cnt].next;
    while p != cnt {
        h = (h ^ nodes[p].key as u64).wrapping_mul(0x100000001b3);
        p = nodes[p].next;
    }
    h
}

/// Port of `uint32_t listsort_elem(uint64_t seed, int64_t idx)` (`wrappers.c`).
pub fn listsort_elem(seed: u64, idx: i64) -> u32 {
    let mut nodes = fill_list(seed, 0);
    list_sort_idx(&mut nodes, N);
    let mut p = nodes[N].next;
    let mut i = idx_mod(idx);
    while p != N && i > 0 {
        p = nodes[p].next;
        i -= 1;
    }
    nodes[p].key
}
