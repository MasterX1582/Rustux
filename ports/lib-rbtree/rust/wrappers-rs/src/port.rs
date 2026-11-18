//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/rbtree.c and the include/linux/{rbtree,rbtree_types,
//! rbtree_augmented}.h inline layer, driven through the same seeded index-pool
//! harness as the C wrappers.  Kernel intrusive structures become index-linked
//! arrays: `struct rb_node *` is a `usize` pool index and NULL is `NIL`
//! (`usize::MAX`; folded into checksums as 0xffff, like the C `NILX`).
//!
//! Node representation mirrors the C packing exactly: `pc` is
//! `enc(parent) + color` with `enc(i) = (i + 1) << 2` and `enc(NIL) = 0`, so
//! `pc & !3` decodes the parent index and `pc & 1` is the color — the same
//! arithmetic the C applies to `__rb_parent_color` (where the pointer's low
//! two bits carry the color).  `RB_CLEAR_NODE`'s "pc == own address" becomes
//! `pc == self_enc(i)` (`u64::MAX - i`, unreachable by any real encoding).
//!
//! `Rig` is the harness "address space": the rb_node array plus the payload
//! arrays (key/val/aug/present, and the rb_node_linked prev/next/key/present
//! slots) that C reaches through rb_entry()/container_of().  Augmented
//! callbacks are a struct of `fn(&mut Rig, usize, usize)` pointers — the same
//! shape as `struct rb_augment_callbacks`; the subtree-max augmentation
//! mirrors `RB_DECLARE_CALLBACKS_MAX` exactly (compute folds own val with
//! child aggs via max, propagate walks to the root, copy/rotate/merge as the
//! macro expands them).

#![forbid(unsafe_code)]
// pub fn names mirror the wrappers.c export names verbatim.
#![allow(non_snake_case)]

const N: usize = 64;
const NIL: usize = usize::MAX;
const NILX: u64 = 0xffff;
const KEYSPACE: u64 = 3 * N as u64;

const RB_RED: u64 = 0;
const RB_BLACK: u64 = 1;

// ---- node / harness state ---------------------------------------------------

/// `struct rb_node`: `pc` packs parent index + color like C packs the
/// pointer's low bits: pc = enc(parent) + color, enc(i) = (i+1)<<2.
#[derive(Clone, Copy)]
struct RbNode {
    pc: u64,
    right: usize,
    left: usize,
}

/// `struct rb_root` — index of the root node (NIL when empty).
struct RbRoot {
    rb_node: usize,
}
/// `struct rb_root_cached`.
struct RbRootCached {
    root: RbRoot,
    rb_leftmost: usize,
}
/// `struct rb_root_linked` (leftmost is an index into the linked pool).
struct RbRootLinked {
    root: RbRoot,
    rb_leftmost: usize,
}

/// The harness pools: `n[]` is the rb_node pool shared by the plain/cached/
/// augmented/linked trees (never live simultaneously, like the C statics).
/// `key`/`val`/`aug`/`present` are the `struct tnode` payload reached via
/// rb_entry in C; `lkey`/`lprev`/`lnext`/`lpresent` are `struct ltnode`'s.
struct Rig {
    n: [RbNode; N],
    key: [u64; N],
    val: [u64; N],
    aug: [u64; N],
    present: [bool; N],
    lkey: [u64; N],
    lprev: [usize; N],
    lnext: [usize; N],
    lpresent: [bool; N],
}

/// C `reset_all()`: the pools are memset to 0 — pc == 0 (NULL parent, red),
/// child pointers NULL (NIL here), linked prev/next NULL.
fn rig_new() -> Rig {
    Rig {
        n: [RbNode {
            pc: 0,
            right: NIL,
            left: NIL,
        }; N],
        key: [0; N],
        val: [0; N],
        aug: [0; N],
        present: [false; N],
        lkey: [0; N],
        lprev: [NIL; N],
        lnext: [NIL; N],
        lpresent: [false; N],
    }
}

// ---- packed parent/color helpers (rbtree.h / rbtree_augmented.h) ------------

fn enc(i: usize) -> u64 {
    if i == NIL { 0 } else { ((i as u64) + 1) << 2 }
}
fn dec(e: u64) -> usize {
    if e == 0 { NIL } else { (e >> 2) as usize - 1 }
}
/// Stands in for "the node's own address" in RB_EMPTY_NODE/RB_CLEAR_NODE;
/// unreachable by any real encoding (those are <= 4*N + 1).
fn self_enc(i: usize) -> u64 {
    u64::MAX - i as u64
}

/// `rb_parent(r)`: (pc & ~3) decoded.
fn rb_parent(r: &Rig, i: usize) -> usize {
    dec(r.n[i].pc & !3u64)
}
/// `__rb_parent(pc)`.
fn __rb_parent(pc: u64) -> usize {
    dec(pc & !3u64)
}
/// `__rb_is_black(pc)` — works on a packed pc word.
fn __rb_is_black(pc: u64) -> bool {
    pc & 1 != 0
}
fn rb_is_black(r: &Rig, i: usize) -> bool {
    __rb_is_black(r.n[i].pc)
}
fn rb_is_red(r: &Rig, i: usize) -> bool {
    r.n[i].pc & 1 == 0
}
/// `rb_red_parent`: the unmasked pc — callers guarantee the node is red,
/// so the low bits are 0 and the whole word is the encoded parent.
fn rb_red_parent(r: &Rig, i: usize) -> usize {
    dec(r.n[i].pc)
}
fn rb_set_parent(r: &mut Rig, i: usize, p: usize) {
    r.n[i].pc = (r.n[i].pc & 1) + enc(p);
}
fn rb_set_parent_color(r: &mut Rig, i: usize, p: usize, color: u64) {
    r.n[i].pc = enc(p) + color;
}
fn rb_set_black(r: &mut Rig, i: usize) {
    r.n[i].pc += RB_BLACK;
}
fn rb_empty_node(r: &Rig, i: usize) -> bool {
    r.n[i].pc == self_enc(i)
}
fn rb_clear_node(r: &mut Rig, i: usize) {
    r.n[i].pc = self_enc(i);
}

fn __rb_change_child(r: &mut Rig, old: usize, new: usize, parent: usize, root: &mut RbRoot) {
    if parent != NIL {
        if r.n[parent].left == old {
            r.n[parent].left = new;
        } else {
            r.n[parent].right = new;
        }
    } else {
        root.rb_node = new;
    }
}
/// `__rb_change_child_rcu`: identical — rcu_assign_pointer is a plain
/// assignment in the shim.
fn __rb_change_child_rcu(r: &mut Rig, old: usize, new: usize, parent: usize, root: &mut RbRoot) {
    __rb_change_child(r, old, new, parent, root)
}

