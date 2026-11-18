//! Stable mergesort for linked lists — port of `lib/list_sort.c`.
//!
//! Verified against the original's recorded behavior through scalar
//! wrappers: `ports/lib-core/rust/core-wrappers-rs` (763/763 cases,
//! gate passed), including the `key % 8` equal-key cases and the
//! `listsort_small` 0..7-element sweeps covering the early-return path.
//!
//! The C operates on `struct list_head` pointers threaded through caller
//! structs. This port works on a caller-provided array of link pairs
//! where each link is an array index (`NIL` = C's NULL); one element is
//! the sentinel `head`. Callers map indices to their own nodes, keeping
//! the same 1:1 link structure the C manipulates.

/// Null link (C's NULL) in the index-linked list model.
pub const NIL: usize = usize::MAX;

/// Index-linked counterpart of `struct list_head`.
#[derive(Clone, Copy)]
pub struct Links {
    pub next: usize,
    pub prev: usize,
}

impl Links {
    pub const fn new(next: usize, prev: usize) -> Self {
        Self { next, prev }
    }
}

/// `merge()`: combine two NIL-terminated singly-linked runs; stability
/// comes from taking 'a' on cmp <= 0. `**tail` is emulated by a slot:
/// `None` = the local `head` variable, `Some(i)` = `links[i].next`.
fn merge(
    links: &mut [Links],
    mut a: usize,
    mut b: usize,
    cmp: &mut dyn FnMut(usize, usize) -> i32,
) -> usize {
    let mut head = NIL;
    let mut slot: Option<usize> = None;
    macro_rules! put {
        ($v:expr) => {
            match slot {
                None => head = $v,
                Some(i) => links[i].next = $v,
            }
        };
    }

    loop {
        if cmp(a, b) <= 0 {
            put!(a);
            slot = Some(a);
            a = links[a].next;
            if a == NIL {
                put!(b);
                break;
            }
        } else {
            put!(b);
            slot = Some(b);
            b = links[b].next;
            if b == NIL {
                put!(a);
                break;
            }
        }
    }
    head
}

/// `merge_final()`: final merge that restores prev links and closes the
/// circular list through the sentinel `head` element.
fn merge_final(
    links: &mut [Links],
    head: usize,
    mut a: usize,
    mut b: usize,
    cmp: &mut dyn FnMut(usize, usize) -> i32,
) {
    let mut tail = head;

    loop {
        if cmp(a, b) <= 0 {
            links[tail].next = a;
            links[a].prev = tail;
            tail = a;
            a = links[a].next;
            if a == NIL {
                break;
            }
        } else {
            links[tail].next = b;
            links[b].prev = tail;
            tail = b;
            b = links[b].next;
            if b == NIL {
                b = a;
                break;
            }
        }
    }

    links[tail].next = b;
    while b != NIL {
        links[b].prev = tail;
        tail = b;
        b = links[b].next;
    }
    links[tail].next = head;
    links[head].prev = tail;
}

