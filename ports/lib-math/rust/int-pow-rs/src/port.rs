//! The Rust port of `int_pow.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

/// Port of `uint64_t int_pow(uint64_t base, unsigned int exp)` (`int_pow.c:19`).
pub fn int_pow(base: u64, exp: std::ffi::c_uint) -> u64 {
    let mut result: u64 = 1;
    let mut base = base;
    let mut exp = exp;

    while exp != 0 {
        if exp & 1 != 0 {
            result = result.wrapping_mul(base);
        }
        exp >>= 1;
        base = base.wrapping_mul(base);
    }

    result
}
