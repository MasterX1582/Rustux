//! Verified ports of `lib/find_bit.c`, `lib/bitmap.c` and the
//! `include/linux/{find,bitmap}.h` inline layer — adopted from
//! `ports/lib-bitmap` where they are behavior-verified against the
//! original C over 30915 recorded cases (every public name, plus the
//! compile-time `small_const_nbits` fast paths via constant-size wrappers).
//! `no_std`, no allocator, `forbid(unsafe_code)`.
//!
//! Conventions:
//! - Bitmaps are `&[u64]`/`&mut [u64]` slices (`unsigned long` words on
//!   LP64); sizes/positions are `usize` (`unsigned long`/`unsigned int`
//!   in C).
//! - Contract, exactly as in C: a bitmap slice must hold at least
//!   `bits_to_longs(nbits)` words. Violating it panics on the bounds
//!   check — the safe-Rust equivalent of C's out-of-bounds access.
//! - The kernel's `small_const_nbits`/`__builtin_constant_p` fast paths
//!   only exist for compile-time-constant sizes; they compute identical
//!   results to the general path (verified by the `*_c` wrappers), so the
//!   general path is the whole implementation here.
//! - On little-endian targets the `*_le` finders are aliases of the
//!   regular finders (the `swab`-munged `_find_*_le` only exists in
//!   `__BIG_ENDIAN` builds); they are included under their real names.
//! - `bitmap_alloc`/`zalloc`/`free`/`devm_*` are allocator glue — they
//!   arrive with the `kmalloc` port (wave 1), not here.
//! - `find_random_bit` takes its `get_random_u32_below(w)` draw as a
//!   caller-supplied `FnMut(u32) -> u32`; klib has no RNG yet, and the
//!   harness shim was a deterministic splitmix64 for the same reason.

use crate::bitops::*;

/// `-ENOMEM` and `-EBUSY` (asm-generic errno values used by region ops).
pub const ENOMEM: i32 = 12;
pub const EBUSY: i32 = 16;

// ---- find.h / lib/find_bit.c -------------------------------------------------

/// `FIND_FIRST_BIT(FETCH, /* nop */, size)` — MUNGE is a nop on LE.
fn find_first_g(f: impl Fn(usize) -> u64, size: usize) -> usize {
    let mut idx = 0;
    while idx * BITS_PER_LONG < size {
        let val = f(idx);
        if val != 0 {
            return (idx * BITS_PER_LONG + __ffs(val)).min(size);
        }
        idx += 1;
    }
    size
}

/// `FIND_NEXT_BIT(FETCH, /* nop */, size, start)`.
fn find_next_g(f: impl Fn(usize) -> u64, size: usize, start: usize) -> usize {
    if start >= size {
        return size;
    }
    let mask = bitmap_first_word_mask(start);
    let mut idx = start / BITS_PER_LONG;
    let mut tmp = f(idx) & mask;
    while tmp == 0 {
        if (idx + 1) * BITS_PER_LONG >= size {
            return size;
        }
        idx += 1;
        tmp = f(idx);
    }
    (idx * BITS_PER_LONG + __ffs(tmp)).min(size)
}

/// `FIND_NTH_BIT(FETCH, size, num)`.
fn find_nth_g(f: impl Fn(usize) -> u64, size: usize, num: usize) -> usize {
    let mut nr = num;
    let mut idx = 0;
    let mut tmp = 0;
    while (idx + 1) * BITS_PER_LONG <= size {
        if idx * BITS_PER_LONG + nr >= size {
            return size;
        }
        tmp = f(idx);
        let w = hweight_long(tmp) as usize;
        if w > nr {
            return idx * BITS_PER_LONG + fns(tmp, nr as u32);
        }
        nr -= w;
        idx += 1;
    }
    if size % BITS_PER_LONG != 0 {
        tmp = f(idx) & bitmap_last_word_mask(size);
    }
    idx * BITS_PER_LONG + fns(tmp, nr as u32)
}

/// `find_first_bit` — first set bit in `addr[..size]`, or `size`.
pub fn find_first_bit(addr: &[u64], size: usize) -> usize {
    find_first_g(|i| addr[i], size)
}

/// `find_first_and_bit`.
pub fn find_first_and_bit(a1: &[u64], a2: &[u64], size: usize) -> usize {
    find_first_g(|i| a1[i] & a2[i], size)
}

/// `_find_first_andnot_bit` (no small-const path exists upstream).
pub fn find_first_andnot_bit(a1: &[u64], a2: &[u64], size: usize) -> usize {
    find_first_g(|i| a1[i] & !a2[i], size)
}

/// `_find_first_and_and_bit` (no inline wrapper upstream).
pub fn find_first_and_and_bit(a1: &[u64], a2: &[u64], a3: &[u64], size: usize) -> usize {
    find_first_g(|i| a1[i] & a2[i] & a3[i], size)
}

/// `find_first_zero_bit`.
pub fn find_first_zero_bit(addr: &[u64], size: usize) -> usize {
    find_first_g(|i| !addr[i], size)
}

/// `find_next_bit` — first set bit at or after `offset`, or `size`.
pub fn find_next_bit(addr: &[u64], size: usize, offset: usize) -> usize {
    find_next_g(|i| addr[i], size, offset)
}

