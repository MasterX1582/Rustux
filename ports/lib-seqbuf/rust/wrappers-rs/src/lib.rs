//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_seqbuf_seq(uint64_t seed, int64_t bsz_i, int64_t nops_i)` (`wrappers.c:203`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_seqbuf_seq(seed: u64, bsz_i: i64, nops_i: i64) -> u64 {
    port::wrap_seqbuf_seq(seed, bsz_i, nops_i)
}

/// `wrap_seqbuf_seq` as `rust_anything_wrap_seqbuf_seq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_seqbuf_seq(seed: u64, bsz_i: i64, nops_i: i64) -> u64 {
    port::wrap_seqbuf_seq(seed, bsz_i, nops_i)
}

/// `uint64_t wrap_seqbuf_printf(uint64_t seed, int64_t bsz_i)` (`wrappers.c:360`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_seqbuf_printf(seed: u64, bsz_i: i64) -> u64 {
    port::wrap_seqbuf_printf(seed, bsz_i)
}

/// `wrap_seqbuf_printf` as `rust_anything_wrap_seqbuf_printf`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_seqbuf_printf(seed: u64, bsz_i: i64) -> u64 {
    port::wrap_seqbuf_printf(seed, bsz_i)
}

/// `uint64_t wrap_seqbuf_hexdump(uint64_t seed, int64_t bsz_i, int64_t n_i)` (`wrappers.c:387`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_seqbuf_hexdump(seed: u64, bsz_i: i64, n_i: i64) -> u64 {
    port::wrap_seqbuf_hexdump(seed, bsz_i, n_i)
}

/// `wrap_seqbuf_hexdump` as `rust_anything_wrap_seqbuf_hexdump`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_seqbuf_hexdump(seed: u64, bsz_i: i64, n_i: i64) -> u64 {
    port::wrap_seqbuf_hexdump(seed, bsz_i, n_i)
}
