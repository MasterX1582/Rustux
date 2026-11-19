//! `lib/idr.c` — the `idr`/`ida` integer-allocator APIs riding the shared
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

fn bmap_canon(i: usize) -> u64 {
    BBASE + i as u64 * 256
}
fn bmap_idx(e: u64) -> usize {
    (e.wrapping_sub(BBASE) / 256) as usize
}
fn find_next_zero_bit1(v: u64, limit: u64, start: u64) -> u64 {
    let mut i = start;
    while i < limit {
        if (v >> i) & 1 == 0 {
            return i;
        }
        i += 1;
    }
    limit
}
fn find_in_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64, want_set: bool) -> u64 {
    let mut i = start;
    while i < limit {
        if ((bm[(i / 64) as usize] >> (i % 64)) & 1 == 1) == want_set {
            return i;
        }
        i += 1;
    }
    limit
}
fn find_next_zero_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64) -> u64 {
    find_in_arr(bm, limit, start, false)
}
fn find_next_arr(bm: &[u64; IDA_BITMAP_LONGS], limit: u64, start: u64) -> u64 {
    find_in_arr(bm, limit, start, true)
}
fn bm_set_bit(bm: &mut [u64; IDA_BITMAP_LONGS], i: u64) {
    bm[(i / 64) as usize] |= 1 << (i % 64);
}
fn bm_clear_bit(bm: &mut [u64; IDA_BITMAP_LONGS], i: u64) {
    bm[(i / 64) as usize] &= !(1 << (i % 64));
}
fn bm_test_bit(bm: &[u64; IDA_BITMAP_LONGS], i: u64) -> bool {
    (bm[(i / 64) as usize] >> (i % 64)) & 1 == 1
}
fn bm_full(bm: &[u64; IDA_BITMAP_LONGS]) -> bool {
    bm.iter().all(|&w| w == u64::MAX)
}
fn bm_empty(bm: &[u64; IDA_BITMAP_LONGS]) -> bool {
    bm.iter().all(|&w| w == 0)
}

/* ================= idr veneer ================= */

impl<'a> Idr<'a> {
pub fn idr_preload(&mut self, gfp_mask: u32) {
    let _ = self.rt.__radix_tree_preload(gfp_mask, IDR_PRELOAD_SIZE);
}

fn idr_get_free(
    &mut self,
    iter: &mut RIter,
    gfp: u32,
    max: u64,
) -> Result<SlotL, i64> {
    let mut node: Option<usize> = None;
    let mut slot = SlotL::Root;
    let mut start = iter.next_index;
    let mut offset = 0u64;

    'grow: loop {
        let (mut child, maxindex, mut shift) = self.rt.rt_load_root();
        if !self.rt.root_tag_get(IDR_FREE) {
            start = max_u64(start, maxindex.wrapping_add(1));
        }
        if start > max {
            return Err(-ENOSPC);
        }
        if start > maxindex {
            let error = self.rt.radix_tree_extend(gfp, start, shift);
            if error < 0 {
                return Err(error);
            }
            shift = error as u32;
            child = self.rt.root.head;
        }
        if start == 0 && shift == 0 {
            shift = RADIX_TREE_MAP_SHIFT;
        }
        while shift != 0 {
            shift -= RADIX_TREE_MAP_SHIFT;
            if child == 0 {
                let ni = match self.rt.radix_tree_node_alloc(gfp,
                    node,
                    shift as u8,
                    offset as u8,
                    0,
                    0,
                ) {
                    None => return Err(-ENOMEM),
                    Some(n) => n,
                };
                all_tag_set(nmat(self.rt.nodes, &mut self.rt.scratch, ni), IDR_FREE);
                self.rt.slot_set(slot, node_to_entry(ni));
                if let Some(pn) = node {
                    nmat(self.rt.nodes, &mut self.rt.scratch, pn).count = nat(self.rt.nodes, &self.rt.scratch, pn).count.wrapping_add(1);
                }
                child = node_to_entry(ni);
            } else if !rt_is_internal(child) {
                break;
            }
            let ni = entry_to_node(child);
            node = Some(ni);
            let (c, off) = self.rt.rt_descend(ni, start);
            child = c;
            offset = off;
            if !tag_get(nat(self.rt.nodes, &self.rt.scratch, ni), IDR_FREE, offset) {
                offset = rt_find_next_bit(nat(self.rt.nodes, &self.rt.scratch, ni), IDR_FREE, offset + 1);
                start = next_index(start, nat(self.rt.nodes, &self.rt.scratch, ni), offset);
                if start > max || start == 0 {
                    return Err(-ENOSPC);
                }
                let mut ni = ni;
                let mut __lim = self.rt.nodes.len() + 1;
                while offset == RADIX_TREE_MAP_SIZE {
                    if __lim == 0 { break; }
                    __lim -= 1;
                    offset = nat(self.rt.nodes, &self.rt.scratch, ni).offset as u64 + 1;
                    if nat(self.rt.nodes, &self.rt.scratch, ni).parent == 0 {
                        node = None;
                        continue 'grow;
                    }
                    ni = nidx_ptr(nat(self.rt.nodes, &self.rt.scratch, ni).parent);
                    node = Some(ni);
                    shift = nat(self.rt.nodes, &self.rt.scratch, ni).shift.min(63) as u32;
                }
                child = nat(self.rt.nodes, &self.rt.scratch, ni).slots[(offset as usize).min(CHUNK_SIZE - 1)];
            }
            slot = SlotL::Node(node.unwrap(), offset as usize);
        }
        break;
    }
    iter.index = start;
    iter.next_index = match node {
        Some(ni) => 1u64.wrapping_add(min_u64(max, start | node_maxindex(nat(self.rt.nodes, &self.rt.scratch, ni)))),
        None => 1,
    };
    iter.node = node;
    self.rt.set_iter_tags(iter, node, offset, IDR_FREE);
    Ok(slot)
}

