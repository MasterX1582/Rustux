//! C ABI for `int_log.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned int intlog2(uint32_t value)` (`int_log.c:51`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn intlog2(value: u32) -> std::ffi::c_uint {
    port::intlog2(value)
}

/// `intlog2` as `rust_anything_intlog2`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_intlog2(value: u32) -> std::ffi::c_uint {
    port::intlog2(value)
}

/// `unsigned int intlog10(uint32_t value)` (`int_log.c:111`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn intlog10(value: u32) -> std::ffi::c_uint {
    port::intlog10(value)
}

/// `intlog10` as `rust_anything_intlog10`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_intlog10(value: u32) -> std::ffi::c_uint {
    port::intlog10(value)
}
