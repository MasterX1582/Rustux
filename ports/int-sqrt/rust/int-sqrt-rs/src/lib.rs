//! C ABI for `int_sqrt.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned long int_sqrt(unsigned long x)` (`int_sqrt.c:20`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn int_sqrt(x: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::int_sqrt(x)
}

/// `int_sqrt` as `rust_anything_int_sqrt`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_int_sqrt(x: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::int_sqrt(x)
}
