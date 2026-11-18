//! C ABI for `crc4.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint8_t crc4(uint8_t c, uint64_t x, int bits)` (`crc4.c:26`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn crc4(c: u8, x: u64, bits: std::ffi::c_int) -> u8 {
    port::crc4(c, x, bits)
}

/// `crc4` as `rust_anything_crc4`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_crc4(c: u8, x: u64, bits: std::ffi::c_int) -> u8 {
    port::crc4(c, x, bits)
}
