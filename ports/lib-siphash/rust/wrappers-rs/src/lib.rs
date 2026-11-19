//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_siphash(uint64_t k0, uint64_t k1, int64_t off, uint64_t seed, int64_t len)` (`wrappers.c:60`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    port::wrap_siphash(k0, k1, off, seed, len)
}

/// `wrap_siphash` as `rust_anything_wrap_siphash`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash(
    k0: u64,
    k1: u64,
    off: i64,
    seed: u64,
    len: i64,
) -> u64 {
    port::wrap_siphash(k0, k1, off, seed, len)
}

/// `uint64_t wrap_siphash_ramp(uint64_t k0, uint64_t k1, int64_t off, int64_t len)` (`wrappers.c:72`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    port::wrap_siphash_ramp(k0, k1, off, len)
}

/// `wrap_siphash_ramp` as `rust_anything_wrap_siphash_ramp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    port::wrap_siphash_ramp(k0, k1, off, len)
}

/// `uint64_t wrap_siphash_1u64(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:81`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_1u64(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_1u64(a, k0, k1)
}

/// `wrap_siphash_1u64` as `rust_anything_wrap_siphash_1u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_1u64(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_1u64(a, k0, k1)
}

/// `uint64_t wrap_siphash_2u64(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:87`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_2u64(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_2u64(a, b, k0, k1)
}

/// `wrap_siphash_2u64` as `rust_anything_wrap_siphash_2u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_2u64(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_2u64(a, b, k0, k1)
}

/// `uint64_t wrap_siphash_3u64(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:93`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_3u64(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_3u64(a, b, c, k0, k1)
}

/// `wrap_siphash_3u64` as `rust_anything_wrap_siphash_3u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_3u64(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_3u64(a, b, c, k0, k1)
}

/// `uint64_t wrap_siphash_4u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:100`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_4u64(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_4u64(a, b, c, d, k0, k1)
}

/// `wrap_siphash_4u64` as `rust_anything_wrap_siphash_4u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_4u64(
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    k0: u64,
    k1: u64,
) -> u64 {
    port::wrap_siphash_4u64(a, b, c, d, k0, k1)
}

/// `uint64_t wrap_siphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:107`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_1u32(a, k0, k1)
}

/// `wrap_siphash_1u32` as `rust_anything_wrap_siphash_1u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_1u32(a, k0, k1)
}

/// `uint64_t wrap_siphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:113`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_2u32(a, b, k0, k1)
}

/// `wrap_siphash_2u32` as `rust_anything_wrap_siphash_2u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_2u32(a, b, k0, k1)
}

/// `uint64_t wrap_siphash_3u32(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:119`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_3u32(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_3u32(a, b, c, k0, k1)
}

/// `wrap_siphash_3u32` as `rust_anything_wrap_siphash_3u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_3u32(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_3u32(a, b, c, k0, k1)
}

/// `uint64_t wrap_siphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:126`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_siphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_siphash_4u32(a, b, c, d, k0, k1)
}

/// `wrap_siphash_4u32` as `rust_anything_wrap_siphash_4u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_siphash_4u32(
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    k0: u64,
    k1: u64,
) -> u64 {
    port::wrap_siphash_4u32(a, b, c, d, k0, k1)
}

/// `uint64_t wrap_hsiphash(uint64_t k0, uint64_t k1, int64_t off, uint64_t seed, int64_t len)` (`wrappers.c:135`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    port::wrap_hsiphash(k0, k1, off, seed, len)
}

/// `wrap_hsiphash` as `rust_anything_wrap_hsiphash`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash(
    k0: u64,
    k1: u64,
    off: i64,
    seed: u64,
    len: i64,
) -> u64 {
    port::wrap_hsiphash(k0, k1, off, seed, len)
}

/// `uint64_t wrap_hsiphash_ramp(uint64_t k0, uint64_t k1, int64_t off, int64_t len)` (`wrappers.c:144`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    port::wrap_hsiphash_ramp(k0, k1, off, len)
}

/// `wrap_hsiphash_ramp` as `rust_anything_wrap_hsiphash_ramp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    port::wrap_hsiphash_ramp(k0, k1, off, len)
}

/// `uint64_t wrap_hsiphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:153`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_1u32(a, k0, k1)
}

/// `wrap_hsiphash_1u32` as `rust_anything_wrap_hsiphash_1u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_1u32(a, k0, k1)
}

/// `uint64_t wrap_hsiphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:159`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_2u32(a, b, k0, k1)
}

/// `wrap_hsiphash_2u32` as `rust_anything_wrap_hsiphash_2u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_2u32(a, b, k0, k1)
}

/// `uint64_t wrap_hsiphash_3u32(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:165`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash_3u32(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_3u32(a, b, c, k0, k1)
}

/// `wrap_hsiphash_3u32` as `rust_anything_wrap_hsiphash_3u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash_3u32(
    a: u64,
    b: u64,
    c: u64,
    k0: u64,
    k1: u64,
) -> u64 {
    port::wrap_hsiphash_3u32(a, b, c, k0, k1)
}

/// `uint64_t wrap_hsiphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:172`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hsiphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    port::wrap_hsiphash_4u32(a, b, c, d, k0, k1)
}

/// `wrap_hsiphash_4u32` as `rust_anything_wrap_hsiphash_4u32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hsiphash_4u32(
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    k0: u64,
    k1: u64,
) -> u64 {
    port::wrap_hsiphash_4u32(a, b, c, d, k0, k1)
}
