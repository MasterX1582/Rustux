//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_siphash(uint64_t k0, uint64_t k1, int64_t off, uint64_t seed, int64_t len)` (`wrappers.c:55`).
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

/// `uint64_t wrap_siphash_1u64(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:67`).
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

/// `uint64_t wrap_siphash_2u64(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:73`).
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

/// `uint64_t wrap_siphash_3u64(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:79`).
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

/// `uint64_t wrap_siphash_4u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:86`).
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

/// `uint64_t wrap_siphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:93`).
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

/// `uint64_t wrap_siphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:99`).
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

/// `uint64_t wrap_siphash_3u32(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:105`).
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

/// `uint64_t wrap_siphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:112`).
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

/// `uint64_t wrap_hsiphash(uint64_t k0, uint64_t k1, int64_t off, uint64_t seed, int64_t len)` (`wrappers.c:121`).
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

/// `uint64_t wrap_hsiphash_1u32(uint64_t a, uint64_t k0, uint64_t k1)` (`wrappers.c:130`).
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

/// `uint64_t wrap_hsiphash_2u32(uint64_t a, uint64_t b, uint64_t k0, uint64_t k1)` (`wrappers.c:136`).
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

/// `uint64_t wrap_hsiphash_3u32(uint64_t a, uint64_t b, uint64_t c, uint64_t k0, uint64_t k1)` (`wrappers.c:142`).
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

/// `uint64_t wrap_hsiphash_4u32(uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t k0, uint64_t k1)` (`wrappers.c:149`).
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

/// `uint64_t wrap_xxh32(uint64_t seed, uint64_t bseed, int64_t len)` (`wrappers.c:158`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xxh32(seed: u64, bseed: u64, len: i64) -> u64 {
    port::wrap_xxh32(seed, bseed, len)
}

/// `wrap_xxh32` as `rust_anything_wrap_xxh32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xxh32(seed: u64, bseed: u64, len: i64) -> u64 {
    port::wrap_xxh32(seed, bseed, len)
}

/// `uint64_t wrap_xxh64(uint64_t seed, uint64_t bseed, int64_t len)` (`wrappers.c:165`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xxh64(seed: u64, bseed: u64, len: i64) -> u64 {
    port::wrap_xxh64(seed, bseed, len)
}

/// `wrap_xxh64` as `rust_anything_wrap_xxh64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xxh64(seed: u64, bseed: u64, len: i64) -> u64 {
    port::wrap_xxh64(seed, bseed, len)
}

/// `uint64_t wrap_xxh64_stream(uint64_t seed, uint64_t bseed, int64_t len, int64_t chunk)` (`wrappers.c:174`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xxh64_stream(seed: u64, bseed: u64, len: i64, chunk: i64) -> u64 {
    port::wrap_xxh64_stream(seed, bseed, len, chunk)
}

/// `wrap_xxh64_stream` as `rust_anything_wrap_xxh64_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xxh64_stream(
    seed: u64,
    bseed: u64,
    len: i64,
    chunk: i64,
) -> u64 {
    port::wrap_xxh64_stream(seed, bseed, len, chunk)
}

/// `uint64_t wrap_jhash(uint64_t initval, uint64_t seed, int64_t len)` (`wrappers.c:196`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_jhash(initval: u64, seed: u64, len: i64) -> u64 {
    port::wrap_jhash(initval, seed, len)
}

/// `wrap_jhash` as `rust_anything_wrap_jhash`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_jhash(initval: u64, seed: u64, len: i64) -> u64 {
    port::wrap_jhash(initval, seed, len)
}

/// `uint64_t wrap_jhash2(uint64_t initval, uint64_t seed, int64_t len)` (`wrappers.c:203`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_jhash2(initval: u64, seed: u64, len: i64) -> u64 {
    port::wrap_jhash2(initval, seed, len)
}

/// `wrap_jhash2` as `rust_anything_wrap_jhash2`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_jhash2(initval: u64, seed: u64, len: i64) -> u64 {
    port::wrap_jhash2(initval, seed, len)
}

/// `uint64_t wrap_jhash_1word(uint64_t a, uint64_t initval)` (`wrappers.c:210`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_jhash_1word(a: u64, initval: u64) -> u64 {
    port::wrap_jhash_1word(a, initval)
}

/// `wrap_jhash_1word` as `rust_anything_wrap_jhash_1word`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_jhash_1word(a: u64, initval: u64) -> u64 {
    port::wrap_jhash_1word(a, initval)
}

/// `uint64_t wrap_jhash_2words(uint64_t a, uint64_t b, uint64_t initval)` (`wrappers.c:215`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_jhash_2words(a: u64, b: u64, initval: u64) -> u64 {
    port::wrap_jhash_2words(a, b, initval)
}

/// `wrap_jhash_2words` as `rust_anything_wrap_jhash_2words`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_jhash_2words(a: u64, b: u64, initval: u64) -> u64 {
    port::wrap_jhash_2words(a, b, initval)
}

/// `uint64_t wrap_jhash_3words(uint64_t a, uint64_t b, uint64_t c, uint64_t initval)` (`wrappers.c:220`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_jhash_3words(a: u64, b: u64, c: u64, initval: u64) -> u64 {
    port::wrap_jhash_3words(a, b, c, initval)
}

/// `wrap_jhash_3words` as `rust_anything_wrap_jhash_3words`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_jhash_3words(a: u64, b: u64, c: u64, initval: u64) -> u64 {
    port::wrap_jhash_3words(a, b, c, initval)
}
