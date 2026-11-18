//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/find_bit.c, lib/bitmap.c and the include/linux/{bitmap,find}.h
//! inline layer, driven through the same seeded-bitmap harness as the C wrappers.
//! `unsigned long` = u64 (LP64), `unsigned int` = u32, `int` = i32.
//! For runtime nbits the C inlines' `small_const_nbits` check is false, so the
//! general out-of-line path is what runs and is ported; `*_c` wrappers pass a
//! compile-time nbits (CN=40, or 128/aligned for the memcmp/memset middle
//! paths) and are ported as the small-path formulas the header evaluates.

#![forbid(unsafe_code)]
// pub fn names mirror the wrappers.c export names verbatim (incl. `wrap___*`).
#![allow(non_snake_case)]

const NW: usize = 16; // unsigned longs per bitmap = 1024 bits
const N32: usize = 16; // u32 slots for arr32 conversions
const CN: u64 = 40; // compile-time nbits for small_const_nbits paths

// ---- harness mirror --------------------------------------------------------

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

fn nbits_of(v: i64) -> u64 {
    iabs(v) % 513
}

fn start_of(v: i64) -> u64 {
    iabs(v) % 640
}

fn fill(seed: u64) -> [u64; NW] {
    let mut b = [0u64; NW];
    let mut r = seed | 1;
    for x in b.iter_mut() {
        *x = xs64(&mut r);
    }
    b
}

fn fill32(seed: u64) -> [u32; N32] {
    let mut b = [0u32; N32];
    let mut r = seed | 1;
    for x in b.iter_mut() {
        *x = xs64(&mut r) as u32;
    }
    b
}

fn fill64(seed: u64) -> [u64; NW] {
    fill(seed)
}

/// sel_fill: same mode semantics as wrappers.c (0=own seed, 1=copy of a,
/// 2=~a, 3=zeros, 4=ones).
fn sel_fill(a: &[u64; NW], seed: u64, sel: u64) -> [u64; NW] {
    match sel % 5 {
        1 => *a,
        2 => {
            let mut b = [0u64; NW];
            for i in 0..NW {
                b[i] = !a[i];
            }
            b
        }
        3 => [0u64; NW],
        4 => [!0u64; NW],
        _ => fill(seed),
    }
}

fn cksum(b: &[u64; NW]) -> u64 {
    let mut h = 0x9e3779b97f4a7c15u64;
    for x in b.iter() {
        h = (h ^ x).wrapping_mul(0x100000001b3);
    }
    h
}

