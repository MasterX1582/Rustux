//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_kfifo_seq(uint64_t seed, int64_t sp_i, int64_t esize_i, int64_t recsize_i, int64_t nops_i, int64_t mode_i)` (`wrappers.c:239`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kfifo_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_seq(seed, sp_i, esize_i, recsize_i, nops_i, mode_i)
}

/// `wrap_kfifo_seq` as `rust_anything_wrap_kfifo_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kfifo_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_seq(seed, sp_i, esize_i, recsize_i, nops_i, mode_i)
}

/// `uint64_t wrap_kfifo_dma_seq(uint64_t seed, int64_t sp_i, int64_t esize_i, int64_t recsize_i, int64_t nops_i, int64_t mode_i)` (`wrappers.c:384`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kfifo_dma_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_dma_seq(seed, sp_i, esize_i, recsize_i, nops_i, mode_i)
}

/// `wrap_kfifo_dma_seq` as `rust_anything_wrap_kfifo_dma_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kfifo_dma_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_dma_seq(seed, sp_i, esize_i, recsize_i, nops_i, mode_i)
}

/// `uint64_t wrap_kfifo_alloc(uint64_t seed, int64_t size_i, int64_t esize_i, int64_t nops_i, int64_t mode_i)` (`wrappers.c:419`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kfifo_alloc(
    seed: u64,
    size_i: i64,
    esize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_alloc(seed, size_i, esize_i, nops_i, mode_i)
}

/// `wrap_kfifo_alloc` as `rust_anything_wrap_kfifo_alloc`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kfifo_alloc(
    seed: u64,
    size_i: i64,
    esize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_alloc(seed, size_i, esize_i, nops_i, mode_i)
}

/// `uint64_t wrap_kfifo_typed(uint64_t seed, int64_t sp_i, int64_t nops_i, int64_t mode_i)` (`wrappers.c:556`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kfifo_typed(seed: u64, sp_i: i64, nops_i: i64, mode_i: i64) -> u64 {
    port::wrap_kfifo_typed(seed, sp_i, nops_i, mode_i)
}

/// `wrap_kfifo_typed` as `rust_anything_wrap_kfifo_typed`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kfifo_typed(
    seed: u64,
    sp_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    port::wrap_kfifo_typed(seed, sp_i, nops_i, mode_i)
}