/// `find_next_and_bit`.
pub fn find_next_and_bit(a1: &[u64], a2: &[u64], size: usize, offset: usize) -> usize {
    find_next_g(|i| a1[i] & a2[i], size, offset)
}

/// `find_next_andnot_bit`.
pub fn find_next_andnot_bit(a1: &[u64], a2: &[u64], size: usize, offset: usize) -> usize {
    find_next_g(|i| a1[i] & !a2[i], size, offset)
}

/// `find_next_or_bit`.
pub fn find_next_or_bit(a1: &[u64], a2: &[u64], size: usize, offset: usize) -> usize {
    find_next_g(|i| a1[i] | a2[i], size, offset)
}

/// `find_next_zero_bit`.
pub fn find_next_zero_bit(addr: &[u64], size: usize, offset: usize) -> usize {
    find_next_g(|i| !addr[i], size, offset)
}

/// `find_next_bit_le` — little-endian alias of `find_next_bit`.
pub fn find_next_bit_le(addr: &[u64], size: usize, offset: usize) -> usize {
    find_next_bit(addr, size, offset)
}

/// `find_next_zero_bit_le` — little-endian alias.
pub fn find_next_zero_bit_le(addr: &[u64], size: usize, offset: usize) -> usize {
    find_next_zero_bit(addr, size, offset)
}

/// `find_first_zero_bit_le` — little-endian alias.
pub fn find_first_zero_bit_le(addr: &[u64], size: usize) -> usize {
    find_first_zero_bit(addr, size)
}

/// `find_nth_bit` — position of the n-th (0-based) set bit, or >= size.
pub fn find_nth_bit(addr: &[u64], size: usize, n: usize) -> usize {
    if n >= size {
        return size;
    }
    find_nth_g(|i| addr[i], size, n)
}

/// `find_nth_and_bit`.
pub fn find_nth_and_bit(a1: &[u64], a2: &[u64], size: usize, n: usize) -> usize {
    if n >= size {
        return size;
    }
    find_nth_g(|i| a1[i] & a2[i], size, n)
}

/// `find_nth_and_andnot_bit`.
pub fn find_nth_and_andnot_bit(a1: &[u64], a2: &[u64], a3: &[u64], size: usize, n: usize) -> usize {
    if n >= size {
        return size;
    }
    find_nth_g(|i| a1[i] & a2[i] & !a3[i], size, n)
}

/// `find_last_bit` — last set bit in `addr[..size]`, or `size`.
pub fn find_last_bit(addr: &[u64], size: usize) -> usize {
    if size != 0 {
        let mut val = bitmap_last_word_mask(size);
        let mut idx = (size - 1) / BITS_PER_LONG;
        loop {
            val &= addr[idx];
            if val != 0 {
                return idx * BITS_PER_LONG + __fls(val);
            }
            val = !0;
            if idx == 0 {
                break;
            }
            idx -= 1;
        }
    }
    size
}

/// `find_next_bit_wrap` — like `find_next_bit`, wrapping around to the
/// start of the bitmap when the tail has no set bit.
pub fn find_next_bit_wrap(addr: &[u64], size: usize, offset: usize) -> usize {
    let bit = find_next_bit(addr, size, offset);
    if bit < size || offset == 0 {
        return bit;
    }
    let bit = find_first_bit(addr, offset);
    if bit < offset {
        bit
    } else {
        size
    }
}

/// `find_next_and_bit_wrap`.
pub fn find_next_and_bit_wrap(a1: &[u64], a2: &[u64], size: usize, offset: usize) -> usize {
    let bit = find_next_and_bit(a1, a2, size, offset);
    if bit < size || offset == 0 {
        return bit;
    }
    let bit = find_first_and_bit(a1, a2, offset);
    if bit < offset {
        bit
    } else {
        size
    }
}

/// `__for_each_wrap` — single step of the wrap-around iteration used by
/// `for_each_set_bit_wrap`.
pub fn for_each_wrap(bitmap: &[u64], size: usize, start: usize, n: usize) -> usize {
    let mut n = n;
    if n > start {
        let bit = find_next_bit(bitmap, size, n);
        if bit < size {
            return bit;
        }
        n = 0;
    }
    let bit = find_next_bit(bitmap, start, n);
    if bit < start {
        bit
    } else {
        size
    }
}

/// `find_next_clump8` — next 8-bit clump containing a set bit at or after
/// `offset`; returns `(offset, clump)`, `(size, 0)` when none.
pub fn find_next_clump8(addr: &[u64], size: usize, offset: usize) -> (usize, u64) {
    let offset = find_next_bit(addr, size, offset);
    if offset == size {
        return (size, 0);
    }
    let offset = offset & !7;
    (offset, bitmap_get_value8(addr, offset))
}

/// `find_first_clump8`.
pub fn find_first_clump8(addr: &[u64], size: usize) -> (usize, u64) {
    find_next_clump8(addr, size, 0)
}

/// `find_random_bit` — a uniformly chosen set-bit position, or `size`.
/// `rand_below` supplies `get_random_u32_below(w)` draws (caller's RNG)
/// and must return a value `< w`, or the result can exceed `size`.
pub fn find_random_bit(
    addr: &[u64],
    size: usize,
    rand_below: &mut impl FnMut(u32) -> u32,
) -> usize {
    let w = bitmap_weight(addr, size);
    match w {
        0 => size,
        1 => find_first_bit(addr, size),
        _ => find_nth_bit(addr, size, rand_below(w) as usize),
    }
}

