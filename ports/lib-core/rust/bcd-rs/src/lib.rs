//! C ABI for `bcd.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned _bcd2bin(unsigned char val)` (`bcd.c:5`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn _bcd2bin(val: std::ffi::c_uchar) -> std::ffi::c_uint {
    port::_bcd2bin(val)
}

/// `_bcd2bin` as `rust_anything__bcd2bin`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything__bcd2bin(val: std::ffi::c_uchar) -> std::ffi::c_uint {
    port::_bcd2bin(val)
}

/// `unsigned char _bin2bcd(unsigned val)` (`bcd.c:11`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn _bin2bcd(val: std::ffi::c_uint) -> std::ffi::c_uchar {
    port::_bin2bcd(val)
}

/// `_bin2bcd` as `rust_anything__bin2bcd`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything__bin2bcd(val: std::ffi::c_uint) -> std::ffi::c_uchar {
    port::_bin2bcd(val)
}