/// `rb_link_node(node, parent, rb_link)`: `Link` stands in for `struct
/// rb_node **` — Root is &root->rb_node, Left(i)/Right(i) are the child
/// slots of node i.
#[derive(Clone, Copy)]
enum Link {
    Root,
    Left(usize),
    Right(usize),
}
fn link_get(r: &Rig, root: &RbRoot, l: Link) -> usize {
    match l {
        Link::Root => root.rb_node,
        Link::Left(i) => r.n[i].left,
        Link::Right(i) => r.n[i].right,
    }
}
fn link_set(r: &mut Rig, root: &mut RbRoot, l: Link, v: usize) {
    match l {
        Link::Root => root.rb_node = v,
        Link::Left(i) => r.n[i].left = v,
        Link::Right(i) => r.n[i].right = v,
    }
}
fn rb_link_node(r: &mut Rig, node: usize, parent: usize, link: Link, root: &mut RbRoot) {
    r.n[node].pc = enc(parent);
    r.n[node].left = NIL;
    r.n[node].right = NIL;
    link_set(r, root, link, node);
}
/// `rb_link_node_rcu`: identical — rcu_assign_pointer is a plain assignment.
fn rb_link_node_rcu(r: &mut Rig, node: usize, parent: usize, link: Link, root: &mut RbRoot) {
    rb_link_node(r, node, parent, link, root)
}

// ---- augmented callbacks (`struct rb_augment_callbacks`) --------------------

type AugFn = fn(&mut Rig, usize, usize);

struct Augment {
    propagate: AugFn,
    copy: AugFn,
    rotate: AugFn,
    merge: AugFn,
}

fn dummy_propagate(_r: &mut Rig, _a: usize, _b: usize) {}
fn dummy_copy(_r: &mut Rig, _a: usize, _b: usize) {}
fn dummy_rotate(_r: &mut Rig, _a: usize, _b: usize) {}
fn dummy_merge(_r: &mut Rig, _a: usize, _b: usize) {}

const DUMMY: Augment = Augment {
    propagate: dummy_propagate,
    copy: dummy_copy,
    rotate: dummy_rotate,
    merge: dummy_merge,
};

/// RB_DECLARE_CALLBACKS_MAX expansion for `aug` = max(val) over the subtree.
fn aug_compute(r: &mut Rig, i: usize, exit: bool) -> bool {
    let old_aug = r.aug[i];
    let mut val = r.val[i];
    if r.n[i].right != NIL {
        val = val.max(r.aug[r.n[i].right]);
    }
    if r.n[i].left != NIL {
        val = val.max(r.aug[r.n[i].left]);
    }
    if exit && old_aug == val {
        return true;
    }
    r.aug[i] = val;
    false
}
fn aug_propagate(r: &mut Rig, rb: usize, stop: usize) {
    let mut rb = rb;
    while rb != stop {
        if aug_compute(r, rb, true) {
            break;
        }
        rb = rb_parent(r, rb);
    }
}
fn aug_copy(r: &mut Rig, old: usize, new: usize) {
    r.aug[new] = r.aug[old];
}
fn aug_rotate(r: &mut Rig, old: usize, new: usize) {
    aug_copy(r, old, new);
    aug_compute(r, old, false);
}
fn aug_merge(r: &mut Rig, node: usize, new: usize) {
    r.aug[node] = r.aug[node].max(r.aug[new]);
}

const AUG_CB: Augment = Augment {
    propagate: aug_propagate,
    copy: aug_copy,
    rotate: aug_rotate,
    merge: aug_merge,
};

// ---- lib/rbtree.c -----------------------------------------------------------

/// `__rb_rotate_set_parents`: old's parent+color go to new; old gets new as
/// parent and `color` as color.
fn __rb_rotate_set_parents(r: &mut Rig, old: usize, new: usize, root: &mut RbRoot, color: u64) {
    let parent = rb_parent(r, old);
    r.n[new].pc = r.n[old].pc;
    rb_set_parent_color(r, old, new, color);
    __rb_change_child(r, old, new, parent, root);
}

/// `__rb_insert` — the rebalance loop; `augment_rotate` is the C callback
/// (dummy_rotate for the non-augmented instantiation).
fn __rb_insert(r: &mut Rig, node0: usize, root: &mut RbRoot, augment_rotate: AugFn) {
    let mut node = node0;
    let mut parent = rb_red_parent(r, node);

    loop {
        // Loop invariant: node is red.
        if parent == NIL {
            rb_set_parent_color(r, node, NIL, RB_BLACK);
            break;
        }

        if rb_is_black(r, parent) {
            break;
        }

        let gparent = rb_red_parent(r, parent);

        let mut tmp = r.n[gparent].right;
        if parent != tmp {
            // parent == gparent->rb_left
            if tmp != NIL && rb_is_red(r, tmp) {
                // Case 1 - color flips; recurse at gparent.
                rb_set_parent_color(r, tmp, gparent, RB_BLACK);
                rb_set_parent_color(r, parent, gparent, RB_BLACK);
                node = gparent;
                parent = rb_parent(r, node);
                rb_set_parent_color(r, node, parent, RB_RED);
                continue;
            }

            tmp = r.n[parent].right;
            if node == tmp {
                // Case 2 - left rotate at parent.
                tmp = r.n[node].left;
                r.n[parent].right = tmp;
                r.n[node].left = parent;
                if tmp != NIL {
                    rb_set_parent_color(r, tmp, parent, RB_BLACK);
                }
                rb_set_parent_color(r, parent, node, RB_RED);
                augment_rotate(r, parent, node);
                parent = node;
                tmp = r.n[node].right;
            }

            // Case 3 - right rotate at gparent.
            r.n[gparent].left = tmp;
            r.n[parent].right = gparent;
            if tmp != NIL {
                rb_set_parent_color(r, tmp, gparent, RB_BLACK);
            }
            __rb_rotate_set_parents(r, gparent, parent, root, RB_RED);
            augment_rotate(r, gparent, parent);
            break;
        } else {
            tmp = r.n[gparent].left;
            if tmp != NIL && rb_is_red(r, tmp) {
                // Case 1 - color flips.
                rb_set_parent_color(r, tmp, gparent, RB_BLACK);
                rb_set_parent_color(r, parent, gparent, RB_BLACK);
                node = gparent;
                parent = rb_parent(r, node);
                rb_set_parent_color(r, node, parent, RB_RED);
                continue;
            }

            tmp = r.n[parent].left;
            if node == tmp {
                // Case 2 - right rotate at parent.
                tmp = r.n[node].right;
                r.n[parent].left = tmp;
                r.n[node].right = parent;
                if tmp != NIL {
                    rb_set_parent_color(r, tmp, parent, RB_BLACK);
                }
                rb_set_parent_color(r, parent, node, RB_RED);
                augment_rotate(r, parent, node);
                parent = node;
                tmp = r.n[node].left;
            }

            // Case 3 - left rotate at gparent.
            r.n[gparent].right = tmp;
            r.n[parent].left = gparent;
            if tmp != NIL {
                rb_set_parent_color(r, tmp, gparent, RB_BLACK);
            }
            __rb_rotate_set_parents(r, gparent, parent, root, RB_RED);
            augment_rotate(r, gparent, parent);
            break;
        }
    }
}