// ---- bitmap.h inline layer (general paths) + lib/bitmap.c -------------------

/// `bitmap_size(nbits)` — bytes the bitmap occupies.
#[inline]
pub const fn bitmap_size(nbits: usize) -> usize {
    nbits.div_ceil(BITS_PER_LONG) * 8
}

/// `bitmap_zero`.
pub fn bitmap_zero(dst: &mut [u64], nbits: usize) {
    dst[..bits_to_longs(nbits)].fill(0);
}

/// `bitmap_fill`.
pub fn bitmap_fill(dst: &mut [u64], nbits: usize) {
    dst[..bits_to_longs(nbits)].fill(!0);
}

/// `bitmap_copy`.
pub fn bitmap_copy(dst: &mut [u64], src: &[u64], nbits: usize) {
    let n = bits_to_longs(nbits);
    dst[..n].copy_from_slice(&src[..n]);
}

/// `bitmap_copy_clear_tail`.
pub fn bitmap_copy_clear_tail(dst: &mut [u64], src: &[u64], nbits: usize) {
    bitmap_copy(dst, src, nbits);
    if nbits % BITS_PER_LONG != 0 {
        dst[nbits / BITS_PER_LONG] &= bitmap_last_word_mask(nbits);
    }
}

/// `bitmap_copy_and_extend`. Contract: `count <= size` (C underflows a
/// `size_t` memset length otherwise — UB; here it panics).
pub fn bitmap_copy_and_extend(to: &mut [u64], from: &[u64], count: usize, size: usize) {
    let copy = bits_to_longs(count);
    to[..copy].copy_from_slice(&from[..copy]);
    if count % BITS_PER_LONG != 0 {
        to[copy - 1] &= bitmap_last_word_mask(count);
    }
    to[copy..bits_to_longs(size)].fill(0);
}

/// `bitmap_empty`.
pub fn bitmap_empty(src: &[u64], nbits: usize) -> bool {
    find_first_bit(src, nbits) == nbits
}

/// `bitmap_full`.
pub fn bitmap_full(src: &[u64], nbits: usize) -> bool {
    find_first_zero_bit(src, nbits) == nbits
}

/// `__bitmap_equal`.
fn __bitmap_equal(b1: &[u64], b2: &[u64], bits: usize) -> bool {
    let lim = bits / BITS_PER_LONG;
    if b1[..lim] != b2[..lim] {
        return false;
    }
    bits % BITS_PER_LONG == 0 || (b1[lim] ^ b2[lim]) & bitmap_last_word_mask(bits) == 0
}

/// `bitmap_equal`.
pub fn bitmap_equal(src1: &[u64], src2: &[u64], nbits: usize) -> bool {
    __bitmap_equal(src1, src2, nbits)
}

/// `__bitmap_or_equal`.
pub fn bitmap_or_equal(b1: &[u64], b2: &[u64], b3: &[u64], bits: usize) -> bool {
    let lim = bits / BITS_PER_LONG;
    for k in 0..lim {
        if (b1[k] | b2[k]) != b3[k] {
            return false;
        }
    }
    if bits % BITS_PER_LONG == 0 {
        return true;
    }
    ((b1[lim] | b2[lim]) ^ b3[lim]) & bitmap_last_word_mask(bits) == 0
}

/// `__bitmap_complement`.
pub fn bitmap_complement(dst: &mut [u64], src: &[u64], nbits: usize) {
    for k in 0..bits_to_longs(nbits) {
        dst[k] = !src[k];
    }
}

/// `__bitmap_shift_right`. Contract: `shift / 64 <= bits_to_longs(nbits)`
/// (for larger shifts C's `memset(&dst[lim - off], …)` is itself UB; the
/// slice bounds check panics in its place).
pub fn bitmap_shift_right(dst: &mut [u64], src: &[u64], shift: usize, nbits: usize) {
    let lim = bits_to_longs(nbits);
    let off = shift / BITS_PER_LONG;
    let rem = shift % BITS_PER_LONG;
    let mask = bitmap_last_word_mask(nbits);
    let mut k = 0;
    while off + k < lim {
        let upper = if rem == 0 || off + k + 1 >= lim {
            0
        } else {
            let mut u = src[off + k + 1];
            if off + k + 1 == lim - 1 {
                u &= mask;
            }
            u << (BITS_PER_LONG - rem)
        };
        let mut lower = src[off + k];
        if off + k == lim - 1 {
            lower &= mask;
        }
        dst[k] = (lower >> rem) | upper;
        k += 1;
    }
    dst[lim - off..lim].fill(0);
}

/// `__bitmap_shift_left`.
pub fn bitmap_shift_left(dst: &mut [u64], src: &[u64], shift: usize, nbits: usize) {
    let lim = bits_to_longs(nbits);
    let off = shift / BITS_PER_LONG;
    let rem = shift % BITS_PER_LONG;
    for k in (0..lim - off.min(lim)).rev() {
        let lower = if rem != 0 && k > 0 {
            src[k - 1] >> (BITS_PER_LONG - rem)
        } else {
            0
        };
        dst[k + off] = lower | (src[k] << rem);
    }
    // C memsets `off` words unconditionally — including past `lim` (caller
    // must provide them; the slice bounds check enforces that contract).
    dst[..off].fill(0);
}

