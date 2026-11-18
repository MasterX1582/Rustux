//! Red-black trees, ported from Linux `lib/rbtree.c` +
//! `include/linux/rbtree{,_types,_augmented}.h`.
//!
//! Nodes live in caller-owned pools (`&mut [RbNode]`); every pointer in the C
//! API becomes a `usize` pool index, with [`NIL`] standing in for NULL.
//! `RbNode::pc` packs the parent index and color into one `u64` exactly like
//! C packs them into `__rb_parent_color`'s low address bits.
//!
//! Comparators and augment callbacks are `impl Fn` / `AugFn` taking node
//! indices; users keep their own payload in parallel arrays indexed the same
//! way (the index-model analogue of `container_of`).

pub const NIL: usize = usize::MAX;

pub const RB_RED: u64 = 0;
pub const RB_BLACK: u64 = 1;

// ---- node / harness state ---------------------------------------------------

/// `struct rb_node`: `pc` packs parent index + color like C packs the
/// pointer's low bits: pc = enc(parent) + color, enc(i) = (i+1)<<2.
#[derive(Clone, Copy)]
pub struct RbNode {
    /// Packed parent+color word (`__rb_parent_color` in C).
    pub pc: u64,
    /// Right child index (`rb_right`), NIL when absent.
    pub right: usize,
    /// Left child index (`rb_left`), NIL when absent.
    pub left: usize,
}

impl RbNode {
    /// Zeroed node ("red, parentless" — like a memset struct rb_node).
    /// Call [`rb_clear_node`] to satisfy the `rb_empty_node` contract.
    pub const fn new() -> Self {
        Self { pc: 0, left: NIL, right: NIL }
    }
}

impl Default for RbNode {
    fn default() -> Self {
        Self::new()
    }
}

/// `struct rb_root` — index of the root node (NIL when empty).
pub struct RbRoot {
    /// Root node index (`rb_node`), NIL when the tree is empty.
    pub rb_node: usize,
}

impl RbRoot {
    /// `RB_ROOT` — an empty tree.
    pub const EMPTY: Self = Self { rb_node: NIL };
}

impl Default for RbRoot {
    fn default() -> Self {
        Self::EMPTY
    }
}
/// `struct rb_root_cached`.
pub struct RbRootCached {
    /// The underlying plain root.
    pub root: RbRoot,
    /// Index of the leftmost node (`rb_leftmost`), NIL when empty.
    pub rb_leftmost: usize,
}

impl RbRootCached {
    /// `RB_ROOT_CACHED` — an empty cached tree.
    pub const EMPTY: Self = Self {
        root: RbRoot::EMPTY,
        rb_leftmost: NIL,
    };
}

impl Default for RbRootCached {
    fn default() -> Self {
        Self::EMPTY
    }
}
/// `struct rb_root_linked` (leftmost is an index into the linked pool).
pub struct RbRootLinked {
    /// The underlying plain root.
    pub root: RbRoot,
    /// Index of the first in-order list node, NIL when empty.
    pub rb_leftmost: usize,
}

impl RbRootLinked {
    /// `RB_ROOT_LINKED` — an empty linked tree.
    pub const EMPTY: Self = Self {
        root: RbRoot::EMPTY,
        rb_leftmost: NIL,
    };
}

