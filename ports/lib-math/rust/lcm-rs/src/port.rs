//! The Rust port of `lcm.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

use std::ffi::c_ulong;

/// The C `lcm.c` links against `gcd` from `gcd.c`; the harness resolves it from
/// the same shared object. This is the same port as `gcd-rs` (binary_gcd path).
fn gcd(a: c_ulong, b: c_ulong) -> c_ulong {
    let r: c_ulong = a | b;

    if a == 0 || b == 0 {
        return r;
    }

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

/// Port of `unsigned long lcm(unsigned long a, unsigned long b)` (`lcm.c:8`).
pub fn lcm(a: c_ulong, b: c_ulong) -> c_ulong {
    if a != 0 && b != 0 {
        (a / gcd(a, b)).wrapping_mul(b)
    } else {
        0
    }
}

/// Port of `unsigned long lcm_not_zero(unsigned long a, unsigned long b)` (`lcm.c:17`).
pub fn lcm_not_zero(a: c_ulong, b: c_ulong) -> c_ulong {
    let l = lcm(a, b);

    if l != 0 {
        return l;
    }

    if b != 0 { b } else { a }
}
