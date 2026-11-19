//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_xa_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:647`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xa_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_seq(seed, nops_i, seed2)
}

/// `wrap_xa_seq` as `rust_anything_wrap_xa_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xa_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_seq(seed, nops_i, seed2)
}

/// `uint64_t wrap_xa_multi(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:903`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xa_multi(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_multi(seed, nops_i, seed2)
}

/// `wrap_xa_multi` as `rust_anything_wrap_xa_multi`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xa_multi(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_multi(seed, nops_i, seed2)
}

/// `uint64_t wrap_xa_alloc(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:968`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xa_alloc(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_alloc(seed, nops_i, seed2)
}

/// `wrap_xa_alloc` as `rust_anything_wrap_xa_alloc`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xa_alloc(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_alloc(seed, nops_i, seed2)
}

/// `uint64_t wrap_xa_iter(uint64_t seed, int64_t n_i, int64_t seed2)` (`wrappers.c:1037`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_xa_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_iter(seed, n_i, seed2)
}

/// `wrap_xa_iter` as `rust_anything_wrap_xa_iter`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_xa_iter(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_xa_iter(seed, n_i, seed2)
}