fn cksum32(b: &[u32; N32]) -> u64 {
    let mut h = 0x9e3779b97f4a7c15u64;
    for x in b.iter() {
        h = (h ^ (*x as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

fn cksum64(b: &[u64; NW]) -> u64 {
    cksum(b)
}

// ---- kernel bitop primitives ------------------------------------------------

fn nwords(n: u64) -> usize {
    n.div_ceil(64) as usize // BITS_TO_LONGS
}

fn last_mask(nbits: u64) -> u64 {
    // BITMAP_LAST_WORD_MASK: ~0UL >> (-(nbits) & 63)
    !0u64 >> (nbits.wrapping_neg() & 63)
}

fn first_mask(start: u64) -> u64 {
    // BITMAP_FIRST_WORD_MASK: ~0UL << (start & 63)
    !0u64 << (start & 63)
}

fn genmask(h: u64, l: u64) -> u64 {
    // GENMASK(h, l); contract: l <= h < 64 (runtime callers guarantee it)
    (!0u64 << l) & (!0u64 >> (63 - h))
}

fn fns(mut word: u64, n: u32) -> u64 {
    // find N'th set bit in a word; BITS_PER_LONG if absent.
    let mut n = n;
    while word != 0 && n != 0 {
        n -= 1;
        word &= word.wrapping_sub(1);
    }
    if word != 0 {
        word.trailing_zeros() as u64
    } else {
        64
    }
}

fn set_bit(nr: u64, a: &mut [u64]) {
    a[(nr / 64) as usize] |= 1u64 << (nr % 64);
}

fn clear_bit(nr: u64, a: &mut [u64]) {
    a[(nr / 64) as usize] &= !(1u64 << (nr % 64));
}

fn test_bit(nr: u64, a: &[u64]) -> bool {
    (a[(nr / 64) as usize] >> (nr % 64)) & 1 == 1
}

fn assign_bit(nr: u64, a: &mut [u64], v: bool) {
    // __assign_bit
    if v {
        set_bit(nr, a);
    } else {
        clear_bit(nr, a);
    }
}

fn memset_map(map: &mut [u64; NW], byte_off: usize, len: usize, val: u8) {
    // memset((char *)map + byte_off, val, len), little-endian byte order.
    for i in byte_off..byte_off + len {
        let sh = (i % 8) * 8;
        map[i / 8] = (map[i / 8] & !(0xffu64 << sh)) | ((val as u64) << sh);
    }
}

// deterministic splitmix64 matching linux/random.h's harness shim
fn get_random_u32_below(st: &mut u64, bound: u32) -> u32 {
    *st = st.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *st;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^= z >> 31;
    (z % bound as u64) as u32
}

// ---- lib/find_bit.c: FIND_*_BIT general paths -------------------------------
// MUNGE is the nop on little-endian (swab only exists in the __BIG_ENDIAN
// build), so the fetch closures below are the whole FETCH expression.

fn find_first_g<F: Fn(usize) -> u64>(f: F, size: u64) -> u64 {
    // FIND_FIRST_BIT(FETCH, /* nop */, size)
    let mut sz = size;
    let mut idx = 0u64;
    while idx * 64 < sz {
        let val = f(idx as usize);
        if val != 0 {
            sz = (idx * 64 + val.trailing_zeros() as u64).min(sz);
            break;
        }
        idx += 1;
    }
    sz
}

fn find_next_g<F: Fn(usize) -> u64>(f: F, size: u64, start: u64) -> u64 {
    // FIND_NEXT_BIT(FETCH, /* nop */, size, start)
    if start >= size {
        return size;
    }
    let mask = first_mask(start);
    let mut idx = start / 64;
    let mut tmp = f(idx as usize) & mask;
    while tmp == 0 {
        if (idx + 1) * 64 >= size {
            return size;
        }
        idx += 1;
        tmp = f(idx as usize);
    }
    (idx * 64 + tmp.trailing_zeros() as u64).min(size)
}

fn find_nth_g<F: Fn(usize) -> u64>(f: F, size: u64, num: u64) -> u64 {
    // FIND_NTH_BIT(FETCH, size, num)
    let mut nr = num;
    let mut idx = 0u64;
    let mut tmp = 0u64;
    while (idx + 1) * 64 <= size {
        if idx * 64 + nr >= size {
            return size;
        }
        tmp = f(idx as usize);
        let w = tmp.count_ones() as u64;
        if w > nr {
            return idx * 64 + fns(tmp, nr as u32);
        }
        nr -= w;
        idx += 1;
    }
    if !size.is_multiple_of(64) {
        tmp = f(idx as usize) & last_mask(size);
    }
    idx * 64 + fns(tmp, nr as u32)
}

fn find_last_g(addr: &[u64], size: u64) -> u64 {
    // _find_last_bit
    if size != 0 {
        let mut val = last_mask(size);
        let mut idx = (size - 1) / 64;
        loop {
            val &= addr[idx as usize];
            if val != 0 {
                return idx * 64 + (63 - val.leading_zeros() as u64);
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

// ---- include/linux/find.h public finders (runtime size -> general path) -----

fn find_next_bit(a: &[u64], size: u64, off: u64) -> u64 {
    find_next_g(|i| a[i], size, off)
}
fn find_next_and_bit(a1: &[u64], a2: &[u64], size: u64, off: u64) -> u64 {
    find_next_g(|i| a1[i] & a2[i], size, off)
}
fn find_next_andnot_bit(a1: &[u64], a2: &[u64], size: u64, off: u64) -> u64 {
    find_next_g(|i| a1[i] & !a2[i], size, off)
}
fn find_next_or_bit(a1: &[u64], a2: &[u64], size: u64, off: u64) -> u64 {
    find_next_g(|i| a1[i] | a2[i], size, off)
}
fn find_next_zero_bit(a: &[u64], size: u64, off: u64) -> u64 {
    find_next_g(|i| !a[i], size, off)
}
fn find_first_bit(a: &[u64], size: u64) -> u64 {
    find_first_g(|i| a[i], size)
}
fn find_first_and_bit(a1: &[u64], a2: &[u64], size: u64) -> u64 {
    find_first_g(|i| a1[i] & a2[i], size)
}
fn find_first_andnot_bit(a1: &[u64], a2: &[u64], size: u64) -> u64 {
    find_first_g(|i| a1[i] & !a2[i], size)
}
fn find_first_and_and_bit(a1: &[u64], a2: &[u64], a3: &[u64], size: u64) -> u64 {
    find_first_g(|i| a1[i] & a2[i] & a3[i], size)
}
fn find_first_zero_bit(a: &[u64], size: u64) -> u64 {
    find_first_g(|i| !a[i], size)
}
fn find_last_bit(a: &[u64], size: u64) -> u64 {
    find_last_g(a, size)
}
fn find_nth_bit(a: &[u64], size: u64, n: u64) -> u64 {
    if n >= size {
        return size;
    }
    find_nth_g(|i| a[i], size, n)
}
fn find_nth_and_bit(a1: &[u64], a2: &[u64], size: u64, n: u64) -> u64 {
    if n >= size {
        return size;
    }
    find_nth_g(|i| a1[i] & a2[i], size, n)
}
fn find_nth_and_andnot_bit(a1: &[u64], a2: &[u64], a3: &[u64], size: u64, n: u64) -> u64 {
    if n >= size {
        return size;
    }
    find_nth_g(|i| a1[i] & a2[i] & !a3[i], size, n)
}
fn find_next_bit_wrap(a: &[u64], size: u64, off: u64) -> u64 {
    let bit = find_next_bit(a, size, off);
    if bit < size || off == 0 {
        return bit;
    }
    let bit = find_first_bit(a, off);
    if bit < off { bit } else { size }
}
fn find_next_and_bit_wrap(a1: &[u64], a2: &[u64], size: u64, off: u64) -> u64 {
    let bit = find_next_and_bit(a1, a2, size, off);
    if bit < size || off == 0 {
        return bit;
    }
    let bit = find_first_and_bit(a1, a2, off);
    if bit < off { bit } else { size }
}
fn for_each_wrap(bitmap: &[u64], size: u64, start: u64, n: u64) -> u64 {
    // __for_each_wrap
    let mut n = n;
    if n > start {
        let bit = find_next_bit(bitmap, size, n);
        if bit < size {
            return bit;
        }
        n = 0;
    }
    let bit = find_next_bit(bitmap, start, n);
    if bit < start { bit } else { size }
}

// ---- find.h small-path formulas (nbits = CN = 40, compile-time) -------------

fn smask(lo: u64) -> u64 {
    genmask(CN - 1, lo) // GENMASK(39, lo)
}

fn find_next_bit_c(a: &[u64], off: u64) -> u64 {
    if off >= CN {
        return CN;
    }
    let val = a[0] & smask(off);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_next_and_bit_c(a1: &[u64], a2: &[u64], off: u64) -> u64 {
    if off >= CN {
        return CN;
    }
    let val = a1[0] & a2[0] & smask(off);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_next_andnot_bit_c(a1: &[u64], a2: &[u64], off: u64) -> u64 {
    if off >= CN {
        return CN;
    }
    let val = a1[0] & !a2[0] & smask(off);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_next_or_bit_c(a1: &[u64], a2: &[u64], off: u64) -> u64 {
    if off >= CN {
        return CN;
    }
    let val = (a1[0] | a2[0]) & smask(off);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_next_zero_bit_c(a: &[u64], off: u64) -> u64 {
    if off >= CN {
        return CN;
    }
    let val = a[0] | !smask(off);
    if val == !0u64 {
        CN
    } else {
        (!val).trailing_zeros() as u64 // ffz
    }
}
fn find_first_bit_c(a: &[u64]) -> u64 {
    let val = a[0] & smask(0);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_nth_bit_c(a: &[u64], n: u64) -> u64 {
    if n >= CN {
        return CN;
    }
    let val = a[0] & smask(0);
    if val != 0 { fns(val, n as u32) } else { CN }
}
fn find_nth_and_bit_c(a1: &[u64], a2: &[u64], n: u64) -> u64 {
    if n >= CN {
        return CN;
    }
    let val = a1[0] & a2[0] & smask(0);
    if val != 0 { fns(val, n as u32) } else { CN }
}
fn find_nth_and_andnot_bit_c(a1: &[u64], a2: &[u64], a3: &[u64], n: u64) -> u64 {
    if n >= CN {
        return CN;
    }
    let val = a1[0] & a2[0] & !a3[0] & smask(0);
    if val != 0 { fns(val, n as u32) } else { CN }
}
fn find_first_and_bit_c(a1: &[u64], a2: &[u64]) -> u64 {
    let val = a1[0] & a2[0] & smask(0);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_first_andnot_bit_c(a1: &[u64], a2: &[u64]) -> u64 {
    let val = a1[0] & !a2[0] & smask(0);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_first_and_and_bit_c(a1: &[u64], a2: &[u64], a3: &[u64]) -> u64 {
    let val = a1[0] & a2[0] & a3[0] & smask(0);
    if val != 0 {
        val.trailing_zeros() as u64
    } else {
        CN
    }
}
fn find_first_zero_bit_c(a: &[u64]) -> u64 {
    let val = a[0] | !smask(0);
    if val == !0u64 {
        CN
    } else {
        (!val).trailing_zeros() as u64
    }
}
fn find_last_bit_c(a: &[u64]) -> u64 {
    let val = a[0] & smask(0);
    if val != 0 {
        63 - val.leading_zeros() as u64 // __fls
    } else {
        CN
    }
}
fn find_next_bit_wrap_c(a: &[u64], off: u64) -> u64 {
    // first call small (size = CN const); the fallback find_first_bit runs
    // with runtime @offset -> general path
    let bit = find_next_bit_c(a, off);
    if bit < CN || off == 0 {
        return bit;
    }
    let bit = find_first_bit(a, off);
    if bit < off { bit } else { CN }
}
fn find_next_and_bit_wrap_c(a1: &[u64], a2: &[u64], off: u64) -> u64 {
    let bit = find_next_and_bit_c(a1, a2, off);
    if bit < CN || off == 0 {
        return bit;
    }
    let bit = find_first_and_bit(a1, a2, off);
    if bit < off { bit } else { CN }
}

// ---- bitmap.h inline helpers that are identical for any nbits ----------------

fn bitmap_zero_g(dst: &mut [u64], nbits: u32) {
    // runtime: memset(dst, 0, bitmap_size(nbits)) = nwords(nbits) words
    for x in dst[..nwords(nbits as u64)].iter_mut() {
        *x = 0;
    }
}

fn bitmap_read(map: &[u64], start: u64, nbits: u64) -> u64 {
    let index = (start / 64) as usize;
    let offset = start % 64;
    let space = 64 - offset;
    if nbits == 0 || nbits > 64 {
        return 0;
    }
    if space >= nbits {
        return (map[index] >> offset) & last_mask(nbits);
    }
    let value_low = map[index] & first_mask(start);
    let value_high = map[index + 1] & last_mask(start + nbits);
    (value_low >> offset) | (value_high << space)
}

fn bitmap_write(map: &mut [u64], value: u64, start: u64, nbits: u64) {
    if nbits == 0 || nbits > 64 {
        return;
    }
    let mask = last_mask(nbits);
    let value = value & mask;
    let offset = start % 64;
    let space = 64 - offset;
    let fit = space >= nbits;
    let index = (start / 64) as usize;
    map[index] &= if fit {
        !(mask << offset)
    } else {
        !first_mask(start)
    };
    map[index] |= value << offset;
    if fit {
        return;
    }
    map[index + 1] &= first_mask(start + nbits);
    map[index + 1] |= value >> space;
}

fn bitmap_get_value8(map: &[u64], start: u64) -> u64 {
    bitmap_read(map, start, 8)
}

fn find_next_clump8(addr: &[u64], size: u64, offset: u64) -> (u64, u64) {
    // returns (offset, clump)
    let offset = find_next_bit(addr, size, offset);
    if offset == size {
        return (size, 0);
    }
    let offset = offset & !7; // round_down(offset, 8)
    (offset, bitmap_get_value8(addr, offset))
}

// ---- lib/bitmap.c __bitmap_* impls -------------------------------------------

fn bm_equal(b1: &[u64], b2: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        if b1[k] != b2[k] {
            return false;
        }
    }
    bits.is_multiple_of(64) || (b1[lim] ^ b2[lim]) & last_mask(bits as u64) == 0
}

fn bm_or_equal(b1: &[u64], b2: &[u64], b3: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        if (b1[k] | b2[k]) != b3[k] {
            return false;
        }
    }
    if bits.is_multiple_of(64) {
        return true;
    }
    ((b1[lim] | b2[lim]) ^ b3[lim]) & last_mask(bits as u64) == 0
}

fn bm_complement(dst: &mut [u64], src: &[u64], bits: u32) {
    for k in 0..nwords(bits as u64) {
        dst[k] = !src[k];
    }
}

fn bm_shift_right(dst: &mut [u64; NW], src: &[u64; NW], shift: u32, nbits: u32) {
    let lim = nwords(nbits as u64);
    let off = (shift / 64) as usize;
    let rem = shift % 64;
    let mask = last_mask(nbits as u64);
    let mut k = 0usize;
    while off + k < lim {
        let upper = if rem == 0 || off + k + 1 >= lim {
            0
        } else {
            let mut u = src[off + k + 1];
            if off + k + 1 == lim - 1 {
                u &= mask;
            }
            u << (64 - rem)
        };
        let mut lower = src[off + k];
        if off + k == lim - 1 {
            lower &= mask;
        }
        lower >>= rem;
        dst[k] = lower | upper;
        k += 1;
    }
    if off > 0 {
        for x in dst[lim - off..lim].iter_mut() {
            *x = 0;
        }
    }
}

fn bm_shift_left(dst: &mut [u64; NW], src: &[u64; NW], shift: u32, nbits: u32) {
    let lim = nwords(nbits as u64);
    let off = (shift / 64) as usize;
    let rem = shift % 64;
    let mut k = lim as i64 - off as i64 - 1;
    while k >= 0 {
        let lower = if rem != 0 && k > 0 {
            src[(k - 1) as usize] >> (64 - rem)
        } else {
            0
        };
        let upper = src[k as usize] << rem;
        dst[k as usize + off] = lower | upper;
        k -= 1;
    }
    if off > 0 {
        for x in dst[..off].iter_mut() {
            *x = 0;
        }
    }
}

fn bm_cut(dst: &mut [u64; NW], src: &[u64; NW], first: u32, cut: u32, nbits: u32) {
    let len = nwords(nbits as u64);
    let mut keep = 0u64;
    if !first.is_multiple_of(64) {
        keep = src[(first / 64) as usize] & (!0u64 >> (64 - first % 64));
    }
    dst[..len].copy_from_slice(&src[..len]); // memmove; dst/src don't alias here
    for _ in 0..cut {
        for i in (first / 64) as usize..len {
            let carry = if i + 1 < len { dst[i + 1] & 1 } else { 0 };
            dst[i] = (dst[i] >> 1) | (carry << 63);
        }
    }
    dst[(first / 64) as usize] &= !0u64 << (first % 64);
    dst[(first / 64) as usize] |= keep;
}

fn bm_and(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    let mut result = 0u64;
    for k in 0..lim {
        dst[k] = b1[k] & b2[k];
        result |= dst[k];
    }
    if !bits.is_multiple_of(64) {
        dst[lim] = b1[lim] & b2[lim] & last_mask(bits as u64);
        result |= dst[lim];
    }
    result != 0
}

fn bm_or(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) {
    for k in 0..nwords(bits as u64) {
        dst[k] = b1[k] | b2[k];
    }
}

fn bm_xor(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) {
    for k in 0..nwords(bits as u64) {
        dst[k] = b1[k] ^ b2[k];
    }
}

fn bm_andnot(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    let mut result = 0u64;
    for k in 0..lim {
        dst[k] = b1[k] & !b2[k];
        result |= dst[k];
    }
    if !bits.is_multiple_of(64) {
        dst[lim] = b1[lim] & !b2[lim] & last_mask(bits as u64);
        result |= dst[lim];
    }
    result != 0
}

fn bm_replace(dst: &mut [u64], old: &[u64], new: &[u64], mask: &[u64], nbits: u32) {
    for k in 0..nwords(nbits as u64) {
        dst[k] = (old[k] & !mask[k]) | (new[k] & mask[k]);
    }
}

fn bm_intersects(b1: &[u64], b2: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        if b1[k] & b2[k] != 0 {
            return true;
        }
    }
    !bits.is_multiple_of(64) && (b1[lim] & b2[lim]) & last_mask(bits as u64) != 0
}

fn bm_intersects_and(b1: &[u64], b2: &[u64], b3: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        if b1[k] & b2[k] & b3[k] != 0 {
            return true;
        }
    }
    !bits.is_multiple_of(64) && (b1[lim] & b2[lim] & b3[lim]) & last_mask(bits as u64) != 0
}

fn bm_subset(b1: &[u64], b2: &[u64], bits: u32) -> bool {
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        if b1[k] & !b2[k] != 0 {
            return false;
        }
    }
    !(!bits.is_multiple_of(64) && (b1[lim] & !b2[lim]) & last_mask(bits as u64) != 0)
}

fn weight_g<F: Fn(usize) -> u64>(f: F, bits: u32) -> u32 {
    // BITMAP_WEIGHT(FETCH, bits)
    let mut w = 0u32;
    let lim = (bits / 64) as usize;
    for idx in 0..lim {
        w += f(idx).count_ones();
    }
    if !bits.is_multiple_of(64) {
        w += (f(lim) & last_mask(bits as u64)).count_ones();
    }
    w
}

fn bm_weighted_or(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) -> u32 {
    let mut w = 0u32;
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        dst[k] = b1[k] | b2[k];
        w += dst[k].count_ones();
    }
    if !bits.is_multiple_of(64) {
        dst[lim] = b1[lim] | b2[lim];
        w += (dst[lim] & last_mask(bits as u64)).count_ones();
    }
    w
}

fn bm_weighted_xor(dst: &mut [u64], b1: &[u64], b2: &[u64], bits: u32) -> u32 {
    let mut w = 0u32;
    let lim = (bits / 64) as usize;
    for k in 0..lim {
        dst[k] = b1[k] ^ b2[k];
        w += dst[k].count_ones();
    }
    if !bits.is_multiple_of(64) {
        dst[lim] = b1[lim] ^ b2[lim];
        w += (dst[lim] & last_mask(bits as u64)).count_ones();
    }
    w
}

fn bm_set(map: &mut [u64], start: u32, len: i32) {
    // __bitmap_set
    let mut p = (start / 64) as usize;
    let size = start.wrapping_add(len as u32);
    let mut bits_to_set = 64i32 - (start % 64) as i32;
    let mut mask_to_set = first_mask(start as u64);
    let mut len = len;
    while len - bits_to_set >= 0 {
        map[p] |= mask_to_set;
        len -= bits_to_set;
        bits_to_set = 64;
        mask_to_set = !0;
        p += 1;
    }
    if len != 0 {
        mask_to_set &= last_mask(size as u64);
        map[p] |= mask_to_set;
    }
}

fn bm_clear(map: &mut [u64], start: u32, len: i32) {
    // __bitmap_clear
    let mut p = (start / 64) as usize;
    let size = start.wrapping_add(len as u32);
    let mut bits_to_clear = 64i32 - (start % 64) as i32;
    let mut mask_to_clear = first_mask(start as u64);
    let mut len = len;
    while len - bits_to_clear >= 0 {
        map[p] &= !mask_to_clear;
        len -= bits_to_clear;
        bits_to_clear = 64;
        mask_to_clear = !0;
        p += 1;
    }
    if len != 0 {
        mask_to_clear &= last_mask(size as u64);
        map[p] &= !mask_to_clear;
    }
}

fn bitmap_weight(bitmap: &[u64], nbits: u32) -> u32 {
    weight_g(|i| bitmap[i], nbits)
}

fn bitmap_find_next_zero_area_off(
    map: &[u64; NW],
    size: u64,
    mut start: u64,
    nr: u32,
    align_mask: u64,
    align_offset: u64,
) -> u64 {
    start = find_next_zero_bit(map, size, start);
    while start < size {
        // __ALIGN_MASK(start + align_offset, align_mask) - align_offset
        start = ((start.wrapping_add(align_offset).wrapping_add(align_mask)) & !align_mask)
            .wrapping_sub(align_offset);
        let end = start + nr as u64;
        if end > size {
            break;
        }
        let off = start & !63; // round_down(start, BITS_PER_LONG)
        let i = find_last_bit(&map[(start / 64) as usize..], end - off).wrapping_add(off);
        if i >= end || i < start {
            return start;
        }
        start = i;
        start = find_next_zero_bit(map, size, start.wrapping_add(1));
    }
    size
}

fn bitmap_pos_to_ord(buf: &[u64], pos: u32, nbits: u32) -> i32 {
    if pos >= nbits || !test_bit(pos as u64, buf) {
        return -1;
    }
    bitmap_weight(buf, pos) as i32
}

fn bitmap_remap(
    dst: &mut [u64; NW],
    src: &[u64; NW],
    old: &[u64; NW],
    new: &[u64; NW],
    nbits: u32,
) {
    bitmap_zero_g(dst, nbits);
    let w = bitmap_weight(new, nbits);
    let mut oldbit = find_next_bit(src, nbits as u64, 0);
    while oldbit < nbits as u64 {
        let n = bitmap_pos_to_ord(old, oldbit as u32, nbits);
        if n < 0 || w == 0 {
            set_bit(oldbit, dst);
        } else {
            let t = find_nth_bit(new, nbits as u64, (n as u32 % w) as u64);
            set_bit(t, dst);
        }
        oldbit = find_next_bit(src, nbits as u64, oldbit + 1);
    }
}

fn bitmap_bitremap(oldbit: i32, old: &[u64; NW], new: &[u64; NW], bits: i32) -> i32 {
    let w = bitmap_weight(new, bits as u32) as i32;
    let n = bitmap_pos_to_ord(old, oldbit as u32, bits as u32);
    if n < 0 || w == 0 {
        oldbit
    } else {
        find_nth_bit(new, bits as u64, (n % w) as u64) as i32
    }
}

fn bitmap_onto(dst: &mut [u64; NW], orig: &[u64; NW], relmap: &[u64; NW], bits: u32) {
    bitmap_zero_g(dst, bits);
    let mut m: u32 = 0;
    let mut n = find_next_bit(relmap, bits as u64, 0);
    while n < bits as u64 {
        if test_bit(m as u64, orig) {
            set_bit(n, dst);
        }
        m += 1;
        n = find_next_bit(relmap, bits as u64, n + 1);
    }
}

fn bitmap_fold(dst: &mut [u64; NW], orig: &[u64; NW], sz: u32, nbits: u32) {
    bitmap_zero_g(dst, nbits);
    let mut oldbit = find_next_bit(orig, nbits as u64, 0);
    while oldbit < nbits as u64 {
        set_bit(oldbit % sz as u64, dst);
        oldbit = find_next_bit(orig, nbits as u64, oldbit + 1);
    }
}

fn bitmap_scatter(dst: &mut [u64; NW], src: &[u64; NW], mask: &[u64; NW], nbits: u32) {
    bitmap_zero_g(dst, nbits);
    let mut n: u32 = 0;
    let mut bit = find_next_bit(mask, nbits as u64, 0);
    while bit < nbits as u64 {
        assign_bit(bit, dst, test_bit(n as u64, src));
        n += 1;
        bit = find_next_bit(mask, nbits as u64, bit + 1);
    }
}

fn bitmap_gather(dst: &mut [u64; NW], src: &[u64; NW], mask: &[u64; NW], nbits: u32) {
    bitmap_zero_g(dst, nbits);
    let mut n: u32 = 0;
    let mut bit = find_next_bit(mask, nbits as u64, 0);
    while bit < nbits as u64 {
        assign_bit(n as u64, dst, test_bit(bit, src));
        n += 1;
        bit = find_next_bit(mask, nbits as u64, bit + 1);
    }
}

fn bitmap_weight_from(bitmap: &[u64], start: u32, end: u32) -> u64 {
    // bitmap_weight_from; runtime end -> general weight path
    if start >= end {
        return end as u64;
    }
    let widx = (start / 64) as usize;
    let end2 = end - (start & !63);
    let start2 = start % 64;
    let mut w = bitmap_weight(&bitmap[widx..], end2) as u64;
    if start2 != 0 {
        w -= (bitmap[widx] & last_mask(start2 as u64)).count_ones() as u64;
    }
    w
}

fn bitmap_copy_and_extend(to: &mut [u64; NW], from: &[u64; NW], count: u32, size: u32) {
    let copy = nwords(count as u64);
    to[..copy].copy_from_slice(&from[..copy]);
    if !count.is_multiple_of(64) {
        to[copy - 1] &= last_mask(count as u64);
    }
    let total = nwords(size as u64);
    for x in to[copy..total].iter_mut() {
        *x = 0;
    }
}

fn bitmap_from_arr32(bitmap: &mut [u64], buf: &[u32], nbits: u32) {
    let halfwords = nbits.div_ceil(32) as usize;
    let mut i = 0usize;
    while i < halfwords {
        bitmap[i / 2] = buf[i] as u64;
        i += 1;
        if i < halfwords {
            bitmap[i / 2] |= (buf[i] as u64) << 32;
            i += 1;
        }
    }
    if !nbits.is_multiple_of(64) {
        bitmap[(halfwords - 1) / 2] &= last_mask(nbits as u64);
    }
}

fn bitmap_to_arr32(buf: &mut [u32], bitmap: &[u64], nbits: u32) {
    let halfwords = nbits.div_ceil(32) as usize;
    let mut i = 0usize;
    while i < halfwords {
        buf[i] = (bitmap[i / 2] & u32::MAX as u64) as u32;
        i += 1;
        if i < halfwords {
            buf[i] = (bitmap[i / 2] >> 32) as u32;
            i += 1;
        }
    }
    if !nbits.is_multiple_of(64) {
        buf[halfwords - 1] &= u32::MAX >> (nbits.wrapping_neg() & 31);
    }
}

fn bitmap_copy_clear_tail(dst: &mut [u64], src: &[u64], nbits: u32) {
    let n = nwords(nbits as u64);
    dst[..n].copy_from_slice(&src[..n]);
    if !nbits.is_multiple_of(64) {
        dst[(nbits / 64) as usize] &= last_mask(nbits as u64);
    }
}

fn find_random_bit(addr: &[u64], size: u64, rseed: u64) -> u64 {
    let w = bitmap_weight(addr, size as u32);
    let mut st = rseed | 1;
    match w {
        0 => size,
        1 => find_first_bit(addr, size),
        _ => find_nth_bit(addr, size, get_random_u32_below(&mut st, w) as u64),
    }
}

// ======================= wrapper ports ======================================
// Each pub fn mirrors its wrappers.c namesake: same fills, same scalar bounds,
// same op, same checksum packing.

// ---- lib/find_bit.c raw exports ----

pub fn wrap_raw_find_first_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_first_g(|i| b1[i], nbits_of(size))
}
pub fn wrap_raw_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_g(|i| b1[i] & b2[i], nbits_of(size))
}
pub fn wrap_raw_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_g(|i| b1[i] & !b2[i], nbits_of(size))
}
pub fn wrap_raw_find_first_and_and_bit(s1: u64, s2: u64, s3: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_first_g(|i| b1[i] & b2[i] & b3[i], nbits_of(size))
}
pub fn wrap_raw_find_first_zero_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_first_g(|i| !b1[i], nbits_of(size))
}
pub fn wrap_raw_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_g(|i| b1[i], nbits_of(size), start_of(start))
}
pub fn wrap_raw_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s);
    find_nth_g(|i| b1[i], nbits_of(size), start_of(n))
}
pub fn wrap_raw_find_nth_and_bit(s1: u64, s2: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_nth_g(|i| b1[i] & b2[i], nbits_of(size), start_of(n))
}
pub fn wrap_raw_find_nth_and_andnot_bit(s1: u64, s2: u64, s3: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_nth_g(|i| b1[i] & b2[i] & !b3[i], nbits_of(size), start_of(n))
}
pub fn wrap_raw_find_next_and_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_g(|i| b1[i] & b2[i], nbits_of(size), start_of(start))
}
pub fn wrap_raw_find_next_andnot_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_g(|i| b1[i] & !b2[i], nbits_of(size), start_of(start))
}
pub fn wrap_raw_find_next_or_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_g(|i| b1[i] | b2[i], nbits_of(size), start_of(start))
}
pub fn wrap_raw_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_g(|i| !b1[i], nbits_of(size), start_of(start))
}
pub fn wrap_raw_find_last_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_last_g(&b1, nbits_of(size))
}
pub fn wrap_find_next_clump8(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let (off, clump) = find_next_clump8(&b1, nbits_of(size), start_of(start));
    off * 256 + (clump & 0xff)
}
pub fn wrap_find_first_clump8(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let (off, clump) = find_next_clump8(&b1, nbits_of(size), 0);
    off * 256 + (clump & 0xff)
}
pub fn wrap_find_random_bit(s: u64, size: i64, rseed: u64, mode: u64) -> u64 {
    let n = nbits_of(size);
    let mut b1 = fill(s);
    match mode % 3 {
        1 => b1 = [0u64; NW],
        2 => {
            b1 = [0u64; NW];
            if n != 0 {
                b1[0] = 1u64 << (s % 64);
            }
        }
        _ => {}
    }
    find_random_bit(&b1, n, rseed)
}

