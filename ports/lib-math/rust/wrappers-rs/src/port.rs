//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Each wrapper calls the real ported unit — `rational.c`, `reciprocal_div.c`,
//! `cordic.c`, `polynomial.c` — through a scalar signature the ctypes bridge
//! can record. C has no error channel: return what the C code returns, and
//! never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

use std::ffi::c_ulong;

/* ------------------------- rational.c ------------------------- */

/// Port of `rational_best_approximation` (`rational.c`): continued-fraction
/// best approximation of `gn/gd` bounded by `mn`/`md`. Returns (n, d).
fn rational_best_approximation(
    gn: c_ulong,
    gd: c_ulong,
    mn: c_ulong,
    md: c_ulong,
) -> (c_ulong, c_ulong) {
    let mut n = gn;
    let mut d = gd;
    let mut n0: c_ulong = 0;
    let mut d0: c_ulong = 1;
    let mut n1: c_ulong = 1;
    let mut d1: c_ulong = 0;

    loop {
        if d == 0 {
            break;
        }
        // Next term 'a' of the continued fraction via Euclidean algorithm.
        let dp = d;
        let a = n / d;
        d = n % d;
        n = dp;

        // Current convergent n2/d2 from the term and the two priors.
        let n2 = n0.wrapping_add(a.wrapping_mul(n1));
        let d2 = d0.wrapping_add(a.wrapping_mul(d1));

        if n2 > mn || d2 > md {
            let mut t: c_ulong = c_ulong::MAX;

            // `if (d1) t = ...; if (n1) t = min(...)` — kept as checked_div
            // with the original `t` as fallback (same value when the guard fails).
            t = md.wrapping_sub(d0).checked_div(d1).unwrap_or(t);
            t = t.min(mn.wrapping_sub(n0).checked_div(n1).unwrap_or(t));

            // Pick the semi-convergent when closer than the previous
            // convergent (always when there is no previous one, d1 == 0).
            if d1 == 0
                || t.wrapping_mul(2) > a
                || (t.wrapping_mul(2) == a && d0.wrapping_mul(dp) > d1.wrapping_mul(d))
            {
                n1 = n0.wrapping_add(t.wrapping_mul(n1));
                d1 = d0.wrapping_add(t.wrapping_mul(d1));
            }
            break;
        }
        n0 = n1;
        n1 = n2;
        d0 = d1;
        d1 = d2;
    }
    (n1, d1)
}

/// Port of `unsigned long rational_n(unsigned long gn, unsigned long gd, unsigned long mn, unsigned long md)` (`wrappers.c:19`).
pub fn rational_n(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    rational_best_approximation(gn, gd, mn, md).0
}

/// Port of `unsigned long rational_d(unsigned long gn, unsigned long gd, unsigned long mn, unsigned long md)` (`wrappers.c:28`).
pub fn rational_d(
    gn: std::ffi::c_ulong,
    gd: std::ffi::c_ulong,
    mn: std::ffi::c_ulong,
    md: std::ffi::c_ulong,
) -> std::ffi::c_ulong {
    rational_best_approximation(gn, gd, mn, md).1
}

/* ------------------------- reciprocal_div.c ------------------------- */

/// `fls` for a u32-sized value: position of the most significant set bit,
/// 1-based; 0 for 0 (matches the shim's `unsigned long` implementation on
/// inputs that fit in 32 bits).
fn fls32(v: u32) -> i32 {
    if v == 0 {
        0
    } else {
        (32 - v.leading_zeros()) as i32
    }
}

struct ReciprocalValue {
    m: u32,
    sh1: u8,
    sh2: u8,
}

/// Port of `reciprocal_value` (`reciprocal_div.c:17`).
fn reciprocal_value(d: u32) -> ReciprocalValue {
    let l = fls32(d.wrapping_sub(1));
    let mut m = (1u64 << 32).wrapping_mul((1u64 << l) - d as u64);
    // do_div(m, d): m /= d — d == 0 faults in C (UB; any value is conforming).
    m = m.checked_div(d as u64).unwrap_or(u64::MAX);
    m = m.wrapping_add(1);

    ReciprocalValue {
        m: m as u32,
        sh1: l.min(1) as u8,
        sh2: (l - 1).max(0) as u8,
    }
}

