//! BCD conversion — port of `lib/bcd.c` from the C kernel.
//!
//! Verified against the original's recorded behavior:
//! `ports/lib-core/rust/bcd-rs` (34/34 cases, gate passed).

/// Port of `_bcd2bin` (`bcd2bin` macro): packed BCD byte to binary.
pub const fn bcd2bin(val: u8) -> u32 {
    (val & 0x0f) as u32 + (val >> 4) as u32 * 10
}

/// Port of `_bin2bcd` (`bin2bcd` macro): binary to packed BCD byte.
/// Uses the upstream multiply-shift division ((val*103)>>10); the result
/// is the low byte of `(t<<4)|remainder`, matching the C's `unsigned char`.
///
/// Note: upstream's `bin2bcd` macro selects `const_bin2bcd` (exact
/// `(x/10)<<4 + x%10`) for compile-time-constant arguments; the two
/// formulas diverge for `val >= 1024` (e.g. C literal `bin2bcd(1024)`
/// yields 100 via the const path, `_bin2bcd` yields 250). This port is the
/// `_bin2bcd` function — the verified unit — so it always uses the
/// multiply-shift variant. Valid BCD inputs are 0..=99, where they agree.
pub const fn bin2bcd(val: u32) -> u8 {
    let t = val.wrapping_mul(103) >> 10;
    ((t << 4) | val.wrapping_sub(t * 10)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd_roundtrip() {
        // Values observed from the recorded harness.
        assert_eq!(bcd2bin(0x45), 45);
        assert_eq!(bcd2bin(0x99), 99);
        assert_eq!(bcd2bin(0x00), 0);
        assert_eq!(bin2bcd(45), 0x45);
        assert_eq!(bin2bcd(99), 0x99);
        assert_eq!(bin2bcd(0), 0x00);
        for v in 0u32..100 {
            assert_eq!(bcd2bin(bin2bcd(v)), v);
        }
    }
}
