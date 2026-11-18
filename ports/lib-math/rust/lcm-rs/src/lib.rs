//! C ABI for `lcm.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned long lcm(unsigned long a, unsigned long b)` (`lcm.c:8`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn lcm(a: std::ffi::c_ulong, b: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::lcm(a, b)
}

/// `lcm` as `rust_anything_lcm`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_lcm(
    a: std::ffi::c_ulong,
    b: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::lcm(a, b)
}

/// `unsigned long lcm_not_zero(unsigned long a, unsigned long b)` (`lcm.c:17`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn lcm_not_zero(a: std::ffi::c_ulong, b: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::lcm_not_zero(a, b)
}

/// `lcm_not_zero` as `rust_anything_lcm_not_zero`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_lcm_not_zero(
    a: std::ffi::c_ulong,
    b: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::lcm_not_zero(a, b)
}
