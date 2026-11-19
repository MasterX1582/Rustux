#!/usr/bin/env python3
"""Extract verified lib-idr-radix port -> klib::radix_tree + klib::idr.

radix_tree.rs: the shared engine (slim xarray + radix-tree node internals)
as `pub struct RadixTree<'a>` (upstream radix_tree_root). idr.rs: `Idr` and
`Ida` wrapping a RadixTree engine (upstream idr.idr_rt / ida.xa).
"""
import re

SRC = "ports/lib-idr-radix/rust/wrappers-rs/src/port.rs"
lines = open(SRC).read().split("\n")

# ---------- fn table (brace-matched) ----------
funcs, order = {}, []
i = 0
while i < len(lines):
    m = re.match(r"^(pub )?fn ([a-zA-Z0-9_]+)", lines[i])
    if m:
        start, depth, j, seen = i, 0, i, False
        while j < len(lines):
            depth += lines[j].count("{") - lines[j].count("}")
            if "{" in lines[j]:
                seen = True
            j += 1
            if seen and depth <= 0:
                break
        funcs[m.group(2)] = (start + 1, j)
        order.append(m.group(2))
        i = j
    else:
        i += 1


def body(name):
    s, e = funcs[name]
    return "\n".join(lines[s - 1:e])


# ---------- classification ----------
IDR_M = {"idr_preload", "idr_get_free", "idr_destroy", "idr_init_base",
         "idr_alloc_u32", "idr_alloc", "idr_alloc_cyclic", "idr_remove",
         "idr_find", "idr_get_next_ul", "idr_get_next", "idr_replace",
         "idr_get_cursor", "idr_set_cursor"}
IDA_M = ({"kzalloc_bitmap", "kfree_bitmap"}
         | {n for n in order if n.startswith("ida_")}) - {"ida_dump", "ida_dump_entry"}
IDA_FREE = {"find_next_zero_bit1", "find_in_arr", "find_next_zero_arr",
            "find_next_arr", "bm_set_bit", "bm_clear_bit", "bm_test_bit",
            "bm_full", "bm_empty", "bmap_canon", "bmap_idx"}
FREE_DROP_P = {"find_marked_out"}
MANUAL = {"scanon", "is_node_ptr", "preload_push", "__radix_tree_preload",
          "each_slot", "gang_lookup_impl", "radix_tree_gang_lookup_tag_slot",
          "idr_for_each", "idr_is_empty_u"}
PUB_RT = {"radix_tree_preload", "radix_tree_maybe_preload", "radix_tree_insert",
          "radix_tree_lookup_slot", "radix_tree_lookup", "radix_tree_replace_slot",
          "radix_tree_iter_replace", "radix_tree_tag_set", "radix_tree_tag_clear",
          "radix_tree_iter_tag_clear", "radix_tree_tag_get", "radix_tree_tagged",
          "radix_tree_next_chunk", "radix_tree_iter_lookup", "radix_tree_next_slot",
          "radix_tree_iter_delete", "radix_tree_delete_item", "radix_tree_delete"}
RENAME_PUB = {"rt_iter_init": "radix_tree_iter_init",
              "rt_iter_retry": "radix_tree_iter_retry",
              "rt_iter_add": "radix_tree_iter_add",
              "rt_iter_resume": "radix_tree_iter_resume",
              "rt_chunk_size": "radix_tree_chunk_size"}
DROP = set()


def uses_p(txt):
    return "p: &" in txt or re.search(r"\bp\.", txt) is not None


for n in order:
    if n in IDR_M or n in IDA_M or n in IDA_FREE or n in FREE_DROP_P or n in MANUAL:
        continue
    if (n.startswith(("gen_", "op_", "wrap_", "seq_"))
            or n in {"iabs", "efold", "xa_dump_index_f", "xa_dump_node_f",
                     "arraycanon", "ida_dump_entry", "ida_dump", "dump_state",
                     "rt_walk_fold", "xa_dump_index", "root_canon"}):
        DROP.add(n)

rt_methods, rt_free, idr_methods, ida_methods, ida_free = [], [], [], [], []

# ---------- transforms ----------
def strip_sig(t):
    t = t.replace("p: &mut Pool", "&mut self").replace("p: &Pool", "&self")
    t = re.sub(r"\bxa: usize,\s*", "", t)
    t = re.sub(r",\s*\bxa: usize", "", t)
    return t


def drop_args(t, recv, lit=None):
    lit = lit if lit is not None else recv
    t = re.sub(recv + r"(\w+\()\s*(xa|xas\.xa|T_IDR|T_IDA|T_RT)\s*\)", lit + r"\1)", t)
    t = re.sub(recv + r"(\w+\()\s*(xa|xas\.xa|T_IDR|T_IDA|T_RT)\s*,\s*", lit + r"\1", t)
    t = re.sub(r",\s*(xas\.xa|T_IDR|T_IDA|T_RT|\bxa\b)\s*,", ",", t)
    t = re.sub(r",\s*(xas\.xa|T_IDR|T_IDA|T_RT|\bxa\b)\s*\)", ")", t)
    return t


def common(t):
    t = re.sub(r"SlotL::Root\([^)]*\)", "SlotL::Root", t)
    t = re.sub(r"Xas::new\([^,]+,\s*", "Xas::new(", t)
    t = re.sub(r"\s*let _ = p;", "", t)
    for old, new in RENAME_PUB.items():
        t = t.replace(old + "(", new + "(")
    # F5 — slot offset arithmetic wraps like C pointer arithmetic; wild
    # SlotL::Node indices hit the bound rather than panic
    t = t.replace("SlotL::Node(ni, o) => SlotL::Node(ni, o + 1),",
                  "SlotL::Node(ni, o) => SlotL::Node(ni, o.wrapping_add(1)),")
    t = t.replace("SlotL::Node(ni, o) => SlotL::Node(ni, o + k as usize),",
                  "SlotL::Node(ni, o) => SlotL::Node(ni, o.wrapping_add(k as usize)),")
    t = t.replace("""        SlotL::Node(ni, o) => self.nodes[ni].slots[o],""",
                  """        SlotL::Node(ni, o) if ni < self.nodes.len() && o < CHUNK_SIZE => {
            self.nodes[ni].slots[o]
        }
        _ => 0,""")
    t = t.replace("""        SlotL::Node(ni, o) => self.nodes[ni].slots[o] = v,""",
                  """        SlotL::Node(ni, o) => {
            if self.warn_on(ni >= self.nodes.len() || o >= CHUNK_SIZE) {
                return;
            }
            self.nodes[ni].slots[o] = v;
        }""")
    return t