// ---- find.h public finders (runtime) ----

pub fn wrap_find_first_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_first_bit(&b1, nbits_of(size))
}
pub fn wrap_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit(&b1, nbits_of(size), start_of(start))
}
pub fn wrap_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s);
    find_nth_bit(&b1, nbits_of(size), start_of(n))
}
pub fn wrap_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_and_bit(&b1, &b2, nbits_of(size))
}
pub fn wrap_find_next_and_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_and_bit(&b1, &b2, nbits_of(size), start_of(start))
}
pub fn wrap_find_nth_and_bit(s1: u64, s2: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_nth_and_bit(&b1, &b2, nbits_of(size), start_of(n))
}
pub fn wrap_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_andnot_bit(&b1, &b2, nbits_of(size))
}
pub fn wrap_find_next_andnot_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_andnot_bit(&b1, &b2, nbits_of(size), start_of(start))
}
pub fn wrap_find_nth_and_andnot_bit(s1: u64, s2: u64, s3: u64, size: i64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_nth_and_andnot_bit(&b1, &b2, &b3, nbits_of(size), start_of(n))
}
pub fn wrap_find_first_and_and_bit(s1: u64, s2: u64, s3: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_first_and_and_bit(&b1, &b2, &b3, nbits_of(size))
}
pub fn wrap_find_next_or_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_or_bit(&b1, &b2, nbits_of(size), start_of(start))
}
pub fn wrap_find_first_zero_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_first_zero_bit(&b1, nbits_of(size))
}
pub fn wrap_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_zero_bit(&b1, nbits_of(size), start_of(start))
}
pub fn wrap_find_last_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_last_bit(&b1, nbits_of(size))
}
pub fn wrap_find_next_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit_wrap(&b1, nbits_of(size), start_of(start))
}
pub fn wrap_find_next_and_bit_wrap(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_and_bit_wrap(&b1, &b2, nbits_of(size), start_of(start))
}
pub fn wrap_find_next_bit_le(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit(&b1, nbits_of(size), start_of(start))
}
pub fn wrap_find_next_zero_bit_le(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_zero_bit(&b1, nbits_of(size), start_of(start))
}
pub fn wrap_find_first_zero_bit_le(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    find_first_zero_bit(&b1, nbits_of(size))
}
pub fn wrap___for_each_wrap(s: u64, size: i64, start: i64, n: i64) -> u64 {
    let b1 = fill(s);
    for_each_wrap(&b1, nbits_of(size), start_of(start), start_of(n))
}