/// Port of `list_sort()`: eager balanced mergesort. `links` contains all
/// nodes plus the sentinel `head`; `cmp(i, j)` compares the nodes whose
/// links live at indices i and j (never called on `head` in-contract).
///
/// `cmp` must be antisymmetric and transitive (the same contract upstream
/// documents); a non-total-order comparator yields garbage order but cannot
/// produce out-of-bounds indexing.
///
/// # Contract
///
/// `head` must be a valid index into `links`. The list must be circular and
/// `next`/`prev`-consistent: every `next`/`prev` on the chain reachable from
/// `head` is either a valid index into `links` or (transiently, after
/// `links[last].next` is cut) [`NIL`]. Malformed input — an invalid `head`,
/// a link index outside `links` and `!= NIL`, or an inconsistent list —
/// panics via bounds-checking where the C had UB; a cycle not passing
/// through `head.prev` loops forever, same as the C.
pub fn list_sort(links: &mut [Links], head: usize, cmp: &mut dyn FnMut(usize, usize) -> i32) {
    assert!(head < links.len(), "list_sort: head index out of bounds");

    let mut list = links[head].next;
    let mut pending = NIL;
    let mut count = 0u64;

    debug_assert!(
        (0..links.len()).contains(&list),
        "list_sort: head.next is not a valid link index"
    );

    if list == links[head].prev {
        return;
    }
    let last = links[head].prev;
    links[last].next = NIL;

    // A "**tail" slot is either the `pending` variable or links[i].prev.
    #[derive(Clone, Copy)]
    enum Slot {
        Pending,
        Prev(usize),
    }
    fn slot_get(links: &[Links], slot: Slot, pending: usize) -> usize {
        match slot {
            Slot::Pending => pending,
            Slot::Prev(i) => links[i].prev,
        }
    }
    fn slot_set(links: &mut [Links], slot: Slot, pending: &mut usize, v: usize) {
        match slot {
            Slot::Pending => *pending = v,
            Slot::Prev(i) => links[i].prev = v,
        }
    }

    loop {
        // Find the least-significant clear bit in count, walking tail
        // down the pending list-of-lists.
        let mut tail = Slot::Pending;
        let mut bits = count;
        while bits & 1 != 0 {
            bits >>= 1;
            tail = Slot::Prev(slot_get(links, tail, pending));
        }
        if bits != 0 {
            let a = slot_get(links, tail, pending);
            let b = links[a].prev;
            let m = merge(links, b, a, cmp);
            links[m].prev = links[b].prev;
            slot_set(links, tail, &mut pending, m);
        }

        links[list].prev = pending;
        pending = list;
        list = links[list].next;
        links[pending].next = NIL;
        count += 1;
        if list == NIL {
            break;
        }
    }

    list = pending;
    pending = links[pending].prev;
    loop {
        let next = links[pending].prev;
        if next == NIL {
            break;
        }
        list = merge(links, pending, list, cmp);
        pending = next;
    }
    merge_final(links, head, pending, list, cmp);
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 64;

    /// xorshift64 fill identical to the recorded C harness.
    fn fill(seed: u64, modulus: u32) -> ([u32; N], [Links; N + 1]) {
        let mut s = seed | 1;
        let mut keys = [0u32; N];
        for k in &mut keys {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *k = s as u32;
            if modulus != 0 {
                *k %= modulus;
            }
        }
        let mut links = [Links::new(NIL, NIL); N + 1];
        for (i, l) in links.iter_mut().take(N).enumerate() {
            l.next = i + 1;
            l.prev = if i == 0 { N } else { i - 1 };
        }
        links[N - 1].next = N;
        links[N] = Links::new(0, N - 1);
        (keys, links)
    }

    fn checksum(keys: &[u32; N], links: &[Links; N + 1]) -> u64 {
        let mut h = 0x9e3779b97f4a7c15u64;
        let mut p = links[N].next;
        while p != N {
            h = (h ^ keys[p] as u64).wrapping_mul(0x100000001b3);
            p = links[p].next;
        }
        h
    }

    #[test]
    fn matches_recorded_seeds() {
        // Checksums observed from the recorded C harness (seed 1).
        let (keys, mut links) = fill(1, 0);
        let mut cmp = |a: usize, b: usize| {
            i32::from(keys[a] > keys[b]) - i32::from(keys[a] < keys[b])
        };
        list_sort(&mut links, N, &mut cmp);
        assert_eq!(checksum(&keys, &links), 0x2dfe02ef4f75134f);
    }

    #[test]
    fn sorts_on_ties() {
        // key%8 collisions — recorded C checksum; exercises the equal-key
        // merge path (cmp <= 0 takes 'a').
        let (keys, mut links) = fill(1, 8);
        let mut cmp = |a: usize, b: usize| {
            i32::from(keys[a] > keys[b]) - i32::from(keys[a] < keys[b])
        };
        list_sort(&mut links, N, &mut cmp);
        assert_eq!(checksum(&keys, &links), 0xc470477e5636a293);
    }

    #[test]
    fn agrees_with_array_sort() {
        // list_sort and sort produce the same order for the same input.
        let (keys, mut links) = fill(42, 0);
        let mut cmp = |a: usize, b: usize| {
            i32::from(keys[a] > keys[b]) - i32::from(keys[a] < keys[b])
        };
        list_sort(&mut links, N, &mut cmp);
        let mut arr = keys;
        crate::sort::sort(&mut arr, |x, y| i32::from(x > y) - i32::from(x < y));
        let mut p = links[N].next;
        let mut i = 0;
        while p != N {
            assert_eq!(keys[p], arr[i]);
            p = links[p].next;
            i += 1;
        }
        assert_eq!(i, N);
    }
}