/// `bitmap_cut` — remove `cut` bits starting at `first`, shifting the tail
/// down. Contract beyond the usual slice bound: word `first / 64` must
/// exist in `src` and `dst` (C writes it unconditionally, even when
/// `first >= nbits`). C also permits `dst == src`/partial overlap via
/// `memmove`; Rust slices can't alias — copy to a scratch buffer for
/// in-place semantics.
pub fn bitmap_cut(dst: &mut [u64], src: &[u64], first: usize, cut: usize, nbits: usize) {
    let len = bits_to_longs(nbits);
    let mut keep = 0u64;
    if first % BITS_PER_LONG != 0 {
        keep = src[first / BITS_PER_LONG]
            & (!0u64 >> (BITS_PER_LONG - first % BITS_PER_LONG));
    }
    dst[..len].copy_from_slice(&src[..len]);
    for _ in 0..cut {
        for i in (first / BITS_PER_LONG)..len {
            let carry = if i < len - 1 { dst[i + 1] & 1 } else { 0 };
            dst[i] = (dst[i] >> 1) | (carry << (BITS_PER_LONG - 1));
        }
    }
    dst[first / BITS_PER_LONG] &= !0u64 << (first % BITS_PER_LONG);
    dst[first / BITS_PER_LONG] |= keep;
}

/// `__bitmap_and` — returns whether the result is nonzero.
pub fn bitmap_and(dst: &mut [u64], src1: &[u64], src2: &[u64], nbits: usize) -> bool {
    let lim = nbits / BITS_PER_LONG;
    let mut result = 0u64;
    for k in 0..lim {
        dst[k] = src1[k] & src2[k];
        result |= dst[k];
    }
    if nbits % BITS_PER_LONG != 0 {
        dst[lim] = src1[lim] & src2[lim] & bitmap_last_word_mask(nbits);
        result |= dst[lim];
    }
    result != 0
}

/// `__bitmap_or`.
pub fn bitmap_or(dst: &mut [u64], src1: &[u64], src2: &[u64], nbits: usize) {
    for k in 0..bits_to_longs(nbits) {
        dst[k] = src1[k] | src2[k];
    }
}

/// `__bitmap_xor`.
pub fn bitmap_xor(dst: &mut [u64], src1: &[u64], src2: &[u64], nbits: usize) {
    for k in 0..bits_to_longs(nbits) {
        dst[k] = src1[k] ^ src2[k];
    }
}

/// `__bitmap_andnot`.
pub fn bitmap_andnot(dst: &mut [u64], src1: &[u64], src2: &[u64], nbits: usize) -> bool {
    let lim = nbits / BITS_PER_LONG;
    let mut result = 0u64;
    for k in 0..lim {
        dst[k] = src1[k] & !src2[k];
        result |= dst[k];
    }
    if nbits % BITS_PER_LONG != 0 {
        dst[lim] = src1[lim] & !src2[lim] & bitmap_last_word_mask(nbits);
        result |= dst[lim];
    }
    result != 0
}

/// `__bitmap_replace`.
pub fn bitmap_replace(dst: &mut [u64], old: &[u64], new: &[u64], mask: &[u64], nbits: usize) {
    for k in 0..bits_to_longs(nbits) {
        dst[k] = (old[k] & !mask[k]) | (new[k] & mask[k]);
    }
}

/// `__bitmap_intersects`.
pub fn bitmap_intersects(src1: &[u64], src2: &[u64], nbits: usize) -> bool {
    let lim = nbits / BITS_PER_LONG;
    for k in 0..lim {
        if src1[k] & src2[k] != 0 {
            return true;
        }
    }
    nbits % BITS_PER_LONG != 0
        && (src1[lim] & src2[lim]) & bitmap_last_word_mask(nbits) != 0
}

/// `__bitmap_intersects_and`.
pub fn bitmap_intersects_and(src1: &[u64], src2: &[u64], src3: &[u64], nbits: usize) -> bool {
    let lim = nbits / BITS_PER_LONG;
    for k in 0..lim {
        if src1[k] & src2[k] & src3[k] != 0 {
            return true;
        }
    }
    nbits % BITS_PER_LONG != 0
        && (src1[lim] & src2[lim] & src3[lim]) & bitmap_last_word_mask(nbits) != 0
}

/// `__bitmap_subset`.
pub fn bitmap_subset(src1: &[u64], src2: &[u64], nbits: usize) -> bool {
    let lim = nbits / BITS_PER_LONG;
    for k in 0..lim {
        if src1[k] & !src2[k] != 0 {
            return false;
        }
    }
    nbits % BITS_PER_LONG == 0
        || (src1[lim] & !src2[lim]) & bitmap_last_word_mask(nbits) == 0
}

/// `BITMAP_WEIGHT(FETCH, bits)` engine.
fn weight_g(mut f: impl FnMut(usize) -> u64, bits: usize) -> u32 {
    let mut w = 0;
    for idx in 0..bits / BITS_PER_LONG {
        w += hweight_long(f(idx));
    }
    if bits % BITS_PER_LONG != 0 {
        w += hweight_long(f(bits / BITS_PER_LONG) & bitmap_last_word_mask(bits));
    }
    w
}

