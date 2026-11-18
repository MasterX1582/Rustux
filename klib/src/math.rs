//! Integer math — ports of `lib/math/` from the C kernel.
//!
//! Verified against the original's recorded behavior in
//! `ports/int-sqrt/rust/int-sqrt-rs` (17/17 cases, gate passed).

/// Port of `lib/math/int_sqrt.c`: `unsigned long int_sqrt(unsigned long x)`.
///
/// Guy Steele's shift-and-subtract integer square root: floor(sqrt(x)).
pub fn int_sqrt(x: u64) -> u64 {
    let mut x = x;
    if x <= 1 {
        return x;
    }

    let mut m: u64 = 1 << ((63 - x.leading_zeros()) & !1);
    let mut y: u64 = 0;
    while m != 0 {
        let b = y + m;
        y >>= 1;
        if x >= b {
            x -= b;
            y += m;
        }
        m >>= 2;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values() {
        for (x, want) in [(0, 0), (1, 1), (2, 1), (3, 1), (4, 2), (15, 3), (16, 4), (17, 4)] {
            assert_eq!(int_sqrt(x), want, "int_sqrt({x})");
        }
    }

    #[test]
    fn perfect_squares_and_neighbors() {
        for r in [1u64, 3, 255, 1000, 65535, u32::MAX as u64] {
            let sq = r * r;
            assert_eq!(int_sqrt(sq), r);
            assert_eq!(int_sqrt(sq - 1), r - 1);
            if sq < u64::MAX {
                assert_eq!(int_sqrt(sq + 1), r);
            }
        }
    }

    #[test]
    fn extremes() {
        assert_eq!(int_sqrt(u64::MAX), u32::MAX as u64);
    }
}
