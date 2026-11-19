//! `lib/generic-radix-tree.c` + `include/linux/generic-radix-tree.h`,
//! ported faithfully. The C macro layer picks an object type per
//! instantiation; this module keeps the byte-offset `__genradix_*` core
//! and parameterizes object size, and the slot helpers below stand in
//! for the typed pointer the C API returns.
//!
//! Nodes live in a caller-owned arena (`GAlloc`) standing in for the
//! page allocator — including the harness's deterministic failure hook
//! (`genradix_alloc_budget`). Every C pointer becomes a `usize` index
//! (`GNIL` for NULL); `size_t` values are `u64`.

// ================= generic radix tree (lib/generic-radix-tree.c) =================

pub const GENRADIX_NODE_SHIFT: u32 = 9;
pub const GENRADIX_NODE_SIZE: u64 = 1 << GENRADIX_NODE_SHIFT; // 512
pub const GENRADIX_ARY: usize = 64; /* NODE_SIZE / sizeof(ptr) */
pub const GENRADIX_ARY_SHIFT: u32 = 6; /* ilog2(GENRADIX_ARY) */
pub const GENRADIX_DEPTH_MASK: u64 = 15; /* roundup_pow2(MAX_DEPTH+1)-1 */
pub const GNIL: usize = usize::MAX;
pub const ENOMEM: i32 = 12;

pub fn ilog2(n: u64) -> i64 {
    if n == 0 {
        -1
    } else {
        63 - n.leading_zeros() as i64
    }
}

pub fn is_power_of_2(n: u64) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

pub fn round_down(x: u64, y: u64) -> u64 {
    x & !(y - 1)
}

pub fn round_up(x: u64, y: u64) -> u64 {
    /* (((x)-1) | (y-1)) + 1, with C's unsigned wrap */
    (x.wrapping_sub(1) | (y - 1)).wrapping_add(1)
}

pub fn genradix_depth_shift(depth: u32) -> u32 {
    GENRADIX_NODE_SHIFT + GENRADIX_ARY_SHIFT * depth
}

/// `genradix_depth_size`; the wrapping shift reproduces the x86-64
/// `1UL << shift` result for shift >= 64 (only reachable at absurd depth).
pub fn genradix_depth_size(depth: u32) -> u64 {
    1u64.wrapping_shl(genradix_depth_shift(depth))
}

/// `struct genradix_node` — a union in C (interior children[] xor leaf
/// data[]); the model keeps both fields since a node's role is fixed by
/// its level.
pub struct GNode {
    pub children: [usize; GENRADIX_ARY],
    pub data: [u8; GENRADIX_NODE_SIZE as usize],
}

impl GNode {
    /// `kzalloc(GENRADIX_NODE_SIZE)` — always zeroed.
    pub fn zeroed() -> Self {
        Self {
            children: [GNIL; GENRADIX_ARY],
            data: [0; GENRADIX_NODE_SIZE as usize],
        }
    }
}

/// The page allocator: node arena plus the harness's deterministic
/// failure hook (`genradix_alloc_budget` in the C harness).
pub struct GAlloc {
    /// -1 = never fail; 0 = fail now; >0 = calls left until a failure.
    pub budget: i64,
    pub nodes: Vec<GNode>,
    pub free: Vec<usize>,
}

impl GAlloc {
    pub fn new() -> Self {
        Self {
            budget: -1,
            nodes: Vec::new(),
            free: Vec::new(),
        }
    }

    /// `genradix_alloc_node` — one kzalloc() call (counts against the
    /// harness budget exactly like the C kzalloc shim).
    pub fn alloc_node(&mut self) -> Option<usize> {
        if self.budget == 0 {
            return None;
        }
        if self.budget > 0 {
            self.budget -= 1;
        }
        if let Some(i) = self.free.pop() {
            self.nodes[i] = GNode::zeroed();
            Some(i)
        } else {
            self.nodes.push(GNode::zeroed());
            Some(self.nodes.len() - 1)
        }
    }

    /// `genradix_free_node`.
    pub fn free_node(&mut self, i: usize) {
        self.free.push(i);
    }
}

/// `struct __genradix` — `root` packs node index + depth in the low bits
/// exactly like C packs the pointer's low GENRADIX_DEPTH_MASK bits:
/// word = (idx+1)<<4 | depth, 0 means NULL (empty tree).
pub struct Genradix {
    pub root: u64,
}