pub fn idr_destroy(&mut self) {
    let node = self.rt.root.head;
    if rt_is_internal(node) {
        self.rt.radix_tree_free_nodes(node);
    }
    self.rt.root.head = 0;
    self.rt.root_tag_set(IDR_FREE);
}

pub fn idr_init_base(&mut self, base: i64) {
    self.rt.xa_init_flags(IDR_RT_MARKER);
    self.idr_base = base as u32;
    self.idr_next = 0;
}

pub fn idr_alloc_u32(&mut self, ptr: u64, nextid: &mut u32, max: u64, gfp: u32) -> i64 {
    if self.rt.warn_on(rt_is_internal(ptr)) {
        return -EINVAL;
    }
    let base = self.idr_base as u64;
    let mut id = *nextid as u64;
    if self.rt.warn_on(self.rt.root.flags & ROOT_IS_IDR == 0) {
        self.rt.root.flags |= IDR_RT_MARKER;
    }
    if max < base {
        return -ENOSPC;
    }
    id = id.saturating_sub(base);
    let mut iter = RIter::new();
    radix_tree_iter_init(&mut iter, id);
    let slot = match self.idr_get_free(&mut iter, gfp, max - base) {
        Err(e) => return e,
        Ok(s) => s,
    };
    *nextid = iter.index.wrapping_add(base) as u32;
    self.rt.radix_tree_iter_replace(&iter, slot, ptr);
    self.rt.radix_tree_iter_tag_clear(&iter, IDR_FREE);
    0
}

pub fn idr_alloc(&mut self, ptr: u64, start: i64, end: i64, gfp: u32) -> i64 {
    let mut id = start as u32;
    if self.rt.warn_on(start < 0) {
        return -EINVAL;
    }
    let max = if end > 0 { end - 1 } else { INT_MAXV };
    let ret = self.idr_alloc_u32(ptr, &mut id, max as u64, gfp);
    if ret != 0 {
        return ret;
    }
    id as i32 as i64
}

