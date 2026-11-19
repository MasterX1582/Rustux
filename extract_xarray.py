#!/usr/bin/env python3
"""Transform ports/lib-xarray/rust/wrappers-rs/src/port.rs -> klib/src/xarray.rs

Dense-arena adoption: Pool -> Xarray<'a> over caller-owned &[Node].
Harness-only items (fold/dump/gen/op/wrap/TRACE/epool) are dropped.
"""
import re

SRC = "ports/lib-xarray/rust/wrappers-rs/src/port.rs"
DST = "klib/src/xarray.rs"

lines = open(SRC, encoding="utf-8").read().split("\n")


def region(a, b):
    """1-based inclusive line range."""
    return "\n".join(lines[a - 1 : b])


# ---- kept source regions ---------------------------------------------------
ENTRY_HELPERS = region(78, 110) + "\n" + region(114, 169)  # skip is_node_ptr
POOL_METHODS = region(281, 319)   # xa_track_free..warn_on_once (self-based)
SLAB = region(353, 393)           # kmem_cache_*, rcu_free, xa_node_free
UPDCB = region(490, 499)          # upd_cb
CORE = region(500, 2166)          # xas_* machinery, marks, split, xa_* API
SIBCHK = region(2186, 2193)       # xas_sibling_check
EXTRACT = region(2223, 2298)      # extract/destroy/init
FIND_B = region(2429, 2485)       # xa_find_b / xa_find_after_b


def strip_trace(text):
    return re.sub(
        r"\n\s*if TRACE\.load\(std::sync::atomic::Ordering::Relaxed\) \{\n"
        r"\s*eprintln!\([^;]*;\n"
        r"\s*\}",
        "",
        text,
    )


def to_method(text):
    """Transform a `fn f(p: &mut Pool, ...)` body into a method."""
    text = text.replace("p: &mut Pool", "&mut self")
    text = text.replace("p: &Pool", "&self")
    text = re.sub(r"\bp\.used\[(\w+)\]", r"self.nodes[\1].used", text)
    text = re.sub(r"\bp\.n\b", "self.nodes", text)
    text = re.sub(r"\bp\.shadow\.insert\(0, (\w+)\)", r"self.shadow_push(\1)", text)
    text = re.sub(r"\bp\.shadow\.retain\(\|&x\| x != (\w+)\)", r"self.shadow_remove(\1)", text)
    text = text.replace("p.shadow.clear()", "self.shadow_head = NIL")
    text = re.sub(r"\bp\.", "self.", text)
    # call sites: f(p, ...) -> self.f(...); f(p) -> self.f()
    text = re.sub(r"\b(\w+)\(\s*p, ", r"self.\1(", text)
    text = re.sub(r"\b(\w+)\(\s*p\)", r"self.\1()", text)
    text = text.replace("let _ = p;", "let _ = self;")
    text = re.sub(r"\bis_node_ptr\(", "self.is_node_ptr(", text)
    text = re.sub(r"for i in 0\.\.NP", "for i in 0..self.nodes.len()", text)
    text = re.sub(r"\bself\.n\[", "self.nodes[", text)
    text = text.replace("Pool", "Xarray")
    return text


def split_items(text):
    """Split a region into (comment-prefix, fn-text) top-level items."""
    items = []
    # split at top-level `fn ` starts, keeping preceding comment lines
    chunks = re.split(r"(?m)(?=^fn |^pub fn )", text)
    for c in chunks:
        if not c.strip():
            continue
        items.append(c)
    return items


methods = []
free = []

for region_text in (SLAB, UPDCB, CORE, SIBCHK, EXTRACT, FIND_B):
    text = strip_trace(region_text)
    for item in split_items(text):
        if re.search(r"^fn \w+\(\s*p[,:)]", item, re.M):
            methods.append(to_method(item))
        else:
            free.append(item)

# rename bounded find variants to the public API names
methods = [
    m.replace("fn xa_find_b(", "fn xa_find(").replace(
        "fn xa_find_after_b(", "fn xa_find_after("
    )
    for m in methods
]
# also fix internal calls xa_find_b( / xa_find_after_b( inside them
methods = [
    m.replace("self.xa_find_b(", "self.xa_find(").replace(
        "self.xa_find_after_b(", "self.xa_find_after("
    )
    for m in methods
]

