//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_sha1(uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:153`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha1(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha1(seed, len, mode)
}

/// `wrap_sha1` as `rust_anything_wrap_sha1`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha1(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha1(seed, len, mode)
}

/// `uint64_t wrap_sha224(uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:162`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha224(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha224(seed, len, mode)
}

/// `wrap_sha224` as `rust_anything_wrap_sha224`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha224(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha224(seed, len, mode)
}

/// `uint64_t wrap_sha256(uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:171`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha256(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha256(seed, len, mode)
}

/// `wrap_sha256` as `rust_anything_wrap_sha256`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha256(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha256(seed, len, mode)
}

/// `uint64_t wrap_sha384(uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:180`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha384(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha384(seed, len, mode)
}

/// `wrap_sha384` as `rust_anything_wrap_sha384`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha384(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha384(seed, len, mode)
}

/// `uint64_t wrap_sha512(uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:189`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha512(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha512(seed, len, mode)
}

/// `wrap_sha512` as `rust_anything_wrap_sha512`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha512(seed: u64, len: i64, mode: i64) -> u64 {
    port::wrap_sha512(seed, len, mode)
}

/// `uint64_t wrap_sha1_stream(uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:204`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha1_stream(seed: u64, len: i64, chunk: i64, mode: i64) -> u64 {
    port::wrap_sha1_stream(seed, len, chunk, mode)
}

/// `wrap_sha1_stream` as `rust_anything_wrap_sha1_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha1_stream(
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha1_stream(seed, len, chunk, mode)
}

/// `uint64_t wrap_sha224_stream(uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:227`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha224_stream(seed: u64, len: i64, chunk: i64, mode: i64) -> u64 {
    port::wrap_sha224_stream(seed, len, chunk, mode)
}

/// `wrap_sha224_stream` as `rust_anything_wrap_sha224_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha224_stream(
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha224_stream(seed, len, chunk, mode)
}

/// `uint64_t wrap_sha256_stream(uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:250`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha256_stream(seed: u64, len: i64, chunk: i64, mode: i64) -> u64 {
    port::wrap_sha256_stream(seed, len, chunk, mode)
}

/// `wrap_sha256_stream` as `rust_anything_wrap_sha256_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha256_stream(
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha256_stream(seed, len, chunk, mode)
}

/// `uint64_t wrap_sha384_stream(uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:276`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha384_stream(seed: u64, len: i64, chunk: i64, mode: i64) -> u64 {
    port::wrap_sha384_stream(seed, len, chunk, mode)
}

/// `wrap_sha384_stream` as `rust_anything_wrap_sha384_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha384_stream(
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha384_stream(seed, len, chunk, mode)
}

/// `uint64_t wrap_sha512_stream(uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:299`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha512_stream(seed: u64, len: i64, chunk: i64, mode: i64) -> u64 {
    port::wrap_sha512_stream(seed, len, chunk, mode)
}

/// `wrap_sha512_stream` as `rust_anything_wrap_sha512_stream`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha512_stream(
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha512_stream(seed, len, chunk, mode)
}

/// `uint64_t wrap_sha256_finup_2x(uint64_t s1, uint64_t s2, int64_t len, int64_t pre, int64_t mode)` (`wrappers.c:331`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sha256_finup_2x(s1: u64, s2: u64, len: i64, pre: i64, mode: i64) -> u64 {
    port::wrap_sha256_finup_2x(s1, s2, len, pre, mode)
}

/// `wrap_sha256_finup_2x` as `rust_anything_wrap_sha256_finup_2x`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sha256_finup_2x(
    s1: u64,
    s2: u64,
    len: i64,
    pre: i64,
    mode: i64,
) -> u64 {
    port::wrap_sha256_finup_2x(s1, s2, len, pre, mode)
}

/// `uint64_t wrap_hmac_sha1_prepared(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:355`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha1_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha1_prepared` as `rust_anything_wrap_hmac_sha1_prepared`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha1_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha224_prepared(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:382`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha224_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha224_prepared` as `rust_anything_wrap_hmac_sha224_prepared`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha224_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha256_prepared(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:408`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha256_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha256_prepared` as `rust_anything_wrap_hmac_sha256_prepared`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha256_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha384_prepared(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:434`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha384_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha384_prepared` as `rust_anything_wrap_hmac_sha384_prepared`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha384_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha512_prepared(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:460`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha512_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha512_prepared` as `rust_anything_wrap_hmac_sha512_prepared`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha512_prepared(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_prepared(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha1_rawkey(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:488`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha1_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha1_rawkey` as `rust_anything_wrap_hmac_sha1_rawkey`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha1_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha224_rawkey(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:512`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha224_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha224_rawkey` as `rust_anything_wrap_hmac_sha224_rawkey`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha224_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha256_rawkey(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:536`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha256_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha256_rawkey` as `rust_anything_wrap_hmac_sha256_rawkey`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha256_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha384_rawkey(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:560`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha384_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha384_rawkey` as `rust_anything_wrap_hmac_sha384_rawkey`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha384_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha512_rawkey(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t chunk, int64_t mode)` (`wrappers.c:584`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha512_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `wrap_hmac_sha512_rawkey` as `rust_anything_wrap_hmac_sha512_rawkey`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha512_rawkey(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    chunk: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_rawkey(kseed, klen, seed, len, chunk, mode)
}

/// `uint64_t wrap_hmac_sha1_oneshot(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:610`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha1_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_oneshot(kseed, klen, seed, len, mode)
}

/// `wrap_hmac_sha1_oneshot` as `rust_anything_wrap_hmac_sha1_oneshot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha1_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha1_oneshot(kseed, klen, seed, len, mode)
}

/// `uint64_t wrap_hmac_sha224_oneshot(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:628`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha224_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_oneshot(kseed, klen, seed, len, mode)
}

/// `wrap_hmac_sha224_oneshot` as `rust_anything_wrap_hmac_sha224_oneshot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha224_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha224_oneshot(kseed, klen, seed, len, mode)
}

/// `uint64_t wrap_hmac_sha256_oneshot(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:646`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha256_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_oneshot(kseed, klen, seed, len, mode)
}

/// `wrap_hmac_sha256_oneshot` as `rust_anything_wrap_hmac_sha256_oneshot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha256_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha256_oneshot(kseed, klen, seed, len, mode)
}

/// `uint64_t wrap_hmac_sha384_oneshot(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:664`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha384_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_oneshot(kseed, klen, seed, len, mode)
}

/// `wrap_hmac_sha384_oneshot` as `rust_anything_wrap_hmac_sha384_oneshot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha384_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha384_oneshot(kseed, klen, seed, len, mode)
}

/// `uint64_t wrap_hmac_sha512_oneshot(uint64_t kseed, int64_t klen, uint64_t seed, int64_t len, int64_t mode)` (`wrappers.c:682`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hmac_sha512_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_oneshot(kseed, klen, seed, len, mode)
}

/// `wrap_hmac_sha512_oneshot` as `rust_anything_wrap_hmac_sha512_oneshot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hmac_sha512_oneshot(
    kseed: u64,
    klen: i64,
    seed: u64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_hmac_sha512_oneshot(kseed, klen, seed, len, mode)
}

/// `uint64_t wrap_zero_message_hashes(uint64_t mode)` (`wrappers.c:743`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_zero_message_hashes(mode: u64) -> u64 {
    port::wrap_zero_message_hashes(mode)
}

/// `wrap_zero_message_hashes` as `rust_anything_wrap_zero_message_hashes`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_zero_message_hashes(mode: u64) -> u64 {
    port::wrap_zero_message_hashes(mode)
}