/// `__bitmap_weight`.
pub fn bitmap_weight(bitmap: &[u64], nbits: usize) -> u32 {
    weight_g(|i| bitmap[i], nbits)
}

/// `__bitmap_weight_and`.
pub fn bitmap_weight_and(b1: &[u64], b2: &[u64], nbits: usize) -> u32 {
    weight_g(|i| b1[i] & b2[i], nbits)
}

/// `__bitmap_weight_andnot`.
pub fn bitmap_weight_andnot(b1: &[u64], b2: &[u64], nbits: usize) -> u32 {
    weight_g(|i| b1[i] & !b2[i], nbits)
}

/// `__bitmap_weighted_or`.
pub fn bitmap_weighted_or(dst: &mut [u64], b1: &[u64], b2: &[u64], nbits: usize) -> u32 {
    weight_g(
        |i| {
            dst[i] = b1[i] | b2[i];
            dst[i]
        },
        nbits,
    )
}

/// `__bitmap_weighted_xor`.
pub fn bitmap_weighted_xor(dst: &mut [u64], b1: &[u64], b2: &[u64], nbits: usize) -> u32 {
    weight_g(
        |i| {
            dst[i] = b1[i] ^ b2[i];
            dst[i]
        },
        nbits,
    )
}

/// `bitmap_weight_from` — weight of bits `start..end` (quirky upstream:
/// returns `end` itself when `start >= end`).
pub fn bitmap_weight_from(bitmap: &[u64], start: usize, end: usize) -> usize {
    if start >= end {
        return end;
    }
    let widx = start / BITS_PER_LONG;
    let end = end - (start & !(BITS_PER_LONG - 1));
    let start = start % BITS_PER_LONG;
    let mut w = bitmap_weight(&bitmap[widx..], end) as usize;
    if start != 0 {
        w -= hweight_long(bitmap[widx] & bitmap_last_word_mask(start)) as usize;
    }
    w
}

/// `__bitmap_set` — set `len` bits starting at `start`. C's `len` is
/// `int` (<= i32::MAX); the usize signature widens that — values above
/// `i64::MAX` wrap negative and set only the partial first-word mask
/// (matching C's `int len` negative-input behavior, which was itself
/// out of contract).
pub fn bitmap_set(map: &mut [u64], start: usize, len: usize) {
    let mut p = bit_word(start);
    // C computes `size` in unsigned int (wraps); only its low 6 bits
    // feed BITMAP_LAST_WORD_MASK so the wrap domain is identical.
    let size = start.wrapping_add(len);
    let mut len = len as i64;
    let mut bits_to_set = (BITS_PER_LONG - start % BITS_PER_LONG) as i64;
    let mut mask = bitmap_first_word_mask(start);
    while len - bits_to_set >= 0 {
        map[p] |= mask;
        len -= bits_to_set;
        bits_to_set = BITS_PER_LONG as i64;
        mask = !0;
        p += 1;
    }
    if len != 0 {
        map[p] |= mask & bitmap_last_word_mask(size);
    }
}

/// `__bitmap_clear`.
pub fn bitmap_clear(map: &mut [u64], start: usize, len: usize) {
    let mut p = bit_word(start);
    let size = start.wrapping_add(len);
    let mut len = len as i64;
    let mut bits_to_clear = (BITS_PER_LONG - start % BITS_PER_LONG) as i64;
    let mut mask = bitmap_first_word_mask(start);
    while len - bits_to_clear >= 0 {
        map[p] &= !mask;
        len -= bits_to_clear;
        bits_to_clear = BITS_PER_LONG as i64;
        mask = !0;
        p += 1;
    }
    if len != 0 {
        map[p] &= !(mask & bitmap_last_word_mask(size));
    }
}

/// `bitmap_find_next_zero_area_off` — find `nr` contiguous aligned zero
/// bits; returns the start offset, or >= `size` when none.
pub fn bitmap_find_next_zero_area_off(
    map: &[u64],
    size: usize,
    mut start: usize,
    nr: usize,
    align_mask: usize,
    align_offset: usize,
) -> usize {
    start = find_next_zero_bit(map, size, start);
    while start < size {
        start = (start
            .wrapping_add(align_offset)
            .wrapping_add(align_mask)
            & !align_mask)
            .wrapping_sub(align_offset);
        // C unsigned wrap on absurd `nr`; wrapping_add avoids a
        // debug/release behavioral split (result is still >= size after
        // wrap, matching C's own wrapped answer).
        let end = start.wrapping_add(nr);
        if end > size {
            break;
        }
        let off = start & !(BITS_PER_LONG - 1);
        let i = find_last_bit(&map[start / BITS_PER_LONG..], end - off).wrapping_add(off);
        if i >= end || i < start {
            return start;
        }
        start = find_next_zero_bit(map, size, i.wrapping_add(1));
    }
    size
}

/// `bitmap_find_next_zero_area`.
pub fn bitmap_find_next_zero_area(
    map: &[u64],
    size: usize,
    start: usize,
    nr: usize,
    align_mask: usize,
) -> usize {
    bitmap_find_next_zero_area_off(map, size, start, nr, align_mask, 0)
}

/// `bitmap_pos_to_ord` (static in C) — ordinal of set bit `pos`, or -1.
fn bitmap_pos_to_ord(buf: &[u64], pos: usize, nbits: usize) -> i64 {
    if pos >= nbits || !test_bit(pos, buf) {
        return -1;
    }
    bitmap_weight(buf, pos) as i64
}