# Pool impl methods: already self-based; mark pub
poolm = POOL_METHODS
poolm = re.sub(r"(?m)^    fn ", "    pub fn ", poolm)

# free fns: add pub (helpers remain pub for parity with C-visible surface)
freepub = []
for item in [ENTRY_HELPERS] + free:
    item = re.sub(r"(?m)^fn ", "pub fn ", item)
    freepub.append(item)

freepub_text = "".join(freepub)

# methods: add pub
methodpub = []
for m in methods:
    m = re.sub(r"(?m)^fn ", "pub fn ", m)
    methodpub.append(m)

HEADER = '''//! `klib::xarray` — Linux `lib/xarray.c` (radix-tree successor) ported to
//! safe Rust, verified against the C implementation (983/983 recorded
//! differential cases, `ports/lib-xarray`).
//!
//! Storage model: a caller-owned dense node arena (`&mut [Node]`) replaces
//! the slab allocator. Node "pointers" are canonical addresses
//! `NBASE + i*NSTRIDE`; entries in slots are plain `u64` values tagged
//! exactly like the kernel's `void *` (values `(v<<1)|1`, internal
//! `(v<<2)|2`, node pointers `ptr|2`).
//!
//! Notes:
//! * `xa_find`/`xa_find_after` bound the upstream `xas_retry` loop at 256
//!   attempts — identical to the verified C harness, which bounds the same
//!   way to avoid upstream's livelock on retry entries.
//! * Allocation-failure injection: set `fail_mask` bit `k` to fail the
//!   (k+1)-th node allocation (`alloc_ctr` counts attempts).
//! * The `update` flag on `Xas` models the kernel's `xa_update_t` callback:
//!   nodes are tracked on the intrusive shadow list (`shadow_head`) while
//!   nonempty, mirroring `xa_track_free` behaviour.

#![forbid(unsafe_code)]
#![allow(clippy::all)]

/* ================= canonical model ================= */

/// Default node-pool capacity (matches the verified harness).
pub const NP: usize = 160;
pub const NBASE: u64 = 0x40000;
pub const NSTRIDE: u64 = 4096;
pub const XA_CANON: u64 = 0x800000;
pub const XA_RCU_FREE: u64 = 1;

pub const CHUNK_SHIFT: u32 = 6;
pub const CHUNK_SIZE: usize = 64;
pub const CHUNK_MASK: u64 = 63;
pub const MAX_MARKS: usize = 3;

pub const XA_FLAGS_TRACK_FREE: u32 = 4;
pub const XA_FLAGS_ZERO_BUSY: u32 = 8;
pub const XA_FLAGS_ALLOC_WRAPPED: u32 = 16;
pub const XA_FLAGS_ACCOUNT: u32 = 32;
pub const GFP_BITS_SHIFT: u32 = 24;

pub const XA_MARK_0: u32 = 0;
pub const XA_MARK_1: u32 = 1;
pub const XA_MARK_2: u32 = 2;
pub const XA_PRESENT: u32 = 8;
pub const XA_FREE_MARK: u32 = XA_MARK_0;
pub const XA_MARK_MAX: u32 = XA_MARK_2;

pub const XA_FLAGS_ALLOC: u32 = XA_FLAGS_TRACK_FREE | xa_flags_mark_const(XA_FREE_MARK);
pub const XA_FLAGS_ALLOC1: u32 = XA_FLAGS_TRACK_FREE | XA_FLAGS_ZERO_BUSY;

pub fn xa_flags_mark(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}
pub const fn xa_flags_mark_const(mark: u32) -> u32 {
    (1u32 << GFP_BITS_SHIFT) << mark
}

pub const ENOMEM: i64 = 12;
pub const EINVAL: i64 = 22;
pub const EBUSY: i64 = 16;
pub const MAX_ERRNO: i64 = 4095;

pub const XAS_BOUNDS: u64 = 1;
pub const XAS_RESTART: u64 = 3;

pub const XA_ZERO_ENTRY: u64 = 1030; /* xa_mk_internal(257) */
pub const XA_RETRY_ENTRY: u64 = 1026; /* xa_mk_internal(256) */

/// Intrusive shadow-list terminator (not a node index).
const NIL: usize = usize::MAX;

/* ================= structures ================= */

/// One radix-tree node: 64 slots plus per-mark bitmaps.
///
/// The arena (`Xarray.nodes`) is caller-owned; `used` models the slab
/// allocated/free state and `shadow_next` the kernel's shadow-node list.
/// Callers must not modify fields while the node is in use.
#[derive(Clone)]
pub struct Node {
    pub shift: u8,
    pub offset: u8,
    pub count: u8,
    pub nr_values: u8,
    pub parent: u64, /* canonical node ptr or 0 */
    pub array: u64,  /* XA_CANON or XA_RCU_FREE */
    pub on_list: bool,
    pub slots: [u64; CHUNK_SIZE],
    pub marks: [u64; MAX_MARKS],
    pub used: bool,
    pub shadow_next: usize,
}

impl Node {
    pub const fn new() -> Self {
        Node {
            shift: 0,
            offset: 0,
            count: 0,
            nr_values: 0,
            parent: 0,
            array: 0,
            on_list: false,
            slots: [0; CHUNK_SIZE],
            marks: [0; MAX_MARKS],
            used: false,
            shadow_next: NIL,
        }
    }
}

/// `xa_state` — the iteration/store cursor. All fields are public exactly
/// like the C struct; use `Xas::new`/`Xas::new_order` to construct.
#[derive(Clone)]
pub struct Xas {
    pub index: u64,
    pub shift: u8,
    pub sibs: u8,
    pub offset: u8,
    pub node: u64,
    pub alloc: u64,
    pub update: bool,
}

impl Xas {
    pub fn new(index: u64) -> Self {
        Xas {
            index,
            shift: 0,
            sibs: 0,
            offset: 0,
            node: XAS_RESTART,
            alloc: 0,
            update: false,
        }
    }
    /// `XA_STATE_ORDER`
    pub fn new_order(index: u64, order: u32) -> Self {
        let mut x = Xas::new(if order < 64 {
            (index >> order) << order
        } else {
            0
        });
        x.shift = (order - order % CHUNK_SHIFT) as u8;
        x.sibs = ((1u32 << (order % CHUNK_SHIFT)) - 1) as u8;
        x
    }
}

/// A sparse array (`struct xarray`) over a caller-owned node arena.
pub struct Xarray<'a> {
    pub nodes: &'a mut [Node],
    pub xa_head: u64,
    pub xa_flags: u32,
    /// Number of node-allocation attempts (for `fail_mask` injection).
    pub alloc_ctr: u64,
    /// Bit `k` set => the (k+1)-th `kmem_cache_alloc_lru` attempt fails.
    pub fail_mask: u64,
    /// WARN_ONCE counters (modelled; observable only).
    pub warn: u64,
    pub warn_once: u64,
    shadow_head: usize,
}
'''