// ---- find.h public finders, nbits = CN = 40 constant (small paths) ----

pub fn wrap_find_first_bit_c(s: u64) -> u64 {
    let b1 = fill(s);
    find_first_bit_c(&b1)
}
pub fn wrap_find_next_bit_c(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit_c(&b1, iabs(start) % 45)
}
pub fn wrap_find_nth_bit_c(s: u64, n: i64) -> u64 {
    let b1 = fill(s);
    find_nth_bit_c(&b1, iabs(n) % 45)
}
pub fn wrap_find_first_and_bit_c(s1: u64, s2: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_and_bit_c(&b1, &b2)
}
pub fn wrap_find_next_and_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_and_bit_c(&b1, &b2, iabs(start) % 45)
}
pub fn wrap_find_nth_and_bit_c(s1: u64, s2: u64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_nth_and_bit_c(&b1, &b2, iabs(n) % 45)
}
pub fn wrap_find_first_andnot_bit_c(s1: u64, s2: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_first_andnot_bit_c(&b1, &b2)
}
pub fn wrap_find_next_andnot_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_andnot_bit_c(&b1, &b2, iabs(start) % 45)
}
pub fn wrap_find_nth_and_andnot_bit_c(s1: u64, s2: u64, s3: u64, n: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_nth_and_andnot_bit_c(&b1, &b2, &b3, iabs(n) % 45)
}
pub fn wrap_find_first_and_and_bit_c(s1: u64, s2: u64, s3: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    find_first_and_and_bit_c(&b1, &b2, &b3)
}
pub fn wrap_find_next_or_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_or_bit_c(&b1, &b2, iabs(start) % 45)
}
pub fn wrap_find_first_zero_bit_c(s: u64) -> u64 {
    let b1 = fill(s);
    find_first_zero_bit_c(&b1)
}
pub fn wrap_find_next_zero_bit_c(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_zero_bit_c(&b1, iabs(start) % 45)
}
pub fn wrap_find_last_bit_c(s: u64) -> u64 {
    let b1 = fill(s);
    find_last_bit_c(&b1)
}
pub fn wrap_find_next_bit_wrap_c(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit_wrap_c(&b1, iabs(start) % 45)
}
pub fn wrap_find_next_and_bit_wrap_c(s1: u64, s2: u64, start: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    find_next_and_bit_wrap_c(&b1, &b2, iabs(start) % 45)
}
pub fn wrap_find_next_bit_le_c(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_bit_c(&b1, iabs(start) % 45)
}
pub fn wrap_find_next_zero_bit_le_c(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    find_next_zero_bit_c(&b1, iabs(start) % 45)
}
pub fn wrap_find_first_zero_bit_le_c(s: u64) -> u64 {
    let b1 = fill(s);
    find_first_zero_bit_c(&b1)
}