struct ReciprocalValueAdv {
    m: u32,
    sh: u8,
    exp: u8,
    is_wide_m: bool,
}

/// Port of `reciprocal_value_adv` (`reciprocal_div.c:35`).
fn reciprocal_value_adv(d: u32, prec: u8) -> ReciprocalValueAdv {
    // ceil(log2(d))
    let l = fls32(d.wrapping_sub(1)) as u32;
    // The kernel WARNs for l == 32 (i.e. d == 0); mlow/mhigh overflow u64
    // there and the subsequent do_div faults — UB either way.
    let mut post_shift = l;
    let mut mlow = 1u64.wrapping_shl(32 + l);
    // do_div(mlow, d)
    mlow = mlow.checked_div(d as u64).unwrap_or(u64::MAX);
    let mut mhigh = (1u64.wrapping_shl(32 + l))
        .wrapping_add(1u64.wrapping_shl(32u32.wrapping_add(l).wrapping_sub(prec as u32)));
    // do_div(mhigh, d)
    mhigh = mhigh.checked_div(d as u64).unwrap_or(u64::MAX);

    while post_shift > 0 {
        let lo = mlow >> 1;
        let hi = mhigh >> 1;
        if lo >= hi {
            break;
        }
        mlow = lo;
        mhigh = hi;
        post_shift -= 1;
    }

    ReciprocalValueAdv {
        m: mhigh as u32,
        sh: post_shift as u8,
        exp: l as u8,
        is_wide_m: mhigh > u32::MAX as u64,
    }
}

/// Port of `uint32_t reciprocal_m(uint32_t d)` (`wrappers.c:37`).
pub fn reciprocal_m(d: u32) -> u32 {
    reciprocal_value(d).m
}

/// Port of `uint32_t reciprocal_sh1(uint32_t d)` (`wrappers.c:42`).
pub fn reciprocal_sh1(d: u32) -> u32 {
    reciprocal_value(d).sh1 as u32
}

/// Port of `uint32_t reciprocal_sh2(uint32_t d)` (`wrappers.c:47`).
pub fn reciprocal_sh2(d: u32) -> u32 {
    reciprocal_value(d).sh2 as u32
}

/// Port of `uint32_t reciprocal_adv_m(uint32_t d, uint32_t prec)` (`wrappers.c:52`).
pub fn reciprocal_adv_m(d: u32, prec: u32) -> u32 {
    reciprocal_value_adv(d, prec as u8).m
}

/// Port of `uint32_t reciprocal_adv_sh(uint32_t d, uint32_t prec)` (`wrappers.c:57`).
pub fn reciprocal_adv_sh(d: u32, prec: u32) -> u32 {
    reciprocal_value_adv(d, prec as u8).sh as u32
}

/// Port of `uint32_t reciprocal_adv_exp(uint32_t d, uint32_t prec)` (`wrappers.c:62`).
pub fn reciprocal_adv_exp(d: u32, prec: u32) -> u32 {
    reciprocal_value_adv(d, prec as u8).exp as u32
}

/// Port of `uint32_t reciprocal_adv_wide(uint32_t d, uint32_t prec)` (`wrappers.c:67`).
pub fn reciprocal_adv_wide(d: u32, prec: u32) -> u32 {
    reciprocal_value_adv(d, prec as u8).is_wide_m as u32
}

/* ------------------------- cordic.c ------------------------- */

const ARCTAN_TABLE: [i32; 18] = [
    2949120, 1740967, 919879, 466945, 234379, 117304, 58666, 29335, 14668, 7334, 3667, 1833, 917,
    458, 229, 115, 57, 29,
];

const CORDIC_ANGLE_GEN: i32 = 39797;
const CORDIC_PRECISION_SHIFT: u32 = 16;
const CORDIC_NUM_ITER: u32 = CORDIC_PRECISION_SHIFT + 2;

// CORDIC_FIXED(X): (s32)(X << 16) — wraps on overflow like the x86 build.
fn cordic_fixed(x: i32) -> i32 {
    x.wrapping_shl(CORDIC_PRECISION_SHIFT)
}