/// `bitmap_remap`. C early-returns when `dst == src` (in-place remap is a
/// no-op); Rust slices can't alias — for in-place semantics copy `src` to
/// a scratch bitmap first.
pub fn bitmap_remap(dst: &mut [u64], src: &[u64], old: &[u64], new: &[u64], nbits: usize) {
    bitmap_zero(dst, nbits);
    let w = bitmap_weight(new, nbits);
    let mut oldbit = find_next_bit(src, nbits, 0);
    while oldbit < nbits {
        let n = bitmap_pos_to_ord(old, oldbit, nbits);
        if n < 0 || w == 0 {
            set_bit(oldbit, dst);
        } else {
            set_bit(find_nth_bit(new, nbits, n as usize % w as usize), dst);
        }
        oldbit = find_next_bit(src, nbits, oldbit + 1);
    }
}

/// `bitmap_bitremap`. Faithful for `bits <= i32::MAX`; C's `int` return
/// truncation and negative `oldbit` domain are not representable here.
pub fn bitmap_bitremap(oldbit: usize, old: &[u64], new: &[u64], bits: usize) -> usize {
    let w = bitmap_weight(new, bits) as i64;
    let n = bitmap_pos_to_ord(old, oldbit, bits);
    if n < 0 || w == 0 {
        oldbit
    } else {
        find_nth_bit(new, bits, n as usize % w as usize)
    }
}

/// `bitmap_onto` (`CONFIG_NUMA` upstream; pure algorithm, always available
/// here). C early-returns on `dst == orig`; pass disjoint slices — see
/// `bitmap_remap`.
pub fn bitmap_onto(dst: &mut [u64], orig: &[u64], relmap: &[u64], bits: usize) {
    bitmap_zero(dst, bits);
    let mut m = 0;
    let mut n = find_next_bit(relmap, bits, 0);
    while n < bits {
        if test_bit(m, orig) {
            set_bit(n, dst);
        }
        m += 1;
        n = find_next_bit(relmap, bits, n + 1);
    }
}

/// `bitmap_fold` (`CONFIG_NUMA` upstream). `sz` must be nonzero —
/// `oldbit % sz` faults in C and panics here when `sz == 0` and `orig`
/// has any set bit. Same `dst == orig` note as `bitmap_onto`.
pub fn bitmap_fold(dst: &mut [u64], orig: &[u64], sz: usize, nbits: usize) {
    bitmap_zero(dst, nbits);
    let mut oldbit = find_next_bit(orig, nbits, 0);
    while oldbit < nbits {
        set_bit(oldbit % sz, dst);
        oldbit = find_next_bit(orig, nbits, oldbit + 1);
    }
}

/// `bitmap_scatter`.
pub fn bitmap_scatter(dst: &mut [u64], src: &[u64], mask: &[u64], nbits: usize) {
    bitmap_zero(dst, nbits);
    let mut n = 0;
    let mut bit = find_next_bit(mask, nbits, 0);
    while bit < nbits {
        assign_bit(bit, dst, test_bit(n, src));
        n += 1;
        bit = find_next_bit(mask, nbits, bit + 1);
    }
}

/// `bitmap_gather`.
pub fn bitmap_gather(dst: &mut [u64], src: &[u64], mask: &[u64], nbits: usize) {
    bitmap_zero(dst, nbits);
    let mut n = 0;
    let mut bit = find_next_bit(mask, nbits, 0);
    while bit < nbits {
        assign_bit(n, dst, test_bit(bit, src));
        n += 1;
        bit = find_next_bit(mask, nbits, bit + 1);
    }
}

/// `bitmap_read` — read `nbits` (1..=64) starting at `start` as a u64.
pub fn bitmap_read(map: &[u64], start: usize, nbits: usize) -> u64 {
    let index = start / BITS_PER_LONG;
    let offset = start % BITS_PER_LONG;
    let space = BITS_PER_LONG - offset;
    if nbits == 0 || nbits > BITS_PER_LONG {
        return 0;
    }
    if space >= nbits {
        return (map[index] >> offset) & bitmap_last_word_mask(nbits);
    }
    let value_low = map[index] & bitmap_first_word_mask(start);
    let value_high = map[index + 1] & bitmap_last_word_mask(start + nbits);
    (value_low >> offset) | (value_high << space)
}

/// `bitmap_write` — write the low `nbits` (1..=64) of `value` at `start`.
pub fn bitmap_write(map: &mut [u64], value: u64, start: usize, nbits: usize) {
    if nbits == 0 || nbits > BITS_PER_LONG {
        return;
    }
    let mask = bitmap_last_word_mask(nbits);
    let value = value & mask;
    let offset = start % BITS_PER_LONG;
    let space = BITS_PER_LONG - offset;
    let fit = space >= nbits;
    let index = start / BITS_PER_LONG;
    map[index] &= if fit {
        !(mask << offset)
    } else {
        !bitmap_first_word_mask(start)
    };
    map[index] |= value << offset;
    if fit {
        return;
    }
    map[index + 1] &= bitmap_first_word_mask(start + nbits);
    map[index + 1] |= value >> space;
}

/// `bitmap_get_value8`.
pub fn bitmap_get_value8(map: &[u64], start: usize) -> u64 {
    bitmap_read(map, start, 8)
}