def x_engine(t):
    t = strip_sig(t)

    # p-calls first (before p. field rewrite)
    t = re.sub(r"(\w+)\(\s*p\s*,\s*", r"self.\1(", t)
    t = re.sub(r"(\w+)\(\s*p\s*\)", r"self.\1()", t)
    t = re.sub(r"\bp\.n\b", "self.nodes", t)
    t = t.replace("p.", "self.")
    t = re.sub(r"self\.roots\[[^\]]*\]", "self.root", t)
    t = re.sub(r"root_canon\([^)]*\)", "self.canon", t)
    t = re.sub(r"self\.used\[([^\]]+)\]", r"self.nodes[\1].used", t)
    t = re.sub(r"self\.shadow\.insert\(0,\s*([^)]+)\)", r"self.shadow_push(\1)", t)
    t = re.sub(r"self\.shadow\.retain\(\|&x\|\s*x != ([^)]+)\);", r"self.shadow_remove(\1);", t)
    t = t.replace("self.preload.pop()", "self.pl_pop()")
    t = t.replace("self.preload.last()", "self.pl_top()")
    t = t.replace("self.preload.len()", "self.plen")
    t = t.replace("self.preload.clear()", "self.plen = 0;")
    t = re.sub(r"\bNP\b", "self.nodes.len()", t)
    t = common(t)
    t = drop_args(t, r"self\.", "self.")
    t = t.replace("self.is_node_ptr(", "self.is_node_ptr(")
    t = re.sub(r"\bis_node_ptr\(", "self.is_node_ptr(", t)
    t = t.replace("self.find_marked_out(", "find_marked_out(")
    if "fn xa_find" in t:
        # run-14 parity: bound the xas_retry livelock — a persistent
        # XA_RETRY_ENTRY in the walked path loops forever in the
        # sequential model (upstream retries only on concurrent mutation)
        t = t.replace("""    let mut xas = Xas::new(*index);
    let mut entry;
    loop {""", """    let mut xas = Xas::new(*index);
    let mut entry;
    let mut tries = 0u32;
    loop {""")
        t = t.replace("""        if !xas_retry(&mut xas, entry) {
            break;
        }""", """        tries += 1;
        if !xas_retry(&mut xas, entry) || tries >= 256 {
            break;
        }""")
    return t


def x_veneer(t, mod):
    t = strip_sig(t)
    if mod == "idr":
        t = t.replace("p.idr_base", "self.idr_base").replace("p.idr_next", "self.idr_next")
    if mod == "ida":
        t = re.sub(r"p\.bmaps\[([^\]]+)\]", r"self.bmaps[\1].bits", t)
        t = re.sub(r"p\.bused\[([^\]]+)\]", r"self.bmaps[\1].used", t)
        t = re.sub(r"\bNB\b", "self.bmaps.len()", t)
    locals_ = IDR_M if mod == "idr" else IDA_M

    def rep(m):
        n = m.group(1)
        return ("self." if n in locals_ else "self.rt.") + n + "("

    def rep0(m):
        n = m.group(1)
        return ("self." if n in locals_ else "self.rt.") + n + "()"

    t = re.sub(r"(\w+)\(\s*p\s*,\s*", rep, t)
    t = re.sub(r"(\w+)\(\s*p\s*\)", rep0, t)
    t = t.replace("p.", "self.rt.")
    t = re.sub(r"self\.rt\.roots\[[^\]]*\]", "self.rt.root", t)
    t = re.sub(r"root_canon\([^)]*\)", "self.rt.canon", t)
    t = re.sub(r"self\.rt\.used\[([^\]]+)\]", r"self.rt.nodes[\1].used", t)
    t = re.sub(r"self\.rt\.n\b", "self.rt.nodes", t)
    t = re.sub(r"\bNP\b", "self.rt.nodes.len()", t)
    t = common(t)
    t = drop_args(t, r"self\.rt\.", "self.rt.")
    t = drop_args(t, r"self\.", "self.")
    t = re.sub(r"\bis_node_ptr\(", "self.rt.is_node_ptr(", t)
    t = t.replace("self.rt.find_marked_out(", "find_marked_out(")
    if "fn ida_is_empty" in t:
        t = t.replace("-> u64", "-> bool").replace(" as u64", "")
    # clippy: C's `id < base ? 0 : id - base` == saturating_sub
    t = t.replace("id = if id < base { 0 } else { id - base };",
                  "id = id.saturating_sub(base);")
    t = t.replace("""    let size;
    let bit;
    if xa_is_value(entry) {
        let tmp = xa_to_value(entry);
        size = BITS_PER_XA_VALUE;
        bit = find_next_zero_bit1(!tmp, size, offset);
    } else {
        let bm = &self.bmaps[bmap_idx(entry)].bits;
        size = IDA_BITMAP_BITS;
        bit = find_next_arr(bm, size, offset);
    }""",
                  """    let (size, bit) = if xa_is_value(entry) {
        let tmp = xa_to_value(entry);
        (BITS_PER_XA_VALUE, find_next_zero_bit1(!tmp, BITS_PER_XA_VALUE, offset))
    } else {
        let bm = &self.bmaps[bmap_idx(entry)].bits;
        (IDA_BITMAP_BITS, find_next_arr(bm, IDA_BITMAP_BITS, offset))
    };""")
    return t


