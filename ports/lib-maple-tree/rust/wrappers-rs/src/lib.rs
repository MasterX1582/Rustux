//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_mt_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_mt_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_seq(seed, nops_i, seed2)
}

/// `wrap_mt_seq` as `rust_anything_wrap_mt_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_mt_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_seq(seed, nops_i, seed2)
}

/// `uint64_t wrap_mt_area(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_mt_area(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_area(seed, nops_i, seed2)
}

/// `wrap_mt_area` as `rust_anything_wrap_mt_area`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_mt_area(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_area(seed, nops_i, seed2)
}

/// `uint64_t wrap_mt_iter(uint64_t seed, int64_t n_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_mt_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_iter(seed, n_i, seed2)
}

/// `wrap_mt_iter` as `rust_anything_wrap_mt_iter`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_mt_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_mt_iter(seed, n_i, seed2)
}