impl Genradix {
    /// `genradix_init`.
    pub fn new() -> Self {
        Self { root: 0 }
    }
}

pub fn root_to_node(w: u64) -> usize {
    if w == 0 {
        GNIL
    } else {
        ((w >> 4) - 1) as usize
    }
}

pub fn root_to_depth(w: u64) -> u32 {
    (w & GENRADIX_DEPTH_MASK) as u32
}

pub fn mk_root(node: usize, depth: u32) -> u64 {
    (((node as u64) + 1) << 4) | depth as u64
}

/// `__idx_to_offset` — byte offset of object `idx` with size `obj_size`.
pub fn idx_to_offset(idx: u64, obj_size: u64) -> u64 {
    debug_assert!(obj_size <= GENRADIX_NODE_SIZE); /* BUILD_BUG_ON/bug_on */
    if !is_power_of_2(obj_size) {
        let objs_per_page = GENRADIX_NODE_SIZE / obj_size;
        (idx / objs_per_page)
            .wrapping_mul(GENRADIX_NODE_SIZE)
            .wrapping_add((idx % objs_per_page).wrapping_mul(obj_size))
    } else {
        idx.wrapping_mul(obj_size)
    }
}

/// A leaf slot location: (node index, offset into `data`).
pub type Slot = (usize, u32);

/// `__genradix_ptr_inlined` — offset must already be a byte offset.
pub fn genradix_ptr_inlined(g: &Genradix, a: &GAlloc, offset: u64) -> Option<Slot> {
    let w = g.root;
    let mut n = root_to_node(w);
    let level = root_to_depth(w);
    let mut shift = genradix_depth_shift(level);
    let mut offset = offset;

    if ilog2(offset) >= shift as i64 {
        return None;
    }
    while n != GNIL && shift > GENRADIX_NODE_SHIFT {
        shift -= GENRADIX_ARY_SHIFT;
        n = a.nodes[n].children[offset.wrapping_shr(shift) as usize];
        offset &= (1u64 << shift) - 1;
    }
    if n != GNIL {
        Some((n, offset as u32))
    } else {
        None
    }
}

/// `__genradix_ptr` / `genradix_ptr` at the object-size level.
pub fn genradix_ptr(g: &Genradix, a: &GAlloc, idx: u64, obj_size: u64) -> Option<Slot> {
    genradix_ptr_inlined(g, a, idx_to_offset(idx, obj_size))
}

/// `__genradix_ptr_alloc` — `preallocated` is `struct genradix_node **`:
/// the callee takes ownership of the node either by linking it or by
/// freeing it, so it is always `None` on return.
pub fn genradix_ptr_alloc_off(
    g: &mut Genradix,
    a: &mut GAlloc,
    offset: u64,
    preallocated: &mut Option<usize>,
) -> Option<Slot> {
    let mut offset = offset;
    let mut v = g.root;
    let mut new_node: Option<usize> = None;

    if preallocated.is_some() {
        core::mem::swap(&mut new_node, preallocated);
    }

    /* Increase tree depth if necessary. In the single-threaded harness
     * cmpxchg_release always succeeds on the first try. */
    let mut n;
    let mut level;
    loop {
        let r = v;
        n = root_to_node(r);
        level = root_to_depth(r);

        if n != GNIL && ilog2(offset) < genradix_depth_shift(level) as i64 {
            break;
        }
        if new_node.is_none() {
            new_node = a.alloc_node();
            new_node?;
        }
        let nn = new_node.unwrap();
        a.nodes[nn].children[0] = n;
        let new_root = mk_root(nn, if n != GNIL { level + 1 } else { 0 });
        g.root = new_root; /* cmpxchg_release(&radix->root, r, new_root) == r */
        v = new_root;
        new_node = None;
    }

    while level > 0 {
        level -= 1;
        let i = offset.wrapping_shr(genradix_depth_shift(level)) as usize;
        offset &= genradix_depth_size(level) - 1;

        let child = a.nodes[n].children[i];
        if child != GNIL {
            n = child;
        } else {
            if new_node.is_none() {
                new_node = a.alloc_node();
                new_node?;
            }
            let nn = new_node.unwrap();
            /* cmpxchg_release(p, NULL, new_node) == NULL */
            a.nodes[n].children[i] = nn;
            n = nn;
            new_node = None;
        }
    }

    if let Some(nn) = new_node {
        a.free_node(nn);
    }
    Some((n, offset as u32))
}

