#!/usr/bin/env python3
"""Transform ports/lib-maple-tree/rust/maple-model/src/lib.rs -> klib/src/maple_tree.rs

Dense-arena adoption: Pool -> MapleTree<'a> over caller-owned arenas
(&mut [Node] + &mut [Sheaf]); NEXTRA scratch pseudo-nodes move into the
struct (`spare`). Harness-only items (fold/trace/h/r/xs64/epool/hot) are
dropped; Pool::new/reset_all become MapleTree::new/reset_all.

Method transform mirrors extract_xarray.py: `p: &mut Pool` -> `&mut self`,
`p.` -> `self.`, `f(p, ...)` -> `self.f(...)`.
"""
import re

SRC = "ports/lib-maple-tree/rust/maple-model/src/lib.rs"
DST = "klib/src/maple_tree.rs"

lines = open(SRC, encoding="utf-8").read().split("\n")


def region(a, b):
    """1-based inclusive line range."""
    return "\n".join(lines[a - 1 : b])


# ---- kept source regions ---------------------------------------------------
CONSTS = region(26, 162)        # canonical model + maple_tree.h + xa helpers
SHEAF = region(166, 170)        # struct Sheaf
IABS = region(460, 466)
CURSORS = region(468, 563)      # Mas / MaWrState / MaTopiary
FREE = region(587, 4914)        # everything: mt_*/mas_*/mtree_* translation


def strip_trace(text):
    """Drop `if p.trace { eprintln!(...) }` debug blocks."""
    return re.sub(
        r"\n\s*if p\.trace \{\n\s*eprintln!\(.*?\n\s*\);\n\s*\}",
        "",
        text,
        flags=re.S,
    )


def fix_dup_spin(text):
    """`if p.trace && asc > 40 { eprintln!; panic!("asc spin") }` becomes an
    unconditional bounded-ascend divergence guard at 256 (klib convention)."""
    return re.sub(
        r"if p\.trace && asc > 40 \{\n\s*eprintln!\(.*?\n\s*\);\n\s*"
        r'panic!\("asc spin"\);\n\s*\}',
        'if asc > 256 {\n                    panic!("asc spin");\n                }',
        text,
        flags=re.S,
    )


def fix_vecs(text):
    """Vec<Mas> scratch -> fixed [Mas; 3] arrays (no alloc in klib)."""
    return text.replace(
        "let mut tmp: Vec<Mas> = vec![Mas::new(mas.tree, 0, 0); 3];",
        "let mut tmp = [Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0)];",
    ).replace(
        "let mut tmp_next: Vec<Mas> = vec![Mas::new(mas.tree, 0, 0); 3];",
        "let mut tmp_next = [Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0), Mas::new(mas.tree, 0, 0)];",
    )