// ---- find.h for_each iteration macros ----

pub fn wrap_each_set_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_bit(&b1, n, 0);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_bit(&b1, n, b + 1);
    }
    h
}
pub fn wrap_each_set_bit_from(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_bit(&b1, n, start_of(start));
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_bit(&b1, n, b + 1);
    }
    h
}
pub fn wrap_each_clear_bit(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_zero_bit(&b1, n, 0);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_zero_bit(&b1, n, b + 1);
    }
    h
}
pub fn wrap_each_clear_bit_from(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_zero_bit(&b1, n, start_of(start));
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_zero_bit(&b1, n, b + 1);
    }
    h
}
pub fn wrap_each_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_and_bit(&b1, &b2, n, 0);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_and_bit(&b1, &b2, n, b + 1);
    }
    h
}
pub fn wrap_each_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_andnot_bit(&b1, &b2, n, 0);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_andnot_bit(&b1, &b2, n, b + 1);
    }
    h
}
pub fn wrap_each_or_bit(s1: u64, s2: u64, size: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = find_next_or_bit(&b1, &b2, n, 0);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = find_next_or_bit(&b1, &b2, n, b + 1);
    }
    h
}
pub fn wrap_each_set_bitrange(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = 0u64;
    loop {
        b = find_next_bit(&b1, n, b);
        let e = find_next_zero_bit(&b1, n, b.wrapping_add(1));
        if b >= n {
            break;
        }
        h = h
            .wrapping_mul(131)
            .wrapping_add(b.wrapping_mul(17).wrapping_add(e));
        b = e + 1;
    }
    h
}
pub fn wrap_each_set_bitrange_from(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = start_of(start);
    loop {
        b = find_next_bit(&b1, n, b);
        let e = find_next_zero_bit(&b1, n, b.wrapping_add(1));
        if b >= n {
            break;
        }
        h = h
            .wrapping_mul(131)
            .wrapping_add(b.wrapping_mul(17).wrapping_add(e));
        b = e + 1;
    }
    h
}
pub fn wrap_each_clear_bitrange(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = 0u64;
    loop {
        b = find_next_zero_bit(&b1, n, b);
        let e = find_next_bit(&b1, n, b.wrapping_add(1));
        if b >= n {
            break;
        }
        h = h
            .wrapping_mul(131)
            .wrapping_add(b.wrapping_mul(17).wrapping_add(e));
        b = e + 1;
    }
    h
}
pub fn wrap_each_clear_bitrange_from(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let mut b = start_of(start);
    loop {
        b = find_next_zero_bit(&b1, n, b);
        let e = find_next_bit(&b1, n, b.wrapping_add(1));
        if b >= n {
            break;
        }
        h = h
            .wrapping_mul(131)
            .wrapping_add(b.wrapping_mul(17).wrapping_add(e));
        b = e + 1;
    }
    h
}
pub fn wrap_each_set_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let st = start_of(start);
    let mut h = 0u64;
    let mut b = find_next_bit_wrap(&b1, n, st);
    while b < n {
        h = h.wrapping_mul(131).wrapping_add(b);
        b = for_each_wrap(&b1, n, st, b + 1);
    }
    h
}
pub fn wrap_each_set_clump8(s: u64, size: i64) -> u64 {
    let b1 = fill(s);
    let n = nbits_of(size);
    let mut h = 0u64;
    let (mut b, mut clump) = find_next_clump8(&b1, n, 0);
    while b < n {
        h = h
            .wrapping_mul(131)
            .wrapping_add(b.wrapping_mul(256).wrapping_add(clump & 0xff));
        let r = find_next_clump8(&b1, n, b + 8);
        b = r.0;
        clump = r.1;
    }
    h
}

