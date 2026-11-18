//! C ABI for `gcd.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned long gcd(unsigned long a, unsigned long b)` (`gcd.c:50`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn gcd(a: std::ffi::c_ulong, b: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::gcd(a, b)
}

/// `gcd` as `rust_anything_gcd`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_gcd(
    a: std::ffi::c_ulong,
    b: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::gcd(a, b)
}