for n in order:
    if n in DROP or n in MANUAL:
        continue
    txt = body(n)
    if n in IDR_M:
        idr_methods.append((n, x_veneer(txt, "idr")))
    elif n in IDA_M:
        ida_methods.append((n, x_veneer(txt, "ida")))
    elif n in IDA_FREE:
        ida_free.append(txt)


def vis(t, level):
    return re.sub(r"^fn ", level + "fn ", t, count=1)


CONST_BLOCK = """
/* ---- canonical encodings / constants ---- */
pub(crate) const NILX: u64 = 0xffff;
pub(crate) const NBASE: u64 = 0x40000;
pub(crate) const NSTRIDE: u64 = 4096;
pub(crate) const BBASE: u64 = 0x200000;
pub(crate) const RTIDR_CANON: u64 = 0x810000;
pub(crate) const IDAX_CANON: u64 = 0x820000;
pub(crate) const RRT_CANON: u64 = 0x830000;
pub(crate) const XA_RCU_FREE: u64 = 1;
pub(crate) const CHUNK_SHIFT: u32 = 6;
pub(crate) const CHUNK_SIZE: usize = 64;
pub(crate) const CHUNK_MASK: u64 = 63;
pub(crate) const MAX_MARKS: usize = 3;
pub(crate) const XA_FLAGS_TRACK_FREE: u32 = 4;
pub(crate) const XA_FLAGS_ZERO_BUSY: u32 = 8;
pub(crate) const GFP_BITS_SHIFT: u32 = 24;
pub(crate) const ROOT_TAG_SHIFT: u32 = GFP_BITS_SHIFT;
pub(crate) const ROOT_IS_IDR: u32 = 4;
pub(crate) const IDR_RT_MARKER: u32 = ROOT_IS_IDR | (1 << (ROOT_TAG_SHIFT + IDR_FREE));
pub(crate) const XA_LOCK_IRQ: u32 = 1;
pub(crate) const XA_FLAGS_LOCK_IRQ: u32 = XA_LOCK_IRQ;
pub(crate) const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
pub(crate) const IDA_INIT_FLAGS: u32 = XA_FLAGS_LOCK_IRQ | XA_FLAGS_ALLOC;
pub(crate) const XA_MARK_0: u32 = 0;
pub(crate) const XA_MARK_2: u32 = 2;
pub(crate) const XA_PRESENT: u32 = 8;
pub(crate) const XA_FREE_MARK: u32 = XA_MARK_0;
pub(crate) const XA_MARK_MAX: u32 = XA_MARK_2;
pub(crate) const IDR_FREE: u32 = 0;
pub(crate) const ENOMEM: i64 = 12;
pub(crate) const EINVAL: i64 = 22;
pub(crate) const ENOENT: i64 = 2;
pub(crate) const EEXIST: i64 = 17;
pub(crate) const ENOSPC: i64 = 28;
pub(crate) const MAX_ERRNO: i64 = 4095;
pub(crate) const XAS_BOUNDS: u64 = 1;
pub(crate) const XAS_RESTART: u64 = 3;
pub(crate) const XA_ZERO_ENTRY: u64 = 1030;
pub(crate) const XA_RETRY_ENTRY: u64 = 1026;
pub(crate) const RADIX_TREE_RETRY: u64 = XA_RETRY_ENTRY;
pub const IDA_BITMAP_LONGS: usize = 16;
pub const IDA_BITMAP_BITS: u64 = (IDA_BITMAP_LONGS * 64) as u64;
pub(crate) const BITS_PER_XA_VALUE: u64 = 63;
pub(crate) const BITS_PER_LONG: u64 = 64;
pub const RADIX_TREE_MAX_TAGS: u32 = MAX_MARKS as u32;
pub const RADIX_TREE_MAP_SHIFT: u32 = CHUNK_SHIFT;
pub const RADIX_TREE_MAP_SIZE: u64 = CHUNK_SIZE as u64;
pub const RADIX_TREE_MAP_MASK: u64 = CHUNK_MASK;
pub(crate) const RADIX_TREE_INDEX_BITS: u32 = 64;
pub const RADIX_TREE_MAX_PATH: u32 = RADIX_TREE_INDEX_BITS.div_ceil(CHUNK_SHIFT);
pub const RADIX_TREE_PRELOAD_SIZE: u32 = RADIX_TREE_MAX_PATH * 2 - 1; /* 21 */
pub(crate) const IDR_MAX_PATH: u32 = 31u32.div_ceil(CHUNK_SHIFT); /* 6 */
pub(crate) const IDR_PRELOAD_SIZE: u32 = IDR_MAX_PATH * 2 - 1; /* 11 */
pub const ITER_TAG_MASK: u32 = 0x0f;
pub const ITER_TAGGED: u32 = 0x10;
pub const ITER_CONTIG: u32 = 0x20;
pub(crate) const GFP_DIRECT_RECLAIM: u32 = 1 << 10;
pub(crate) const GFP_KSWAPD_RECLAIM: u32 = 1 << 11;
pub(crate) const GFP_NOWARN: u32 = 1 << 13;
pub(crate) const GFP_ACCOUNT: u32 = 1 << 22;
pub(crate) const GFP_HIGH: u32 = 1 << 5;
pub(crate) const GFP_IO: u32 = 1 << 6;
pub(crate) const GFP_FS: u32 = 1 << 7;
pub(crate) const GFP_BITS_MASK: u32 = (1 << GFP_BITS_SHIFT) - 1;
pub(crate) const GFP_ZONEMASK: u32 = 0x0f;
pub const GFP_KERNEL: u32 = GFP_DIRECT_RECLAIM | GFP_KSWAPD_RECLAIM | GFP_IO | GFP_FS;
pub const GFP_ATOMIC: u32 = GFP_HIGH | GFP_KSWAPD_RECLAIM;
pub const GFP_NOWAIT: u32 = GFP_KSWAPD_RECLAIM | GFP_NOWARN;
pub(crate) const INT_MAXV: i64 = 0x7fffffff;
pub(crate) const ULONG_MAXV: u64 = u64::MAX;

pub(crate) const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}
"""

