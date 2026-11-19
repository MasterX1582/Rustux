//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `void xa_dump_index(unsigned long index, unsigned int shift)` (`wrappers.c:352`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn xa_dump_index(index: std::ffi::c_ulong, shift: std::ffi::c_uint) {
    port::xa_dump_index(index, shift)
}

/// `xa_dump_index` as `rust_anything_xa_dump_index`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_xa_dump_index(index: std::ffi::c_ulong, shift: std::ffi::c_uint) {
    port::xa_dump_index(index, shift)
}

/// `uint64_t wrap_idr_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:715`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_idr_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_idr_seq(seed, nops_i, seed2)
}

/// `wrap_idr_seq` as `rust_anything_wrap_idr_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_idr_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_idr_seq(seed, nops_i, seed2)
}

/// `uint64_t wrap_ida_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:866`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_ida_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_ida_seq(seed, nops_i, seed2)
}

/// `wrap_ida_seq` as `rust_anything_wrap_ida_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_ida_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_ida_seq(seed, nops_i, seed2)
}

/// `uint64_t wrap_radix_seq(uint64_t seed, int64_t nops_i, int64_t seed2)` (`wrappers.c:1193`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_radix_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_radix_seq(seed, nops_i, seed2)
}

/// `wrap_radix_seq` as `rust_anything_wrap_radix_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_radix_seq(seed: u64, nops_i: i64, seed2: i64) -> u64 {
    port::wrap_radix_seq(seed, nops_i, seed2)
}