/// `____rb_erase_color` — the rebalance-after-erase loop; `node` is the
/// black (or initially "empty") child position under `parent`.
fn ____rb_erase_color(r: &mut Rig, mut parent: usize, root: &mut RbRoot, augment_rotate: AugFn) {
    let mut node = NIL;

    loop {
        // Loop invariants (as in C): node is black (or "NULL" first
        // iteration), node is not the root, and leaf paths through
        // parent+node are one black short.
        let mut sibling = r.n[parent].right;
        if node != sibling {
            // node == parent->rb_left
            if rb_is_red(r, sibling) {
                // Case 1 - left rotate at parent.
                let tmp1 = r.n[sibling].left;
                r.n[parent].right = tmp1;
                r.n[sibling].left = parent;
                rb_set_parent_color(r, tmp1, parent, RB_BLACK);
                __rb_rotate_set_parents(r, parent, sibling, root, RB_RED);
                augment_rotate(r, parent, sibling);
                sibling = tmp1;
            }
            let mut tmp1 = r.n[sibling].right;
            if tmp1 == NIL || rb_is_black(r, tmp1) {
                let tmp2 = r.n[sibling].left;
                if tmp2 == NIL || rb_is_black(r, tmp2) {
                    // Case 2 - sibling color flip.
                    rb_set_parent_color(r, sibling, parent, RB_RED);
                    if rb_is_red(r, parent) {
                        rb_set_black(r, parent);
                    } else {
                        node = parent;
                        parent = rb_parent(r, node);
                        if parent != NIL {
                            continue;
                        }
                    }
                    break;
                }
                // Case 3 - right rotate at sibling.
                tmp1 = r.n[tmp2].right;
                r.n[sibling].left = tmp1;
                r.n[tmp2].right = sibling;
                r.n[parent].right = tmp2;
                if tmp1 != NIL {
                    rb_set_parent_color(r, tmp1, sibling, RB_BLACK);
                }
                augment_rotate(r, sibling, tmp2);
                tmp1 = sibling;
                sibling = tmp2;
            }
            // Case 4 - left rotate at parent + color flips.
            let tmp2 = r.n[sibling].left;
            r.n[parent].right = tmp2;
            r.n[sibling].left = parent;
            rb_set_parent_color(r, tmp1, sibling, RB_BLACK);
            if tmp2 != NIL {
                rb_set_parent(r, tmp2, parent);
            }
            __rb_rotate_set_parents(r, parent, sibling, root, RB_BLACK);
            augment_rotate(r, parent, sibling);
            break;
        } else {
            sibling = r.n[parent].left;
            if rb_is_red(r, sibling) {
                // Case 1 - right rotate at parent.
                let tmp1 = r.n[sibling].right;
                r.n[parent].left = tmp1;
                r.n[sibling].right = parent;
                rb_set_parent_color(r, tmp1, parent, RB_BLACK);
                __rb_rotate_set_parents(r, parent, sibling, root, RB_RED);
                augment_rotate(r, parent, sibling);
                sibling = tmp1;
            }
            let mut tmp1 = r.n[sibling].left;
            if tmp1 == NIL || rb_is_black(r, tmp1) {
                let tmp2 = r.n[sibling].right;
                if tmp2 == NIL || rb_is_black(r, tmp2) {
                    // Case 2 - sibling color flip.
                    rb_set_parent_color(r, sibling, parent, RB_RED);
                    if rb_is_red(r, parent) {
                        rb_set_black(r, parent);
                    } else {
                        node = parent;
                        parent = rb_parent(r, node);
                        if parent != NIL {
                            continue;
                        }
                    }
                    break;
                }
                // Case 3 - left rotate at sibling.
                tmp1 = r.n[tmp2].left;
                r.n[sibling].right = tmp1;
                r.n[tmp2].left = sibling;
                r.n[parent].left = tmp2;
                if tmp1 != NIL {
                    rb_set_parent_color(r, tmp1, sibling, RB_BLACK);
                }
                augment_rotate(r, sibling, tmp2);
                tmp1 = sibling;
                sibling = tmp2;
            }
            // Case 4 - right rotate at parent + color flips.
            let tmp2 = r.n[sibling].right;
            r.n[parent].left = tmp2;
            r.n[sibling].right = parent;
            rb_set_parent_color(r, tmp1, sibling, RB_BLACK);
            if tmp2 != NIL {
                rb_set_parent(r, tmp2, parent);
            }
            __rb_rotate_set_parents(r, parent, sibling, root, RB_BLACK);
            augment_rotate(r, parent, sibling);
            break;
        }
    }
}

/// `__rb_erase_color` — non-inline version for rb_erase_augmented() use.
fn __rb_erase_color(r: &mut Rig, parent: usize, root: &mut RbRoot, augment_rotate: AugFn) {
    ____rb_erase_color(r, parent, root, augment_rotate);
}

/// `__rb_erase_augmented` — detach `node`, repair locally, and return the
/// parent under which ____rb_erase_color must run (NIL = no rebalance).
/// `tmp` tracks the deepest changed position for augment->propagate.
fn __rb_erase_augmented(r: &mut Rig, node: usize, root: &mut RbRoot, augment: &Augment) -> usize {
    let child = r.n[node].right;
    let tmp_l = r.n[node].left;
    let mut parent;
    let rebalance;
    let tmp;
    let pc: u64;

    if tmp_l == NIL {
        // Case 1: no more than 1 child.
        pc = r.n[node].pc;
        parent = __rb_parent(pc);
        __rb_change_child(r, node, child, parent, root);
        if child != NIL {
            r.n[child].pc = pc;
            rebalance = NIL;
        } else {
            rebalance = if __rb_is_black(pc) { parent } else { NIL };
        }
        tmp = parent;
    } else if child == NIL {
        // Still case 1, but this time the child is node->rb_left.
        pc = r.n[node].pc;
        r.n[tmp_l].pc = pc;
        parent = __rb_parent(pc);
        __rb_change_child(r, node, tmp_l, parent, root);
        rebalance = NIL;
        tmp = parent;
    } else {
        let mut successor = child;
        let child2: usize;
        let mut t = r.n[child].left;
        if t == NIL {
            // Case 2: node's successor is its right child.
            parent = successor;
            child2 = r.n[successor].right;
            (augment.copy)(r, node, successor);
        } else {
            // Case 3: node's successor is leftmost under node's right
            // child subtree.
            loop {
                parent = successor;
                successor = t;
                t = r.n[t].left;
                if t == NIL {
                    break;
                }
            }
            child2 = r.n[successor].right;
            r.n[parent].left = child2; // WRITE_ONCE
            r.n[successor].right = child; // WRITE_ONCE
            rb_set_parent(r, child, successor);

            (augment.copy)(r, node, successor);
            (augment.propagate)(r, parent, successor);
        }

        let l = r.n[node].left;
        r.n[successor].left = l; // WRITE_ONCE
        rb_set_parent(r, l, successor);

        pc = r.n[node].pc;
        let par = __rb_parent(pc);
        __rb_change_child(r, node, successor, par, root);

        if child2 != NIL {
            rb_set_parent_color(r, child2, parent, RB_BLACK);
            rebalance = NIL;
        } else {
            rebalance = if rb_is_black(r, successor) {
                parent
            } else {
                NIL
            };
        }
        r.n[successor].pc = pc;
        tmp = successor;
    }

    (augment.propagate)(r, tmp, NIL);
    rebalance
}

/// `rb_insert_color` — dummy augmented callbacks (compiled out in C).
fn rb_insert_color(r: &mut Rig, node: usize, root: &mut RbRoot) {
    __rb_insert(r, node, root, dummy_rotate);
}

fn rb_erase(r: &mut Rig, node: usize, root: &mut RbRoot) {
    let rebalance = __rb_erase_augmented(r, node, root, &DUMMY);
    if rebalance != NIL {
        ____rb_erase_color(r, rebalance, root, dummy_rotate);
    }
}