RT_PREAMBLE = '''//! `lib/radix-tree.c` + the shared node internals it shares with `lib/idr.c`
//! — adopted, verified port (879/879 differential cases + gate in
//! `ports/lib-idr-radix`).
//!
//! Upstream `struct radix_tree_node` IS `struct xa_node` — this engine uses
//! `klib::xarray::Node` as its node type. Nodes live in a caller-owned
//! `&'a mut [Node]` arena (dense indices, canonical `NBASE`-encoded
//! pointers); `used` on each node is the slab bitmap; dirty nodes ride the
//! intrusive `shadow_head` list; `preload` models the per-cpu preload stack
//! (capped at `RADIX_TREE_PRELOAD_SIZE`, the kernel's bound).
//! `alloc_ctr`/`fail_mask` inject allocation failures; `warn`/`warn_once`
//! count `WARN_ON` trips.
//!
//! Invariants: this engine is order-0 — `radix_tree_insert`/`idr_alloc`
//! never create multi-order (sibling) entries, matching the upstream
//! deprecated API surface (which cannot create them either). `RadixTree`
//! is the engine embedded in `Idr`/`Ida` (upstream `idr.idr_rt`,
//! `ida.xa`); keep each arena stable and exclusive to one instance.

use crate::xarray::Node;
'''

TYPES = '''
#[derive(Clone, Copy)]
pub(crate) struct Root {
    pub head: u64,
    pub flags: u32,
}

#[derive(Clone)]
pub(crate) struct Xas {
    pub index: u64,
    pub offset: u8,
    pub node: u64,
    pub alloc: u64,
    pub update: bool,
}

impl Xas {
    pub(crate) fn new(index: u64) -> Self {
        Xas { index, offset: 0, node: XAS_RESTART, alloc: 0, update: false }
    }
}

/// Slot lvalue: `&root->xa_head` or `&node->slots[off]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotL {
    Root,
    Node(usize, usize),
}

/// `struct radix_tree_iter` — iteration cursor over chunk granularity.
pub struct RIter {
    pub index: u64,
    pub next_index: u64,
    pub tags: u64,
    pub node: Option<usize>,
}

impl RIter {
    pub fn new() -> Self {
        RIter { index: 0, next_index: 0, tags: 0, node: None }
    }
}

impl Default for RIter {
    fn default() -> Self {
        Self::new()
    }
}
'''

STRUCT_AND_HELPERS = '''
/// A radix tree root plus its caller-owned node arena — the engine for the
/// deprecated `radix_tree_*` API and for `Idr`/`Ida` (which embed it).
pub struct RadixTree<'a> {
    /// Caller-owned node arena (`used` = slab allocation bit).
    pub nodes: &'a mut [Node],
    pub(crate) root: Root,
    pub(crate) canon: u64,
    preload: [usize; RADIX_TREE_PRELOAD_SIZE as usize],
    plen: usize,
    /// Allocation counter paired with `fail_mask` (bit i = fail alloc i).
    pub alloc_ctr: u64,
    /// Allocation-failure injection bitmap (test/model knob).
    pub fail_mask: u64,
    /// `WARN_ON` trip counters.
    pub warn: u64,
    pub warn_once: u64,
    shadow_head: usize,
    /// NULLN-style scratch for wild node pointers — the `nidx`
    /// convention from `maple_tree`: decoding a pointer outside the
    /// arena must not panic (C would dereference garbage; the
    /// sequential model reads/writes this throwaway node instead).
    pub(crate) scratch: Node,
}

/// `nodes[i]` for reads — out-of-arena index maps to `scratch`.
#[inline]
pub(crate) fn nat<'n>(nodes: &'n [Node], scratch: &'n Node, i: usize) -> &'n Node {
    nodes.get(i).unwrap_or(scratch)
}

/// `nodes[i]` for writes — out-of-arena index maps to `scratch`.
#[inline]
pub(crate) fn nmat<'n>(
    nodes: &'n mut [Node],
    scratch: &'n mut Node,
    i: usize,
) -> &'n mut Node {
    match nodes.get_mut(i) {
        Some(n) => n,
        None => scratch,
    }
}

impl<'a> RadixTree<'a> {
    /// `RADIX_TREE(name, mask)`/`INIT_RADIX_TREE` — a fresh radix tree root.
    pub fn new(nodes: &'a mut [Node], gfp_mask: u32) -> Self {
        let mut t = RadixTree {
            nodes,
            root: Root { head: 0, flags: 0 },
            canon: RRT_CANON,
            preload: [0; RADIX_TREE_PRELOAD_SIZE as usize],
            plen: 0,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
            shadow_head: NILX as usize,
            scratch: Node::new(),
        };
        t.xa_init_flags(gfp_mask);
        t
    }

    pub(crate) fn new_idr(nodes: &'a mut [Node]) -> Self {
        let mut t = RadixTree::new(nodes, 0);
        t.canon = RTIDR_CANON;
        t.xa_init_flags(IDR_RT_MARKER);
        t
    }

    pub(crate) fn new_ida(nodes: &'a mut [Node]) -> Self {
        let mut t = RadixTree::new(nodes, 0);
        t.canon = IDAX_CANON;
        t.xa_init_flags(IDA_INIT_FLAGS);
        t
    }

    pub(crate) fn is_node_ptr(&self, p: u64) -> bool {
        p >= NBASE && p < NBASE + self.nodes.len() as u64 * NSTRIDE && p & 3 == 0
    }

    fn shadow_push(&mut self, ni: usize) {
        self.nodes[ni].shadow_next = self.shadow_head;
        self.shadow_head = ni;
    }

    fn shadow_remove(&mut self, ni: usize) {
        let mut cur = self.shadow_head;
        let mut prev = NILX as usize;
        while cur != NILX as usize {
            let next = self.nodes[cur].shadow_next;
            if cur == ni {
                if prev == NILX as usize {
                    self.shadow_head = next;
                } else {
                    self.nodes[prev].shadow_next = next;
                }
                return;
            }
            prev = cur;
            cur = next;
        }
    }

    fn pl_push(&mut self, ni: usize) {
        if self.plen < self.preload.len() {
            self.preload[self.plen] = ni;
            self.plen += 1;
        }
    }

    fn pl_pop(&mut self) -> Option<usize> {
        if self.plen == 0 {
            None
        } else {
            self.plen -= 1;
            Some(self.preload[self.plen])
        }
    }

    fn pl_top(&self) -> Option<usize> {
        if self.plen == 0 { None } else { Some(self.preload[self.plen - 1]) }
    }

    pub(crate) fn xa_track_free(&self) -> bool {
        self.root.flags & XA_FLAGS_TRACK_FREE != 0
    }
    pub(crate) fn xa_zero_busy(&self) -> bool {
        self.root.flags & XA_FLAGS_ZERO_BUSY != 0
    }
    pub(crate) fn xa_marked(&self, mark: u32) -> bool {
        self.root.flags & xa_flags_mark(mark) != 0
    }
    pub(crate) fn xa_mark_set(&mut self, mark: u32) {
        if !self.xa_marked(mark) {
            self.root.flags |= xa_flags_mark(mark);
        }
    }
    pub(crate) fn xa_mark_clear(&mut self, mark: u32) {
        if self.xa_marked(mark) {
            self.root.flags &= !xa_flags_mark(mark);
        }
    }
    pub(crate) fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }
'''

