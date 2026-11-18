//! C ABI for `div64.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t mul_u64_add_u64_div_u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d)` (`div64.c:67`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn mul_u64_add_u64_div_u64(a: u64, b: u64, c: u64, d: u64) -> u64 {
    port::mul_u64_add_u64_div_u64(a, b, c, d)
}

/// `mul_u64_add_u64_div_u64` as `rust_anything_mul_u64_add_u64_div_u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_mul_u64_add_u64_div_u64(a: u64, b: u64, c: u64, d: u64) -> u64 {
    port::mul_u64_add_u64_div_u64(a, b, c, d)
}

/// `uint32_t iter_div_u64_rem(uint64_t dividend, uint32_t divisor, uint64_t *remainder)` (`div64.c:42`).
///
/// Manual addition (the bridge leaves pointer-arg exports to C): verified by
/// `ports/lib-math/check-iter-div.py` against the original `.so`.
///
/// # Safety
///
/// The caller must pass a valid, writable `*mut u64` for `remainder` — the
/// same contract the original `iter_div_u64_rem` has at the C ABI.
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
// SAFETY: the caller must pass a valid, writable `*mut u64` — the same
// contract the original `iter_div_u64_rem` has at the C ABI.
pub unsafe extern "C" fn iter_div_u64_rem(dividend: u64, divisor: u32, remainder: *mut u64) -> u32 {
    // SAFETY: C callers pass a valid `*mut u64`; a null pointer is a caller bug
    // exactly as in the original (which would write through it identically).
    unsafe { port::iter_div_u64_rem(dividend, divisor, &mut *remainder) }
}

/// `iter_div_u64_rem` as `rust_anything_iter_div_u64_rem`, for shadow builds beside the original.
///
/// # Safety
///
/// The caller must pass a valid, writable `*mut u64` for `remainder` — the
/// same contract the original `iter_div_u64_rem` has at the C ABI.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
// SAFETY: the caller must pass a valid, writable `*mut u64` — the same
// contract the original `iter_div_u64_rem` has at the C ABI.
pub unsafe extern "C" fn rust_anything_iter_div_u64_rem(
    dividend: u64,
    divisor: u32,
    remainder: *mut u64,
) -> u32 {
    // SAFETY: same contract as `iter_div_u64_rem`.
    unsafe { port::iter_div_u64_rem(dividend, divisor, &mut *remainder) }
}