IMPL_HEAD = '''
impl<'a> Xarray<'a> {
    /// Fresh array over `nodes`, equivalent to `reset_all`/`xa_init_flags`:
    /// every node is reset, the array is empty, flags set.
    pub fn new(nodes: &'a mut [Node], flags: u32) -> Self {
        for n in nodes.iter_mut() {
            *n = Node::new();
        }
        Xarray {
            nodes,
            xa_head: 0,
            xa_flags: flags,
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
            shadow_head: NIL,
        }
    }

    fn is_node_ptr(&self, p: u64) -> bool {
        p >= NBASE && p < NBASE + self.nodes.len() as u64 * NSTRIDE && p & 3 == 0
    }

    fn shadow_push(&mut self, ni: usize) {
        self.nodes[ni].shadow_next = self.shadow_head;
        self.shadow_head = ni;
    }

    fn shadow_remove(&mut self, ni: usize) {
        let mut cur = self.shadow_head;
        let mut prev = NIL;
        while cur != NIL {
            let next = self.nodes[cur].shadow_next;
            if cur == ni {
                if prev == NIL {
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
'''

FOOT = '''
}
'''

out = HEADER + "\n/* ================= entry classification ================= */\n\n"
out += freepub_text
out += IMPL_HEAD
out += poolm
for m in methodpub:
    out += m if m.startswith("\n") else "\n" + m
out += FOOT

open(DST, "w", encoding="utf-8", newline="\n").write(out)
print(f"wrote {DST}: {len(out.splitlines())} lines, {len(methodpub)} methods, {len(freepub)} free fns")