MANUAL_ENGINE = '''
    fn preload_push(&mut self, ni: usize) {
        let head = self.pl_top().map(node_ptr).unwrap_or(0);
        self.nodes[ni].parent = head;
        self.pl_push(ni);
    }

    pub(crate) fn __radix_tree_preload(&mut self, gfp_mask: u32, nr: u32) -> i64 {
        let _gfp = gfp_mask & !GFP_ACCOUNT;
        while self.plen < self.preload.len() && (self.plen as u32) < nr {
            let ni = match self.kmem_cache_alloc() {
                None => return -ENOMEM,
                Some(i) => i,
            };
            self.preload_push(ni);
        }
        0
    }

    /// `radix_tree_preload_end` — drops the per-cpu preload list upstream;
    /// here the preload stack is simply model state (no preemption).
    pub fn radix_tree_preload_end(&mut self) {}

    pub(crate) fn each_slot<F>(&mut self, start: u64, flags: u32, mut body: F)
    where
        F: FnMut(&mut Self, &mut RIter, SlotL) -> bool,
    {
        let mut iter = RIter::new();
        let mut slot = radix_tree_iter_init(&mut iter, start);
        loop {
            let s = match slot {
                Some(s) => s,
                None => match self.radix_tree_next_chunk(&mut iter, flags) {
                    None => break,
                    Some(s) => s,
                },
            };
            if !body(self, &mut iter, s) {
                break;
            }
            slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
        }
    }

    fn gang_entries(&mut self, first: u64, out: &mut [u64], tag: Option<u32>) -> u32 {
        let max_items = out.len() as u32;
        if max_items == 0 {
            return 0;
        }
        let flags = tag.map(|t| ITER_TAGGED | t).unwrap_or(0);
        let mut iter = RIter::new();
        let mut slot = radix_tree_iter_init(&mut iter, first);
        let mut n = 0u32;
        loop {
            let s = match slot {
                Some(s) => s,
                None => match self.radix_tree_next_chunk(&mut iter, flags) {
                    None => break,
                    Some(s) => s,
                },
            };
            let item = self.slot_get(s);
            if item == 0 {
                slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
                continue;
            }
            if rt_is_internal(item) {
                slot = radix_tree_iter_retry(&mut iter);
                continue;
            }
            out[n as usize] = item;
            n += 1;
            if n == max_items {
                break;
            }
            slot = self.radix_tree_next_slot(Some(s), &mut iter, flags);
        }
        n
    }

    fn gang_slots(&mut self, first: u64, out: &mut [SlotL], tag: Option<u32>) -> u32 {
        let max_items = out.len() as u32;
        if max_items == 0 {
            return 0;
        }
        let flags = tag.map(|t| ITER_TAGGED | t).unwrap_or(0);
        let mut n = 0u32;
        self.each_slot(first, flags, |_, _, s| {
            out[n as usize] = s;
            n += 1;
            n != max_items
        });
        n
    }

    /// `radix_tree_gang_lookup` — collect up to `out.len()` entries starting
    /// at or after `first`; returns the count collected.
    pub fn radix_tree_gang_lookup(&mut self, out: &mut [u64], first: u64) -> u32 {
        self.gang_entries(first, out, None)
    }

    /// `radix_tree_gang_lookup_tag` — tag-filtered `radix_tree_gang_lookup`.
    pub fn radix_tree_gang_lookup_tag(&mut self, out: &mut [u64], first: u64, tag: u32) -> u32 {
        self.gang_entries(first, out, Some(tag))
    }

    /// `radix_tree_gang_lookup_slot` — collect slot lvalues (untagged).
    pub fn radix_tree_gang_lookup_slot(&mut self, out: &mut [SlotL], first: u64) -> u32 {
        self.gang_slots(first, out, None)
    }

    /// `radix_tree_gang_lookup_tag_slot` — collect slot lvalues (tagged).
    pub fn radix_tree_gang_lookup_tag_slot(&mut self, out: &mut [SlotL], first: u64, tag: u32) -> u32 {
        self.gang_slots(first, out, Some(tag))
    }

    /// `radix_tree_empty` — is the tree empty?
    pub fn radix_tree_empty(&self) -> bool {
        self.xa_empty()
    }

    /// `radix_tree_shrink` — shrink the tree (internal helper, also used by
    /// the deferred free path).
    pub fn radix_tree_shrink_pub(&mut self) -> bool {
        self.radix_tree_shrink()
    }
'''

