//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_rbtree_run(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:231`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_run(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_rbtree_run(seed, nops_i, seed2)
}

/// `wrap_rbtree_run` as `rust_anything_wrap_rbtree_run`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_run(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_rbtree_run(seed, nops_i, seed2)
}

/// `uint64_t wrap_rbtree_insert_seq(uint64_t seed, int64_t n_i)` (`wrappers.c:373`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_insert_seq(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_insert_seq(seed, n_i)
}

/// `wrap_rbtree_insert_seq` as `rust_anything_wrap_rbtree_insert_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_insert_seq(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_insert_seq(seed, n_i)
}

/// `uint64_t wrap_rbtree_erase_all(uint64_t seed, int64_t n_i)` (`wrappers.c:394`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_erase_all(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_erase_all(seed, n_i)
}

/// `wrap_rbtree_erase_all` as `rust_anything_wrap_rbtree_erase_all`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_erase_all(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_erase_all(seed, n_i)
}

/// `uint64_t wrap_rbtree_find(uint64_t seed, int64_t n_i, int64_t seed2)` (`wrappers.c:439`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_find(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_rbtree_find(seed, n_i, seed2)
}

/// `wrap_rbtree_find` as `rust_anything_wrap_rbtree_find`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_find(seed: u64, n_i: i64, seed2: i64) -> u64 {
    port::wrap_rbtree_find(seed, n_i, seed2)
}

/// `uint64_t wrap_rbtree_cached(uint64_t seed, int64_t nops_i)` (`wrappers.c:496`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_cached(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_cached(seed, nops_i)
}

/// `wrap_rbtree_cached` as `rust_anything_wrap_rbtree_cached`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_cached(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_cached(seed, nops_i)
}

/// `uint64_t wrap_rbtree_linked(uint64_t seed, int64_t nops_i)` (`wrappers.c:608`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_linked(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_linked(seed, nops_i)
}

/// `wrap_rbtree_linked` as `rust_anything_wrap_rbtree_linked`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_linked(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_linked(seed, nops_i)
}

/// `uint64_t wrap_rbtree_augmented(uint64_t seed, int64_t nops_i)` (`wrappers.c:743`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_augmented(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_augmented(seed, nops_i)
}

/// `wrap_rbtree_augmented` as `rust_anything_wrap_rbtree_augmented`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_augmented(seed: u64, nops_i: i64) -> u64 {
    port::wrap_rbtree_augmented(seed, nops_i)
}

/// `uint64_t wrap_rbtree_postorder(uint64_t seed, int64_t n_i)` (`wrappers.c:806`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rbtree_postorder(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_postorder(seed, n_i)
}

/// `wrap_rbtree_postorder` as `rust_anything_wrap_rbtree_postorder`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rbtree_postorder(seed: u64, n_i: i64) -> u64 {
    port::wrap_rbtree_postorder(seed, n_i)
}

/// `uint64_t wrap_rb_replace(uint64_t seed, int64_t n_i, int64_t k_i)` (`wrappers.c:833`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rb_replace(seed: u64, n_i: i64, k_i: i64) -> u64 {
    port::wrap_rb_replace(seed, n_i, k_i)
}

/// `wrap_rb_replace` as `rust_anything_wrap_rb_replace`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rb_replace(seed: u64, n_i: i64, k_i: i64) -> u64 {
    port::wrap_rb_replace(seed, n_i, k_i)
}
