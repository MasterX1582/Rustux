//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_llist_seq(uint64_t seed, int64_t npool_i, int64_t nops_i)` (`wrappers.c:141`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_llist_seq(seed: u64, npool_i: i64, nops_i: i64) -> u64 {
    port::wrap_llist_seq(seed, npool_i, nops_i)
}

/// `wrap_llist_seq` as `rust_anything_wrap_llist_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_llist_seq(seed: u64, npool_i: i64, nops_i: i64) -> u64 {
    port::wrap_llist_seq(seed, npool_i, nops_i)
}

/// `uint64_t wrap_llist_reverse(uint64_t seed, int64_t n_i)` (`wrappers.c:308`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_llist_reverse(seed: u64, n_i: i64) -> u64 {
    port::wrap_llist_reverse(seed, n_i)
}

/// `wrap_llist_reverse` as `rust_anything_wrap_llist_reverse`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_llist_reverse(seed: u64, n_i: i64) -> u64 {
    port::wrap_llist_reverse(seed, n_i)
}

/// `uint64_t wrap_llist_batch(uint64_t seed, int64_t k_i)` (`wrappers.c:345`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_llist_batch(seed: u64, k_i: i64) -> u64 {
    port::wrap_llist_batch(seed, k_i)
}

/// `wrap_llist_batch` as `rust_anything_wrap_llist_batch`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_llist_batch(seed: u64, k_i: i64) -> u64 {
    port::wrap_llist_batch(seed, k_i)
}
