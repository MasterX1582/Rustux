//! The Rust port of `bcd.c`: safe Rust; `lib.rs` exports each function under its C name.

#![forbid(unsafe_code)]

/// Port of `unsigned _bcd2bin(unsigned char val)` (`bcd.c:5`).
pub fn _bcd2bin(val: std::ffi::c_uchar) -> std::ffi::c_uint {
    (val & 0x0f) as std::ffi::c_uint + (val >> 4) as std::ffi::c_uint * 10
}

/// Port of `unsigned char _bin2bcd(unsigned val)` (`bcd.c:11`).
pub fn _bin2bcd(val: std::ffi::c_uint) -> std::ffi::c_uchar {
    let t = (val.wrapping_mul(103)) >> 10;
    ((t << 4) | val.wrapping_sub(t * 10)) as std::ffi::c_uchar
}