def fix_wrapping(text):
    """u64 index/pivot arithmetic wraps mod 2^64 in C; plain `+`/`-` panics
    in Rust debug builds. Convert only the reachable boundary sites (the
    verified harness runs release, where they wrapped silently)."""
    pairs = [
        ("        min += 1;", "        min = min.wrapping_add(1);"),
        ("max - min + 1 < size", "max.wrapping_sub(min).wrapping_add(1) < size"),
        ("mas.last = min + size - 1;", "mas.last = min.wrapping_add(size).wrapping_sub(1);"),
        ("mas.index = max - size + 1;", "mas.index = max.wrapping_sub(size).wrapping_add(1);"),
        ("slot_min = slot_max + 1;", "slot_min = slot_max.wrapping_add(1);"),
        ("if r_mas.last + 1 != 0 {", "if r_mas.last.wrapping_add(1) != 0 {"),
        ("let min = mas.max + 1;", "let min = mas.max.wrapping_add(1);"),
        ("mas.index = mas.last + 1;", "mas.index = mas.last.wrapping_add(1);"),
        ("min = pivot + 1;", "min = pivot.wrapping_add(1);"),
        (
            "gap = pivot.min(mas.last) - mas.index.max(min) + 1;",
            "gap = pivot.min(mas.last).wrapping_sub(mas.index.max(min)).wrapping_add(1);",
        ),
        ("let max = mas.min - 1;", "let max = mas.min.wrapping_sub(1);"),
        ("mas.last = mas.index - 1;", "mas.last = mas.index.wrapping_sub(1);"),
        ("        max = min - 1;", "        max = min.wrapping_sub(1);"),
        (
            "if mas.index > max || size - 1 > max - mas.index {",
            "if mas.index > max || size.wrapping_sub(1) > max.wrapping_sub(mas.index) {",
        ),
        ("*gap_max = min + gap - 1;", "*gap_max = min.wrapping_add(gap).wrapping_sub(1);"),
        ("max - min < size - 1", "max.wrapping_sub(min) < size - 1"),
        ("mas.last = mas.index + size - 1;", "mas.last = mas.index.wrapping_add(size).wrapping_sub(1);"),
        ("*next = *startp + 1;", "*next = startp.wrapping_add(1);"),
        ("*index = mas.last + 1;", "*index = mas.last.wrapping_add(1);"),
        # further C-wrap sites found by debug-mode audit: pivot stores,
        # reverse empty-area tail, dense-offset and gap-size math
        ("p.sb(cp, 251, data - 1);", "p.sb(cp, 251, data.wrapping_sub(1));"),
        ("(data - 1) / 2", "data.wrapping_sub(1) / 2"),
        ("d_count - 1); /* cp->end", "d_count.wrapping_sub(1)); /* cp->end"),
        ("gap = max - min + 1;", "gap = max.wrapping_sub(min).wrapping_add(1);"),
        ("mas.last - min + 1", "mas.last.wrapping_sub(min).wrapping_add(1)"),
        ("mas.index - mas.min", "mas.index.wrapping_sub(mas.min)"),
        ("mas.index = mas.last - size + 1;", "mas.index = mas.last.wrapping_sub(size).wrapping_add(1);"),
        ("mas.index - 1", "mas.index.wrapping_sub(1)"),  # all pivot stores
        # F1/F3 — `x - 1` sites where 0 is reachable (valid trees: a 1-entry
        # sibling leaf leaves mas.offset == 0; crafted mas/enode states make
        # the rest reachable). C wraps; match it exactly.
        ("8 * (mas.offset as u64 - 1)", "8u64.wrapping_mul((mas.offset as u64).wrapping_sub(1))"),
        ("8 * (offset as u64 - 1)", "8u64.wrapping_mul((offset as u64).wrapping_sub(1))"),
        ("8 * (a_slot as u64 - 1)", "8u64.wrapping_mul((a_slot as u64).wrapping_sub(1))"),
        ("8 * (i as u64 - 1)", "8u64.wrapping_mul((i as u64).wrapping_sub(1))"),
        ("8 * (size as u64 - 1)", "8u64.wrapping_mul((size as u64).wrapping_sub(1))"),
        ("(dst_offset - 1) as u64", "dst_offset.wrapping_sub(1) as u64"),
        ("ma_data_end(p, mn, mt, pivots, max) - 1", "ma_data_end(p, mn, mt, pivots, max).wrapping_sub(1)"),
        ("max_gap = p.rd(pivots) - min + 1;", "max_gap = p.rd(pivots).wrapping_sub(min).wrapping_add(1);"),
        ("let gap = p.rd(pivots + 8 * i as u64) - pstart;", "let gap = p.rd(pivots + 8 * i as u64).wrapping_sub(pstart);"),
        ("max_gap > p.rd(pivots + 8 * max_piv as u64) - min", "max_gap > p.rd(pivots + 8 * max_piv as u64).wrapping_sub(min)"),
        ("mas.offset = (p_slot - 1) as u8;", "mas.offset = p_slot.wrapping_sub(1) as u8;"),
        ("(start as usize + size as usize - 1)", "(start as usize).wrapping_add(size as usize).wrapping_sub(1)"),
        ("let off = l_mas.offset - 1;", "let off = l_mas.offset.wrapping_sub(1);"),
        ("mas.offset = slot - 1;", "mas.offset = slot.wrapping_sub(1);"),
        ("l_slot - 1", "l_slot.wrapping_sub(1)"),
        # `X.offset`/`*_offset` forms must precede the bare `offset` pattern.
        ("mas.offset -= 1;", "mas.offset = mas.offset.wrapping_sub(1);"),
        ("l_tmp.offset -= 1;", "l_tmp.offset = l_tmp.offset.wrapping_sub(1);"),
        ("sib.offset -= 1;", "sib.offset = sib.offset.wrapping_sub(1);"),
        ("s_offset -= 1;", "s_offset = s_offset.wrapping_sub(1);"),
        ("data_offset -= 1;", "data_offset = data_offset.wrapping_sub(1);"),
        ("dst_offset -= 1;", "dst_offset = dst_offset.wrapping_sub(1);"),
        ("offset -= 1;", "offset = offset.wrapping_sub(1);"),
        # cp-walk `s` is a u8 command-list cursor: reaching 0 with
        # s_offset still == src_start wraps to 255 in C (bounded read),
        # panics in debug Rust.
        ("                s -= 1;", "                s = s.wrapping_sub(1);"),
    ]
    for old, new in pairs:
        assert old in text, f"wrapping site missing: {old}"
        text = text.replace(old, new)
    return text


