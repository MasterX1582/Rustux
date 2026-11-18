//! Verified ports of the generic `unsigned long` bitops layer
//! (`include/linux/bitops.h` generic macros + `asm-generic/bitops/*`
//! semantics on LP64) — adopted from `ports/lib-bitmap` where every
//! primitive is exercised transitively by the 30915 recorded bitmap/find
//! cases (plus direct `wrap_{ffs,fls,fns,hweight_long}` records).
//! `no_std`, no allocator, `forbid(unsafe_code)`.
//!
//! Fidelity notes:
//! - `unsigned long` == `u64` == one bitmap word on LP64; bit positions and
//!   sizes are `usize` here.
//! - `__ffs`/`__fls` have a nonzero-input contract in C (UB on 0). They use
//!   `trailing_zeros`/`leading_zeros`, which return 64 for 0 — a defined
//!   Rust choice documenting the same "don't call on 0" contract.
//! - The kernel's `set_bit`/`test_bit` are atomic ops in `asm/bitops.h`;
//!   these are the *non-atomic* word-array semantics (`__set_bit` family),
//!   which is what every bitmap user actually requires. Atomic bitops are
//!   a wave-3 concern.

/// Kernel `BITS_PER_LONG` on LP64.
pub const BITS_PER_LONG: usize = 64;

/// `BIT(nr)` — single-bit mask. Upstream is unmasked `1UL << nr` (UB for
/// `nr >= 64`); the mod-64 wrap here is a defined-Rust choice (matching
/// `BIT_MASK`'s own masking) — callers must keep `nr < 64` anyway since
/// every real caller means BIT, not the wrap.
#[inline]
pub const fn bit(nr: usize) -> u64 {
    1u64 << (nr & 63)
}

/// `BIT_MASK(nr)`.
#[inline]
pub const fn bit_mask(nr: usize) -> u64 {
    bit(nr)
}

/// `BIT_WORD(nr)` — index of the word containing bit `nr`.
#[inline]
pub const fn bit_word(nr: usize) -> usize {
    nr / BITS_PER_LONG
}

/// `BITS_TO_LONGS(nbits)`.
#[inline]
pub const fn bits_to_longs(nbits: usize) -> usize {
    nbits.div_ceil(BITS_PER_LONG)
}

/// `GENMASK(h, l)` — mask of bits `l..=h`. Contract: `l <= h < 64`
/// (the kernel enforces it at compile time via BUILD_BUG_ON; here
/// out-of-contract indices wrap mod 64 silently — uphold the contract).
#[inline]
pub const fn genmask(h: usize, l: usize) -> u64 {
    (!0u64 << (l & 63)) & (!0u64 >> (63 - (h & 63)))
}

/// `BITMAP_LAST_WORD_MASK(nbits)` — mask of valid bits in the last
/// (possibly partial) word; `u64::MAX` when `nbits` is a multiple of 64
/// (C: `~0UL >> (-nbits & 63)`, and callers only apply it to a partial
/// word or a fully-valid last word).
#[inline]
pub const fn bitmap_last_word_mask(nbits: usize) -> u64 {
    !0u64 >> (nbits.wrapping_neg() & 63)
}

/// `BITMAP_FIRST_WORD_MASK(start)` — mask of bits at-or-above `start`
/// within its word.
#[inline]
pub const fn bitmap_first_word_mask(start: usize) -> u64 {
    !0u64 << (start & 63)
}

/// `__ffs(word)` — index of the least significant set bit.
/// Contract (as in C): `word` must be nonzero; 0 yields 64.
#[inline]
pub const fn __ffs(word: u64) -> usize {
    word.trailing_zeros() as usize
}

/// `ffz(word)` — index of the least significant *clear* bit.
/// All-ones input yields 64 (C's `~word` == 0 is the same UB case).
#[inline]
pub const fn ffz(word: u64) -> usize {
    (!word).trailing_zeros() as usize
}

/// `ffs(x)` — upstream takes `int`: 1 + index of the least significant
/// set bit of a 32-bit word; 0 if none. See `ffs64` for the u64 variant.
#[inline]
pub const fn ffs(word: u32) -> u32 {
    if word == 0 {
        0
    } else {
        word.trailing_zeros() + 1
    }
}

/// `ffs64(word)` — same for a 64-bit word.
#[inline]
pub const fn ffs64(word: u64) -> usize {
    if word == 0 {
        0
    } else {
        word.trailing_zeros() as usize + 1
    }
}

/// `fls(x)` — upstream takes `unsigned int`: 1 + index of the most
/// significant set bit of a 32-bit word; 0 if none. See `fls64`.
#[inline]
pub const fn fls(word: u32) -> u32 {
    if word == 0 {
        0
    } else {
        32 - word.leading_zeros()
    }
}

/// `fls64(word)` — same for a 64-bit word.
#[inline]
pub const fn fls64(word: u64) -> usize {
    if word == 0 {
        0
    } else {
        64 - word.leading_zeros() as usize
    }
}