/// `rb_erase_linked` — unlink from the list, erase from the tree, clear;
/// returns "list still non-empty".
fn rb_erase_linked(r: &mut Rig, i: usize, root: &mut RbRootLinked) -> bool {
    let p = r.lprev[i];
    let x = r.lnext[i];
    if p != NIL {
        r.lnext[p] = x;
    } else {
        root.rb_leftmost = x;
    }
    if x != NIL {
        r.lprev[x] = p;
    }
    rb_erase(r, i, &mut root.root);
    // RB_CLEAR_LINKED_NODE
    rb_clear_node(r, i);
    r.lprev[i] = NIL;
    r.lnext[i] = NIL;
    root.rb_leftmost != NIL
}
fn rb_empty_linked_node(r: &Rig, i: usize) -> bool {
    rb_empty_node(r, i)
}

fn rb_insert_augmented(r: &mut Rig, node: usize, root: &mut RbRoot, augment: &Augment) {
    __rb_insert(r, node, root, augment.rotate);
}
fn rb_insert_augmented_cached(
    r: &mut Rig,
    node: usize,
    root: &mut RbRootCached,
    newleft: bool,
    augment: &Augment,
) {
    if newleft {
        root.rb_leftmost = node;
    }
    rb_insert_augmented(r, node, &mut root.root, augment);
}
fn rb_erase_augmented(r: &mut Rig, node: usize, root: &mut RbRoot, augment: &Augment) {
    let rebalance = __rb_erase_augmented(r, node, root, augment);
    if rebalance != NIL {
        __rb_erase_color(r, rebalance, root, augment.rotate);
    }
}
fn rb_erase_augmented_cached(r: &mut Rig, node: usize, root: &mut RbRootCached, augment: &Augment) {
    if root.rb_leftmost == node {
        root.rb_leftmost = rb_next(r, node);
    }
    rb_erase_augmented(r, node, &mut root.root, augment);
}