def to_method(text):
    """Transform a `fn f(p: &mut Pool, ...)` body into a method."""
    text = text.replace("_p: &mut Pool", "&mut self")
    text = text.replace("_p: &Pool", "&self")
    text = text.replace("p: &mut Pool", "&mut self")
    text = text.replace("p: &Pool", "&self")
    text = re.sub(r"\bp\.", "self.", text)
    # call sites: f(p, ...) -> self.f(...); f(p) -> self.f()
    text = re.sub(r"\b(\w+)\(\s*p, ", r"self.\1(", text)
    text = re.sub(r"\b(\w+)\(\s*p,\n", r"self.\1(\n", text)
    text = re.sub(r"\b(\w+)\(\s*p\)", r"self.\1()", text)
    text = text.replace("let _ = p;", "let _ = self;")
    return text


def split_items(text):
    """Split a region into top-level items at `pub fn `/`pub const ` starts,
    keeping preceding comment lines attached."""
    chunks = re.split(r"(?m)(?=^pub fn |^pub const )", text)
    return [c for c in chunks if c.strip()]


methods = []
free = []
consts = []


def fix_adversarial(t):
    """Coverage-critic wave-2 hardening: bound pointer-chasing loops, clamp
    node-type table indices, wrap reachable u8 offsets. Applied AFTER
    `to_method`, so patterns match the emitted (`self.`-prefixed) text.

    Bound hits `return` a type-sentinel rather than `break`: many maple
    `loop`s are value-yielding tail expressions (`break entry`), and
    `break` would also skip declarations the post-loop code uses."""
    can_warn = "&mut self" in t
    warn = "self.warn += 1; " if can_warn else ""

    m = re.search(r"pub fn \w+\s*\([^)]*\)\s*(?:->\s*([^\{]+))?\{", t)
    rty = (m.group(1) or "").strip() if m else ""
    if "Option" in rty:
        ret = "return None;"
    elif rty == "bool":
        ret = "return false;"
    elif re.fullmatch(r"(u8|u16|u32|u64|usize|i8|i16|i32|i64|isize)", rty):
        ret = "return 0;"
    elif rty == "":
        ret = "return;"
    else:
        ret = None  # non-scalar return: fall back to plain break

    # MT_* node-type tables have 5 rows; mte_node_type masks to 0..15.
    # In-contract types are <= 4; crafted types clamp to the last (copy)
    # row instead of panicking on an OOB index.
    def mt_clamp(m):
        e = m.group(2)
        if e.strip().isdigit() and int(e) < 5:
            return m.group(0)
        return f"{m.group(1)}[({e}).min({m.group(1)}.len() - 1)]"

    t = re.sub(r"\b(MT_(?:SLOTS|PIVOTS|MIN_SLOTS))\[([^\]]+)\]", mt_clamp, t)

    # u8 slot/offset arithmetic: C wraps at 255, Rust debug panics.
    for old, new in [
        ("mas.offset = offset + 1;", "mas.offset = offset.wrapping_add(1);"),
        (
            "self.sb(cp, 249, count as u8 + 1);",
            "self.sb(cp, 249, (count as u8).wrapping_add(1));",
        ),
        ("let dst_offset = mas.offset + 1;", "let dst_offset = mas.offset.wrapping_add(1);"),
        (
            "let copy_size = mas.end - offset_end + 1;",
            "let copy_size = mas.end.wrapping_sub(offset_end).wrapping_add(1);",
        ),
        (
            "self.mt_slot_locked(mas.tree, slots, offset + 1)",
            "self.mt_slot_locked(mas.tree, slots, offset.wrapping_add(1))",
        ),
    ]:
        t = t.replace(old, new)

    # Every `loop {}` (incl. labeled retry loops) gets an iteration cap:
    # legit traversals are depth-bounded (~16 levels) or retry-bounded;
    # corrupt parent/slot state must not hang. 1e6 matches the existing
    # mas_erase retries bound.
    bail = ret if ret is not None else "break;"
    t = re.sub(
        r"(?m)^([ \t]*)((?:'\w+: )?loop) \{$",
        lambda m: (
            f"{m.group(1)}let mut __lim = 0u64;\n"
            f"{m.group(1)}{m.group(2)} {{\n"
            f"{m.group(1)}    __lim += 1;\n"
            f"{m.group(1)}    if __lim > MAS_ITER_LIMIT {{ {warn}{bail} }}"
        ),
        t,
    )

    # Pointer-chasing `while`s (parent climbs, sheaf-list walks, descends)
    # get the same cap; index/counter-bounded whiles stay untouched.
    for cond in [
        "while mat.head != 0 {",
        "while !mte_is_leaf(tmp[0].node) {",
        "while !self.mte_is_root(mas.node) {",
        "while !mas_is_err(mas) && !self.mas_anode_descend(mas, size) {",
        "while !self.mas_rev_awalk(mas, size, &mut gmin, &mut gmax) {",
    ]:
        pat = re.compile(r"(?m)^([ \t]*)" + re.escape(cond) + r"$")
        for m in reversed(list(pat.finditer(t))):
            ind = m.group(1)
            rep = (
                f"{ind}let mut __limw = 0u64;\n"
                f"{m.group(0)}\n"
                f"{ind}    __limw += 1;\n"
                f"{ind}    if __limw > MAS_ITER_LIMIT {{ {warn}{bail} }}"
            )
            t = t[: m.start()] + rep + t[m.end():]
    return t