pub fn idr_alloc_cyclic(&mut self, ptr: u64, start: i64, end: i64, gfp: u32) -> i64 {
    let mut id = self.idr_next;
    let max = if end > 0 { end - 1 } else { INT_MAXV };
    if (id as i32 as i64) < start {
        id = start as u32;
    }
    let mut err = self.idr_alloc_u32(ptr, &mut id, max as u64, gfp);
    if err == -ENOSPC && (id as i32 as i64) > start {
        id = start as u32;
        err = self.idr_alloc_u32(ptr, &mut id, max as u64, gfp);
    }
    if err != 0 {
        return err;
    }
    self.idr_next = id.wrapping_add(1);
    id as i32 as i64
}

pub fn idr_remove(&mut self, id: u64) -> u64 {
    self.rt.radix_tree_delete_item(id.wrapping_sub(self.idr_base as u64), 0)
}

pub fn idr_find(&self, id: u64) -> u64 {
    self.rt.radix_tree_lookup(id.wrapping_sub(self.idr_base as u64))
}

pub fn idr_get_next_ul(&mut self, nextid: &mut u64) -> u64 {
    let base = self.idr_base as u64;
    let mut id = *nextid;
    id = id.saturating_sub(base);
    let mut iter = RIter::new();
    let mut slot = radix_tree_iter_init(&mut iter, id);
    let entry;
    loop {
        let s = match slot {
            Some(s) => s,
            None => match self.rt.radix_tree_next_chunk(&mut iter, 0) {
                None => {
                    return 0;
                }
                Some(s) => s,
            },
        };
        let e = self.rt.slot_get(s);
        if e == 0 {
            slot = self.rt.radix_tree_next_slot(Some(s), &mut iter, 0);
            continue;
        }
        if !xa_is_internal(e) {
            entry = e;
            break;
        }
        if s != SlotL::Root && !xa_is_retry(e) {
            entry = e;
            break;
        }
        slot = radix_tree_iter_retry(&mut iter);
    }
    *nextid = iter.index.wrapping_add(base);
    entry
}

pub fn idr_get_next(&mut self, nextid: &mut i64) -> u64 {
    let mut id = *nextid as i32 as i64 as u64;
    let entry = self.idr_get_next_ul(&mut id);
    if self.rt.warn_on(id > INT_MAXV as u64) {
        return 0;
    }
    *nextid = id as u32 as i32 as i64;
    entry
}

pub fn idr_replace(&mut self, ptr: u64, id: u64) -> u64 {
    let index = id.wrapping_sub(self.idr_base as u64);
    let (entry, node, slot) = self.rt.__radix_tree_lookup(index);
    let slot = match slot {
        None => return err_ptr(ENOENT),
        Some(s) => s,
    };
    if self.rt.radix_tree_tag_get(index, IDR_FREE) != 0 {
        return err_ptr(ENOENT);
    }
    self.rt.__radix_tree_replace(node, slot, ptr);
    entry
}

pub fn idr_get_cursor(&self) -> u64 {
    self.idr_next as u64
}

pub fn idr_set_cursor(&mut self, val: u64) {
    self.idr_next = val as u32;
}
}


/* ================= ida veneer ================= */