/// `genradix_ptr_alloc` / `genradix_ptr_alloc_preallocated` /
/// `genradix_ptr_alloc_inlined` / `_preallocated_inlined` at the
/// object-size level.
pub fn genradix_ptr_alloc(
    g: &mut Genradix,
    a: &mut GAlloc,
    idx: u64,
    preallocated: &mut Option<usize>,
    obj_size: u64,
) -> Option<Slot> {
    genradix_ptr_alloc_off(g, a, idx_to_offset(idx, obj_size), preallocated)
}

/// `struct genradix_iter`.
pub struct GIter {
    pub offset: u64,
    pub pos: u64,
}

/// `genradix_iter_init`.
pub fn genradix_iter_init(idx: u64, obj_size: u64) -> GIter {
    GIter {
        pos: idx,
        offset: idx_to_offset(idx, obj_size),
    }
}

/// `__genradix_iter_peek`.
pub fn genradix_iter_peek(
    iter: &mut GIter,
    g: &Genradix,
    a: &GAlloc,
    objs_per_page: u64,
) -> Option<Slot> {
    if iter.offset == u64::MAX {
        return None;
    }

    'restart: loop {
        let w = g.root;
        if w == 0 {
            return None;
        }
        let mut n = root_to_node(w);
        let mut level = root_to_depth(w);

        if ilog2(iter.offset) >= genradix_depth_shift(level) as i64 {
            return None;
        }

        while level > 0 {
            level -= 1;
            let mut i = (iter.offset.wrapping_shr(genradix_depth_shift(level))
                & (GENRADIX_ARY as u64 - 1)) as usize;

            while a.nodes[n].children[i] == GNIL {
                let objs_per_ptr = genradix_depth_size(level);

                if iter.offset.wrapping_add(objs_per_ptr) < iter.offset {
                    iter.offset = u64::MAX;
                    iter.pos = u64::MAX;
                    return None;
                }
                i += 1;
                iter.offset = round_down(iter.offset.wrapping_add(objs_per_ptr), objs_per_ptr);
                iter.pos = (iter.offset >> GENRADIX_NODE_SHIFT).wrapping_mul(objs_per_page);
                if i == GENRADIX_ARY {
                    continue 'restart;
                }
            }
            n = a.nodes[n].children[i];
        }

        return Some((n, (iter.offset & (GENRADIX_NODE_SIZE - 1)) as u32));
    }
}

/// `__genradix_iter_peek_prev`.
pub fn genradix_iter_peek_prev(
    iter: &mut GIter,
    g: &Genradix,
    a: &GAlloc,
    objs_per_page: u64,
    obj_size_plus_page_remainder: u64,
) -> Option<Slot> {
    if iter.offset == u64::MAX {
        return None;
    }

    'restart: loop {
        let w = g.root;
        if w == 0 {
            return None;
        }
        let mut n = root_to_node(w);
        let mut level = root_to_depth(w);

        if ilog2(iter.offset) >= genradix_depth_shift(level) as i64 {
            iter.offset = genradix_depth_size(level);
            iter.pos = (iter.offset >> GENRADIX_NODE_SHIFT).wrapping_mul(objs_per_page);
            iter.offset = iter.offset.wrapping_sub(obj_size_plus_page_remainder);
            iter.pos = iter.pos.wrapping_sub(1);
        }

        while level > 0 {
            level -= 1;
            let mut i = (iter.offset.wrapping_shr(genradix_depth_shift(level))
                & (GENRADIX_ARY as u64 - 1)) as usize;

            while a.nodes[n].children[i] == GNIL {
                let objs_per_ptr = genradix_depth_size(level);

                iter.offset = round_down(iter.offset, objs_per_ptr);
                iter.pos = (iter.offset >> GENRADIX_NODE_SHIFT).wrapping_mul(objs_per_page);
                if iter.offset == 0 {
                    return None;
                }
                iter.offset = iter.offset.wrapping_sub(obj_size_plus_page_remainder);
                iter.pos = iter.pos.wrapping_sub(1);
                if i == 0 {
                    continue 'restart;
                }
                i -= 1;
            }
            n = a.nodes[n].children[i];
        }

        return Some((n, (iter.offset & (GENRADIX_NODE_SIZE - 1)) as u32));
    }
}