text = fix_wrapping(fix_vecs(fix_dup_spin(strip_trace(FREE))))
for item in split_items(text):
    if re.match(r"pub const ", item):
        consts.append(item)
    elif re.search(r"\b_?p\s*:\s*&\s*(mut\s+)?Pool\b", item):
        methods.append(fix_adversarial(to_method(item)))
    else:
        free.append(fix_adversarial(item))

HEADER = '''//! `klib::maple_tree` — Linux `lib/maple_tree.c` ported to safe Rust,
//! verified against the C implementation (666/666 recorded differential
//! cases, `ports/lib-maple-tree`).
//!
//! Storage model: a caller-owned dense node arena (`&mut [Node]` of 256-byte
//! nodes, 32 `u64` words each) plus a caller-owned sheaf arena
//! (`&mut [Sheaf]`) replace the maple slab cache and its per-CPU sheaf
//! magazine layer. Every C pointer is a canonical `u64`: pool node `i` is
//! `NBASE + i*256`, sheaf `i` is `SHBASE + i*256`, and interior
//! `unsigned long *`/`void **` pointers are `node_base + byte_offset`, so
//! the union views (slots/pivots/gaps/meta/copy/topiary/rcu) share storage
//! byte-for-byte exactly like C on x86-64. The four reserved pseudo-node
//! slots (`CPBASE`/`NULLN`/`REUSE` + one spare) live in `spare`, at the
//! fixed canonical indices `NP..NP+NEXTRA`; `nidx` maps any out-of-range
//! pointer to the `NULLN` scratch slot, mirroring the verified model.
//!
//! Tree identity: upstream takes `struct maple_tree *`; here the `mt: u64`
//! argument is a canonical tree id — `MT_CANON` for the first tree,
//! `MT2_CANON` for the second. `MapleTree` hosts both heads
//! (`ma_root`/`ma_flags`, indexed by `tidx`) because the verified harness
//! shares one slab between `&mt` and `&mt2` for `mtree_dup`; callers that
//! need a single tree always pass `MT_CANON`. `struct ma_state` is `Mas`,
//! `ma_wr_state` is `MaWrState`, `ma_topiary` is `MaTopiary`
//! (`Mas::new(tree, index, last)` ≡ `MA_STATE(mas, mt, index, last)`).
//!
//! Notes:
//! * `nodes.len()` must be `<= NP` (448) so pool addresses can never
//!   collide with the reserved pseudo-node range; `sheafs` may be any
//!   length (upstream `maple_node_cache` sheaf pool sized to taste).
//! * Allocation-failure injection: set `fail_mask` bit `k` to fail the
//!   (k+1)-th node/sheaf allocation (`alloc_ctr` counts attempts, bits
//!   0..64 effective). `warn`/`warn_once` count MAS_BUG_ON/WARN_ON hits.
//! * `node_free` of a scratch/wild canonical pointer is ignored rather
//!   than panicking (the verified model indexed out of bounds there; no
//!   valid path frees such pointers, so behaviour is unchanged).
//! * Divergence guards: upstream `mas_dup_build` can livelock on a corrupt
//!   tree; the port bounds the outer copy loop at 100000 iterations
//!   (`panic!("dup spin")`, as verified) and the inner ascend at 256
//!   (`panic!("asc spin")` — the model only checked this under trace).
//!   Neither bound is reachable on a valid tree.
//! * The harness fold/trace/RNG machinery (`fold`, `h`, `r`, `xs64`,
//!   `trace`, `epool`, `hot`, `eprintln!`) was harness residue and is not
//!   adopted; `Vec<Mas>` scratch in `mas_topiary_replace` became `[Mas; 3]`.

#![forbid(unsafe_code)]
#![allow(clippy::all)]
#![allow(unused)]

'''

