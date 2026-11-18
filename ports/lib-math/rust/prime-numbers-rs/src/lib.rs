//! C ABI for `prime_numbers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `bool slow_is_prime_number(unsigned long x)` (`prime_numbers.c:88`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn slow_is_prime_number(x: std::ffi::c_ulong) -> bool {
    port::slow_is_prime_number(x)
}

/// `slow_is_prime_number` as `rust_anything_slow_is_prime_number`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_slow_is_prime_number(x: std::ffi::c_ulong) -> bool {
    port::slow_is_prime_number(x)
}

/// `unsigned long next_prime_number(unsigned long x)` (`prime_numbers.c:207`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn next_prime_number(x: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::next_prime_number(x)
}

/// `next_prime_number` as `rust_anything_next_prime_number`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_next_prime_number(x: std::ffi::c_ulong) -> std::ffi::c_ulong {
    port::next_prime_number(x)
}

/// `bool is_prime_number(unsigned long x)` (`prime_numbers.c:240`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn is_prime_number(x: std::ffi::c_ulong) -> bool {
    port::is_prime_number(x)
}

/// `is_prime_number` as `rust_anything_is_prime_number`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_is_prime_number(x: std::ffi::c_ulong) -> bool {
    port::is_prime_number(x)
}