impl<'a> Ida<'a> {
pub fn kzalloc_bitmap(&mut self) -> Option<usize> {
    self.rt.alloc_ctr += 1;
    if self.rt.alloc_ctr <= 64 && (self.rt.fail_mask >> (self.rt.alloc_ctr - 1)) & 1 == 1 {
        return None;
    }
    for i in 0..self.bmaps.len() {
        if !self.bmaps[i].used {
            self.bmaps[i].used = true;
            self.bmaps[i].bits = [0; IDA_BITMAP_LONGS];
            return Some(i);
        }
    }
    None
}

pub fn kfree_bitmap(&mut self, i: usize) {
    if i < self.bmaps.len() {
        self.bmaps[i].used = false;
    }
}

pub fn ida_alloc_range(&mut self, min: u32, max_in: u32, _gfp: u32) -> i64 {
    let mut xas = Xas::new((min as u64) / IDA_BITMAP_BITS);
    let mut bit = (min as u64) % IDA_BITMAP_BITS;
    let mut max = max_in;
    let mut alloc: Option<usize> = None;

    if (min as i32) < 0 {
        return -ENOSPC;
    }
    if (max as i32) < 0 {
        max = i32::MAX as u32;
    }
    let maxb = max as u64;

    'retry: loop {
        let mut bitmap: u64;
        'body: loop {
            /* next: */
            bitmap = self.rt.xas_find_marked(&mut xas, maxb / IDA_BITMAP_BITS, XA_FREE_MARK as usize);
            if xas.index > (min as u64) / IDA_BITMAP_BITS {
                bit = 0;
            }
            if xas.index * IDA_BITMAP_BITS + bit > maxb {
                /* nospc: */
                if let Some(i) = alloc {
                    self.kfree_bitmap(i);
                }
                return -ENOSPC;
            }

            if xa_is_value(bitmap) {
                let mut tmp = xa_to_value(bitmap);
                if bit < BITS_PER_XA_VALUE {
                    bit = find_next_zero_bit1(tmp, BITS_PER_XA_VALUE, bit);
                    if xas.index * IDA_BITMAP_BITS + bit > maxb {
                        if let Some(i) = alloc {
                            self.kfree_bitmap(i);
                        }
                        return -ENOSPC;
                    }
                    if bit < BITS_PER_XA_VALUE {
                        tmp |= 1 << bit;
                        self.rt.xas_store(&mut xas, xa_mk_value(tmp));
                        break 'body;
                    }
                }
                let bi = match alloc {
                    Some(i) => i,
                    None => match self.kzalloc_bitmap() {
                        Some(i) => i,
                        None => {
                            /* goto alloc */
                            match self.kzalloc_bitmap() {
                                Some(i) => {
                                    alloc = Some(i);
                                }
                                None => return -ENOMEM,
                            }
                            xas_set(&mut xas, (min as u64) / IDA_BITMAP_BITS);
                            bit = (min as u64) % IDA_BITMAP_BITS;
                            continue 'retry;
                        }
                    },
                };
                self.bmaps[bi].bits[0] = tmp;
                self.rt.xas_store(&mut xas, bmap_canon(bi));
                if xas_error(&xas) != 0 {
                    self.bmaps[bi].bits[0] = 0;
                    break 'body;
                }
                bitmap = bmap_canon(bi);
            }

            if bitmap != 0 {
                let bi = bmap_idx(bitmap);
                if self.rt.warn_on(bi >= self.bmaps.len()) {
                    continue 'body;
                }
                bit = find_next_zero_arr(&self.bmaps[bi].bits, IDA_BITMAP_BITS, bit);
                if xas.index * IDA_BITMAP_BITS + bit > maxb {
                    if let Some(i) = alloc {
                        self.kfree_bitmap(i);
                    }
                    return -ENOSPC;
                }
                if bit == IDA_BITMAP_BITS {
                    continue 'body; /* goto next */
                }
                bm_set_bit(&mut self.bmaps[bi].bits, bit);
                if bm_full(&self.bmaps[bi].bits) {
                    self.rt.xas_clear_mark(&xas, XA_FREE_MARK as usize);
                }
            } else {
                if bit < BITS_PER_XA_VALUE {
                    bitmap = xa_mk_value(1 << bit);
                } else {
                    let bi = match alloc {
                        Some(i) => i,
                        None => match self.kzalloc_bitmap() {
                            Some(i) => i,
                            None => {
                                match self.kzalloc_bitmap() {
                                    Some(i) => {
                                        alloc = Some(i);
                                    }
                                    None => return -ENOMEM,
                                }
                                xas_set(&mut xas, (min as u64) / IDA_BITMAP_BITS);
                                bit = (min as u64) % IDA_BITMAP_BITS;
                                continue 'retry;
                            }
                        },
                    };
                    bm_set_bit(&mut self.bmaps[bi].bits, bit);
                    bitmap = bmap_canon(bi);
                }
                self.rt.xas_store(&mut xas, bitmap);
            }
            break 'body;
        }
        /* out: */
        if self.rt.xas_nomem(&mut xas) {
            xas.index = (min as u64) / IDA_BITMAP_BITS;
            bit = (min as u64) % IDA_BITMAP_BITS;
            continue 'retry;
        }
        if Some(bitmap) != alloc.map(bmap_canon) {
            if let Some(i) = alloc {
                self.kfree_bitmap(i);
            }
        }
        if xas_error(&xas) != 0 {
            return xas_error(&xas);
        }
        return (xas.index * IDA_BITMAP_BITS + bit) as i64;
    }
}