STRUCTS = '''
/* ================= structures ================= */

/// One 256-byte maple node: 32 `u64` words.
///
/// The arena (`MapleTree.nodes`) is caller-owned; `used` models the slab
/// allocated/free state. All C access is through the canonical word/byte
/// helpers (`w`/`sw`/`b`/`sb`/`g32`/`s32`/`rd`/`wr`), so `w` must not be
/// touched directly while the node is in use.
#[derive(Clone)]
pub struct Node {
    pub w: [u64; 32],
    pub used: bool,
}

impl Node {
    pub const fn new() -> Self {
        Node {
            w: [0; 32],
            used: false,
        }
    }
}

/// `struct maple_tree` world over caller-owned arenas.
///
/// Caller invariants (same contract as the C API):
/// * `nodes.len() <= NP` (checked in `new`); `nodes`/`sheafs` must not be
///   modified while in use (`MapleTree::new`/`reset_all` resets them);
/// * stored entries are the kernel's `void *` — any `u64` except the
///   internal/reserved encodings (`xa_is_advanced`, `mt_is_reserved`);
///   `mtree_store_range` WARNs (`warn_once` bit 9) on `xa_is_advanced`;
/// * `Mas` fields are public like the C struct, but mutating
///   `node`/`status`/`offset` mid-operation is out of contract — start
///   each operation with `Mas::new`/`mas_set`/`mas_set_range`;
/// * `gfp` takes the upstream flag values (`GFP_KERNEL`/`GFP_ATOMIC`/…);
///   `mas_preallocate`/`mas_nomem` model the blocking prealloc path.
pub struct MapleTree<'a> {
    /// Allocatable node pool; node `i` is canonical `NBASE + i*256`.
    pub nodes: &'a mut [Node],
    /// Sheaf pool (`maple_node_cache` magazines); sheaf `i` is `SHBASE + i*256`.
    pub sheafs: &'a mut [Sheaf],
    /// Reserved pseudo-nodes at canonical indices `NP..NP+NEXTRA`
    /// (`CPBASE` copy buffer, `NULLN`, `REUSE`, one spare).
    spare: [Node; NEXTRA],
    /// `struct maple_tree.ma_flags` for `MT_CANON`/`MT2_CANON` (`tidx`).
    pub ma_flags: [u32; 2],
    /// `struct maple_tree.ma_root` for `MT_CANON`/`MT2_CANON` (`tidx`).
    pub ma_root: [u64; 2],
    /// Number of node/sheaf allocation attempts (for `fail_mask`).
    pub alloc_ctr: u64,
    /// Bit `k` set => the (k+1)-th allocation attempt fails.
    pub fail_mask: u64,
    /// MAS_BUG_ON/WARN_ON counters (modelled; observable only).
    pub warn: u64,
    pub warn_once: u64,
}

'''