impl Default for RbRootLinked {
    fn default() -> Self {
        Self::EMPTY
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

/// `rb_parent`: (pc & ~3) decoded.
pub fn rb_parent(nodes: &[RbNode], i: usize) -> usize {
    dec(nodes[i].pc & !3u64)
}
/// `__rb_parent(pc)`.
fn __rb_parent(pc: u64) -> usize {
    dec(pc & !3u64)
}
/// `__rb_is_black(pc)` — works on a packed pc word.
fn __rb_is_black(pc: u64) -> bool {
    pc & 1 != 0
}
pub fn rb_is_black(nodes: &[RbNode], i: usize) -> bool {
    __rb_is_black(nodes[i].pc)
}
pub fn rb_is_red(nodes: &[RbNode], i: usize) -> bool {
    nodes[i].pc & 1 == 0
}
/// `rb_red_parent`: the unmasked pc — callers guarantee the node is red,
/// so the low bits are 0 and the whole word is the encoded parent.
fn rb_red_parent(nodes: &[RbNode], i: usize) -> usize {
    dec(nodes[i].pc)
}
fn rb_set_parent(nodes: &mut [RbNode], i: usize, p: usize) {
    nodes[i].pc = (nodes[i].pc & 1) + enc(p);
}
fn rb_set_parent_color(nodes: &mut [RbNode], i: usize, p: usize, color: u64) {
    nodes[i].pc = enc(p) + color;
}
fn rb_set_black(nodes: &mut [RbNode], i: usize) {
    nodes[i].pc += RB_BLACK;
}
pub fn rb_empty_node(nodes: &[RbNode], i: usize) -> bool {
    nodes[i].pc == self_enc(i)
}
pub fn rb_clear_node(nodes: &mut [RbNode], i: usize) {
    nodes[i].pc = self_enc(i);
}
/// `RB_EMPTY_ROOT` — true when the tree is empty.
pub fn rb_empty_root(root: &RbRoot) -> bool {
    root.rb_node == NIL
}
/// `RB_CLEAR_LINKED_NODE` — clear the rb links and the in-order list links.
pub fn rb_clear_linked_node(nodes: &mut [RbNode], lprev: &mut [usize], lnext: &mut [usize], i: usize) {
    rb_clear_node(nodes, i);
    lprev[i] = NIL;
    lnext[i] = NIL;
}

fn __rb_change_child(nodes: &mut [RbNode], old: usize, new: usize, parent: usize, root: &mut RbRoot) {
    if parent != NIL {
        if nodes[parent].left == old {
            nodes[parent].left = new;
        } else {
            nodes[parent].right = new;
        }
    } else {
        root.rb_node = new;
    }
}
/// `__rb_change_child_rcu`: identical — rcu_assign_pointer is a plain
/// assignment in the shim.
fn __rb_change_child_rcu(nodes: &mut [RbNode], old: usize, new: usize, parent: usize, root: &mut RbRoot) {
    __rb_change_child(nodes, old, new, parent, root)
}

/// `rb_link_node(node, parent, rb_link)`: `Link` stands in for `struct
/// rb_node **` — Root is &root->rb_node, Left(i)/Right(i) are the child
/// slots of node i.
#[derive(Clone, Copy)]
pub enum Link {
    Root,
    Left(usize),
    Right(usize),
}
fn link_get(nodes: &[RbNode], root: &RbRoot, l: Link) -> usize {
    match l {
        Link::Root => root.rb_node,
        Link::Left(i) => nodes[i].left,
        Link::Right(i) => nodes[i].right,
    }
}
fn link_set(nodes: &mut [RbNode], root: &mut RbRoot, l: Link, v: usize) {
    match l {
        Link::Root => root.rb_node = v,
        Link::Left(i) => nodes[i].left = v,
        Link::Right(i) => nodes[i].right = v,
    }
}
pub fn rb_link_node(nodes: &mut [RbNode], node: usize, parent: usize, link: Link, root: &mut RbRoot) {
    nodes[node].pc = enc(parent);
    nodes[node].left = NIL;
    nodes[node].right = NIL;
    link_set(nodes, root, link, node);
}
/// `rb_link_node_rcu`: identical — rcu_assign_pointer is a plain assignment.
pub fn rb_link_node_rcu(nodes: &mut [RbNode], node: usize, parent: usize, link: Link, root: &mut RbRoot) {
    rb_link_node(nodes, node, parent, link, root)
}

// ---- augmented callbacks (`struct rb_augment_callbacks`) --------------------

pub type AugFn<C> = fn(&[RbNode], &mut C, usize, usize);

pub struct Augment<C> {
    /// `propagate(node, stop)` — recompute augmentation from `node` up to
    /// `stop` (exclusive).
    pub propagate: AugFn<C>,
    /// `copy(old, new)` — copy augmentation from `old` to `new`.
    pub copy: AugFn<C>,
    /// `rotate(old, new)` — fix augmentation after a rotation.
    pub rotate: AugFn<C>,
    /// `merge(node, new)` — merge `new`'s augmentation into `node`.
    pub merge: AugFn<C>,
}

fn dummy_propagate(_nodes: &[RbNode], _c: &mut (), _a: usize, _b: usize) {}
fn dummy_copy(_nodes: &[RbNode], _c: &mut (), _a: usize, _b: usize) {}
fn dummy_rotate(_nodes: &[RbNode], _c: &mut (), _a: usize, _b: usize) {}
fn dummy_merge(_nodes: &[RbNode], _c: &mut (), _a: usize, _b: usize) {}

const DUMMY: Augment<()> = Augment {
    propagate: dummy_propagate,
    copy: dummy_copy,
    rotate: dummy_rotate,
    merge: dummy_merge,
};

// ---- lib/rbtree.c -----------------------------------------------------------

/// `__rb_rotate_set_parents`: old's parent+color go to new; old gets new as
/// parent and `color` as color.
fn __rb_rotate_set_parents(nodes: &mut [RbNode], old: usize, new: usize, root: &mut RbRoot, color: u64) {
    let parent = rb_parent(nodes, old);
    nodes[new].pc = nodes[old].pc;
    rb_set_parent_color(nodes, old, new, color);
    __rb_change_child(nodes, old, new, parent, root);
}

/// `__rb_insert` — the rebalance loop; `augment_rotate` is the C callback
/// (dummy_rotate for the non-augmented instantiation).
fn __rb_insert<C>(nodes: &mut [RbNode], node0: usize, root: &mut RbRoot, ctx: &mut C, augment_rotate: AugFn<C>) {
    let mut node = node0;
    let mut parent = rb_red_parent(nodes, node);

    loop {
        // Loop invariant: node is red.
        if parent == NIL {
            rb_set_parent_color(nodes, node, NIL, RB_BLACK);
            break;
        }

        if rb_is_black(nodes, parent) {
            break;
        }

        let gparent = rb_red_parent(nodes, parent);

        let mut tmp = nodes[gparent].right;
        if parent != tmp {
            // parent == gparent->rb_left
            if tmp != NIL && rb_is_red(nodes, tmp) {
                // Case 1 - color flips; recurse at gparent.
                rb_set_parent_color(nodes, tmp, gparent, RB_BLACK);
                rb_set_parent_color(nodes, parent, gparent, RB_BLACK);
                node = gparent;
                parent = rb_parent(nodes, node);
                rb_set_parent_color(nodes, node, parent, RB_RED);
                continue;
            }

            tmp = nodes[parent].right;
            if node == tmp {
                // Case 2 - left rotate at parent.
                tmp = nodes[node].left;
                nodes[parent].right = tmp;
                nodes[node].left = parent;
                if tmp != NIL {
                    rb_set_parent_color(nodes, tmp, parent, RB_BLACK);
                }
                rb_set_parent_color(nodes, parent, node, RB_RED);
                augment_rotate(nodes, ctx, parent, node);
                parent = node;
                tmp = nodes[node].right;
            }

            // Case 3 - right rotate at gparent.
            nodes[gparent].left = tmp;
            nodes[parent].right = gparent;
            if tmp != NIL {
                rb_set_parent_color(nodes, tmp, gparent, RB_BLACK);
            }
            __rb_rotate_set_parents(nodes, gparent, parent, root, RB_RED);
            augment_rotate(nodes, ctx, gparent, parent);
            break;
        } else {
            tmp = nodes[gparent].left;
            if tmp != NIL && rb_is_red(nodes, tmp) {
                // Case 1 - color flips.
                rb_set_parent_color(nodes, tmp, gparent, RB_BLACK);
                rb_set_parent_color(nodes, parent, gparent, RB_BLACK);
                node = gparent;
                parent = rb_parent(nodes, node);
                rb_set_parent_color(nodes, node, parent, RB_RED);
                continue;
            }

            tmp = nodes[parent].left;
            if node == tmp {
                // Case 2 - right rotate at parent.
                tmp = nodes[node].right;
                nodes[parent].left = tmp;
                nodes[node].right = parent;
                if tmp != NIL {
                    rb_set_parent_color(nodes, tmp, parent, RB_BLACK);
                }
                rb_set_parent_color(nodes, parent, node, RB_RED);
                augment_rotate(nodes, ctx, parent, node);
                parent = node;
                tmp = nodes[node].left;
            }

            // Case 3 - left rotate at gparent.
            nodes[gparent].right = tmp;
            nodes[parent].left = gparent;
            if tmp != NIL {
                rb_set_parent_color(nodes, tmp, gparent, RB_BLACK);
            }
            __rb_rotate_set_parents(nodes, gparent, parent, root, RB_RED);
            augment_rotate(nodes, ctx, gparent, parent);
            break;
        }
    }
}

/// `____rb_erase_color` — the rebalance-after-erase loop; `node` is the
/// black (or initially "empty") child position under `parent`.
fn ____rb_erase_color<C>(nodes: &mut [RbNode], mut parent: usize, root: &mut RbRoot, ctx: &mut C, augment_rotate: AugFn<C>) {
    let mut node = NIL;

    loop {
        // Loop invariants (as in C): node is black (or "NULL" first
        // iteration), node is not the root, and leaf paths through
        // parent+node are one black short.
        let mut sibling = nodes[parent].right;
        if node != sibling {
            // node == parent->rb_left
            if rb_is_red(nodes, sibling) {
                // Case 1 - left rotate at parent.
                let tmp1 = nodes[sibling].left;
                nodes[parent].right = tmp1;
                nodes[sibling].left = parent;
                rb_set_parent_color(nodes, tmp1, parent, RB_BLACK);
                __rb_rotate_set_parents(nodes, parent, sibling, root, RB_RED);
                augment_rotate(nodes, ctx, parent, sibling);
                sibling = tmp1;
            }
            let mut tmp1 = nodes[sibling].right;
            if tmp1 == NIL || rb_is_black(nodes, tmp1) {
                let tmp2 = nodes[sibling].left;
                if tmp2 == NIL || rb_is_black(nodes, tmp2) {
                    // Case 2 - sibling color flip.
                    rb_set_parent_color(nodes, sibling, parent, RB_RED);
                    if rb_is_red(nodes, parent) {
                        rb_set_black(nodes, parent);
                    } else {
                        node = parent;
                        parent = rb_parent(nodes, node);
                        if parent != NIL {
                            continue;
                        }
                    }
                    break;
                }
                // Case 3 - right rotate at sibling.
                tmp1 = nodes[tmp2].right;
                nodes[sibling].left = tmp1;
                nodes[tmp2].right = sibling;
                nodes[parent].right = tmp2;
                if tmp1 != NIL {
                    rb_set_parent_color(nodes, tmp1, sibling, RB_BLACK);
                }
                augment_rotate(nodes, ctx, sibling, tmp2);
                tmp1 = sibling;
                sibling = tmp2;
            }
            // Case 4 - left rotate at parent + color flips.
            let tmp2 = nodes[sibling].left;
            nodes[parent].right = tmp2;
            nodes[sibling].left = parent;
            rb_set_parent_color(nodes, tmp1, sibling, RB_BLACK);
            if tmp2 != NIL {
                rb_set_parent(nodes, tmp2, parent);
            }
            __rb_rotate_set_parents(nodes, parent, sibling, root, RB_BLACK);
            augment_rotate(nodes, ctx, parent, sibling);
            break;
        } else {
            sibling = nodes[parent].left;
            if rb_is_red(nodes, sibling) {
                // Case 1 - right rotate at parent.
                let tmp1 = nodes[sibling].right;
                nodes[parent].left = tmp1;
                nodes[sibling].right = parent;
                rb_set_parent_color(nodes, tmp1, parent, RB_BLACK);
                __rb_rotate_set_parents(nodes, parent, sibling, root, RB_RED);
                augment_rotate(nodes, ctx, parent, sibling);
                sibling = tmp1;
            }
            let mut tmp1 = nodes[sibling].left;
            if tmp1 == NIL || rb_is_black(nodes, tmp1) {
                let tmp2 = nodes[sibling].right;
                if tmp2 == NIL || rb_is_black(nodes, tmp2) {
                    // Case 2 - sibling color flip.
                    rb_set_parent_color(nodes, sibling, parent, RB_RED);
                    if rb_is_red(nodes, parent) {
                        rb_set_black(nodes, parent);
                    } else {
                        node = parent;
                        parent = rb_parent(nodes, node);
                        if parent != NIL {
                            continue;
                        }
                    }
                    break;
                }
                // Case 3 - left rotate at sibling.
                tmp1 = nodes[tmp2].left;
                nodes[sibling].right = tmp1;
                nodes[tmp2].left = sibling;
                nodes[parent].left = tmp2;
                if tmp1 != NIL {
                    rb_set_parent_color(nodes, tmp1, sibling, RB_BLACK);
                }
                augment_rotate(nodes, ctx, sibling, tmp2);
                tmp1 = sibling;
                sibling = tmp2;
            }
            // Case 4 - right rotate at parent + color flips.
            let tmp2 = nodes[sibling].right;
            nodes[parent].left = tmp2;
            nodes[sibling].right = parent;
            rb_set_parent_color(nodes, tmp1, sibling, RB_BLACK);
            if tmp2 != NIL {
                rb_set_parent(nodes, tmp2, parent);
            }
            __rb_rotate_set_parents(nodes, parent, sibling, root, RB_BLACK);
            augment_rotate(nodes, ctx, parent, sibling);
            break;
        }
    }
}

/// `__rb_erase_color` — non-inline version for rb_erase_augmented() use.
fn __rb_erase_color<C>(nodes: &mut [RbNode], parent: usize, root: &mut RbRoot, ctx: &mut C, augment_rotate: AugFn<C>) {
    ____rb_erase_color(nodes, parent, root, ctx, augment_rotate);
}

/// `__rb_erase_augmented` — detach `node`, repair locally, and return the
/// parent under which ____rb_erase_color must run (NIL = no rebalance).
/// `tmp` tracks the deepest changed position for augment->propagate.
fn __rb_erase_augmented<C>(nodes: &mut [RbNode], node: usize, root: &mut RbRoot, ctx: &mut C, augment: &Augment<C>) -> usize {
    let child = nodes[node].right;
    let tmp_l = nodes[node].left;
    let mut parent;
    let rebalance;
    let tmp;
    let pc: u64;

    if tmp_l == NIL {
        // Case 1: no more than 1 child.
        pc = nodes[node].pc;
        parent = __rb_parent(pc);
        __rb_change_child(nodes, node, child, parent, root);
        if child != NIL {
            nodes[child].pc = pc;
            rebalance = NIL;
        } else {
            rebalance = if __rb_is_black(pc) { parent } else { NIL };
        }
        tmp = parent;
    } else if child == NIL {
        // Still case 1, but this time the child is node->rb_left.
        pc = nodes[node].pc;
        nodes[tmp_l].pc = pc;
        parent = __rb_parent(pc);
        __rb_change_child(nodes, node, tmp_l, parent, root);
        rebalance = NIL;
        tmp = parent;
    } else {
        let mut successor = child;
        let child2: usize;
        let mut t = nodes[child].left;
        if t == NIL {
            // Case 2: node's successor is its right child.
            parent = successor;
            child2 = nodes[successor].right;
            (augment.copy)(nodes, ctx, node, successor);
        } else {
            // Case 3: node's successor is leftmost under node's right
            // child subtree.
            loop {
                parent = successor;
                successor = t;
                t = nodes[t].left;
                if t == NIL {
                    break;
                }
            }
            child2 = nodes[successor].right;
            nodes[parent].left = child2; // WRITE_ONCE
            nodes[successor].right = child; // WRITE_ONCE
            rb_set_parent(nodes, child, successor);

            (augment.copy)(nodes, ctx, node, successor);
            (augment.propagate)(nodes, ctx, parent, successor);
        }

        let l = nodes[node].left;
        nodes[successor].left = l; // WRITE_ONCE
        rb_set_parent(nodes, l, successor);

        pc = nodes[node].pc;
        let par = __rb_parent(pc);
        __rb_change_child(nodes, node, successor, par, root);

        if child2 != NIL {
            rb_set_parent_color(nodes, child2, parent, RB_BLACK);
            rebalance = NIL;
        } else {
            rebalance = if rb_is_black(nodes, successor) {
                parent
            } else {
                NIL
            };
        }
        nodes[successor].pc = pc;
        tmp = successor;
    }

    (augment.propagate)(nodes, ctx, tmp, NIL);
    rebalance
}

/// `rb_insert_color` — dummy augmented callbacks (compiled out in C).
pub fn rb_insert_color(nodes: &mut [RbNode], node: usize, root: &mut RbRoot) {
    __rb_insert(nodes, node, root, &mut (), dummy_rotate);
}

pub fn rb_erase(nodes: &mut [RbNode], node: usize, root: &mut RbRoot) {
    let rebalance = __rb_erase_augmented(nodes, node, root, &mut (), &DUMMY);
    if rebalance != NIL {
        ____rb_erase_color(nodes, rebalance, root, &mut (), dummy_rotate);
    }
}

/// `rb_erase_linked` — unlink from the list, erase from the tree, clear;
/// returns "list still non-empty".
/// `lprev`/`lnext` are the caller's parallel in-order link arrays, indexed
/// like `nodes` (same index space; out-of-range entries panic, like C
/// dereferencing a corrupt list pointer).
pub fn rb_erase_linked(nodes: &mut [RbNode], lprev: &mut [usize], lnext: &mut [usize], i: usize, root: &mut RbRootLinked) -> bool {
    let p = lprev[i];
    let x = lnext[i];
    if p != NIL {
        lnext[p] = x;
    } else {
        root.rb_leftmost = x;
    }
    if x != NIL {
        lprev[x] = p;
    }
    rb_erase(nodes, i, &mut root.root);
    rb_clear_linked_node(nodes, lprev, lnext, i);
    root.rb_leftmost != NIL
}
pub fn rb_empty_linked_node(nodes: &[RbNode], i: usize) -> bool {
    rb_empty_node(nodes, i)
}

pub fn rb_insert_augmented<C>(nodes: &mut [RbNode], node: usize, root: &mut RbRoot, ctx: &mut C, augment: &Augment<C>) {
    __rb_insert(nodes, node, root, ctx, augment.rotate);
}
pub fn rb_insert_augmented_cached<C>(
    nodes: &mut [RbNode],
    node: usize,
    root: &mut RbRootCached,
    newleft: bool,
    ctx: &mut C,
    augment: &Augment<C>,
) {
    if newleft {
        root.rb_leftmost = node;
    }
    rb_insert_augmented(nodes, node, &mut root.root, ctx, augment);
}
pub fn rb_erase_augmented<C>(nodes: &mut [RbNode], node: usize, root: &mut RbRoot, ctx: &mut C, augment: &Augment<C>) {
    let rebalance = __rb_erase_augmented(nodes, node, root, ctx, augment);
    if rebalance != NIL {
        __rb_erase_color(nodes, rebalance, root, ctx, augment.rotate);
    }
}
pub fn rb_erase_augmented_cached<C>(nodes: &mut [RbNode], node: usize, root: &mut RbRootCached, ctx: &mut C, augment: &Augment<C>) {
    if root.rb_leftmost == node {
        root.rb_leftmost = rb_next(nodes, node);
    }
    rb_erase_augmented(nodes, node, &mut root.root, ctx, augment);
}

/// `rb_add_augmented_cached`: merge the new node's aug into every ancestor
/// on the way down, then insert like rb_add_cached. The caller must first
/// compute `node`'s augmented data so it describes `node` alone (the C
/// contract: "augmented data of @node must already describe @node alone").
pub fn rb_add_augmented_cached<C>(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRootCached,
    less: impl Fn(&[RbNode], usize, usize) -> bool,
    ctx: &mut C,
    augment: &Augment<C>,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;
    let mut leftmost = true;

    loop {
        let cur = link_get(nodes, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        (augment.merge)(nodes, ctx, parent, node);
        if less(nodes, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
            leftmost = false;
        }
    }

    rb_link_node(nodes, node, parent, link, &mut tree.root);
    rb_insert_augmented_cached(nodes, node, tree, leftmost, ctx, augment);

    if leftmost { node } else { NIL }
}

pub fn rb_next(nodes: &[RbNode], node: usize) -> usize {
    if rb_empty_node(nodes, node) {
        return NIL;
    }
    let mut node = node;
    // Right-hand child: go down then left as far as possible.
    if nodes[node].right != NIL {
        node = nodes[node].right;
        while nodes[node].left != NIL {
            node = nodes[node].left;
        }
        return node;
    }
    // No right child: first ancestor for which node is a left child.
    let mut parent;
    loop {
        parent = rb_parent(nodes, node);
        if parent != NIL && node == nodes[parent].right {
            node = parent;
        } else {
            break;
        }
    }
    parent
}

pub fn rb_prev(nodes: &[RbNode], node: usize) -> usize {
    if rb_empty_node(nodes, node) {
        return NIL;
    }
    let mut node = node;
    // Left-hand child: go down then right as far as possible.
    if nodes[node].left != NIL {
        node = nodes[node].left;
        while nodes[node].right != NIL {
            node = nodes[node].right;
        }
        return node;
    }
    // No left child: first ancestor for which node is a right child.
    let mut parent;
    loop {
        parent = rb_parent(nodes, node);
        if parent != NIL && node == nodes[parent].left {
            node = parent;
        } else {
            break;
        }
    }
    parent
}

pub fn rb_first(nodes: &[RbNode], root: &RbRoot) -> usize {
    let mut n = root.rb_node;
    if n == NIL {
        return NIL;
    }
    while nodes[n].left != NIL {
        n = nodes[n].left;
    }
    n
}

pub fn rb_last(nodes: &[RbNode], root: &RbRoot) -> usize {
    let mut n = root.rb_node;
    if n == NIL {
        return NIL;
    }
    while nodes[n].right != NIL {
        n = nodes[n].right;
    }
    n
}

pub fn rb_replace_node(nodes: &mut [RbNode], victim: usize, new: usize, root: &mut RbRoot) {
    let parent = rb_parent(nodes, victim);

    // Copy the pointers/colour from the victim to the replacement.
    nodes[new] = nodes[victim];

    // Set the surrounding nodes to point to the replacement.
    if nodes[victim].left != NIL {
        rb_set_parent(nodes, nodes[victim].left, new);
    }
    if nodes[victim].right != NIL {
        rb_set_parent(nodes, nodes[victim].right, new);
    }
    __rb_change_child(nodes, victim, new, parent, root);
}

pub fn rb_replace_node_rcu(nodes: &mut [RbNode], victim: usize, new: usize, root: &mut RbRoot) {
    let parent = rb_parent(nodes, victim);

    nodes[new] = nodes[victim];

    if nodes[victim].left != NIL {
        rb_set_parent(nodes, nodes[victim].left, new);
    }
    if nodes[victim].right != NIL {
        rb_set_parent(nodes, nodes[victim].right, new);
    }
    // __rb_change_child_rcu: same assignment, RCU ordering is a no-op here.
    __rb_change_child_rcu(nodes, victim, new, parent, root);
}

pub fn rb_left_deepest_node(nodes: &[RbNode], node: usize) -> usize {
    let mut node = node;
    loop {
        if nodes[node].left != NIL {
            node = nodes[node].left;
        } else if nodes[node].right != NIL {
            node = nodes[node].right;
        } else {
            return node;
        }
    }
}

pub fn rb_next_postorder(nodes: &[RbNode], node: usize) -> usize {
    if node == NIL {
        return NIL;
    }
    let parent = rb_parent(nodes, node);
    // If we're the parent's left node and it has a right node, go to that
    // then all the way down to the left; otherwise the parent is next.
    if parent != NIL && node == nodes[parent].left && nodes[parent].right != NIL {
        rb_left_deepest_node(nodes, nodes[parent].right)
    } else {
        parent
    }
}

pub fn rb_first_postorder(nodes: &[RbNode], root: &RbRoot) -> usize {
    if root.rb_node == NIL {
        return NIL;
    }
    rb_left_deepest_node(nodes, root.rb_node)
}

// ---- cached / linked / find-add inlines (rbtree.h / rbtree_augmented.h) ----

pub fn rb_first_cached(root: &RbRootCached) -> usize {
    root.rb_leftmost
}

pub fn rb_insert_color_cached(nodes: &mut [RbNode], node: usize, root: &mut RbRootCached, leftmost: bool) {
    if leftmost {
        root.rb_leftmost = node;
    }
    rb_insert_color(nodes, node, &mut root.root);
}

pub fn rb_erase_cached(nodes: &mut [RbNode], node: usize, root: &mut RbRootCached) -> usize {
    let mut leftmost = NIL;
    if root.rb_leftmost == node {
        leftmost = rb_next(nodes, node);
        root.rb_leftmost = leftmost;
    }
    rb_erase(nodes, node, &mut root.root);
    leftmost
}

pub fn rb_replace_node_cached(nodes: &mut [RbNode], victim: usize, new: usize, root: &mut RbRootCached) {
    if root.rb_leftmost == victim {
        root.rb_leftmost = new;
    }
    rb_replace_node(nodes, victim, new, &mut root.root);
}

/// `rb_add_cached` — returns `node` when it's the new leftmost, else NIL.
pub fn rb_add_cached(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRootCached,
    less: impl Fn(&[RbNode], usize, usize) -> bool,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;
    let mut leftmost = true;

    loop {
        let cur = link_get(nodes, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        if less(nodes, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
            leftmost = false;
        }
    }

    rb_link_node(nodes, node, parent, link, &mut tree.root);
    rb_insert_color_cached(nodes, node, tree, leftmost);

    if leftmost { node } else { NIL }
}

/// `rb_link_linked_node`: splice the linked node before/after its tree
/// parent in the in-order list. `link` encodes which child slot was taken.
fn rb_link_linked_node(_nodes: &mut [RbNode], lprev: &mut [usize], lnext: &mut [usize], node: usize, parent: usize, link: Link) {
    if parent == NIL {
        return;
    }
    match link {
        Link::Left(_) => {
            lprev[node] = lprev[parent];
            lnext[node] = parent;
            lprev[parent] = node;
            if lprev[node] != NIL {
                let p = lprev[node];
                lnext[p] = node;
            }
        }
        _ => {
            lnext[node] = lnext[parent];
            lprev[node] = parent;
            lnext[parent] = node;
            if lnext[node] != NIL {
                let x = lnext[node];
                lprev[x] = node;
            }
        }
    }
}

/// `rb_link_noop` — optimized away in C.
fn rb_link_noop(_nodes: &mut [RbNode], _node: usize, _parent: usize, _link: Link) {}

/// `__rb_add`.
fn __rb_add(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRoot,
    less: impl Fn(&[RbNode], usize, usize) -> bool,
    mut linkop: impl FnMut(&mut [RbNode], usize, usize, Link),
) {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(nodes, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        if less(nodes, node, parent) {
            link = Link::Left(parent);
        } else {
            link = Link::Right(parent);
        }
    }

    linkop(nodes, node, parent, link);
    rb_link_node(nodes, node, parent, link, tree);
    rb_insert_color(nodes, node, tree);
}

/// `rb_add_linked` — returns true when `node` is the new leftmost.
/// `lprev`/`lnext` must be parallel arrays indexed like `nodes`.
pub fn rb_add_linked(
    nodes: &mut [RbNode],
    lprev: &mut [usize],
    lnext: &mut [usize],
    node: usize,
    tree: &mut RbRootLinked,
    less: impl Fn(&[RbNode], usize, usize) -> bool,
) -> bool {
    __rb_add(nodes, node, &mut tree.root, less, |n, i, p, l| {
        rb_link_linked_node(n, lprev, lnext, i, p, l)
    });
    if lprev[node] == NIL {
        tree.rb_leftmost = node;
    }
    lprev[node] == NIL
}

pub fn rb_add(nodes: &mut [RbNode], node: usize, tree: &mut RbRoot, less: impl Fn(&[RbNode], usize, usize) -> bool) {
    __rb_add(nodes, node, tree, less, rb_link_noop);
}

/// `rb_find_add_cached` — returns the existing match, or NIL after inserting.
pub fn rb_find_add_cached(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRootCached,
    cmp: impl Fn(&[RbNode], usize, usize) -> i32,
) -> usize {
    let mut leftmost = true;
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(nodes, &tree.root, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(nodes, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
            leftmost = false;
        } else {
            return parent;
        }
    }

    rb_link_node(nodes, node, parent, link, &mut tree.root);
    rb_insert_color_cached(nodes, node, tree, leftmost);
    NIL
}

/// `rb_find_add` — returns the existing match, or NIL after inserting.
pub fn rb_find_add(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRoot,
    cmp: impl Fn(&[RbNode], usize, usize) -> i32,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(nodes, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(nodes, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
        } else {
            return parent;
        }
    }

    rb_link_node(nodes, node, parent, link, tree);
    rb_insert_color(nodes, node, tree);
    NIL
}

/// `rb_find_add_rcu` — adds via rb_link_node_rcu (plain assignment here).
pub fn rb_find_add_rcu(
    nodes: &mut [RbNode],
    node: usize,
    tree: &mut RbRoot,
    cmp: impl Fn(&[RbNode], usize, usize) -> i32,
) -> usize {
    let mut link = Link::Root;
    let mut parent = NIL;

    loop {
        let cur = link_get(nodes, tree, link);
        if cur == NIL {
            break;
        }
        parent = cur;
        let c = cmp(nodes, node, parent);
        if c < 0 {
            link = Link::Left(parent);
        } else if c > 0 {
            link = Link::Right(parent);
        } else {
            return parent;
        }
    }

    rb_link_node_rcu(nodes, node, parent, link, tree);
    rb_insert_color(nodes, node, tree);
    NIL
}

/// `rb_find` — returns the node matching `key` or NIL.
pub fn rb_find<K>(nodes: &[RbNode], key: &K, tree: &RbRoot, cmp: impl Fn(&[RbNode], &K, usize) -> i32) -> usize {
    let mut node = tree.rb_node;
    while node != NIL {
        let c = cmp(nodes, key, node);
        if c < 0 {
            node = nodes[node].left;
        } else if c > 0 {
            node = nodes[node].right;
        } else {
            return node;
        }
    }
    NIL
}

/// `rb_find_rcu` — identical (rcu_dereference_raw is a plain deref).
pub fn rb_find_rcu<K>(nodes: &[RbNode], key: &K, tree: &RbRoot, cmp: impl Fn(&[RbNode], &K, usize) -> i32) -> usize {
    rb_find(nodes, key, tree, cmp)
}

/// `rb_find_first` — leftmost node matching `key`, or NIL.
pub fn rb_find_first<K>(nodes: &[RbNode], key: &K, tree: &RbRoot, cmp: impl Fn(&[RbNode], &K, usize) -> i32) -> usize {
    let mut node = tree.rb_node;
    let mut m = NIL;
    while node != NIL {
        let c = cmp(nodes, key, node);
        if c <= 0 {
            if c == 0 {
                m = node;
            }
            node = nodes[node].left;
        } else {
            node = nodes[node].right;
        }
    }
    m
}

/// `rb_next_match` — next node matching `key`, or NIL.
pub fn rb_next_match<K>(nodes: &[RbNode], key: &K, node: usize, cmp: impl Fn(&[RbNode], &K, usize) -> i32) -> usize {
    let n = rb_next(nodes, node);
    if n != NIL && cmp(nodes, key, n) != 0 {
        NIL
    } else {
        n
    }
}

#[cfg(test)]
pub(crate) mod testaug {
    use super::*;

    /// Subtree-max augmentation payload for tests (`RB_DECLARE_CALLBACKS_MAX`).
    pub struct AugCtx<const M: usize> {
        /// Augmented value per node index (max `val` over subtree).
        pub aug: [u64; M],
        /// Base payload key per node index.
        pub val: [u64; M],
        /// Base payload key per node index (unused in this impl).
        pub key: [u64; M],
    }

    /// `compute(i, exit)` — recompute `aug[i]` = max(val[i], children's augs).
    /// RB_DECLARE_CALLBACKS_MAX expansion for `aug` = max(val) over the subtree.
pub fn aug_compute<const M: usize>(nodes: &[RbNode], ctx: &mut AugCtx<M>, i: usize, exit: bool) -> bool {
    let old_aug = ctx.aug[i];
    let mut val = ctx.val[i];
    if nodes[i].right != NIL {
        val = val.max(ctx.aug[nodes[i].right]);
    }
    if nodes[i].left != NIL {
        val = val.max(ctx.aug[nodes[i].left]);
    }
    if exit && old_aug == val {
        return true;
    }
    ctx.aug[i] = val;
    false
}
pub fn aug_propagate<const M: usize>(nodes: &[RbNode], ctx: &mut AugCtx<M>, rb: usize, stop: usize) {
    let mut rb = rb;
    while rb != stop {
        if aug_compute(nodes, ctx, rb, true) {
            break;
        }
        rb = rb_parent(nodes, rb);
    }
}
pub fn aug_copy<const M: usize>(_nodes: &[RbNode], ctx: &mut AugCtx<M>, old: usize, new: usize) {
    ctx.aug[new] = ctx.aug[old];
}
pub fn aug_rotate<const M: usize>(nodes: &[RbNode], ctx: &mut AugCtx<M>, old: usize, new: usize) {
    aug_copy(nodes, ctx, old, new);
    aug_compute(nodes, ctx, old, false);
}
pub fn aug_merge<const M: usize>(_nodes: &[RbNode], ctx: &mut AugCtx<M>, node: usize, new: usize) {
    ctx.aug[node] = ctx.aug[node].max(ctx.aug[new]);
}

pub fn aug_cb<const M: usize>() -> Augment<AugCtx<M>> {
        Augment {
            propagate: aug_propagate,
            copy: aug_copy,
            rotate: aug_rotate,
            merge: aug_merge,
        }
    }

}

#[cfg(test)]
mod tests {
    use super::testaug::*;
    use super::*;

    const M: usize = 64;

    fn xs64(s: &mut u64) -> u64 {
        let mut x = *s;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *s = x;
        x
    }

    fn collect_inorder(nodes: &[RbNode], root: &RbRoot) -> ([usize; M], usize) {
        let mut out = [NIL; M];
        let mut k = 0;
        let mut i = rb_first(nodes, root);
        while i != NIL {
            out[k] = i;
            k += 1;
            i = rb_next(nodes, i);
        }
        (out, k)
    }

    /// Black-height + no-red-red check; returns black height of subtree.
    fn check_rb(nodes: &[RbNode], i: usize) -> usize {
        if i == NIL {
            return 1;
        }
        let lb = check_rb(nodes, nodes[i].left);
        let rbh = check_rb(nodes, nodes[i].right);
        assert_eq!(lb, rbh, "black-height mismatch at {}", i);
        if rb_is_red(nodes, i) {
            let l = nodes[i].left;
            let r = nodes[i].right;
            assert!(l == NIL || rb_is_black(nodes, l), "red-red left at {}", i);
            assert!(r == NIL || rb_is_black(nodes, r), "red-red right at {}", i);
        }
        lb + rb_is_black(nodes, i) as usize
    }

    /// Insert `i` (key = `key[i]`) into the plain tree, duplicates go right.
    fn insert(nodes: &mut [RbNode], key: &[u64], root: &mut RbRoot, i: usize) {
        let mut link = Link::Root;
        let mut parent = NIL;
        let mut cur = root.rb_node;
        while cur != NIL {
            parent = cur;
            link = if key[i] < key[cur] {
                Link::Left(cur)
            } else {
                Link::Right(cur)
            };
            cur = match link {
                Link::Left(p) => nodes[p].left,
                Link::Right(p) => nodes[p].right,
                Link::Root => unreachable!(),
            };
        }
        rb_link_node(nodes, i, parent, link, root);
        rb_insert_color(nodes, i, root);
    }

    #[test]
    fn insert_erase_invariants() {
        let mut nodes = [RbNode::new(); M];
        let mut key = [0u64; M];
        let mut root = RbRoot::EMPTY;
        let mut s = 0x9e3779b97f4a7c15u64;
        let mut n = 0usize;

        for _ in 0..40 {
            key[n] = xs64(&mut s) % (3 * M as u64);
            insert(&mut nodes, &key, &mut root, n);
            n += 1;
        }
        check_rb(&nodes, root.rb_node);
        let (order, cnt) = collect_inorder(&nodes, &root);
        assert_eq!(cnt, n);
        for w in order[..cnt].windows(2) {
            assert!(key[w[0]] <= key[w[1]], "in-order not sorted");
        }

        // erase every third node, re-check invariants each time
        for (j, &v) in order.iter().enumerate() {
            if j % 3 == 0 && v != NIL {
                rb_erase(&mut nodes, v, &mut root);
                check_rb(&nodes, root.rb_node);
            }
        }
    }

    #[test]
    fn find_family() {
        let mut nodes = [RbNode::new(); M];
        let mut key = [0u64; M];
        let mut root = RbRoot::EMPTY;
        for i in 0..20 {
            key[i] = (i % 5) as u64; // heavy duplicates
            insert(&mut nodes, &key, &mut root, i);
        }
        let cmp = |_n: &[RbNode], k: &u64, i: usize| (*k as i64 - key[i] as i64).signum() as i32;
        let m = rb_find(&nodes, &3u64, &root, cmp);
        assert_ne!(m, NIL);
        assert_eq!(key[m], 3);
        assert_eq!(rb_find(&nodes, &9u64, &root, cmp), NIL);
        // first match is the leftmost key-3 node
        let f = rb_find_first(&nodes, &3u64, &root, cmp);
        assert_ne!(f, NIL);
        let mut seen = 0;
        let mut cur = f;
        while cur != NIL {
            assert_eq!(key[cur], 3);
            seen += 1;
            cur = rb_next_match(&nodes, &3u64, cur, cmp);
        }
        assert_eq!(seen, 4);
    }

    #[test]
    fn cached_root() {
        let mut nodes = [RbNode::new(); M];
        let mut key = [0u64; M];
        let mut root = RbRootCached::EMPTY;
        let mut s = 0xdeadbeefu64;
        for i in 0..30 {
            key[i] = xs64(&mut s) % 100;
            let mut link = Link::Root;
            let mut parent = NIL;
            let mut leftmost = true;
            let mut cur = root.root.rb_node;
            while cur != NIL {
                parent = cur;
                if key[i] < key[cur] {
                    link = Link::Left(cur);
                    cur = nodes[cur].left;
                } else {
                    link = Link::Right(cur);
                    cur = nodes[cur].right;
                    leftmost = false;
                }
            }
            rb_link_node(&mut nodes, i, parent, link, &mut root.root);
            rb_insert_color_cached(&mut nodes, i, &mut root, leftmost);
            let expect = (0..=i).min_by_key(|&j| key[j]).unwrap_or(NIL);
            assert_eq!(rb_first_cached(&root), expect, "cached leftmost");
        }
        assert_eq!(rb_first_cached(&root), rb_first(&nodes, &root.root));
        // erase the leftmost; cache must advance
        let lm = rb_first_cached(&root);
        rb_erase_cached(&mut nodes, lm, &mut root);
        assert_eq!(rb_first_cached(&root), rb_first(&nodes, &root.root));
    }

    #[test]
    fn replace_and_postorder() {
        let mut nodes = [RbNode::new(); M];
        let mut key = [0u64; M];
        let mut root = RbRoot::EMPTY;
        for i in 0..16 {
            key[i] = i as u64 * 7 % 23;
            insert(&mut nodes, &key, &mut root, i);
        }
        // replace node 5 with node 40 (same key)
        key[40] = key[5];
        rb_replace_node(&mut nodes, 5, 40, &mut root);
        let (order, cnt) = collect_inorder(&nodes, &root);
        assert_eq!(cnt, 16);
        for w in order[..cnt].windows(2) {
            assert!(key[w[0]] <= key[w[1]]);
        }
        // postorder visits all nodes exactly once
        let mut seen = [false; M];
        let mut cnt2 = 0;
        let mut i = rb_first_postorder(&nodes, &root);
        while i != NIL {
            assert!(!seen[i]);
            seen[i] = true;
            cnt2 += 1;
            i = rb_next_postorder(&nodes, i);
        }
        assert_eq!(cnt2, 16);
    }

    #[test]
    fn augmented_subtree_max() {
        let mut nodes = [RbNode::new(); M];
        let mut ctx = AugCtx { aug: [0; M], val: [0; M], key: [0; M] };
        let aug = aug_cb();
        let mut root = RbRootCached::EMPTY;
        let mut key = [0u64; M];
        let mut s = 0x1234567u64;
        for i in 0..24 {
            ctx.val[i] = xs64(&mut s) % 50;
            key[i] = xs64(&mut s) % 50;
            ctx.aug[i] = ctx.val[i];
            // rb_add_augmented_cached merges the new node's aug into every
            // ancestor on the way down, then rebalances.
            rb_add_augmented_cached(
                &mut nodes,
                i,
                &mut root,
                |n, a, b| {
                    let _ = n;
                    key[a] < key[b]
                },
                &mut ctx,
                &aug,
            );
            // brute-force: root aug = max over all present vals
            let expect = ctx.val[..=i].iter().copied().max().unwrap();
            assert_eq!(ctx.aug[root.root.rb_node], expect);
        }
        // every node's aug equals the max val of its subtree
        for i in 0..24 {
            if nodes[i].left == NIL && nodes[i].right == NIL {
                assert_eq!(ctx.aug[i], ctx.val[i]);
            }
        }
    }

    #[test]
    fn linked_nodes() {
        let mut nodes = [RbNode::new(); M];
        let mut lprev = [NIL; M];
        let mut lnext = [NIL; M];
        let mut key = [0u64; M];
        let mut root = RbRootLinked::EMPTY;
        let mut s = 0xabcdefu64;
        for i in 0..20 {
            key[i] = xs64(&mut s) % 100;
            rb_add_linked(&mut nodes, &mut lprev, &mut lnext, i, &mut root, |_n, a, b| {
                key[a] < key[b]
            });
        }
        // forward list order == in-order traversal
        let (order, cnt) = collect_inorder(&nodes, &root.root);
        assert_eq!(cnt, 20);
        let mut k = 0;
        let mut i = root.rb_leftmost;
        while i != NIL {
            assert_eq!(i, order[k]);
            assert_eq!(lprev[i], if k == 0 { NIL } else { order[k - 1] });
            k += 1;
            i = lnext[i];
        }
        assert_eq!(k, cnt);
        // erase half via rb_erase_linked; list stays consistent
        for j in (0..20).step_by(2) {
            rb_erase_linked(&mut nodes, &mut lprev, &mut lnext, order[j], &mut root);
        }
        let mut cnt3 = 0;
        let mut i = root.rb_leftmost;
        let mut prev = NIL;
        while i != NIL {
            cnt3 += 1;
            assert_eq!(lprev[i], prev);
            prev = i;
            i = lnext[i];
        }
        assert_eq!(cnt3, 10);
    }

    #[test]
    fn find_add_dedup() {
        let mut nodes = [RbNode::new(); M];
        let mut key = [0u64; M];
        let mut root = RbRoot::EMPTY;
        for i in 0..12 {
            key[i] = (i % 4) as u64;
            let dup = rb_find_add(&mut nodes, i, &mut root, |_n: &[RbNode], a: usize, b: usize| {
                (key[a] as i64 - key[b] as i64).signum() as i32
            });
            if i < 4 {
                assert_eq!(dup, NIL, "first insert of key {}", key[i]);
            } else {
                assert_ne!(dup, NIL, "dup of key {}", key[i]);
            }
        }
        let (_order, cnt) = collect_inorder(&nodes, &root);
        assert_eq!(cnt, 4, "only 4 distinct keys should be stored");
    }
}