pub fn ida_free(&mut self, id: u32) {
    let mut xas = Xas::new((id as u64) / IDA_BITMAP_BITS);
    let bit = (id as u64) % IDA_BITMAP_BITS;
    if (id as i32) < 0 {
        return;
    }
    let bitmap = self.rt.xas_load(&mut xas);
    if xa_is_value(bitmap) {
        let mut v = xa_to_value(bitmap);
        if bit >= BITS_PER_XA_VALUE {
            self.rt.warn_on(true);
            return;
        }
        if v & (1 << bit) == 0 {
            self.rt.warn_on(true);
            return;
        }
        v &= !(1 << bit);
        if v == 0 {
            self.rt.xas_store(&mut xas, 0);
            return;
        }
        self.rt.xas_store(&mut xas, xa_mk_value(v));
    } else {
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
    }
}

pub fn ida_destroy(&mut self) {
    let mut xas = Xas::new(0);
    let mut bitmap = self.rt.xas_find(&mut xas, ULONG_MAXV);
    while bitmap != 0 {
        if !xa_is_value(bitmap) {
            self.kfree_bitmap(bmap_idx(bitmap));
        }
        self.rt.xas_store(&mut xas, 0);
        bitmap = self.rt.xas_next_entry(&mut xas, ULONG_MAXV);
    }
}

pub fn ida_find_first_range(&mut self, min: u32, max_in: u32) -> i64 {
    let mut max = max_in;
    if (min as i32) < 0 {
        return -EINVAL;
    }
    if (max as i32) < 0 {
        max = i32::MAX as u32;
    }
    let mut index = (min as u64) / IDA_BITMAP_BITS;
    let mut offset = (min as u64) % IDA_BITMAP_BITS;
    let entry = self.rt.xa_find(&mut index,
        (max as u64) / IDA_BITMAP_BITS,
        XA_PRESENT,
    );
    if entry == 0 {
        return -ENOENT;
    }
    if index > (min as u64) / IDA_BITMAP_BITS {
        offset = 0;
    }
    if index * IDA_BITMAP_BITS + offset > max as u64 {
        return -ENOENT;
    }
    let (size, bit) = if xa_is_value(entry) {
        let tmp = xa_to_value(entry);
        (BITS_PER_XA_VALUE, find_next_zero_bit1(!tmp, BITS_PER_XA_VALUE, offset))
    } else {
        let bm = match self.bmaps.get(bmap_idx(entry)) {
            Some(b) => &b.bits,
            None => {
                self.rt.warn_on(true);
                return -ENOENT;
            }
        };
        (IDA_BITMAP_BITS, find_next_arr(bm, IDA_BITMAP_BITS, offset))
    };
    if bit == size || index * IDA_BITMAP_BITS + bit > max as u64 {
        return -ENOENT;
    }
    (index * IDA_BITMAP_BITS + bit) as i64
}

pub fn ida_exists(&mut self, id: u32) -> u64 {
    (self.ida_find_first_range(id, id) == id as i64) as u64
}

pub fn ida_find_first(&mut self) -> i64 {
    self.ida_find_first_range(0, !0)
}

pub fn ida_is_empty(&self) -> bool {
    self.rt.xa_empty()
}

pub fn ida_alloc(&mut self, gfp: u32) -> i64 {
    self.ida_alloc_range(0, !0, gfp)
}

pub fn ida_alloc_min(&mut self, min: u32, gfp: u32) -> i64 {
    self.ida_alloc_range(min, !0, gfp)
}

pub fn ida_alloc_max(&mut self, max: u32, gfp: u32) -> i64 {
    self.ida_alloc_range(0, max, gfp)
}
}


#[cfg(test)]
mod tests {
    use super::*;
    include!("idr_tests.inc");
}