/// `__fls(word)` — index of the most significant set bit.
/// Contract (as in C): `word` must be nonzero; 0 yields `usize::MAX`
/// (matching C's `fls(0) - 1` = -1 under wraparound).
#[inline]
pub const fn __fls(word: u64) -> usize {
    fls64(word).wrapping_sub(1)
}

/// `fns(word, n)` — bit index of the n-th set bit (0-based) in a word, or
/// `BITS_PER_LONG` if the word has <= n bits set.
#[inline]
pub const fn fns(mut word: u64, mut n: u32) -> usize {
    while word != 0 && n != 0 {
        word &= word.wrapping_sub(1);
        n -= 1;
    }
    if word == 0 {
        BITS_PER_LONG
    } else {
        __ffs(word)
    }
}

/// `hweight_long(word)` — popcount of a bitmap word.
#[inline]
pub const fn hweight_long(word: u64) -> u32 {
    word.count_ones()
}

/// `__set_bit(nr, map)` — set bit `nr` (non-atomic).
#[inline]
pub fn set_bit(nr: usize, map: &mut [u64]) {
    map[bit_word(nr)] |= bit_mask(nr);
}

/// `__clear_bit(nr, map)` — clear bit `nr` (non-atomic).
#[inline]
pub fn clear_bit(nr: usize, map: &mut [u64]) {
    map[bit_word(nr)] &= !bit_mask(nr);
}

/// `__change_bit(nr, map)` — toggle bit `nr` (non-atomic).
#[inline]
pub fn change_bit(nr: usize, map: &mut [u64]) {
    map[bit_word(nr)] ^= bit_mask(nr);
}

/// `__assign_bit(nr, map, value)` — set bit `nr` to `value` (non-atomic).
#[inline]
pub fn assign_bit(nr: usize, map: &mut [u64], value: bool) {
    if value {
        set_bit(nr, map);
    } else {
        clear_bit(nr, map);
    }
}

/// `test_bit(nr, map)` — non-atomic bit test.
#[inline]
pub fn test_bit(nr: usize, map: &[u64]) -> bool {
    map[bit_word(nr)] & bit_mask(nr) != 0
}

/// `__test_and_set_bit` — set bit `nr`, returning its previous value.
#[inline]
pub fn test_and_set_bit(nr: usize, map: &mut [u64]) -> bool {
    let old = test_bit(nr, map);
    map[bit_word(nr)] |= bit_mask(nr);
    old
}

/// `__test_and_clear_bit` — clear bit `nr`, returning its previous value.
#[inline]
pub fn test_and_clear_bit(nr: usize, map: &mut [u64]) -> bool {
    let old = test_bit(nr, map);
    map[bit_word(nr)] &= !bit_mask(nr);
    old
}

/// `__test_and_change_bit` — toggle bit `nr`, returning its previous value.
#[inline]
pub fn test_and_change_bit(nr: usize, map: &mut [u64]) -> bool {
    let old = test_bit(nr, map);
    map[bit_word(nr)] ^= bit_mask(nr);
    old
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks() {
        assert_eq!(bitmap_last_word_mask(0), u64::MAX);
        assert_eq!(bitmap_last_word_mask(64), u64::MAX);
        assert_eq!(bitmap_last_word_mask(65), 1);
        assert_eq!(bitmap_last_word_mask(63), u64::MAX >> 1);
        assert_eq!(bitmap_first_word_mask(0), u64::MAX);
        assert_eq!(bitmap_first_word_mask(5), u64::MAX << 5);
        assert_eq!(genmask(7, 3), 0xf8);
        assert_eq!(bits_to_longs(0), 0);
        assert_eq!(bits_to_longs(65), 2);
    }

    #[test]
    fn scans() {
        // Recorded values from wrap_{ffs,fls,fns,hweight_long} cases.
        assert_eq!(ffs(0), 0);
        assert_eq!(ffs(0x8000_0000), 32);
        assert_eq!(ffs64(0x8000_0000_0000_0000), 64);
        assert_eq!(fls(0x8000_0000), 32);
        assert_eq!(fls64(0x8000_0000_0000_0000), 64);
        assert_eq!(fls64(1), 1);
        assert_eq!(__ffs(6), 1);
        assert_eq!(ffz(!0u64 ^ 0b10), 1);
        assert_eq!(fns(0b101101, 2), 3);
        assert_eq!(fns(0b1, 1), 64);
        assert_eq!(hweight_long(0xffff_ffff), 32);
    }

    #[test]
    fn bit_ops() {
        let mut m = [0u64; 2];
        set_bit(65, &mut m);
        assert_eq!(m[1], 2);
        assert!(test_bit(65, &m));
        assert!(test_and_set_bit(65, &mut m));
        assert!(!test_and_set_bit(0, &mut m));
        assert!(test_and_clear_bit(65, &mut m));
        assert!(!test_bit(65, &m));
        change_bit(65, &mut m);
        assert!(test_bit(65, &m));
        assign_bit(65, &mut m, false);
        assert!(!test_bit(65, &m));
    }
}