// ---- bitmap.h ops (runtime nbits -> __bitmap_* general paths) ----

pub fn wrap_bitmap_and(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    let r = bm_and(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst) * 4 + r as u64
}
pub fn wrap_bitmap_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    bm_or(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_xor(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    bm_xor(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_andnot(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    let r = bm_andnot(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst) * 4 + r as u64
}
pub fn wrap_bitmap_weighted_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    let w = bm_weighted_or(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst) * 1024 + w as u64
}
pub fn wrap_bitmap_weighted_xor(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    let w = bm_weighted_xor(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst) * 1024 + w as u64
}
pub fn wrap_bitmap_complement(s: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    bm_complement(&mut dst, &b1, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_replace(s1: u64, s2: u64, s3: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    let mut dst = fill(ds);
    bm_replace(&mut dst, &b1, &b2, &b3, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_equal(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    bm_equal(&b1, &b2, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_or_equal(s1: u64, s2: u64, s3: u64, sel: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = or_eq_fill(&b1, &b2, s3, sel);
    bm_or_equal(&b1, &b2, &b3, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_intersects(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    bm_intersects(&b1, &b2, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_intersects_and(s1: u64, s2: u64, s3: u64, sel: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = sel_fill(&b1, s3, sel);
    bm_intersects_and(&b1, &b2, &b3, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_subset(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    bm_subset(&b1, &b2, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_empty(s: u64, mode: u64, nb: i64) -> u64 {
    let mut b1 = fill(s);
    match mode % 3 {
        1 => b1 = [0u64; NW],
        2 => b1 = [!0u64; NW],
        _ => {}
    }
    let n = nbits_of(nb);
    // runtime nbits: find_first_bit(src, nbits) == nbits
    (find_first_bit(&b1, n) == n) as u64
}
pub fn wrap_bitmap_full(s: u64, mode: u64, nb: i64) -> u64 {
    let mut b1 = fill(s);
    match mode % 3 {
        1 => b1 = [0u64; NW],
        2 => b1 = [!0u64; NW],
        _ => {}
    }
    let n = nbits_of(nb);
    (find_first_zero_bit(&b1, n) == n) as u64
}
pub fn wrap_bitmap_weight(s: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    bitmap_weight(&b1, nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_weight_and(s1: u64, s2: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    weight_g(|i| b1[i] & b2[i], nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_weight_andnot(s1: u64, s2: u64, nb: i64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    weight_g(|i| b1[i] & !b2[i], nbits_of(nb) as u32) as u64
}
pub fn wrap_bitmap_weight_from(s: u64, start: i64, end: i64) -> u64 {
    let b1 = fill(s);
    bitmap_weight_from(&b1, start_of(start) as u32, start_of(end) as u32)
}
pub fn wrap_bitmap_set(s: u64, start: i64, len: i64) -> u64 {
    let mut dst = fill(s);
    bm_set(
        &mut dst,
        (iabs(start) % 512) as u32,
        (iabs(len) % 513) as i32,
    );
    cksum(&dst)
}
pub fn wrap_bitmap_clear(s: u64, start: i64, len: i64) -> u64 {
    let mut dst = fill(s);
    bm_clear(
        &mut dst,
        (iabs(start) % 512) as u32,
        (iabs(len) % 513) as i32,
    );
    cksum(&dst)
}
pub fn wrap_bitmap_shift_right(s: u64, ds: u64, shift: i64, nb: i64) -> u64 {
    let n = nbits_of(nb);
    let sh = if n != 0 {
        (iabs(shift) % (n + 64)) as u32
    } else {
        0
    };
    let b1 = fill(s);
    let mut dst = fill(ds);
    bm_shift_right(&mut dst, &b1, sh, n as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_shift_left(s: u64, ds: u64, shift: i64, nb: i64) -> u64 {
    let n = nbits_of(nb);
    let sh = (iabs(shift) % 1024) as u32;
    let b1 = fill(s);
    let mut dst = fill(ds);
    bm_shift_left(&mut dst, &b1, sh, n as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_cut(s: u64, ds: u64, first: i64, cut: i64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    bm_cut(
        &mut dst,
        &b1,
        (iabs(first) % 640) as u32,
        (iabs(cut) % 513) as u32,
        nbits_of(nb) as u32,
    );
    cksum(&dst)
}
pub fn wrap_bitmap_scatter(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let b2 = fill(sm);
    let mut dst = fill(ds);
    bitmap_scatter(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_gather(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let b2 = fill(sm);
    let mut dst = fill(ds);
    bitmap_gather(&mut dst, &b1, &b2, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_remap(s: u64, so: u64, sn: u64, sel: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let b2 = fill(so);
    let b3 = sel_fill(&b2, sn, sel);
    let mut dst = fill(ds);
    bitmap_remap(&mut dst, &b1, &b2, &b3, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_bitremap(so: u64, sn: u64, sel: u64, oldbit: i64, bits: i64) -> u64 {
    let b2 = fill(so);
    let b3 = sel_fill(&b2, sn, sel);
    let r = bitmap_bitremap((iabs(oldbit) % 640) as i32, &b2, &b3, nbits_of(bits) as i32);
    r as i64 as u64
}
pub fn wrap_bitmap_onto(s: u64, sr: u64, sel: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let b2 = fill(sr);
    let b3 = sel_fill(&b2, sr, sel);
    let mut dst = fill(ds);
    bitmap_onto(&mut dst, &b1, &b3, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_fold(s: u64, ds: u64, sz: i64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    bitmap_fold(
        &mut dst,
        &b1,
        (iabs(sz) % 511 + 1) as u32,
        nbits_of(nb) as u32,
    );
    cksum(&dst)
}
pub fn wrap_bitmap_find_next_zero_area(s: u64, size: i64, start: i64, nr: i64, am: i64) -> u64 {
    let dst = fill(s);
    bitmap_find_next_zero_area_off(
        &dst,
        nbits_of(size),
        start_of(start),
        (iabs(nr) % 65) as u32,
        (1u64 << (iabs(am) % 7)) - 1,
        0,
    )
}
pub fn wrap_bitmap_find_next_zero_area_off(
    s: u64,
    size: i64,
    start: i64,
    nr: i64,
    am: i64,
    ao: i64,
) -> u64 {
    let dst = fill(s);
    bitmap_find_next_zero_area_off(
        &dst,
        nbits_of(size),
        start_of(start),
        (iabs(nr) % 65) as u32,
        (1u64 << (iabs(am) % 7)) - 1,
        iabs(ao) % 64,
    )
}
pub fn wrap_bitmap_zero(ds: u64, nb: i64) -> u64 {
    let mut dst = fill(ds);
    bitmap_zero_g(&mut dst, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_fill(ds: u64, nb: i64) -> u64 {
    let mut dst = fill(ds);
    for x in dst[..nwords(nbits_of(nb))].iter_mut() {
        *x = !0;
    }
    cksum(&dst)
}
pub fn wrap_bitmap_copy(s: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    let n = nwords(nbits_of(nb));
    dst[..n].copy_from_slice(&b1[..n]);
    cksum(&dst)
}
pub fn wrap_bitmap_copy_clear_tail(s: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    bitmap_copy_clear_tail(&mut dst, &b1, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_copy_and_extend(s: u64, ds: u64, count: i64, size: i64) -> u64 {
    let n = nbits_of(size);
    let c = if n != 0 {
        (iabs(count) % (n + 1)) as u32
    } else {
        0
    };
    let b1 = fill(s);
    let mut dst = fill(ds);
    bitmap_copy_and_extend(&mut dst, &b1, c, n as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_from_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    let buf = fill32(s);
    let mut dst = fill(ds);
    bitmap_from_arr32(&mut dst, &buf, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_to_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut buf = fill32(ds);
    bitmap_to_arr32(&mut buf, &b1, nbits_of(nb) as u32);
    cksum32(&buf)
}
pub fn wrap_bitmap_from_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    let buf = fill64(s);
    let mut dst = fill(ds);
    // LP64: bitmap_from_arr64 = bitmap_copy_clear_tail
    bitmap_copy_clear_tail(&mut dst, &buf, nbits_of(nb) as u32);
    cksum(&dst)
}
pub fn wrap_bitmap_to_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    let b1 = fill(s);
    let mut buf = fill64(ds);
    bitmap_copy_clear_tail(&mut buf, &b1, nbits_of(nb) as u32);
    cksum64(&buf)
}
pub fn wrap_bitmap_from_u64(v: u64, ds: u64) -> u64 {
    let mut dst = fill(ds);
    let mask = [v, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    bitmap_copy_clear_tail(&mut dst, &mask, 64);
    cksum(&dst)
}
pub fn wrap_bitmap_read(s: u64, start: i64, nb: i64) -> u64 {
    let b1 = fill(s);
    bitmap_read(&b1, iabs(start) % 960, iabs(nb) % 65)
}
pub fn wrap_bitmap_write(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    let mut dst = fill(s);
    bitmap_write(&mut dst, v, iabs(start) % 960, iabs(nb) % 65);
    cksum(&dst)
}
pub fn wrap_bitmap_read_hi(s: u64, start: i64, nb: i64) -> u64 {
    let b1 = fill(s);
    bitmap_read(&b1, iabs(start) % 950, 64 + iabs(nb) % 4)
}
pub fn wrap_bitmap_write_hi(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    let mut dst = fill(s);
    bitmap_write(&mut dst, v, iabs(start) % 950, 64 + iabs(nb) % 4);
    cksum(&dst)
}
pub fn wrap_bitmap_get_value8(s: u64, start: i64) -> u64 {
    let b1 = fill(s);
    bitmap_get_value8(&b1, iabs(start) % 960)
}
pub fn wrap_bitmap_set_value8(s: u64, start: i64, v: i64) -> u64 {
    let mut dst = fill(s);
    bitmap_write(&mut dst, iabs(v) % 300, iabs(start) % 960, 8);
    cksum(&dst)
}
pub fn wrap_bitmap_release_region(s: u64, pos: i64, order: i64) -> u64 {
    let mut dst = fill(s);
    bm_clear(
        &mut dst,
        (iabs(pos) % 513) as u32,
        (1u64 << (iabs(order) % 8)) as i32,
    );
    cksum(&dst)
}
pub fn wrap_bitmap_allocate_region(s: u64, pos: i64, order: i64) -> u64 {
    let mut dst = fill(s);
    let p = iabs(pos) % 513;
    let len = 1u64 << (iabs(order) % 8);
    // bitmap_allocate_region: busy check then bitmap_set
    let r: i64 = if find_next_bit(&dst, p + len, p) < p + len {
        -16 // -EBUSY
    } else {
        bm_set(&mut dst, p as u32, len as i32);
        0
    };
    cksum(&dst) * 32 + (r + 16) as u64
}
pub fn wrap_bitmap_find_free_region(s: u64, mode: u64, bits: i64, order: i64) -> u64 {
    let mut dst = fill(s);
    match mode % 3 {
        1 => dst = [0u64; NW],
        2 => dst = [!0u64; NW],
        _ => {}
    }
    let bits = nbits_of(bits) as u32;
    let len = 1u64 << (iabs(order) % 8);
    // bitmap_find_free_region
    let mut r: i64 = -12; // -ENOMEM
    let mut pos = 0u64;
    while pos + len <= bits as u64 {
        if find_next_bit(&dst, pos + len, pos) >= pos + len {
            bm_set(&mut dst, pos as u32, len as i32);
            r = pos as i64;
            break;
        }
        pos += len;
    }
    cksum(&dst) * 1024 + (r + 13) as u64
}

// ---- bitmap.h small_const_nbits paths (nbits = CN = 40 compile-time) ---------
// These wrappers pass a literal 40 in C, so the header's single-word formulas
// execute; GENMASK(39, x) / BITMAP_LAST_WORD_MASK(40) ported literally.

fn lm40() -> u64 {
    last_mask(CN) // BITMAP_LAST_WORD_MASK(40) = low 40 bits
}

pub fn wrap_bitmap_and_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] & b2[0] & lm40();
    let r = dst[0] != 0;
    cksum(&dst) * 4 + r as u64
}
pub fn wrap_bitmap_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] | b2[0];
    cksum(&dst)
}
pub fn wrap_bitmap_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] ^ b2[0];
    cksum(&dst)
}
pub fn wrap_bitmap_andnot_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] & !b2[0] & lm40();
    let r = dst[0] != 0;
    cksum(&dst) * 4 + r as u64
}
pub fn wrap_bitmap_weighted_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] | b2[0];
    cksum(&dst) * 1024 + (dst[0] & lm40()).count_ones() as u64
}
pub fn wrap_bitmap_weighted_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let mut dst = fill(ds);
    dst[0] = b1[0] ^ b2[0];
    cksum(&dst) * 1024 + (dst[0] & lm40()).count_ones() as u64
}
pub fn wrap_bitmap_complement_c(s: u64, ds: u64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    dst[0] = !b1[0];
    cksum(&dst)
}
pub fn wrap_bitmap_replace_c(s1: u64, s2: u64, s3: u64, ds: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = fill(s3);
    let mut dst = fill(ds);
    dst[0] = (b1[0] & !b3[0]) | (b2[0] & b3[0]);
    cksum(&dst)
}
pub fn wrap_bitmap_equal_c(s1: u64, s2: u64, sel: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    ((b1[0] ^ b2[0]) & lm40() == 0) as u64
}
pub fn wrap_bitmap_equal_c128(s1: u64, s2: u64, sel: u64) -> u64 {
    // nbits=128: __builtin_constant_p(128 & 7) false path -> !memcmp(16 bytes)
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    (b1[..2] == b2[..2]) as u64
}
fn or_eq_fill(b1: &[u64; NW], b2: &[u64; NW], s3: u64, sel: u64) -> [u64; NW] {
    // the sel%4 switch shared by wrap_bitmap_or_equal{,_c}
    match sel % 4 {
        1 => {
            let mut b = [0u64; NW];
            for i in 0..NW {
                b[i] = b1[i] | b2[i];
            }
            b
        }
        2 => [0u64; NW],
        3 => {
            let mut b = [0u64; NW];
            for i in 0..NW {
                b[i] = !(b1[i] | b2[i]);
            }
            b
        }
        _ => fill(s3),
    }
}

pub fn wrap_bitmap_or_equal_c(s1: u64, s2: u64, s3: u64, sel: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = or_eq_fill(&b1, &b2, s3, sel);
    (((b1[0] | b2[0]) ^ b3[0]) & lm40() == 0) as u64
}
pub fn wrap_bitmap_intersects_c(s1: u64, s2: u64, sel: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    (b1[0] & b2[0] & lm40() != 0) as u64
}
pub fn wrap_bitmap_intersects_and_c(s1: u64, s2: u64, s3: u64, sel: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    let b3 = sel_fill(&b1, s3, sel);
    (b1[0] & b2[0] & b3[0] & lm40() != 0) as u64
}
pub fn wrap_bitmap_subset_c(s1: u64, s2: u64, sel: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = sel_fill(&b1, s2, sel);
    (b1[0] & !b2[0] & lm40() == 0) as u64
}
pub fn wrap_bitmap_empty_c(s: u64, mode: u64) -> u64 {
    let mut b1 = fill(s);
    match mode % 3 {
        1 => b1 = [0u64; NW],
        2 => b1 = [!0u64; NW],
        _ => {}
    }
    (b1[0] & lm40() == 0) as u64
}
pub fn wrap_bitmap_full_c(s: u64, mode: u64) -> u64 {
    let mut b1 = fill(s);
    match mode % 3 {
        1 => b1 = [0u64; NW],
        2 => b1 = [!0u64; NW],
        _ => {}
    }
    (!b1[0] & lm40() == 0) as u64
}
pub fn wrap_bitmap_weight_c(s: u64) -> u64 {
    let b1 = fill(s);
    (b1[0] & lm40()).count_ones() as u64
}
pub fn wrap_bitmap_weight_and_c(s1: u64, s2: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    (b1[0] & b2[0] & lm40()).count_ones() as u64
}
pub fn wrap_bitmap_weight_andnot_c(s1: u64, s2: u64) -> u64 {
    let b1 = fill(s1);
    let b2 = fill(s2);
    (b1[0] & !b2[0] & lm40()).count_ones() as u64
}
pub fn wrap_bitmap_weight_from_c(s: u64, start: i64) -> u64 {
    // bitmap_weight_from(bm, start, 40): start>=40 -> 40 else popcount(GENMASK(39,start))
    let b1 = fill(s);
    let st = iabs(start) % 45;
    if st >= CN {
        return CN;
    }
    (b1[0] & genmask(39, st)).count_ones() as u64
}
pub fn wrap_bitmap_zero_c(ds: u64) -> u64 {
    let mut dst = fill(ds);
    dst[0] = 0;
    cksum(&dst)
}
pub fn wrap_bitmap_fill_c(ds: u64) -> u64 {
    let mut dst = fill(ds);
    dst[0] = !0;
    cksum(&dst)
}
pub fn wrap_bitmap_copy_c(s: u64, ds: u64) -> u64 {
    let b1 = fill(s);
    let mut dst = fill(ds);
    dst[0] = b1[0];
    cksum(&dst)
}
pub fn wrap_bitmap_set_c1(s: u64, start: i64) -> u64 {
    // start const 64 path: __set_bit
    let mut dst = fill(s);
    set_bit(iabs(start) % 640, &mut dst);
    cksum(&dst)
}
pub fn wrap_bitmap_clear_c1(s: u64, start: i64) -> u64 {
    let mut dst = fill(s);
    clear_bit(iabs(start) % 640, &mut dst);
    cksum(&dst)
}
pub fn wrap_bitmap_set_cs(s: u64) -> u64 {
    // bitmap_set(map, 10, 30): GENMASK(39, 10) |= into word 0
    let mut dst = fill(s);
    dst[0] |= genmask(39, 10);
    cksum(&dst)
}
pub fn wrap_bitmap_clear_cs(s: u64) -> u64 {
    let mut dst = fill(s);
    dst[0] &= !genmask(39, 10);
    cksum(&dst)
}
pub fn wrap_bitmap_set_cm(s: u64) -> u64 {
    // bitmap_set(map, 64, 128): memset((char *)map + 8, 0xff, 16)
    let mut dst = fill(s);
    memset_map(&mut dst, 8, 16, 0xff);
    cksum(&dst)
}
pub fn wrap_bitmap_clear_cm(s: u64) -> u64 {
    let mut dst = fill(s);
    memset_map(&mut dst, 8, 16, 0);
    cksum(&dst)
}
pub fn wrap_bitmap_shift_right_c(s: u64, ds: u64, shift: i64) -> u64 {
    let sh = iabs(shift) % 64;
    let b1 = fill(s);
    let mut dst = fill(ds);
    dst[0] = (b1[0] & lm40()) >> sh;
    cksum(&dst)
}
pub fn wrap_bitmap_shift_left_c(s: u64, ds: u64, shift: i64) -> u64 {
    let sh = iabs(shift) % 64;
    let b1 = fill(s);
    let mut dst = fill(ds);
    dst[0] = (b1[0] << sh) & lm40();
    cksum(&dst)
}

// ---- bitops primitives ----

pub fn wrap_test_bit(s: u64, nr: i64) -> u64 {
    let b1 = fill(s);
    test_bit(iabs(nr) % 1024, &b1) as u64
}
pub fn wrap_set_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    set_bit(iabs(nr) % 1024, &mut dst);
    cksum(&dst)
}
pub fn wrap_clear_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    clear_bit(iabs(nr) % 1024, &mut dst);
    cksum(&dst)
}
pub fn wrap_change_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    let n = iabs(nr) % 1024;
    dst[(n / 64) as usize] ^= 1u64 << (n % 64);
    cksum(&dst)
}
pub fn wrap___set_bit(s: u64, nr: i64) -> u64 {
    wrap_set_bit(s, nr)
}
pub fn wrap___clear_bit(s: u64, nr: i64) -> u64 {
    wrap_clear_bit(s, nr)
}
pub fn wrap___change_bit(s: u64, nr: i64) -> u64 {
    wrap_change_bit(s, nr)
}
pub fn wrap___assign_bit(s: u64, nr: i64, v: i64) -> u64 {
    let mut dst = fill(s);
    assign_bit(iabs(nr) % 1024, &mut dst, v & 1 != 0);
    cksum(&dst)
}
pub fn wrap_test_and_set_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    let n = iabs(nr) % 1024;
    let old = test_bit(n, &dst);
    set_bit(n, &mut dst);
    cksum(&dst) * 4 + old as u64
}
pub fn wrap_test_and_clear_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    let n = iabs(nr) % 1024;
    let old = test_bit(n, &dst);
    clear_bit(n, &mut dst);
    cksum(&dst) * 4 + old as u64
}
pub fn wrap_test_and_change_bit(s: u64, nr: i64) -> u64 {
    let mut dst = fill(s);
    let n = iabs(nr) % 1024;
    let old = test_bit(n, &dst);
    dst[(n / 64) as usize] ^= 1u64 << (n % 64);
    cksum(&dst) * 4 + old as u64
}
pub fn wrap_hweight_long(v: u64) -> u64 {
    v.count_ones() as u64
}
pub fn wrap_fns(w: u64, n: i64) -> u64 {
    fns(w, (iabs(n) % 65) as u32)
}
pub fn wrap_ffs(v: i64) -> u64 {
    // ffs(int): 1 + ctz(v), 0 if v == 0
    let x = iabs(v) as u32;
    if x == 0 {
        0
    } else {
        x.trailing_zeros() as u64 + 1
    }
}
pub fn wrap_fls(v: u64) -> u64 {
    // fls(unsigned int): 32 - clz(v), 0 if v == 0
    let x = v as u32;
    if x == 0 {
        0
    } else {
        32 - x.leading_zeros() as u64
    }
}
