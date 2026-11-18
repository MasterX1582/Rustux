//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `unsigned long rational_n(unsigned long gn, unsigned long gd, unsigned long mn, unsigned long md)` (`wrappers.c:19`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn rational_n(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::rational_n(gn, gd, mn, md)
}

/// `rational_n` as `rust_anything_rational_n`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_rational_n(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::rational_n(gn, gd, mn, md)
}

/// `unsigned long rational_d(unsigned long gn, unsigned long gd, unsigned long mn, unsigned long md)` (`wrappers.c:28`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn rational_d(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::rational_d(gn, gd, mn, md)
}

/// `rational_d` as `rust_anything_rational_d`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_rational_d(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    port::rational_d(gn, gd, mn, md)
}

/// `uint32_t reciprocal_m(uint32_t d)` (`wrappers.c:37`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_m(d: u32) -> u32 {
    port::reciprocal_m(d)
}

/// `reciprocal_m` as `rust_anything_reciprocal_m`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_m(d: u32) -> u32 {
    port::reciprocal_m(d)
}

/// `uint32_t reciprocal_sh1(uint32_t d)` (`wrappers.c:42`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_sh1(d: u32) -> u32 {
    port::reciprocal_sh1(d)
}

/// `reciprocal_sh1` as `rust_anything_reciprocal_sh1`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_sh1(d: u32) -> u32 {
    port::reciprocal_sh1(d)
}

/// `uint32_t reciprocal_sh2(uint32_t d)` (`wrappers.c:47`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_sh2(d: u32) -> u32 {
    port::reciprocal_sh2(d)
}

/// `reciprocal_sh2` as `rust_anything_reciprocal_sh2`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_sh2(d: u32) -> u32 {
    port::reciprocal_sh2(d)
}

/// `uint32_t reciprocal_adv_m(uint32_t d, uint32_t prec)` (`wrappers.c:52`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_adv_m(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_m(d, prec)
}

/// `reciprocal_adv_m` as `rust_anything_reciprocal_adv_m`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_adv_m(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_m(d, prec)
}

/// `uint32_t reciprocal_adv_sh(uint32_t d, uint32_t prec)` (`wrappers.c:57`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_adv_sh(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_sh(d, prec)
}

/// `reciprocal_adv_sh` as `rust_anything_reciprocal_adv_sh`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_adv_sh(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_sh(d, prec)
}

/// `uint32_t reciprocal_adv_exp(uint32_t d, uint32_t prec)` (`wrappers.c:62`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_adv_exp(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_exp(d, prec)
}

/// `reciprocal_adv_exp` as `rust_anything_reciprocal_adv_exp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_adv_exp(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_exp(d, prec)
}

/// `uint32_t reciprocal_adv_wide(uint32_t d, uint32_t prec)` (`wrappers.c:67`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn reciprocal_adv_wide(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_wide(d, prec)
}

/// `reciprocal_adv_wide` as `rust_anything_reciprocal_adv_wide`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_reciprocal_adv_wide(d: u32, prec: u32) -> u32 {
    port::reciprocal_adv_wide(d, prec)
}

/// `int32_t cordic_i(int32_t theta)` (`wrappers.c:72`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn cordic_i(theta: i32) -> i32 {
    port::cordic_i(theta)
}

/// `cordic_i` as `rust_anything_cordic_i`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_cordic_i(theta: i32) -> i32 {
    port::cordic_i(theta)
}

/// `int32_t cordic_q(int32_t theta)` (`wrappers.c:77`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn cordic_q(theta: i32) -> i32 {
    port::cordic_q(theta)
}

/// `cordic_q` as `rust_anything_cordic_q`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_cordic_q(theta: i32) -> i32 {
    port::cordic_q(theta)
}

/// `long polynomial_deg2(long td, long c0, long dv0, long lo0, long c1, long dv1, long lo1, long c2, long dv2, long lo2, long data)` (`wrappers.c:87`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn polynomial_deg2(
    td: std::ffi::c_long,
    c0: std::ffi::c_long,
    dv0: std::ffi::c_long,
    lo0: std::ffi::c_long,
    c1: std::ffi::c_long,
    dv1: std::ffi::c_long,
    lo1: std::ffi::c_long,
    c2: std::ffi::c_long,
    dv2: std::ffi::c_long,
    lo2: std::ffi::c_long,
    data: std::ffi::c_long,
) -> std::ffi::c_long {
    port::polynomial_deg2(td, c0, dv0, lo0, c1, dv1, lo1, c2, dv2, lo2, data)
}

/// `polynomial_deg2` as `rust_anything_polynomial_deg2`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_polynomial_deg2(
    td: std::ffi::c_long,
    c0: std::ffi::c_long,
    dv0: std::ffi::c_long,
    lo0: std::ffi::c_long,
    c1: std::ffi::c_long,
    dv1: std::ffi::c_long,
    lo1: std::ffi::c_long,
    c2: std::ffi::c_long,
    dv2: std::ffi::c_long,
    lo2: std::ffi::c_long,
    data: std::ffi::c_long,
) -> std::ffi::c_long {
    port::polynomial_deg2(td, c0, dv0, lo0, c1, dv1, lo1, c2, dv2, lo2, data)
}
