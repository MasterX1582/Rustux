//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned int num_possible_cpus(void)` (`wrappers.c:133`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn num_possible_cpus() -> std::ffi::c_uint {
    port::num_possible_cpus()
}

/// `num_possible_cpus` as `rust_anything_num_possible_cpus`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_num_possible_cpus() -> std::ffi::c_uint {
    port::num_possible_cpus()
}

/// `uint64_t wrap_rht_seq(uint64_t seed, int64_t nops_i, int64_t fail_i)` (`wrappers.c:517`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rht_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_seq(seed, nops_i, fail_i)
}

/// `wrap_rht_seq` as `rust_anything_wrap_rht_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rht_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_seq(seed, nops_i, fail_i)
}

/// `uint64_t wrap_rhlt_seq(uint64_t seed, int64_t nops_i, int64_t fail_i)` (`wrappers.c:738`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rhlt_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    port::wrap_rhlt_seq(seed, nops_i, fail_i)
}

/// `wrap_rhlt_seq` as `rust_anything_wrap_rhlt_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rhlt_seq(seed: u64, nops_i: i64, fail_i: i64) -> u64 {
    port::wrap_rhlt_seq(seed, nops_i, fail_i)
}

/// `uint64_t wrap_rht_walk(uint64_t seed, int64_t n_i, int64_t fail_i)` (`wrappers.c:845`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rht_walk(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_walk(seed, n_i, fail_i)
}

/// `wrap_rht_walk` as `rust_anything_wrap_rht_walk`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rht_walk(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_walk(seed, n_i, fail_i)
}

/// `uint64_t wrap_rht_elastic(uint64_t seed, int64_t n_i, int64_t fail_i)` (`wrappers.c:945`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_rht_elastic(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_elastic(seed, n_i, fail_i)
}

/// `wrap_rht_elastic` as `rust_anything_wrap_rht_elastic`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_rht_elastic(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_rht_elastic(seed, n_i, fail_i)
}

/// `uint64_t wrap_bkl(uint64_t seed, int64_t n_i, int64_t fail_i)` (`wrappers.c:1017`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bkl(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_bkl(seed, n_i, fail_i)
}

/// `wrap_bkl` as `rust_anything_wrap_bkl`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bkl(seed: u64, n_i: i64, fail_i: i64) -> u64 {
    port::wrap_bkl(seed, n_i, fail_i)
}
