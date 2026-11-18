//! The Rust port of `gcd.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

use std::ffi::c_ulong;

/// Port of `unsigned long gcd(unsigned long a, unsigned long b)` (`gcd.c:50`).
///
/// The harness builds with `static_branch_likely` always true, so the recorded
/// behavior is the `binary_gcd` (`__ffs`) path.
pub fn gcd(a: c_ulong, b: c_ulong) -> c_ulong {
    let r: c_ulong = a | b;

    if a == 0 || b == 0 {
        return r;
    }

    // binary_gcd()
    let mut a = a;
    let mut b = b >> b.trailing_zeros();
    if b == 1 {
        return r.isolate_lowest_one();
    }

    loop {
        a >>= a.trailing_zeros();
        if a == 1 {
            return r.isolate_lowest_one();
        }
        if a == b {
            return a << r.trailing_zeros();
        }

        if a < b {
            std::mem::swap(&mut a, &mut b);
        }
        a = a.wrapping_sub(b);
    }
}