IDR_PREAMBLE = '''//! `lib/idr.c` — the `idr`/`ida` integer-allocator APIs riding the shared
//! radix-tree engine (`Idr.rt` is upstream `idr.idr_rt`; `Ida.rt` is
//! `ida.xa`). Adopted from the verified `ports/lib-idr-radix` port.
//!
//! `Idr` maps small integer IDs to entries; `Ida` allocates bare IDs backed
//! by a caller-owned `Bmap` arena (the per-cpu `ida_bitmap` pool). Both use
//! the same `&mut [Node]` caller-arena convention as `RadixTree`.

use crate::radix_tree::*;
use crate::xarray::Node;

// The idr/ida alloc APIs take upstream gfp flags; re-export the constants
// callers need so `klib::idr::*` is self-contained.
pub use crate::radix_tree::{GFP_ATOMIC, GFP_KERNEL, GFP_NOWAIT};

/// One `ida_bitmap` — `used` models its slab/percpu allocation state.
pub struct Bmap {
    pub bits: [u64; IDA_BITMAP_LONGS],
    pub used: bool,
}

impl Bmap {
    pub const fn new() -> Self {
        Bmap { bits: [0; IDA_BITMAP_LONGS], used: false }
    }
}

impl Default for Bmap {
    fn default() -> Self {
        Self::new()
    }
}

/// `struct idr` — integer ID -> entry mapping (upstream `idr.idr_rt` is a
/// radix_tree_root; ours wraps `RadixTree` exactly the same way).
pub struct Idr<'a> {
    pub rt: RadixTree<'a>,
    /// `idr.idr_base` — lowest valid ID (usually 0 or 1).
    pub idr_base: u32,
    /// `idr.idr_next` — cyclic-allocation cursor.
    pub idr_next: u32,
}

/// `struct ida` — integer ID allocator (xarray of `ida_bitmap` pointers +
/// value-packed low bits).
pub struct Ida<'a> {
    pub rt: RadixTree<'a>,
    /// Caller-owned `ida_bitmap` arena (upstream per-cpu pool).
    pub bmaps: &'a mut [Bmap],
}
'''

IDR_MANUAL = '''
impl<'a> Idr<'a> {
    /// `idr_init` — fresh IDR with `idr_base == 0`.
    pub fn new(nodes: &'a mut [Node]) -> Self {
        Idr { rt: RadixTree::new_idr(nodes), idr_base: 0, idr_next: 0 }
    }

    /// `idr_init_base` — fresh IDR with a nonzero base.
    pub fn new_base(nodes: &'a mut [Node], base: i64) -> Self {
        Idr { rt: RadixTree::new_idr(nodes), idr_base: base as u32, idr_next: 0 }
    }

    /// `idr_preload_end` — upstream re-enables preemption; a no-op in the
    /// sequential model.
    pub fn idr_preload_end(&mut self) {}

    /// `idr_is_empty` — `radix_tree_empty && radix_tree_tagged(FREE)`.
    pub fn idr_is_empty(&self) -> bool {
        self.rt.xa_empty() && self.rt.radix_tree_tagged(IDR_FREE) != 0
    }

    /// `idr_for_each` — iterate `(id, entry)` pairs, stopping when `f`
    /// returns nonzero; returns that value (upstream contract).
    pub fn idr_for_each<F>(&mut self, mut f: F) -> i32
    where
        F: FnMut(u32, u64) -> i32,
    {
        let base = self.rt_base() as u64;
        let mut ret = 0;
        self.rt.each_slot(0, 0, |rt, iter, s| {
            let id = iter.index.wrapping_add(base);
            if rt.warn_on(id > INT_MAXV as u64) {
                return false;
            }
            let entry = rt.slot_get(s);
            ret = f(id as u32, entry);
            ret == 0
        });
        ret
    }

    fn rt_base(&self) -> u32 {
        self.idr_base
    }
}

impl<'a> Ida<'a> {
    /// `ida_init` — fresh IDA over a caller bitmap arena.
    pub fn new(nodes: &'a mut [Node], bmaps: &'a mut [Bmap]) -> Self {
        Ida { rt: RadixTree::new_ida(nodes), bmaps }
    }
}
'''

# F4 — tag/mark bounds at public entry points (upstream leaves tag
# unvalidated; a `tag >= RADIX_TREE_MAX_TAGS` caller input reaches
# `marks[tag]` OOB / `1 << (24+tag)` shift panics)
HEAD_GUARD = {
    "radix_tree_tag_set": "    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return 0; }",
    "radix_tree_tag_clear": "    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return 0; }",
    "radix_tree_tag_get": "    if tag >= RADIX_TREE_MAX_TAGS { return 0; }",
    "radix_tree_tagged": "    if tag >= RADIX_TREE_MAX_TAGS { return 0; }",
    "radix_tree_iter_tag_clear": "    if self.warn_on(tag >= RADIX_TREE_MAX_TAGS) { return; }",
}

# ---------- emit radix_tree.rs ----------
out = [RT_PREAMBLE, CONST_BLOCK]

for name in order:
    if name in DROP or name in MANUAL or name in IDR_M or name in IDA_M or name in IDA_FREE:
        continue
    txt = body(name)
    if name in FREE_DROP_P:
        t = re.sub(r"p: &mut Pool,\s*", "", txt)
        t = re.sub(r"\s*let _ = p;", "", t)
        rt_free.append(t)
        continue
    if uses_p(txt):
        t = x_engine(txt)
        if name in HEAD_GUARD:
            t = t.replace("{\n", "{\n" + HEAD_GUARD[name] + "\n", 1)
        if name == "radix_tree_next_chunk":
            t = t.replace("""    let contig = flags & ITER_CONTIG != 0;""",
                          """    let contig = flags & ITER_CONTIG != 0;
    if tagged && self.warn_on(tag >= RADIX_TREE_MAX_TAGS) {
        return None;
    }""")
        lvl = "pub " if name in PUB_RT else "pub(crate) "
        rt_methods.append(vis(t, lvl))
    else:
        t = common(txt)
        newn = RENAME_PUB.get(name)
        if newn:
            t = t.replace("fn " + name, "fn " + newn)
        lvl = "pub " if name in RENAME_PUB else "pub(crate) "
        rt_free.append(vis(t, lvl))

for t in rt_free:
    out.append(t)


