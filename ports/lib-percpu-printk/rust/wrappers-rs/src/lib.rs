//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_pcpu_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_pcpu_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_pcpu_seq(seed, nops_i, seed2)
}

/// `wrap_pcpu_seq` as `rust_anything_wrap_pcpu_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_pcpu_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_pcpu_seq(seed, nops_i, seed2)
}

/// `uint64_t wrap_prb_ops(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_prb_ops(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_prb_ops(seed, nops_i, seed2)
}

/// `wrap_prb_ops` as `rust_anything_wrap_prb_ops`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_prb_ops(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_prb_ops(seed, nops_i, seed2)
}

/// `uint64_t wrap_prb_wrap(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_prb_wrap(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_prb_wrap(seed, nops_i, seed2)
}

/// `wrap_prb_wrap` as `rust_anything_wrap_prb_wrap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_prb_wrap(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_prb_wrap(seed, nops_i, seed2)
}
