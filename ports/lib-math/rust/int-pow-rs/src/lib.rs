//! C ABI for `int_pow.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t int_pow(uint64_t base, unsigned int exp)` (`int_pow.c:19`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn int_pow(base: u64, exp: std::ffi::c_uint) -> u64 {
    port::int_pow(base, exp)
}

/// `int_pow` as `rust_anything_int_pow`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_int_pow(base: u64, exp: std::ffi::c_uint) -> u64 {
    port::int_pow(base, exp)
}