def _bound(t, anchor, lim, val=""):
    """Decl + first-stmt iteration guard around a loop head."""
    i = anchor.rindex("\n") + 1 if "\n" in anchor else 0
    pre, head = anchor[:i], anchor[i:]
    ind = head[: len(head) - len(head.lstrip())]
    new = (
        pre
        + f"{ind}let mut __lim = {lim};\n"
        + head
        + f"\n{ind}    if __lim == 0 {{ break{val}; }}\n{ind}    __lim -= 1;"
    )
    assert anchor in t, f"loop anchor missing: {anchor[:60]!r}"
    return t.replace(anchor, new)


def _nat_rewrite(t, pfx):
    """node-array access -> nat()/nmat() scratch accessors."""
    P = re.escape(pfx)
    t = re.sub(rf"&mut {P}\.nodes\[([^\[\]]+)\]",
               rf"nmat({pfx}.nodes, &mut {pfx}.scratch, \1)", t)
    t = re.sub(rf"&{P}\.nodes\[([^\[\]]+)\]",
               rf"nat({pfx}.nodes, &{pfx}.scratch, \1)", t)
    t = re.sub(
        rf"{P}\.nodes\[([^\[\]]+)\]((?:\.[\w]+|\[[^\[\]]*\])*\s*(?:[-+*/%&|^]|<<|>>)?=(?![=>]))",
        lambda m: f"nmat({pfx}.nodes, &mut {pfx}.scratch, {m.group(1)}){m.group(2)}",
        t,
    )
    t = re.sub(rf"{P}\.nodes\[([^\[\]]+)\]",
               rf"nat({pfx}.nodes, &{pfx}.scratch, \1)", t)
    return t


def _clamps(t):
    """slot/mark index clamps + node.shift member-read clamp."""

    def clamp_arr(maxv, name):
        def f(m):
            e = m.group(1)
            # literal indices already provably < maxv: leave untouched
            if e.isdigit() and int(e) < maxv:
                return e
            return f"({e}).min({name} - 1)"
        return f

    t = re.sub(r"\.slots\[([^\]]+)\]",
               lambda m: ".slots[" + clamp_arr(64, "CHUNK_SIZE")(m) + "]", t)
    t = re.sub(r"\.marks\[([^\]]+)\]",
               lambda m: ".marks[" + clamp_arr(3, "MAX_MARKS")(m) + "]", t)
    t = re.sub(r"\.shift\b(?!\s*=[^=])", ".shift.min(63)", t)
    return t


def fix_adversarial(t, pfx="self"):
    """run-16 critic wave-2: wild-pointer scratch convention, index
    clamps, C-wrap arithmetic, and iteration bounds on pointer-chasing
    loops — same adversarial class as maple's `nidx` scratch."""
    L1 = f"{pfx}.nodes.len() + 1"
    LB = f"({pfx}.nodes.len() + 1) * 66"

    # 1. iteration bounds for pointer-chasing loops: crafted parent/
    # shadow/child cycles would spin forever upstream too; exhaustion
    # is corruption-only, so break silently (xa_find's 256-retry
    # bound precedent). `next_chunk`'s inner loop decls need init so a
    # bounded break leaves them initialized.
    t = t.replace(
        "        let mut node;\n        let mut offset;",
        "        let mut node = 0usize;\n        let mut offset = 0u64;",
    )
    for entry in [
        ("        while cur != NILX as usize {", L1),
        ("    while self.is_node_ptr(node) {", L1),
        ("    while xa_is_node(entry) {", L1),
        ("    while node != 0 {", L1),
        ("    let mut node = top;\n    loop {", LB),
        ("    let mut node = nidx_ptr(xas.node);\n    loop {", L1),
        ("    while self.is_node_ptr(xas.node) && xas.index <= max {", LB),
        ("    while xas.index <= max {", LB),
        ("    let mut shrunk = false;\n    loop {", L1),
        ("    let top = child;\n    loop {", LB),
        ("    while rt_is_internal(node) {", L1),
        ("    while let Some(ni) = node {", L1),
        ("    'restart: loop {", LB, " None"),
        ("        let mut node = 0usize;\n        let mut offset = 0u64;\n        loop {", L1),
    ]:
        t = _bound(t, *entry)

    # 2. wild-pointer decodes wrap to out-of-arena indices (scratch).
    for old, new in [
        ("((e - 2 - NBASE) / NSTRIDE) as usize",
         "(e.wrapping_sub(2 + NBASE) / NSTRIDE) as usize"),
        ("((p - NBASE) / NSTRIDE) as usize",
         "(p.wrapping_sub(NBASE) / NSTRIDE) as usize"),
        ("((e & !3) - NBASE) as usize / NSTRIDE as usize",
         "(e & !3).wrapping_sub(NBASE) as usize / NSTRIDE as usize"),
        ("NBASE + ni as u64 * NSTRIDE",
         "NBASE.wrapping_add((ni as u64).wrapping_mul(NSTRIDE))"),
        ("p < NBASE + self.nodes.len() as u64 * NSTRIDE",
         "p < NBASE.wrapping_add((self.nodes.len() as u64).wrapping_mul(NSTRIDE))"),
    ]:
        assert old in t, f"decode anchor missing: {old[:50]!r}"
        t = t.replace(old, new)

    t = _nat_rewrite(t, pfx)

    # 4. wrap shifts on crafted offset/shift locals, then index clamps.
    for old, new in [
        ("(n.marks[mark] >> offset)", "(n.marks[mark].wrapping_shr(offset as u32))"),
        ("|= 1 << offset;", "|= 1u64.wrapping_shl(offset as u32);"),
        ("&= !(1 << offset);", "&= !(1u64.wrapping_shl(offset as u32));"),
        ("u64::MAX << offset", "u64::MAX.wrapping_shl(offset as u32)"),
        ("((RADIX_TREE_MAP_SIZE) << shift)", "((RADIX_TREE_MAP_SIZE).wrapping_shl(shift))"),
        ("xas.offset += 1;", "xas.offset = xas.offset.wrapping_add(1);"),
        ("xas.index += 1;", "xas.index = xas.index.wrapping_add(1);"),
    ]:
        assert old in t, f"shift anchor missing: {old[:50]!r}"
        t = t.replace(old, new)
    return _clamps(t)