/// `__genradix_iter_advance`.
pub fn genradix_iter_advance(iter: &mut GIter, obj_size: u64) {
    if iter.offset.wrapping_add(obj_size) < iter.offset {
        iter.offset = u64::MAX;
        iter.pos = u64::MAX;
        return;
    }
    iter.offset = iter.offset.wrapping_add(obj_size);
    if !is_power_of_2(obj_size)
        && (iter.offset & (GENRADIX_NODE_SIZE - 1)) + obj_size > GENRADIX_NODE_SIZE
    {
        iter.offset = round_up(iter.offset, GENRADIX_NODE_SIZE);
    }
    iter.pos = iter.pos.wrapping_add(1);
}

/// `__genradix_iter_rewind`.
pub fn genradix_iter_rewind(iter: &mut GIter, obj_size: u64) {
    if iter.offset == 0 || iter.offset == u64::MAX {
        iter.offset = u64::MAX;
        return;
    }
    if (iter.offset & (GENRADIX_NODE_SIZE - 1)) == 0 {
        iter.offset = iter.offset.wrapping_sub(GENRADIX_NODE_SIZE % obj_size);
    }
    iter.offset = iter.offset.wrapping_sub(obj_size);
    iter.pos = iter.pos.wrapping_sub(1);
}

/// `genradix_last_pos`.
pub fn genradix_last_pos(objs_per_page: u64) -> u64 {
    u64::MAX
        .wrapping_div(GENRADIX_NODE_SIZE)
        .wrapping_mul(objs_per_page)
        .wrapping_sub(1)
}

/// `genradix_free_recurse`.
pub fn genradix_free_recurse(a: &mut GAlloc, n: usize, level: u32) {
    if n == GNIL {
        return; /* genradix_free_node(NULL) is a no-op */
    }
    if level > 0 {
        for i in 0..GENRADIX_ARY {
            let c = a.nodes[n].children[i];
            if c != GNIL {
                genradix_free_recurse(a, c, level - 1);
            }
        }
    }
    a.free_node(n);
}

/// `__genradix_free` — `xchg(&radix->root, NULL)` then recursive free.
pub fn genradix_free(g: &mut Genradix, a: &mut GAlloc) {
    let w = g.root;
    g.root = 0;
    genradix_free_recurse(a, root_to_node(w), root_to_depth(w));
}

/// `__genradix_prealloc`.
pub fn genradix_prealloc(g: &mut Genradix, a: &mut GAlloc, size: u64) -> i32 {
    let mut offset = 0;
    while offset < size {
        if genradix_ptr_alloc_off(g, a, offset, &mut None).is_none() {
            return -ENOMEM;
        }
        offset += GENRADIX_NODE_SIZE;
    }
    0
}

// ---- leaf slot access (the object the C pointer points to) ----

pub fn slot_u64(a: &GAlloc, s: Slot) -> u64 {
    let (n, off) = s;
    u64::from_le_bytes(
        a.nodes[n].data[off as usize..off as usize + 8]
            .try_into()
            .unwrap(),
    )
}

pub fn slot_u64_set(a: &mut GAlloc, s: Slot, v: u64) {
    let (n, off) = s;
    a.nodes[n].data[off as usize..off as usize + 8].copy_from_slice(&v.to_le_bytes());
}

pub fn slot_u64_at(a: &GAlloc, s: Slot, field_off: u32) -> u64 {
    slot_u64(a, (s.0, s.1 + field_off))
}

pub fn slot_u64_at_set(a: &mut GAlloc, s: Slot, field_off: u32, v: u64) {
    slot_u64_set(a, (s.0, s.1 + field_off), v)
}

pub fn slot_u32_at(a: &GAlloc, s: Slot, field_off: u32) -> u32 {
    let (n, off) = (s.0, s.1 + field_off);
    u32::from_le_bytes(
        a.nodes[n].data[off as usize..off as usize + 4]
            .try_into()
            .unwrap(),
    )
}

pub fn slot_u32_at_set(a: &mut GAlloc, s: Slot, field_off: u32, v: u32) {
    let (n, off) = (s.0, s.1 + field_off);
    a.nodes[n].data[off as usize..off as usize + 4].copy_from_slice(&v.to_le_bytes());
}