/// `bitmap_set_value8`.
pub fn bitmap_set_value8(map: &mut [u64], value: u64, start: usize) {
    bitmap_write(map, value, start, 8)
}

/// `bitmap_from_arr32` (LP64 version).
pub fn bitmap_from_arr32(bitmap: &mut [u64], buf: &[u32], nbits: usize) {
    let halfwords = nbits.div_ceil(32);
    let mut i = 0;
    while i < halfwords {
        bitmap[i / 2] = buf[i] as u64;
        i += 1;
        if i < halfwords {
            bitmap[i / 2] |= (buf[i] as u64) << 32;
            i += 1;
        }
    }
    if nbits % BITS_PER_LONG != 0 {
        bitmap[(halfwords - 1) / 2] &= bitmap_last_word_mask(nbits);
    }
}

/// `bitmap_to_arr32` (LP64 version).
pub fn bitmap_to_arr32(buf: &mut [u32], bitmap: &[u64], nbits: usize) {
    let halfwords = nbits.div_ceil(32);
    let mut i = 0;
    while i < halfwords {
        buf[i] = (bitmap[i / 2] & u32::MAX as u64) as u32;
        i += 1;
        if i < halfwords {
            buf[i] = (bitmap[i / 2] >> 32) as u32;
            i += 1;
        }
    }
    if nbits % BITS_PER_LONG != 0 {
        buf[halfwords - 1] &= u32::MAX >> (nbits.wrapping_neg() & 31);
    }
}

/// `bitmap_from_u64` — copy a u64 into the bitmap (64 bits; on LP64 this
/// is `bitmap_copy_clear_tail` over one word).
pub fn bitmap_from_u64(dst: &mut [u64], v: u64) {
    bitmap_copy_clear_tail(dst, &[v], 64);
}

/// `bitmap_from_arr64` — on LP64 this is `bitmap_copy_clear_tail`.
pub fn bitmap_from_arr64(bitmap: &mut [u64], buf: &[u64], nbits: usize) {
    bitmap_copy_clear_tail(bitmap, buf, nbits);
}

/// `bitmap_to_arr64` — on LP64 this is `bitmap_copy_clear_tail`.
pub fn bitmap_to_arr64(buf: &mut [u64], bitmap: &[u64], nbits: usize) {
    bitmap_copy_clear_tail(buf, bitmap, nbits);
}

/// `bitmap_allocate_region` — set `1 << order` bits at `pos` if all are
/// clear; 0 on success, `-EBUSY` (16) otherwise. Contract: `order < 64`
/// (C's `BIT(order)` is UB beyond that; here it wraps mod 64).
pub fn bitmap_allocate_region(bitmap: &mut [u64], pos: usize, order: u32) -> i32 {
    let len = bit(order as usize) as usize;
    let end = pos.wrapping_add(len);
    if find_next_bit(bitmap, end, pos) < end {
        return -EBUSY;
    }
    bitmap_set(bitmap, pos, len);
    0
}

/// `bitmap_release_region`. Contract: `order < 64` (see
/// `bitmap_allocate_region`).
pub fn bitmap_release_region(bitmap: &mut [u64], pos: usize, order: u32) {
    bitmap_clear(bitmap, pos, bit(order as usize) as usize);
}

