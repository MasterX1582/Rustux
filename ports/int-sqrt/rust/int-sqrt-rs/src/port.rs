//! The Rust port of `int_sqrt.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

/// Port of `unsigned long int_sqrt(unsigned long x)` (`int_sqrt.c:20`).
///
/// Guy Steele's shift-and-subtract integer square root: floor(sqrt(x)).
/// `63 - leading_zeros` is `__fls`; the seed mask is the highest even power
/// of four not above `x`, exactly as the C code computes it.
pub fn int_sqrt(x: std::ffi::c_ulong) -> std::ffi::c_ulong {
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
        for (x, want) in [
            (0, 0),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (15, 3),
            (16, 4),
            (17, 4),
        ] {
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
        assert_eq!(
            int_sqrt(u64::MAX as std::ffi::c_ulong),
            u32::MAX as u64 as std::ffi::c_ulong
        );
    }
}