IMPL_HEAD = '''
impl<'a> MapleTree<'a> {
    /// Fresh world over `nodes` + `sheafs`: every node/sheaf is reset, both
    /// tree heads are empty, `ma_flags` set — equivalent to `reset_all` on a
    /// caller arena, i.e. `mt_init_flags`/`MT_INIT` for both trees.
    ///
    /// Panics if `nodes.len() > NP` (pool addresses would collide with the
    /// reserved pseudo-node range).
    pub fn new(nodes: &'a mut [Node], sheafs: &'a mut [Sheaf], flags: u32) -> Self {
        assert!(nodes.len() <= NP, "maple node arena must be <= NP");
        let mut t = MapleTree {
            nodes,
            sheafs,
            spare: [Node::new(), Node::new(), Node::new(), Node::new()],
            ma_flags: [0; 2],
            ma_root: [0; 2],
            alloc_ctr: 0,
            fail_mask: 0,
            warn: 0,
            warn_once: 0,
        };
        t.reset_all(flags);
        t
    }

    /// `wrappers.c` reset / `mt_init_flags`: zero every node word, free all
    /// nodes and sheafs, reset both tree heads and the observable counters.
    /// `fail_mask` persists, faithful to the verified harness.
    pub fn reset_all(&mut self, flags: u32) {
        for n in self.nodes.iter_mut() {
            *n = Node::new();
        }
        for n in self.spare.iter_mut() {
            *n = Node::new();
        }
        for s in self.sheafs.iter_mut() {
            *s = Sheaf::new();
        }
        self.ma_flags = [flags, flags];
        self.ma_root = [0, 0];
        self.alloc_ctr = 0;
        self.warn = 0;
        self.warn_once = 0;
    }

    /* ---- node word / byte access over the canonical space ---- */
    /* `p` may carry low encoding bits; the node index is p & ~0xff.
     * Returns a pool index (< nodes.len()) or a reserved index in
     * NP..NP+NEXTRA (`spare`); out-of-range pointers hit the NULLN scratch
     * slot exactly like the verified model. */
    #[inline]
    fn nidx(&self, p: u64) -> usize {
        let raw = (p & !0xff).wrapping_sub(NBASE) >> 8;
        let raw = raw as usize;
        if raw < self.nodes.len() || (NP..NP + NEXTRA).contains(&raw) {
            return raw;
        }
        /* C would dereference an invalid pointer; use scratch */
        NP + 1
    }
    #[inline]
    pub fn w(&self, p: u64, k: usize) -> u64 {
        /* word index k >= 32 is out of node bounds: C reads adjacent
         * memory; route to the scratch slot like wild pointers. */
        let i = if k < 32 { self.nidx(p) } else { NP + 1 };
        if i < self.nodes.len() {
            self.nodes[i].w[k % 32]
        } else {
            self.spare[i - NP].w[k % 32]
        }
    }
    #[inline]
    pub fn sw(&mut self, p: u64, k: usize, v: u64) {
        let i = if k < 32 { self.nidx(p) } else { NP + 1 };
        if i < self.nodes.len() {
            self.nodes[i].w[k % 32] = v;
        } else {
            self.spare[i - NP].w[k % 32] = v;
        }
    }
    /* little-endian byte access inside node p */
    #[inline]
    pub fn b(&self, p: u64, off: usize) -> u8 {
        (self.w(p, off / 8) >> ((off % 8) * 8)) as u8
    }
    #[inline]
    pub fn sb(&mut self, p: u64, off: usize, v: u8) {
        let sh = (off % 8) * 8;
        let m = !(0xffu64 << sh);
        let nv = (self.w(p, off / 8) & m) | ((v as u64) << sh);
        self.sw(p, off / 8, nv);
    }
    #[inline]
    pub fn g32(&self, p: u64, off: usize) -> u32 {
        (self.w(p, off / 8) >> ((off % 8) * 8)) as u32
    }
    #[inline]
    pub fn s32(&mut self, p: u64, off: usize, v: u32) {
        let sh = (off % 8) * 8;
        let m = !(0xffff_ffffu64 << sh);
        let nv = (self.w(p, off / 8) & m) | ((v as u64) << sh);
        self.sw(p, off / 8, nv);
    }
    /* interior pointer access: canonical node_base + byte_offset */
    #[inline]
    pub fn rd(&self, p: u64) -> u64 {
        self.w(p, ((p & 0xff) >> 3) as usize)
    }
    #[inline]
    pub fn wr(&mut self, p: u64, v: u64) {
        self.sw(p, ((p & 0xff) >> 3) as usize, v);
    }
    /* copy n u64 words node-to-node (memcpy semantics) */
    pub fn wcopy(&mut self, dst: u64, src: u64, n: usize) {
        /* C would run off the end of the 32-word node for n > 32;
         * bound the copy like the model's bounded scratch. */
        let n = n.min(32);
        let mut tmp = [0u64; 32];
        for k in 0..n {
            tmp[k] = self.w(src, k);
        }
        for k in 0..n {
            self.sw(dst, k, tmp[k]);
        }
    }
    /* copy n u64 words between interior pointers */
    pub fn rcopy(&mut self, dst: u64, src: u64, n: usize) {
        let n = n.min(32);
        let mut tmp = [0u64; 32];
        for k in 0..n {
            tmp[k] = self.rd(src.wrapping_add(8 * k as u64));
        }
        for k in 0..n {
            self.wr(dst.wrapping_add(8 * k as u64), tmp[k]);
        }
    }
    pub fn wzero(&mut self, p: u64, k: usize, n: usize) {
        let n = n.min(32);
        for i in 0..n {
            self.sw(p, k.wrapping_add(i), 0);
        }
    }
    pub fn rzero(&mut self, p: u64, n: usize) {
        let n = n.min(32);
        for i in 0..n {
            self.wr(p.wrapping_add(8 * i as u64), 0);
        }
    }

    /* ---- warn counters (MAS_BUG_ON / MAS_WARN_ON / warn_on_once) ---- */
    pub fn warn_on(&mut self, c: bool) -> bool {
        if c {
            self.warn += 1;
        }
        c
    }
    pub fn warn_on_once(&mut self, bit: u32, c: bool) -> bool {
        if c && self.warn_once & (1 << bit) == 0 {
            self.warn_once |= 1 << bit;
            self.warn += 1;
        }
        c
    }

    /* ---- allocator (mirrors wrappers.c slab + sheaf layer) ---- */
    pub fn node_alloc(&mut self) -> Option<u64> {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            return None;
        }
        for i in 0..self.nodes.len() {
            if !self.nodes[i].used {
                self.nodes[i].used = true;
                return Some(NBASE + i as u64 * 256);
            }
        }
        None
    }
    pub fn node_free(&mut self, p: u64) {
        if p == 0 {
            return;
        }
        let i = self.nidx(p);
        if i < self.nodes.len() {
            self.nodes[i].used = false;
        }
    }
    pub fn sheaf_alloc(&mut self) -> Option<u64> {
        self.alloc_ctr += 1;
        if self.alloc_ctr <= 64 && (self.fail_mask >> (self.alloc_ctr - 1)) & 1 == 1 {
            return None;
        }
        for i in 0..self.sheafs.len() {
            if !self.sheafs[i].used {
                self.sheafs[i].used = true;
                return Some(SHBASE + i as u64 * 256);
            }
        }
        None
    }
    pub fn sheaf_free(&mut self, sh: u64) {
        if sh == 0 {
            return;
        }
        let i = self.shidx(sh);
        if self.warn_on(i == usize::MAX) {
            return;
        }
        self.sheafs[i].used = false;
    }
    /// Sheaf pointer -> pool index; a wild `sh` (below `SHBASE`, or past the
    /// caller-owned arena) maps to `usize::MAX` — the `nidx` scratch
    /// convention expressed as "invalid index". Callers must guard it.
    pub fn shidx(&self, sh: u64) -> usize {
        let raw = sh.wrapping_sub(SHBASE) >> 8;
        if raw as usize >= self.sheafs.len() {
            usize::MAX
        } else {
            raw as usize
        }
    }

    /* kmem_cache API */
    pub fn kmem_cache_alloc(&mut self) -> Option<u64> {
        self.node_alloc()
    }
    pub fn kmem_cache_free(&mut self, p: u64) {
        self.node_free(p);
    }
    pub fn kfree(&mut self, p: u64) {
        self.node_free(p);
    }
    pub fn kmem_cache_free_bulk(&mut self, size: usize, p_: &[u64]) {
        for i in 0..size {
            self.node_free(p_[i]);
        }
    }
    pub fn kmem_cache_prefill_sheaf(&mut self, size: usize) -> Option<u64> {
        let sh = self.sheaf_alloc()?;
        let si = self.shidx(sh);
        self.sheafs[si].objs = [0; 32];
        self.sheafs[si].size = 0;
        let mut i = 0usize;
        while i < size && i < 32 {
            match self.node_alloc() {
                Some(n) => self.sheafs[si].objs[i] = n,
                None => {
                    let objs = self.sheafs[si].objs;
                    for j in 0..i {
                        self.node_free(objs[j]);
                    }
                    self.sheaf_free(sh);
                    return None;
                }
            }
            i += 1;
        }
        self.sheafs[si].size = i as u32;
        Some(sh)
    }
    pub fn kmem_cache_refill_sheaf(&mut self, sheaf: u64, size: usize) -> i64 {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return -EINVAL;
        }
        let mut n = self.sheafs[si].size as usize;
        while n < size && n < 32 {
            match self.node_alloc() {
                Some(nn) => {
                    self.sheafs[si].objs[n] = nn;
                    n += 1;
                }
                None => {
                    self.sheafs[si].size = n as u32;
                    return -ENOMEM;
                }
            }
        }
        self.sheafs[si].size = n as u32;
        0
    }
    pub fn kmem_cache_return_sheaf(&mut self, sheaf: u64) {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return;
        }
        let n = self.sheafs[si].size as usize;
        let objs = self.sheafs[si].objs;
        for i in 0..n {
            self.node_free(objs[i]);
        }
        self.sheafs[si].size = 0;
        self.sheaf_free(sheaf);
    }
    pub fn kmem_cache_alloc_from_sheaf(&mut self, sheaf: u64) -> u64 {
        let si = self.shidx(sheaf);
        if self.warn_on(si == usize::MAX) {
            return 0;
        }
        let n = self.sheafs[si].size as usize;
        if n == 0 {
            self.warn += 1;
            return 0;
        }
        let v = self.sheafs[si].objs[n - 1];
        self.sheafs[si].objs[n - 1] = 0;
        self.sheafs[si].size = (n - 1) as u32;
        v
    }
    pub fn sheaf_size(&self, sheaf: u64) -> u32 {
        let si = self.shidx(sheaf);
        if si == usize::MAX {
            return 0;
        }
        self.sheafs[si].size
    }

    /* ---- tree-head access (mt canonical id -> tidx) ---- */
    pub fn tidx(&self, mt: u64) -> usize {
        ((mt >> 12) & 1) as usize
    }
    pub fn tflags(&self, mt: u64) -> u32 {
        self.ma_flags[self.tidx(mt)]
    }
    pub fn set_tflags(&mut self, mt: u64, v: u32) {
        let i = self.tidx(mt);
        self.ma_flags[i] = v;
    }
    pub fn troot(&self, mt: u64) -> u64 {
        self.ma_root[self.tidx(mt)]
    }
    pub fn set_troot(&mut self, mt: u64, v: u64) {
        let i = self.tidx(mt);
        self.ma_root[i] = v;
    }
'''