/// `bitmap_find_free_region` — find `1 << order` contiguous zero bits and
/// allocate them; returns the position or `-ENOMEM` (-12). C's domain is
/// `unsigned int bits`/`int pos`; keep `bits <= i32::MAX` or the returned
/// `pos` truncates exactly like C's `int` result. Contract: `order < 64`.
pub fn bitmap_find_free_region(bitmap: &mut [u64], bits: usize, order: u32) -> i32 {
    let step = bit(order as usize) as usize;
    let mut pos = 0usize;
    while pos.wrapping_add(step) <= bits {
        if bitmap_allocate_region(bitmap, pos, order) == 0 {
            return pos as i32;
        }
        pos += step;
    }
    -ENOMEM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finders() {
        // word0: bits {0, 2, 33}; word1: bit {64}
        let bm = [0x2_0000_0005u64, 0x1];
        assert_eq!(find_first_bit(&bm, 65), 0);
        assert_eq!(find_next_bit(&bm, 65, 1), 2);
        assert_eq!(find_next_bit(&bm, 65, 3), 33);
        assert_eq!(find_next_bit(&bm, 65, 64), 64);
        assert_eq!(find_next_bit(&bm, 65, 65), 65);
        assert_eq!(find_first_zero_bit(&[u64::MAX], 64), 64);
        assert_eq!(find_last_bit(&bm, 65), 64);
        assert_eq!(find_last_bit(&bm, 63), 33);
        assert_eq!(find_last_bit(&[0], 64), 64);
        assert_eq!(find_nth_bit(&bm, 65, 2), 33);
        assert_eq!(find_nth_bit(&bm, 65, 3), 64);
        // nth-bit contract is ">= size", and the word-tail path returns
        // idx*64 + BITS_PER_LONG when absent — C-faithful 128, not 65.
        assert_eq!(find_nth_bit(&bm, 65, 4), 128);
    }

    #[test]
    fn composite_finders() {
        let a = [0b1111u64, 0];
        let b = [0b0110u64, 0];
        assert_eq!(find_first_and_bit(&a, &b, 64), 1);
        assert_eq!(find_first_andnot_bit(&a, &b, 64), 0);
        assert_eq!(find_next_or_bit(&[0x100], &[0x8000], 64, 0), 8);
        assert_eq!(find_next_andnot_bit(&a, &b, 64, 1), 3);
        assert_eq!(find_next_bit_wrap(&[0b11], 64, 62), 0);
        assert_eq!(find_next_clump8(&[0x10000], 64, 0), (16, 1));
        assert_eq!(find_first_clump8(&[0x10000], 64), (16, 1));
    }

    #[test]
    fn logic_ops() {
        let a = [0b1100u64];
        let b = [0b1010u64];
        let mut d = [0u64; 1];
        assert!(bitmap_and(&mut d, &a, &b, 64));
        assert_eq!(d[0], 0b1000);
        bitmap_or(&mut d, &a, &b, 64);
        assert_eq!(d[0], 0b1110);
        bitmap_xor(&mut d, &a, &b, 64);
        assert_eq!(d[0], 0b0110);
        assert!(bitmap_andnot(&mut d, &a, &b, 64));
        assert_eq!(d[0], 0b0100);
        assert!(bitmap_intersects(&a, &b, 64));
        assert!(!bitmap_intersects(&[0xf0], &[0x0f], 8));
        assert!(bitmap_subset(&[0b1000], &a, 64));
        assert!(bitmap_equal(&a, &[0b1100], 64));
        assert!(!bitmap_equal(&a, &[0b1101], 64));
        assert!(bitmap_or_equal(&[0b1000], &[0b0100], &a, 64));
        assert_eq!(bitmap_weight(&a, 64), 2);
        assert_eq!(bitmap_weight(&[u64::MAX], 3), 3);
        assert!(bitmap_empty(&[0], 64));
        assert!(bitmap_full(&[u64::MAX], 64));
        assert!(bitmap_full(&[u64::MAX, 1], 65));
    }

    #[test]
    fn set_clear_shift_cut() {
        let mut m = [0u64; 2];
        bitmap_set(&mut m, 3, 5);
        assert_eq!(m[0] & 0xff, 0b11111000);
        bitmap_set(&mut m, 62, 10);
        assert_eq!(m[1] & 0xff, 0xff);
        bitmap_clear(&mut m, 62, 4);
        assert_eq!(m[0] >> 60, 0);
        assert_eq!(m[1] & 0xff, 0xfc);
        let mut s = [0u64; 2];
        bitmap_shift_left(&mut s, &[1, 0], 65, 128);
        assert_eq!(s[1], 2);
        bitmap_shift_right(&mut s, &[0, 2], 65, 128);
        assert_eq!(s[0], 1);
        // bitmap_cut: remove bits 1..3 of {0b1111} → {0b1001}? verify vs C
        let mut c = [0u64; 1];
        bitmap_cut(&mut c, &[0b1111], 1, 2, 8);
        assert_eq!(c[0], 0b0011);
    }

    #[test]
    fn remap_scatter_regions() {
        // C example: old={4..7}, new={12..15}, src={1,5,7} -> dst={1,13,15}
        let mut old = [0u64; 1];
        bitmap_set(&mut old, 4, 4);
        let mut new = [0u64; 1];
        bitmap_set(&mut new, 12, 4);
        let mut src = [0u64; 1];
        set_bit(1, &mut src);
        set_bit(5, &mut src);
        set_bit(7, &mut src);
        let mut dst = [0u64; 1];
        bitmap_remap(&mut dst, &src, &old, &new, 64);
        assert_eq!(dst[0], bit(1) | bit(13) | bit(15));
        assert_eq!(bitmap_bitremap(5, &old, &new, 64), 13);
        // scatter/gather roundtrip
        let mut g = [0u64; 1];
        let mask = [0b10110u64];
        bitmap_gather(&mut g, &src, &mask, 64);
        let mut back = [0u64; 1];
        bitmap_scatter(&mut back, &g, &mask, 64);
        assert_eq!(back[0] & mask[0], src[0] & mask[0]);
        // regions
        let mut r = [0u64; 1];
        assert_eq!(bitmap_find_free_region(&mut r, 64, 2), 0);
        assert_eq!(r[0] & 0xf, 0xf);
        assert_eq!(bitmap_allocate_region(&mut r, 0, 1), -EBUSY);
        bitmap_release_region(&mut r, 0, 2);
        assert_eq!(r[0], 0);
    }

    #[test]
    fn arr_and_value() {
        let mut bm = [0u64; 2];
        let buf32 = [0xdead_beefu32, 0xcafe_babe];
        bitmap_from_arr32(&mut bm, &buf32, 64);
        assert_eq!(bm[0], 0xcafe_babe_dead_beef);
        let mut out32 = [0u32; 2];
        bitmap_to_arr32(&mut out32, &bm, 64);
        assert_eq!(out32, buf32);
        assert_eq!(bitmap_read(&bm, 4, 8), 0xee);
        let mut m = [0u64; 2];
        bitmap_set_value8(&mut m, 0xa5, 4);
        assert_eq!(bitmap_get_value8(&m, 4), 0xa5);
        // cross-word write
        bitmap_write(&mut m, 0xffff, 60, 8);
        assert_eq!(bitmap_get_value8(&m, 60), 0xff);
    }
}