/// `rb_add_augmented_cached`: merge the new node's aug into every ancestor
/// on the way down, then insert like rb_add_cached.
fn rb_add_augmented_cached(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRootCached,
    less: fn(&Rig, usize, usize) -> bool,
    augment: &Augment,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;
    let mut leftmost = true;

    loop {
        let cur = link_get(r, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        (augment.merge)(r, parent, node);
        if less(r, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
            leftmost = false;
        }
    }

    rb_link_node(r, node, parent, link, &mut tree.root);
    rb_insert_augmented_cached(r, node, tree, leftmost, augment);

    if leftmost { node } else { NIL }
}

fn rb_next(r: &Rig, node: usize) -> usize {
    if rb_empty_node(r, node) {
        return NIL;
    }
    let mut node = node;
    // Right-hand child: go down then left as far as possible.
    if r.n[node].right != NIL {
        node = r.n[node].right;
        while r.n[node].left != NIL {
            node = r.n[node].left;
        }
        return node;
    }
    // No right child: first ancestor for which node is a left child.
    let mut parent;
    loop {
        parent = rb_parent(r, node);
        if parent != NIL && node == r.n[parent].right {
            node = parent;
        } else {
            break;
        }
    }
    parent
}

fn rb_prev(r: &Rig, node: usize) -> usize {
    if rb_empty_node(r, node) {
        return NIL;
    }
    let mut node = node;
    // Left-hand child: go down then right as far as possible.
    if r.n[node].left != NIL {
        node = r.n[node].left;
        while r.n[node].right != NIL {
            node = r.n[node].right;
        }
        return node;
    }
    // No left child: first ancestor for which node is a right child.
    let mut parent;
    loop {
        parent = rb_parent(r, node);
        if parent != NIL && node == r.n[parent].left {
            node = parent;
        } else {
            break;
        }
    }
    parent
}

fn rb_first(r: &Rig, root: &RbRoot) -> usize {
    let mut n = root.rb_node;
    if n == NIL {
        return NIL;
    }
    while r.n[n].left != NIL {
        n = r.n[n].left;
    }
    n
}

fn rb_last(r: &Rig, root: &RbRoot) -> usize {
    let mut n = root.rb_node;
    if n == NIL {
        return NIL;
    }
    while r.n[n].right != NIL {
        n = r.n[n].right;
    }
    n
}

fn rb_replace_node(r: &mut Rig, victim: usize, new: usize, root: &mut RbRoot) {
    let parent = rb_parent(r, victim);

    // Copy the pointers/colour from the victim to the replacement.
    r.n[new] = r.n[victim];

    // Set the surrounding nodes to point to the replacement.
    if r.n[victim].left != NIL {
        rb_set_parent(r, r.n[victim].left, new);
    }
    if r.n[victim].right != NIL {
        rb_set_parent(r, r.n[victim].right, new);
    }
    __rb_change_child(r, victim, new, parent, root);
}

fn rb_replace_node_rcu(r: &mut Rig, victim: usize, new: usize, root: &mut RbRoot) {
    let parent = rb_parent(r, victim);

    r.n[new] = r.n[victim];

    if r.n[victim].left != NIL {
        rb_set_parent(r, r.n[victim].left, new);
    }
    if r.n[victim].right != NIL {
        rb_set_parent(r, r.n[victim].right, new);
    }
    // __rb_change_child_rcu: same assignment, RCU ordering is a no-op here.
    __rb_change_child_rcu(r, victim, new, parent, root);
}

fn rb_left_deepest_node(r: &Rig, node: usize) -> usize {
    let mut node = node;
    loop {
        if r.n[node].left != NIL {
            node = r.n[node].left;
        } else if r.n[node].right != NIL {
            node = r.n[node].right;
        } else {
            return node;
        }
    }
}

fn rb_next_postorder(r: &Rig, node: usize) -> usize {
    if node == NIL {
        return NIL;
    }
    let parent = rb_parent(r, node);
    // If we're the parent's left node and it has a right node, go to that
    // then all the way down to the left; otherwise the parent is next.
    if parent != NIL && node == r.n[parent].left && r.n[parent].right != NIL {
        rb_left_deepest_node(r, r.n[parent].right)
    } else {
        parent
    }
}

fn rb_first_postorder(r: &Rig, root: &RbRoot) -> usize {
    if root.rb_node == NIL {
        return NIL;
    }
    rb_left_deepest_node(r, root.rb_node)
}

// ---- cached / linked / find-add inlines (rbtree.h / rbtree_augmented.h) ----

fn rb_first_cached(root: &RbRootCached) -> usize {
    root.rb_leftmost
}

fn rb_insert_color_cached(r: &mut Rig, node: usize, root: &mut RbRootCached, leftmost: bool) {
    if leftmost {
        root.rb_leftmost = node;
    }
    rb_insert_color(r, node, &mut root.root);
}

fn rb_erase_cached(r: &mut Rig, node: usize, root: &mut RbRootCached) -> usize {
    let mut leftmost = NIL;
    if root.rb_leftmost == node {
        leftmost = rb_next(r, node);
        root.rb_leftmost = leftmost;
    }
    rb_erase(r, node, &mut root.root);
    leftmost
}

fn rb_replace_node_cached(r: &mut Rig, victim: usize, new: usize, root: &mut RbRootCached) {
    if root.rb_leftmost == victim {
        root.rb_leftmost = new;
    }
    rb_replace_node(r, victim, new, &mut root.root);
}

/// `rb_add_cached` — returns `node` when it's the new leftmost, else NIL.
fn rb_add_cached(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRootCached,
    less: fn(&Rig, usize, usize) -> bool,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;
    let mut leftmost = true;

    loop {
        let cur = link_get(r, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        if less(r, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
            leftmost = false;
        }
    }

    rb_link_node(r, node, parent, link, &mut tree.root);
    rb_insert_color_cached(r, node, tree, leftmost);

    if leftmost { node } else { NIL }
}

/// `rb_link_linked_node`: splice the linked node before/after its tree
/// parent in the in-order list. `link` encodes which child slot was taken.
fn rb_link_linked_node(r: &mut Rig, node: usize, parent: usize, link: Link) {
    if parent == NIL {
        return;
    }
    match link {
        Link::Left(_) => {
            r.lprev[node] = r.lprev[parent];
            r.lnext[node] = parent;
            r.lprev[parent] = node;
            if r.lprev[node] != NIL {
                let p = r.lprev[node];
                r.lnext[p] = node;
            }
        }
        _ => {
            r.lnext[node] = r.lnext[parent];
            r.lprev[node] = parent;
            r.lnext[parent] = node;
            if r.lnext[node] != NIL {
                let x = r.lnext[node];
                r.lprev[x] = node;
            }
        }
    }
}

/// `rb_link_noop` — optimized away in C.
fn rb_link_noop(_r: &mut Rig, _node: usize, _parent: usize, _link: Link) {}

/// `__rb_add`.
fn __rb_add(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRoot,
    less: fn(&Rig, usize, usize) -> bool,
    linkop: fn(&mut Rig, usize, usize, Link),
) {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(r, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        if less(r, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
        }
    }

    linkop(r, node, parent, link);
    rb_link_node(r, node, parent, link, tree);
    rb_insert_color(r, node, tree);
}

/// `rb_add_linked` — returns true when `node` is the new leftmost.
fn rb_add_linked(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRootLinked,
    less: fn(&Rig, usize, usize) -> bool,
) -> bool {
    __rb_add(r, node, &mut tree.root, less, rb_link_linked_node);
    if r.lprev[node] == NIL {
        tree.rb_leftmost = node;
    }
    r.lprev[node] == NIL
}

fn rb_add(r: &mut Rig, node: usize, tree: &mut RbRoot, less: fn(&Rig, usize, usize) -> bool) {
    __rb_add(r, node, tree, less, rb_link_noop);
}

/// `rb_find_add_cached` — returns the existing match, or NIL after inserting.
fn rb_find_add_cached(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRootCached,
    cmp: fn(&Rig, usize, usize) -> i32,
) -> usize {
    let mut leftmost = true;
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(r, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(r, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
            leftmost = false;
        } else {
            return parent;
        }
    }

    rb_link_node(r, node, parent, link, &mut tree.root);
    rb_insert_color_cached(r, node, tree, leftmost);
    NIL
}

/// `rb_find_add` — returns the existing match, or NIL after inserting.
fn rb_find_add(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRoot,
    cmp: fn(&Rig, usize, usize) -> i32,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(r, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(r, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
        } else {
            return parent;
        }
    }

    rb_link_node(r, node, parent, link, tree);
    rb_insert_color(r, node, tree);
    NIL
}

/// `rb_find_add_rcu` — adds via rb_link_node_rcu (plain assignment here).
fn rb_find_add_rcu(
    r: &mut Rig,
    node: usize,
    tree: &mut RbRoot,
    cmp: fn(&Rig, usize, usize) -> i32,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(r, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(r, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
        } else {
            return parent;
        }
    }

    rb_link_node_rcu(r, node, parent, link, tree);
    rb_insert_color(r, node, tree);
    NIL
}

/// `rb_find` — returns the node matching `key` or NIL.
fn rb_find(r: &Rig, key: u64, tree: &RbRoot, cmp: fn(&Rig, u64, usize) -> i32) -> usize {
    let mut node = tree.rb_node;
    while node != NIL {
        let c = cmp(r, key, node);
        if c < 0 {
            node = r.n[node].left;
        } else if c > 0 {
            node = r.n[node].right;
        } else {
            return node;
        }
    }
    NIL
}

/// `rb_find_rcu` — identical (rcu_dereference_raw is a plain deref).
fn rb_find_rcu(r: &Rig, key: u64, tree: &RbRoot, cmp: fn(&Rig, u64, usize) -> i32) -> usize {
    rb_find(r, key, tree, cmp)
}

/// `rb_find_first` — leftmost node matching `key`, or NIL.
fn rb_find_first(r: &Rig, key: u64, tree: &RbRoot, cmp: fn(&Rig, u64, usize) -> i32) -> usize {
    let mut node = tree.rb_node;
    let mut m = NIL;
    while node != NIL {
        let c = cmp(r, key, node);
        if c <= 0 {
            if c == 0 {
                m = node;
            }
            node = r.n[node].left;
        } else {
            node = r.n[node].right;
        }
    }
    m
}

/// `rb_next_match` — next node matching `key`, or NIL.
fn rb_next_match(r: &Rig, key: u64, node: usize, cmp: fn(&Rig, u64, usize) -> i32) -> usize {
    let n = rb_next(r, node);
    if n != NIL && cmp(r, key, n) != 0 {
        NIL
    } else {
        n
    }
}

// ---- harness mirror (wrappers.c) --------------------------------------------

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
    *h = (*h ^ v).wrapping_mul(0x100000001b3);
}

/// `tidx`/`lnidx`/`ltidx` all map a node index (or NIL) to the folded value.
fn fidx(p: usize) -> u64 {
    if p == NIL { NILX } else { p as u64 }
}

// comparators / less ops
fn cmp_key(r: &Rig, a: u64, b: usize) -> i32 {
    (a > r.key[b]) as i32 - (a < r.key[b]) as i32
}
fn cmp_node(r: &Rig, a: usize, b: usize) -> i32 {
    (r.key[a] > r.key[b]) as i32 - (r.key[a] < r.key[b]) as i32
}
fn tless(r: &Rig, a: usize, b: usize) -> bool {
    r.key[a] < r.key[b]
}
fn lless(r: &Rig, a: usize, b: usize) -> bool {
    r.lkey[a] < r.lkey[b]
}

fn tree_count_r(r: &Rig, root: &RbRoot) -> usize {
    let mut p = rb_first(r, root);
    let mut c = 0;
    while p != NIL {
        c += 1;
        p = rb_next(r, p);
    }
    c
}
fn tree_kth_r(r: &Rig, root: &RbRoot, k: usize) -> usize {
    let mut p = rb_first(r, root);
    let mut k = k;
    while p != NIL && k > 0 {
        k -= 1;
        p = rb_next(r, p);
    }
    p
}
fn free_slot(r: &Rig) -> usize {
    (0..N).find(|&i| !r.present[i]).unwrap_or(NIL)
}

/// User-side search+insert (every rbtree user implements its own);
/// duplicates go right at the equal node.
fn tree_insert(r: &mut Rig, root: &mut RbRoot, i: usize) {
    let mut link = Link::Root;
    let mut parent = NIL;
    let k = r.key[i];

    loop {
        let cur = link_get(r, root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        if k < r.key[parent] {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
        }
    }
    r.present[i] = true;
    rb_link_node(r, i, parent, link, root);
    rb_insert_color(r, i, root);
}

/// Full-state checksum: in-order (idx, key, color, parent, left, right
/// [, aug]) per node, then the count.
fn fold_tree(r: &Rig, root: &RbRoot, with_aug: bool, h: &mut u64) {
    let mut p = rb_first(r, root);
    let mut c = 0usize;
    while p != NIL {
        fold(h, fidx(p));
        fold(h, r.key[p]);
        fold(h, rb_is_black(r, p) as u64);
        fold(h, fidx(rb_parent(r, p)));
        fold(h, fidx(r.n[p].left));
        fold(h, fidx(r.n[p].right));
        if with_aug {
            fold(h, r.aug[p]);
        }
        c += 1;
        p = rb_next(r, p);
    }
    fold(h, c as u64);
}

fn fold_ltree(r: &Rig, root: &RbRoot, h: &mut u64) {
    let mut p = rb_first(r, root);
    let mut c = 0usize;
    while p != NIL {
        fold(h, fidx(p));
        fold(h, r.lkey[p]);
        fold(h, rb_is_black(r, p) as u64);
        fold(h, fidx(rb_parent(r, p)));
        fold(h, fidx(r.n[p].left));
        fold(h, fidx(r.n[p].right));
        c += 1;
        p = rb_next(r, p);
    }
    fold(h, c as u64);
}

/// victim pick for erases: odd r2 -> random present slot, even -> kth
/// in-order node; NIL when the tree is empty / slot absent.
fn pick_victim(r: &Rig, root: &RbRoot, r2: u64) -> usize {
    if r2 & 1 != 0 {
        let idx = ((r2 >> 8) % N as u64) as usize;
        if r.present[idx] { idx } else { NIL }
    } else {
        let c = tree_count_r(r, root);
        if c != 0 {
            tree_kth_r(r, root, ((r2 >> 8) % c as u64) as usize)
        } else {
            NIL
        }
    }
}

/// Port of `uint64_t wrap_rbtree_run(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:231`).
pub fn wrap_rbtree_run(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    const MIX: [[u64; 3]; 4] = [[55, 30, 15], [75, 15, 10], [40, 40, 20], [30, 50, 20]];
    let m = MIX[(iabs(seed2) % 4) as usize];
    let nops = (iabs(nops_i) % 200) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for _ in 0..nops {
        let d = xs64(&mut rs);
        let t = d % 100;
        if t < m[0] {
            let idx = (xs64(&mut rs) % N as u64) as usize;
            if !r.present[idx] {
                r.key[idx] = xs64(&mut rs) % KEYSPACE;
                r.val[idx] = r.key[idx];
                tree_insert(&mut r, &mut root, idx);
                fold(&mut h, idx as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else if t < m[0] + m[1] {
            let idx = pick_victim(&r, &root, xs64(&mut rs));
            if idx != NIL {
                rb_erase(&mut r, idx, &mut root);
                r.present[idx] = false;
                rb_clear_node(&mut r, idx);
                fold(&mut h, idx as u64);
                fold(&mut h, rb_empty_node(&r, idx) as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else {
            let q = xs64(&mut rs) % 10;
            match q {
                0 => fold(&mut h, fidx(rb_first(&r, &root))),
                1 => fold(&mut h, fidx(rb_last(&r, &root))),
                2 => {
                    let c = tree_count_r(&r, &root);
                    if c != 0 {
                        let k = (xs64(&mut rs) % c as u64) as usize;
                        let v = tree_kth_r(&r, &root, k);
                        fold(&mut h, fidx(rb_next(&r, v)));
                    } else {
                        fold(&mut h, NILX);
                    }
                }
                3 => {
                    let c = tree_count_r(&r, &root);
                    if c != 0 {
                        let k = (xs64(&mut rs) % c as u64) as usize;
                        let v = tree_kth_r(&r, &root, k);
                        fold(&mut h, fidx(rb_prev(&r, v)));
                    } else {
                        fold(&mut h, NILX);
                    }
                }
                4 => {
                    let mut p = rb_first_postorder(&r, &root);
                    while p != NIL {
                        fold(&mut h, fidx(p));
                        p = rb_next_postorder(&r, p);
                    }
                    fold(&mut h, NILX);
                }
                5 | 6 => {
                    let c = tree_count_r(&r, &root);
                    if c != 0 {
                        let v = tree_kth_r(&r, &root, (xs64(&mut rs) % c as u64) as usize);
                        let ni = free_slot(&r);
                        if ni == NIL {
                            fold(&mut h, NILX);
                        } else {
                            r.key[ni] = r.key[v];
                            r.val[ni] = r.val[v];
                            if q == 5 {
                                rb_replace_node(&mut r, v, ni, &mut root);
                            } else {
                                rb_replace_node_rcu(&mut r, v, ni, &mut root);
                            }
                            r.present[v] = false;
                            r.present[ni] = true;
                            fold(&mut h, v as u64);
                            fold(&mut h, ni as u64);
                        }
                    } else {
                        fold(&mut h, NILX);
                    }
                }
                7 => {
                    let k = xs64(&mut rs) % KEYSPACE;
                    fold(&mut h, fidx(rb_find(&r, k, &root, cmp_key)));
                }
                8 => {
                    let k = xs64(&mut rs) % KEYSPACE;
                    let mut p = rb_find_first(&r, k, &root, cmp_key);
                    let mut j = 0;
                    while p != NIL {
                        fold(&mut h, fidx(p));
                        j += 1;
                        if j == 4 {
                            break;
                        }
                        p = rb_next_match(&r, k, p, cmp_key);
                    }
                    fold(&mut h, NILX);
                }
                _ => {
                    let ni = free_slot(&r);
                    if ni == NIL {
                        fold(&mut h, NILX);
                    } else {
                        r.key[ni] = xs64(&mut rs) % KEYSPACE;
                        r.val[ni] = r.key[ni];
                        let res = rb_find_add(&mut r, ni, &mut root, cmp_node);
                        if res != NIL {
                            fold(&mut h, fidx(res));
                        } else {
                            r.present[ni] = true;
                            fold(&mut h, ni as u64);
                        }
                    }
                }
            }
        }
    }
    fold_tree(&r, &root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_insert_seq(uint64_t seed, int64_t n_i)` (`wrappers.c:373`).
pub fn wrap_rbtree_insert_seq(seed: u64, n_i: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 + 1)) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for i in 0..n {
        r.key[i] = xs64(&mut rs) % KEYSPACE;
        r.val[i] = r.key[i];
        tree_insert(&mut r, &mut root, i);
    }
    fold(&mut h, fidx(rb_first(&r, &root)));
    fold(&mut h, fidx(rb_last(&r, &root)));
    fold_tree(&r, &root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_erase_all(uint64_t seed, int64_t n_i)` (`wrappers.c:394`).
pub fn wrap_rbtree_erase_all(seed: u64, n_i: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 + 1)) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for i in 0..n {
        r.key[i] = xs64(&mut rs) % KEYSPACE;
        r.val[i] = r.key[i];
        tree_insert(&mut r, &mut root, i);
    }
    let mode = xs64(&mut rs) % 3;
    if mode == 0 {
        loop {
            let p = rb_first(&r, &root);
            if p == NIL {
                break;
            }
            rb_erase(&mut r, p, &mut root);
            r.present[p] = false;
            fold(&mut h, p as u64);
        }
    } else if mode == 1 {
        loop {
            let p = rb_last(&r, &root);
            if p == NIL {
                break;
            }
            rb_erase(&mut r, p, &mut root);
            r.present[p] = false;
            fold(&mut h, p as u64);
        }
    } else {
        for i in 0..N {
            if r.present[i] {
                rb_erase(&mut r, i, &mut root);
                r.present[i] = false;
                fold(&mut h, i as u64);
            }
        }
    }
    fold_tree(&r, &root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_find(uint64_t seed, int64_t n_i, int64_t seed2)` (`wrappers.c:439`).
pub fn wrap_rbtree_find(seed: u64, n_i: i64, seed2: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 / 2)) as usize;
    let rcu_j = (iabs(seed2) % 8) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for i in 0..n {
        r.key[i] = xs64(&mut rs) % KEYSPACE;
        r.val[i] = r.key[i];
        tree_insert(&mut r, &mut root, i);
    }
    for _ in 0..12 {
        let k = xs64(&mut rs) % KEYSPACE;
        fold(&mut h, fidx(rb_find(&r, k, &root, cmp_key)));
        fold(&mut h, fidx(rb_find_rcu(&r, k, &root, cmp_key)));
        let mut p = rb_find_first(&r, k, &root, cmp_key);
        let mut c = 0;
        while p != NIL {
            fold(&mut h, fidx(p));
            c += 1;
            if c == 4 {
                break;
            }
            p = rb_next_match(&r, k, p, cmp_key);
        }
        fold(&mut h, NILX);
    }
    let mut ni = n;
    for j in 0..8 {
        if ni >= N {
            break;
        }
        r.key[ni] = xs64(&mut rs) % KEYSPACE;
        r.val[ni] = r.key[ni];
        let res = if j == rcu_j {
            rb_find_add_rcu(&mut r, ni, &mut root, cmp_node)
        } else {
            rb_find_add(&mut r, ni, &mut root, cmp_node)
        };
        if res != NIL {
            fold(&mut h, fidx(res));
        } else {
            r.present[ni] = true;
            fold(&mut h, ni as u64);
        }
        ni += 1;
    }
    for _ in 0..4 {
        if ni >= N {
            break;
        }
        r.key[ni] = xs64(&mut rs) % KEYSPACE;
        r.val[ni] = r.key[ni];
        r.present[ni] = true;
        rb_add(&mut r, ni, &mut root, tless);
        fold(&mut h, ni as u64);
        ni += 1;
    }
    fold_tree(&r, &root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_cached(uint64_t seed, int64_t nops_i)` (`wrappers.c:482`).
pub fn wrap_rbtree_cached(seed: u64, nops_i: i64) -> u64 {
    let nops = (iabs(nops_i) % 200) as usize;

    let mut r = rig_new();
    let mut croot = RbRootCached {
        root: RbRoot { rb_node: NIL },
        rb_leftmost: NIL,
    };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for _ in 0..nops {
        let d = xs64(&mut rs);
        let t = d % 100;
        if t < 55 {
            let idx = (xs64(&mut rs) % N as u64) as usize;
            if !r.present[idx] {
                r.key[idx] = xs64(&mut rs) % KEYSPACE;
                r.val[idx] = r.key[idx];
                if d & 0x200 != 0 {
                    let res = rb_add_cached(&mut r, idx, &mut croot, tless);
                    fold(&mut h, fidx(res));
                } else {
                    let mut link = Link::Root;
                    let mut parent = NIL;
                    let k = r.key[idx];
                    let mut leftmost = true;
                    loop {
                        let cur = link_get(&r, &croot.root, link);
                        if cur == NIL {
                            break;
                        }
                        parent = cur;
                        if k < r.key[parent] {
                            link = Link::Left(parent);
                        } else {
                            link = Link::Right(parent);
                            leftmost = false;
                        }
                    }
                    rb_link_node(&mut r, idx, parent, link, &mut croot.root);
                    rb_insert_color_cached(&mut r, idx, &mut croot, leftmost);
                    fold(&mut h, leftmost as u64);
                }
                r.present[idx] = true;
            } else {
                fold(&mut h, NILX);
            }
        } else if t < 85 {
            let idx = pick_victim(&r, &croot.root, xs64(&mut rs));
            if idx != NIL {
                let lm = rb_erase_cached(&mut r, idx, &mut croot);
                r.present[idx] = false;
                fold(&mut h, idx as u64);
                fold(&mut h, fidx(lm));
            } else {
                fold(&mut h, NILX);
            }
        } else {
            let q = xs64(&mut rs) % 6;
            match q {
                0 => fold(&mut h, fidx(rb_first_cached(&croot))),
                1 => fold(&mut h, fidx(rb_first(&r, &croot.root))),
                2 => fold(
                    &mut h,
                    (fidx(rb_first_cached(&croot)) == fidx(rb_first(&r, &croot.root))) as u64,
                ),
                3 => {
                    let c = tree_count_r(&r, &croot.root);
                    if c != 0 {
                        let v = tree_kth_r(&r, &croot.root, (xs64(&mut rs) % c as u64) as usize);
                        let ni = free_slot(&r);
                        if ni == NIL {
                            fold(&mut h, NILX);
                        } else {
                            r.key[ni] = r.key[v];
                            r.val[ni] = r.val[v];
                            rb_replace_node_cached(&mut r, v, ni, &mut croot);
                            r.present[v] = false;
                            r.present[ni] = true;
                            fold(&mut h, v as u64);
                            fold(&mut h, ni as u64);
                        }
                    } else {
                        fold(&mut h, NILX);
                    }
                }
                4 => {
                    let ni = free_slot(&r);
                    if ni == NIL {
                        fold(&mut h, NILX);
                    } else {
                        r.key[ni] = xs64(&mut rs) % KEYSPACE;
                        r.val[ni] = r.key[ni];
                        let res = rb_find_add_cached(&mut r, ni, &mut croot, cmp_node);
                        if res != NIL {
                            fold(&mut h, fidx(res));
                        } else {
                            r.present[ni] = true;
                            fold(&mut h, ni as u64);
                        }
                    }
                }
                _ => fold(&mut h, fidx(rb_last(&r, &croot.root))),
            }
        }
    }
    fold(&mut h, fidx(croot.rb_leftmost));
    fold_tree(&r, &croot.root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_linked(uint64_t seed, int64_t nops_i)` (`wrappers.c:594`).
pub fn wrap_rbtree_linked(seed: u64, nops_i: i64) -> u64 {
    let nops = (iabs(nops_i) % 200) as usize;

    let mut r = rig_new();
    let mut lroot = RbRootLinked {
        root: RbRoot { rb_node: NIL },
        rb_leftmost: NIL,
    };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    fn llist_count(r: &Rig, lroot: &RbRootLinked) -> usize {
        let mut l = lroot.rb_leftmost;
        let mut c = 0;
        while l != NIL && c <= N {
            c += 1;
            l = r.lnext[l];
        }
        c
    }

    for _ in 0..nops {
        let d = xs64(&mut rs);
        let t = d % 100;
        if t < 60 {
            let idx = (xs64(&mut rs) % N as u64) as usize;
            if !r.lpresent[idx] {
                r.lkey[idx] = xs64(&mut rs) % KEYSPACE;
                let lm = rb_add_linked(&mut r, idx, &mut lroot, lless);
                r.lpresent[idx] = true;
                fold(&mut h, idx as u64);
                fold(&mut h, lm as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else if t < 85 {
            let r2 = xs64(&mut rs);
            let mut idx = NIL;
            if r2 & 1 != 0 {
                let s = ((r2 >> 8) % N as u64) as usize;
                if r.lpresent[s] {
                    idx = s;
                }
            } else {
                let c = llist_count(&r, &lroot);
                if c != 0 {
                    let mut k = ((r2 >> 8) % c as u64) as usize;
                    let mut l = lroot.rb_leftmost;
                    while l != NIL && k > 0 {
                        k -= 1;
                        l = r.lnext[l];
                    }
                    idx = l;
                }
            }
            if idx != NIL {
                let more = rb_erase_linked(&mut r, idx, &mut lroot);
                r.lpresent[idx] = false;
                fold(&mut h, idx as u64);
                fold(&mut h, more as u64);
                fold(&mut h, rb_empty_linked_node(&r, idx) as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else {
            let q = xs64(&mut rs) % 4;
            match q {
                0 => fold(&mut h, fidx(lroot.rb_leftmost)),
                1 => fold(&mut h, fidx(rb_first(&r, &lroot.root))),
                2 => {
                    let mut l = lroot.rb_leftmost;
                    let mut j = 0;
                    while l != NIL && j < 8 {
                        fold(&mut h, fidx(l));
                        fold(&mut h, r.lkey[l]);
                        l = r.lnext[l];
                        j += 1;
                    }
                    fold(&mut h, NILX);
                }
                _ => {
                    let c = tree_count_r(&r, &lroot.root);
                    if c != 0 {
                        let k = (xs64(&mut rs) % c as u64) as usize;
                        let p = tree_kth_r(&r, &lroot.root, k);
                        fold(&mut h, fidx(p));
                    } else {
                        fold(&mut h, NILX);
                    }
                }
            }
        }
    }
    fold(&mut h, fidx(lroot.rb_leftmost));
    {
        let mut l = lroot.rb_leftmost;
        while l != NIL {
            fold(&mut h, fidx(l));
            fold(&mut h, r.lkey[l]);
            l = r.lnext[l];
        }
        fold(&mut h, NILX);
    }
    fold_ltree(&r, &lroot.root, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_augmented(uint64_t seed, int64_t nops_i)` (`wrappers.c:723`).
pub fn wrap_rbtree_augmented(seed: u64, nops_i: i64) -> u64 {
    let nops = (iabs(nops_i) % 200) as usize;

    let mut r = rig_new();
    let mut croot = RbRootCached {
        root: RbRoot { rb_node: NIL },
        rb_leftmost: NIL,
    };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    // manual descent insert, rbtree_test.c idiom: fold the new node's val
    // into every ancestor's aug on the way down.
    fn aug_insert(r: &mut Rig, croot: &mut RbRootCached, i: usize, h: &mut u64) {
        let mut link = Link::Root;
        let mut parent = NIL;
        let k = r.key[i];
        let v = r.val[i];
        let mut leftmost = true;

        loop {
            let cur = link_get(r, &croot.root, link);
            if cur == NIL {
                break;
            }
            parent = cur;
            if r.aug[parent] < v {
                r.aug[parent] = v;
            }
            if k < r.key[parent] {
                link = Link::Left(parent);
            } else {
                link = Link::Right(parent);
                leftmost = false;
            }
        }
        r.aug[i] = v;
        rb_link_node(r, i, parent, link, &mut croot.root);
        rb_insert_augmented_cached(r, i, croot, leftmost, &AUG_CB);
        fold(h, leftmost as u64);
    }

    for _ in 0..nops {
        let d = xs64(&mut rs);
        let t = d % 100;
        if t < 55 {
            let idx = (xs64(&mut rs) % N as u64) as usize;
            if !r.present[idx] {
                r.key[idx] = xs64(&mut rs) % KEYSPACE;
                r.val[idx] = xs64(&mut rs) % KEYSPACE;
                if d & 0x200 != 0 {
                    aug_insert(&mut r, &mut croot, idx, &mut h);
                } else {
                    r.aug[idx] = r.val[idx];
                    let res = rb_add_augmented_cached(&mut r, idx, &mut croot, tless, &AUG_CB);
                    fold(&mut h, fidx(res));
                }
                r.present[idx] = true;
            } else {
                fold(&mut h, NILX);
            }
        } else if t < 85 {
            let idx = pick_victim(&r, &croot.root, xs64(&mut rs));
            if idx != NIL {
                rb_erase_augmented_cached(&mut r, idx, &mut croot, &AUG_CB);
                r.present[idx] = false;
                fold(&mut h, idx as u64);
            } else {
                fold(&mut h, NILX);
            }
        } else {
            let q = xs64(&mut rs) % 3;
            match q {
                0 => fold(&mut h, fidx(rb_first_cached(&croot))),
                1 => {
                    if croot.root.rb_node != NIL {
                        fold(&mut h, r.aug[croot.root.rb_node]);
                    } else {
                        fold(&mut h, NILX);
                    }
                }
                _ => fold(&mut h, fidx(croot.rb_leftmost)),
            }
        }
    }
    fold(&mut h, fidx(croot.rb_leftmost));
    fold_tree(&r, &croot.root, true, &mut h);
    h
}

/// Port of `uint64_t wrap_rbtree_postorder(uint64_t seed, int64_t n_i)` (`wrappers.c:791`).
pub fn wrap_rbtree_postorder(seed: u64, n_i: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 + 1)) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for i in 0..n {
        r.key[i] = xs64(&mut rs) % KEYSPACE;
        r.val[i] = r.key[i];
        tree_insert(&mut r, &mut root, i);
    }
    let mut p = rb_first_postorder(&r, &root);
    fold(&mut h, fidx(p));
    while p != NIL {
        p = rb_next_postorder(&r, p);
        fold(&mut h, fidx(p));
    }
    fold(&mut h, fidx(rb_next_postorder(&r, NIL)));
    fold_tree(&r, &root, false, &mut h);
    h
}

/// Port of `uint64_t wrap_rb_replace(uint64_t seed, int64_t n_i, int64_t k_i)` (`wrappers.c:817`).
pub fn wrap_rb_replace(seed: u64, n_i: i64, k_i: i64) -> u64 {
    let n = (iabs(n_i) % (N as u64 / 2 + 1)) as usize;
    let m = (iabs(k_i) % 8) as usize;

    let mut r = rig_new();
    let mut root = RbRoot { rb_node: NIL };
    let mut h = 0x9e3779b97f4a7c15u64;
    let mut rs = seed | 1;

    for i in 0..n {
        r.key[i] = xs64(&mut rs) % KEYSPACE;
        r.val[i] = r.key[i];
        tree_insert(&mut r, &mut root, i);
    }
    for j in 0..m {
        let c = tree_count_r(&r, &root);
        if c == 0 {
            break;
        }
        let v = tree_kth_r(&r, &root, (xs64(&mut rs) % c as u64) as usize);
        let ni = free_slot(&r);
        if ni == NIL {
            break;
        }
        r.key[ni] = r.key[v];
        r.val[ni] = r.val[v];
        if j & 1 != 0 {
            rb_replace_node_rcu(&mut r, v, ni, &mut root);
        } else {
            rb_replace_node(&mut r, v, ni, &mut root);
        }
        r.present[v] = false;
        r.present[ni] = true;
        fold(&mut h, v as u64);
        fold(&mut h, ni as u64);
    }
    fold_tree(&r, &root, false, &mut h);
    h
}