// CORDIC_FLOAT(X): rounding fixed->float conversion, verbatim semantics.
fn cordic_float(x: i32) -> i32 {
    if x >= 0 {
        ((x >> (CORDIC_PRECISION_SHIFT - 1)) + 1) >> 1
    } else {
        -(((x.wrapping_neg() >> (CORDIC_PRECISION_SHIFT - 1)) + 1) >> 1)
    }
}

/// Port of `cordic_calc_iq` (`cordic.c:46`). Returns (i, q).
fn cordic_calc_iq(theta: i32) -> (i32, i32) {
    let mut coord_i: i32 = CORDIC_ANGLE_GEN;
    let mut coord_q: i32 = 0;
    let mut angle: i32 = 0;
    let mut signx: i32 = 1;

    let mut theta = cordic_fixed(theta);
    let signtheta: i32 = if theta < 0 { -1 } else { 1 };
    theta = theta
        .wrapping_add(cordic_fixed(180).wrapping_mul(signtheta))
        .wrapping_rem(cordic_fixed(360))
        .wrapping_sub(cordic_fixed(180).wrapping_mul(signtheta));

    if cordic_float(theta) > 90 {
        theta = theta.wrapping_sub(cordic_fixed(180));
        signx = -1;
    } else if cordic_float(theta) < -90 {
        theta = theta.wrapping_add(cordic_fixed(180));
        signx = -1;
    }

    for iter in 0..CORDIC_NUM_ITER {
        let valtmp;
        if theta > angle {
            valtmp = coord_i.wrapping_sub(coord_q >> iter);
            coord_q = coord_q.wrapping_add(coord_i >> iter);
            angle = angle.wrapping_add(ARCTAN_TABLE[iter as usize]);
        } else {
            valtmp = coord_i.wrapping_add(coord_q >> iter);
            coord_q = coord_q.wrapping_sub(coord_i >> iter);
            angle = angle.wrapping_sub(ARCTAN_TABLE[iter as usize]);
        }
        coord_i = valtmp;
    }

    (coord_i.wrapping_mul(signx), coord_q.wrapping_mul(signx))
}

/// Port of `int32_t cordic_i(int32_t theta)` (`wrappers.c:72`).
pub fn cordic_i(theta: i32) -> i32 {
    cordic_calc_iq(theta).0
}

/// Port of `int32_t cordic_q(int32_t theta)` (`wrappers.c:77`).
pub fn cordic_q(theta: i32) -> i32 {
    cordic_calc_iq(theta).1
}

/* ------------------------- polynomial.c ------------------------- */

/// `mult_frac(x, n, d)` verbatim (`linux/math.h`): `x * n / d` with reduced
/// overflow. Division by zero faults in C (UB; any value is conforming).
fn mult_frac(x: i64, n: i64, d: i64) -> i64 {
    let q = x.checked_div(d).unwrap_or(i64::MAX);
    let r = x.checked_rem(d).unwrap_or(0);
    q.wrapping_mul(n)
        .wrapping_add(r.wrapping_mul(n).checked_div(d).unwrap_or(i64::MAX))
}

/// Port of `polynomial_calc` (`polynomial.c:76`): terms are
/// `(deg, coef, divider, divider_leftover)`; the list ends at the first
/// `deg == 0` term (which is also the constant coefficient).
fn polynomial_calc(terms: &[(u32, i64, i64, i64)], total_divider: i64, data: i64) -> i64 {
    let td = if total_divider != 0 { total_divider } else { 1 };
    let mut ret: i64 = 0;

    for &(deg, coef, divider, leftover) in terms {
        let mut tmp = coef;
        for _ in 0..deg {
            tmp = mult_frac(tmp, data, divider);
        }
        ret = ret.wrapping_add(tmp.checked_div(leftover).unwrap_or(i64::MAX));
        if deg == 0 {
            break;
        }
    }

    ret.checked_div(td).unwrap_or(i64::MAX)
}

/// Port of `long polynomial_deg2(long td, long c0, long dv0, long lo0, long c1, long dv1, long lo1, long c2, long dv2, long lo2, long data)` (`wrappers.c:87`).
#[allow(clippy::too_many_arguments)]
pub fn polynomial_deg2(
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
    let terms = [(2, c0, dv0, lo0), (1, c1, dv1, lo1), (0, c2, dv2, lo2)];
    polynomial_calc(&terms, td, data)
}
