//! Lockless singly-linked list — port of `lib/llist.c` +
//! `include/linux/llist.h`.
//!
//! `struct llist_node` chains are modelled as indices into a
//! caller-owned `next` slice (`next[i]` is `node[i].next`) and
//! `NULL` is [`NIL`]. `struct llist_head` is [`LlistHead`].
//!
//! The atomic ops (`try_cmpxchg`, `xchg`, `READ_ONCE`) are sequential:
//! on a single thread each cmpxchg succeeds on the first pass, so the
//! C do/while retry loops collapse to one iteration. Real lockless
//! concurrency needs kernel atomics — this models the algorithm, not
//! the memory ordering.

/// `NULL` in the index model.
pub const NIL: usize = usize::MAX;

/// `struct llist_head` — the list anchor. `first` is an index into the
/// caller's `next` slice, or `NIL` when empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct LlistHead {
    pub first: usize,
}

impl LlistHead {
    /// `INIT_LLIST_HEAD` — an empty head.
    pub const fn new() -> Self {
        LlistHead { first: NIL }
    }
}

/// `init_llist_node`: `WRITE_ONCE(node->next, node)` — the self-pointer
/// marks the node as not on any list.
pub fn init_llist_node(next: &mut [usize], node: usize) {
    next[node] = node;
}

/// `llist_on_list`: true while `node->next != node` — i.e. the node has
/// been linked (or was never initialised). C notes this is only
/// meaningful on initialised nodes.
pub fn llist_on_list(next: &[usize], node: usize) -> bool {
    next[node] != node
}

/// `llist_empty`: `READ_ONCE(head->first) == NULL`.
pub fn llist_empty(head: &LlistHead) -> bool {
    head.first == NIL
}

/// `llist_next`: `READ_ONCE(node->next)` — index of the successor or NIL.
pub fn llist_next(next: &[usize], node: usize) -> usize {
    next[node]
}

/// `llist_del_first`: pop the head's first node. Returns its index or
/// NIL when the list was empty.
pub fn llist_del_first(head: &mut LlistHead, next: &mut [usize]) -> usize {
    // smp_load_acquire(&head->first)
    let entry = head.first;
    if entry == NIL {
        return NIL;
    }
    let succ = next[entry]; // READ_ONCE(entry->next)
    head.first = succ; // try_cmpxchg(&head->first, &entry, next) == true
    entry
}

/// `llist_del_first_this`: pop the head only if it is `this`.
pub fn llist_del_first_this(head: &mut LlistHead, next: &mut [usize], this: usize) -> bool {
    let entry = head.first;
    if entry != this {
        return false;
    }
    let succ = next[entry];
    head.first = succ;
    true
}

/// `llist_del_first_init`: del_first + init_llist_node on the result,
/// so a returned node reads as off-list again.
pub fn llist_del_first_init(head: &mut LlistHead, next: &mut [usize]) -> usize {
    let node = llist_del_first(head, next);
    if node != NIL {
        init_llist_node(next, node);
    }
    node
}

/// `llist_del_all` (`xchg(&head->first, NULL)`): detach the whole chain.
/// Returns the old first index (a valid chain to walk) or NIL.
pub fn llist_del_all(head: &mut LlistHead) -> usize {
    let old = head.first;
    head.first = NIL;
    old
}

/// `__llist_del_all`: the non-atomic twin — identical sequentially.
pub fn __llist_del_all(head: &mut LlistHead) -> usize {
    let first = head.first;
    head.first = NIL;
    first
}

/// `llist_reverse_order`: reverse the chain rooted at `first`
/// (typically the result of `llist_del_all`). Returns the new head.
pub fn llist_reverse_order(next: &mut [usize], mut first: usize) -> usize {
    let mut new_head = NIL;
    while first != NIL {
        let tmp = first;
        first = next[first];
        next[tmp] = new_head;
        new_head = tmp;
    }
    new_head
}

/// `llist_add_batch`: link the chain `new_first..new_last` at the head.
/// Returns true iff the list was empty.
pub fn llist_add_batch(
    head: &mut LlistHead,
    next: &mut [usize],
    new_first: usize,
    new_last: usize,
) -> bool {
    let first = head.first; // READ_ONCE(head->first)
    next[new_last] = first;
    head.first = new_first; // try_cmpxchg succeeds
    first == NIL
}

/// `__llist_add_batch`: non-atomic twin.
pub fn __llist_add_batch(
    head: &mut LlistHead,
    next: &mut [usize],
    new_first: usize,
    new_last: usize,
) -> bool {
    next[new_last] = head.first;
    head.first = new_first;
    next[new_last] == NIL
}

/// `llist_add`: push `node` at the head. Returns true iff the list was
/// empty (used to decide whether to wake a consumer).
pub fn llist_add(head: &mut LlistHead, next: &mut [usize], node: usize) -> bool {
    llist_add_batch(head, next, node, node)
}

/// `__llist_add`: non-atomic twin.
pub fn __llist_add(head: &mut LlistHead, next: &mut [usize], node: usize) -> bool {
    __llist_add_batch(head, next, node, node)
}

#[cfg(test)]
mod tests {
    include!("llist_tests.inc");
}