# methods/free fns get pub on the fn line (all source fns are already pub)
methodpub = methods
freepub = free

out = HEADER
out += CONSTS + "\n\n"
out += "#[derive(Clone)]\n" + SHEAF + '''
impl Sheaf {
    pub const fn new() -> Self {
        Sheaf {
            used: false,
            size: 0,
            objs: [0; 32],
        }
    }
}
'''
out += "\n" + IABS + "\n"
out += CURSORS + "\n"
out += STRUCTS
out += "\n/* ================= constants (interleaved decls) ================= */\n\n"
out += "".join(c if c.startswith("\n") else "\n" + c for c in consts)
out += "\n/* Iteration cap for pointer-chasing loops on crafted state; matches\n"
out += " * the existing `retries > 1_000_000` bound in `mas_erase`. */\n"
out += "pub const MAS_ITER_LIMIT: u64 = 1_000_000;\n"
out += "\n/* ================= free helpers (no pool) ================= */\n"
out += "".join(f if f.startswith("\n") else "\n" + f for f in freepub)
out += IMPL_HEAD
for m in methodpub:
    out += m if m.startswith("\n") else "\n" + m
out += "\n}\n"
out += '''
#[cfg(test)]
mod tests {
    use super::*;
    include!("maple_tree_tests.inc");
}
'''

open(DST, "w", encoding="utf-8", newline="\n").write(out)
print(
    f"wrote {DST}: {len(out.splitlines())} lines, "
    f"{len(methodpub)} methods, {len(freepub)} free fns, {len(consts)} consts"
)
