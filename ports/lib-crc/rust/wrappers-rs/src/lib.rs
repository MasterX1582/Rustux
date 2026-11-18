//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_crc32_le(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:59`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc32_le(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32_le(crc, seed, len)
}

/// `wrap_crc32_le` as `rust_anything_wrap_crc32_le`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc32_le(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32_le(crc, seed, len)
}

/// `uint64_t wrap_crc32_be(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:68`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc32_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32_be(crc, seed, len)
}

/// `wrap_crc32_be` as `rust_anything_wrap_crc32_be`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc32_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32_be(crc, seed, len)
}

/// `uint64_t wrap_crc32c(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:77`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc32c(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32c(crc, seed, len)
}

/// `wrap_crc32c` as `rust_anything_wrap_crc32c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc32c(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc32c(crc, seed, len)
}

/// `uint64_t wrap_crc64_be(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:86`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc64_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc64_be(crc, seed, len)
}

/// `wrap_crc64_be` as `rust_anything_wrap_crc64_be`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc64_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc64_be(crc, seed, len)
}

/// `uint64_t wrap_crc64_nvme(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:95`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc64_nvme(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc64_nvme(crc, seed, len)
}

/// `wrap_crc64_nvme` as `rust_anything_wrap_crc64_nvme`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc64_nvme(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc64_nvme(crc, seed, len)
}

/// `uint64_t wrap_crc16(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:104`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc16(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc16(crc, seed, len)
}

/// `wrap_crc16` as `rust_anything_wrap_crc16`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc16(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc16(crc, seed, len)
}

/// `uint64_t wrap_crc_ccitt(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:113`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc_ccitt(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc_ccitt(crc, seed, len)
}

/// `wrap_crc_ccitt` as `rust_anything_wrap_crc_ccitt`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc_ccitt(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc_ccitt(crc, seed, len)
}

/// `uint64_t wrap_crc_itu_t(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:122`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc_itu_t(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc_itu_t(crc, seed, len)
}

/// `wrap_crc_itu_t` as `rust_anything_wrap_crc_itu_t`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc_itu_t(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc_itu_t(crc, seed, len)
}

/// `uint64_t wrap_crc7_be(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:131`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc7_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc7_be(crc, seed, len)
}

/// `wrap_crc7_be` as `rust_anything_wrap_crc7_be`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc7_be(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc7_be(crc, seed, len)
}

/// `uint64_t wrap_t10dif(uint64_t crc, uint64_t seed, int64_t len)` (`wrappers.c:140`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_t10dif(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_t10dif(crc, seed, len)
}

/// `wrap_t10dif` as `rust_anything_wrap_t10dif`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_t10dif(crc: u64, seed: u64, len: i64) -> u64 {
    port::wrap_t10dif(crc, seed, len)
}

/// `uint64_t wrap_crc8(uint64_t poly, uint64_t seed, int64_t len)` (`wrappers.c:151`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_crc8(poly: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc8(poly, seed, len)
}

/// `wrap_crc8` as `rust_anything_wrap_crc8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_crc8(poly: u64, seed: u64, len: i64) -> u64 {
    port::wrap_crc8(poly, seed, len)
}
