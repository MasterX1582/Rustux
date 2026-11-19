//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_genradix_run(uint64_t seed, int64_t nops_i, int64_t variant_i, int64_t budget_i)` (`wrappers.c:304`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_genradix_run(seed: u64, nops_i: i64, variant_i: i64, budget_i: i64) -> u64 {
    port::wrap_genradix_run(seed, nops_i, variant_i, budget_i)
}

/// `wrap_genradix_run` as `rust_anything_wrap_genradix_run`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_genradix_run(
    seed: u64,
    nops_i: i64,
    variant_i: i64,
    budget_i: i64,
) -> u64 {
    port::wrap_genradix_run(seed, nops_i, variant_i, budget_i)
}

/// `uint64_t wrap_genradix_seq(uint64_t seed, int64_t n_i, int64_t variant_i)` (`wrappers.c:330`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_genradix_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    port::wrap_genradix_seq(seed, n_i, variant_i)
}

/// `wrap_genradix_seq` as `rust_anything_wrap_genradix_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_genradix_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    port::wrap_genradix_seq(seed, n_i, variant_i)
}

/// `uint64_t wrap_interval_run(uint64_t seed, int64_t nops_i, int64_t variant_i)` (`wrappers.c:499`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_interval_run(seed: u64, nops_i: i64, variant_i: i64) -> u64 {
    port::wrap_interval_run(seed, nops_i, variant_i)
}

/// `wrap_interval_run` as `rust_anything_wrap_interval_run`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_interval_run(seed: u64, nops_i: i64, variant_i: i64) -> u64 {
    port::wrap_interval_run(seed, nops_i, variant_i)
}

/// `uint64_t wrap_interval_seq(uint64_t seed, int64_t n_i, int64_t variant_i)` (`wrappers.c:569`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_interval_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    port::wrap_interval_seq(seed, n_i, variant_i)
}

/// `wrap_interval_seq` as `rust_anything_wrap_interval_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_interval_seq(seed: u64, n_i: i64, variant_i: i64) -> u64 {
    port::wrap_interval_seq(seed, n_i, variant_i)
}