def fix_idr_adversarial(t):
    """Same wave-2 class over `self.rt.nodes` accesses in idr.rs."""
    t = _bound(
        t,
        "                while offset == RADIX_TREE_MAP_SIZE {",
        "self.rt.nodes.len() + 1",
    )
    t = _nat_rewrite(t, "self.rt")
    return _clamps(t)


out.append(TYPES)
out.append(STRUCT_AND_HELPERS)

# methods go inside the open impl block from STRUCT_AND_HELPERS
method_text = "\n\n".join(t for t in rt_methods)
out.append(method_text)
out.append(MANUAL_ENGINE)
out.append("}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    include!(\"radix_tree_tests.inc\");\n}\n")
open("klib/src/radix_tree.rs", "w").write(fix_adversarial("\n".join(out)))

# ---------- emit idr.rs ----------
o = [IDR_PREAMBLE, IDR_MANUAL]
o.append("\n".join(ida_free))
o.append("\n/* ================= idr veneer ================= */\n")
idr_body = "\n\n".join(vis(t, "" if n == "idr_get_free" else "pub ") for n, t in idr_methods)
o.append("impl<'a> Idr<'a> {\n" + idr_body + "\n}\n")
o.append("\n/* ================= ida veneer ================= */\n")
ida_body = "\n\n".join(vis(t, "pub ") for n, t in ida_methods)
o.append("impl<'a> Ida<'a> {\n" + ida_body + "\n}\n")
o.append("\n#[cfg(test)]\nmod tests {\n    use super::*;\n    include!(\"idr_tests.inc\");\n}\n")
idr_out = "\n".join(o)


def repd(t, old, new):
    """Checked replace — a drifted pattern must fail loudly, not no-op."""
    assert old in t, f"pattern missing: {old[:60]!r}"
    return t.replace(old, new)

# F6 — upstream doesn't check either, but a stored internal-encoded ptr
# panics later at lookup-descend; warn + -EINVAL converts it to an early
# rejection (same class as the radix_tree_insert bug_on)
idr_out = repd(idr_out, """pub fn idr_alloc_u32(&mut self, ptr: u64, nextid: &mut u32, max: u64, gfp: u32) -> i64 {
    let base = self.idr_base as u64;""",
"""pub fn idr_alloc_u32(&mut self, ptr: u64, nextid: &mut u32, max: u64, gfp: u32) -> i64 {
    if self.rt.warn_on(rt_is_internal(ptr)) {
        return -EINVAL;
    }
    let base = self.idr_base as u64;""")

# F7 — `bmap_idx` arithmetic wraps (wild entries -> huge index -> bounds
# check catches) and every `bmaps[bmap_idx(x)]` site gets a checked index.
idr_out = repd(idr_out, "((e - BBASE) / 256) as usize",
                          "(e.wrapping_sub(BBASE) / 256) as usize")
idr_out = repd(idr_out, """pub fn kfree_bitmap(&mut self, i: usize) {
    self.bmaps[i].used = false;
}""",
"""pub fn kfree_bitmap(&mut self, i: usize) {
    if i < self.bmaps.len() {
        self.bmaps[i].used = false;
    }
}""")
idr_out = repd(idr_out, """    } else {
        if bitmap == 0 || !bm_test_bit(&self.bmaps[bmap_idx(bitmap)].bits, bit) {
            self.rt.warn_on(true);
            return;
        }
        bm_clear_bit(&mut self.bmaps[bmap_idx(bitmap)].bits, bit);
        self.rt.xas_set_mark(&xas, XA_FREE_MARK as usize);
        if bm_empty(&self.bmaps[bmap_idx(bitmap)].bits) {
            self.kfree_bitmap(bmap_idx(bitmap));
            self.rt.xas_store(&mut xas, 0);
        }
    }""",
"""    } else {
        let bi = bmap_idx(bitmap);
        if bitmap == 0 || bi >= self.bmaps.len() || !bm_test_bit(&self.bmaps[bi].bits, bit) {
            self.rt.warn_on(true);
            return;
        }
        bm_clear_bit(&mut self.bmaps[bi].bits, bit);
        self.rt.xas_set_mark(&xas, XA_FREE_MARK as usize);
        if bm_empty(&self.bmaps[bi].bits) {
            self.kfree_bitmap(bi);
            self.rt.xas_store(&mut xas, 0);
        }
    }""")
_idr_old = """        let bm = &self.bmaps[bmap_idx(entry)].bits;
        (IDA_BITMAP_BITS, find_next_arr(bm, IDA_BITMAP_BITS, offset))"""
idr_out = repd(idr_out, _idr_old,
"""        let bm = match self.bmaps.get(bmap_idx(entry)) {
            Some(b) => &b.bits,
            None => {
                self.rt.warn_on(true);
                return -ENOENT;
            }
        };
        (IDA_BITMAP_BITS, find_next_arr(bm, IDA_BITMAP_BITS, offset))""")
idr_out = repd(idr_out, """            if bitmap != 0 {
                let bi = bmap_idx(bitmap);
                bit = find_next_zero_arr(&self.bmaps[bi].bits, IDA_BITMAP_BITS, bit);""",
"""            if bitmap != 0 {
                let bi = bmap_idx(bitmap);
                if self.rt.warn_on(bi >= self.bmaps.len()) {
                    continue 'body;
                }
                bit = find_next_zero_arr(&self.bmaps[bi].bits, IDA_BITMAP_BITS, bit);""")
open("klib/src/idr.rs", "w").write(fix_idr_adversarial(idr_out))

print("wrote klib/src/radix_tree.rs ({} fns: {} methods, {} free)".format(
    len(rt_methods) + len(rt_free), len(rt_methods), len(rt_free)))
print("wrote klib/src/idr.rs ({} idr, {} ida, {} helpers)".format(
    len(idr_methods), len(ida_methods), len(ida_free)))
