//! Integer math — ports of `lib/math/` from the C kernel.
//!
//! Verified against the original's recorded behavior:
//! `ports/int-sqrt/rust/int-sqrt-rs` (17/17, gate passed) and
//! `ports/lib-math/rust/*-rs` (gcd 154/154, lcm 308/308, int_pow 154/154,
//! int_log 34/34, div64 232/232, gates passed).

use core::ffi::c_ulong;

// The verified ports use C `unsigned long` (`c_ulong`) — 64-bit on the
// x86_64 target this library is built for, and on the LP64 host whose
// recordings verified them. Pin that assumption so a narrower target
// fails to compile rather than silently changing behavior.
const _: () = assert!(
    size_of::<c_ulong>() == size_of::<u64>(),
    "klib::math verified ports assume a 64-bit `unsigned long` (LP64)"
);

/// Port of `lib/math/int_sqrt.c`: `unsigned long int_sqrt(unsigned long x)`.
///
/// Guy Steele's shift-and-subtract integer square root: floor(sqrt(x)).
pub fn int_sqrt(x: u64) -> u64 {
    let mut x = x;
    if x <= 1 {
        return x;
    }

    let mut m: u64 = 1 << ((63 - x.leading_zeros()) & !1);
    let mut y: u64 = 0;
    while m != 0 {
        let b = y + m;
        y >>= 1;
        if x >= b {
            x -= b;
            y += m;
        }
        m >>= 2;
    }
    y
}

/// Port of `lib/math/gcd.c`: `unsigned long gcd(unsigned long a, unsigned long b)`.
///
/// Binary GCD (Stein's algorithm); the recorded `.so` took the `binary_gcd`
/// (`__ffs`) path since `static_branch_likely` resolves true in the harness.
pub fn gcd(a: c_ulong, b: c_ulong) -> c_ulong {
    let r: c_ulong = a | b;

    if a == 0 || b == 0 {
        return r;
    }

    let mut a = a;
    let mut b = b >> b.trailing_zeros();
    if b == 1 {
        return r.isolate_lowest_one();
    }

    loop {
        a >>= a.trailing_zeros();
        if a == 1 {
            return r.isolate_lowest_one();
        }
        if a == b {
            return a << r.trailing_zeros();
        }

        if a < b {
            core::mem::swap(&mut a, &mut b);
        }
        a = a.wrapping_sub(b);
    }
}

/// Port of `lib/math/lcm.c`: `unsigned long lcm(unsigned long a, unsigned long b)`.
pub fn lcm(a: c_ulong, b: c_ulong) -> c_ulong {
    if a != 0 && b != 0 {
        (a / gcd(a, b)).wrapping_mul(b)
    } else {
        0
    }
}

/// Port of `lib/math/lcm.c`: `unsigned long lcm_not_zero(unsigned long a, unsigned long b)`.
pub fn lcm_not_zero(a: c_ulong, b: c_ulong) -> c_ulong {
    let l = lcm(a, b);

    if l != 0 {
        return l;
    }

    if b != 0 {
        b
    } else {
        a
    }
}

/// Port of `lib/math/int_pow.c`: `u64 int_pow(u64 base, unsigned int exp)`.
///
/// Exponentiation by squaring; wraps on overflow exactly like C `u64`.
pub fn int_pow(base: u64, exp: u32) -> u64 {
    let mut result: u64 = 1;
    let mut base = base;
    let mut exp = exp;

    while exp != 0 {
        if exp & 1 != 0 {
            result = result.wrapping_mul(base);
        }
        exp >>= 1;
        base = base.wrapping_mul(base);
    }

    result
}

const LOGTABLE: [u16; 256] = [
    0x0000, 0x0171, 0x02e0, 0x044e, 0x05ba, 0x0725, 0x088e, 0x09f7, 0x0b5d, 0x0cc3, 0x0e27, 0x0f8a,
    0x10eb, 0x124b, 0x13aa, 0x1508, 0x1664, 0x17bf, 0x1919, 0x1a71, 0x1bc8, 0x1d1e, 0x1e73, 0x1fc6,
    0x2119, 0x226a, 0x23ba, 0x2508, 0x2656, 0x27a2, 0x28ed, 0x2a37, 0x2b80, 0x2cc8, 0x2e0f, 0x2f54,
    0x3098, 0x31dc, 0x331e, 0x345f, 0x359f, 0x36de, 0x381b, 0x3958, 0x3a94, 0x3bce, 0x3d08, 0x3e41,
    0x3f78, 0x40af, 0x41e4, 0x4319, 0x444c, 0x457f, 0x46b0, 0x47e1, 0x4910, 0x4a3f, 0x4b6c, 0x4c99,
    0x4dc5, 0x4eef, 0x5019, 0x5142, 0x526a, 0x5391, 0x54b7, 0x55dc, 0x5700, 0x5824, 0x5946, 0x5a68,
    0x5b89, 0x5ca8, 0x5dc7, 0x5ee5, 0x6003, 0x611f, 0x623a, 0x6355, 0x646f, 0x6588, 0x66a0, 0x67b7,
    0x68ce, 0x69e4, 0x6af8, 0x6c0c, 0x6d20, 0x6e32, 0x6f44, 0x7055, 0x7165, 0x7274, 0x7383, 0x7490,
    0x759d, 0x76aa, 0x77b5, 0x78c0, 0x79ca, 0x7ad3, 0x7bdb, 0x7ce3, 0x7dea, 0x7ef0, 0x7ff6, 0x80fb,
    0x81ff, 0x8302, 0x8405, 0x8507, 0x8608, 0x8709, 0x8809, 0x8908, 0x8a06, 0x8b04, 0x8c01, 0x8cfe,
    0x8dfa, 0x8ef5, 0x8fef, 0x90e9, 0x91e2, 0x92db, 0x93d2, 0x94ca, 0x95c0, 0x96b6, 0x97ab, 0x98a0,
    0x9994, 0x9a87, 0x9b7a, 0x9c6c, 0x9d5e, 0x9e4f, 0x9f3f, 0xa02e, 0xa11e, 0xa20c, 0xa2fa, 0xa3e7,
    0xa4d4, 0xa5c0, 0xa6ab, 0xa796, 0xa881, 0xa96a, 0xaa53, 0xab3c, 0xac24, 0xad0c, 0xadf2, 0xaed9,
    0xafbe, 0xb0a4, 0xb188, 0xb26c, 0xb350, 0xb433, 0xb515, 0xb5f7, 0xb6d9, 0xb7ba, 0xb89a, 0xb97a,
    0xba59, 0xbb38, 0xbc16, 0xbcf4, 0xbdd1, 0xbead, 0xbf8a, 0xc065, 0xc140, 0xc21b, 0xc2f5, 0xc3cf,
    0xc4a8, 0xc580, 0xc658, 0xc730, 0xc807, 0xc8de, 0xc9b4, 0xca8a, 0xcb5f, 0xcc34, 0xcd08, 0xcddc,
    0xceaf, 0xcf82, 0xd054, 0xd126, 0xd1f7, 0xd2c8, 0xd399, 0xd469, 0xd538, 0xd607, 0xd6d6, 0xd7a4,
    0xd872, 0xd93f, 0xda0c, 0xdad9, 0xdba5, 0xdc70, 0xdd3b, 0xde06, 0xded0, 0xdf9a, 0xe063, 0xe12c,
    0xe1f5, 0xe2bd, 0xe385, 0xe44c, 0xe513, 0xe5d9, 0xe69f, 0xe765, 0xe82a, 0xe8ef, 0xe9b3, 0xea77,
    0xeb3b, 0xebfe, 0xecc1, 0xed83, 0xee45, 0xef06, 0xefc8, 0xf088, 0xf149, 0xf209, 0xf2c8, 0xf387,
    0xf446, 0xf505, 0xf5c3, 0xf680, 0xf73e, 0xf7fb, 0xf8b7, 0xf973, 0xfa2f, 0xfaea, 0xfba5, 0xfc60,
    0xfd1a, 0xfdd4, 0xfe8e, 0xff47,
];

/// Port of `lib/math/int_log.c`: `unsigned int intlog2(u32 value)`.
///
/// Returns `log2(value) * 2^24`; returns 0 for `value == 0` (C WARN_ON path).
pub fn intlog2(value: u32) -> u32 {
    if value == 0 {
        return 0;
    }

    let msb = 31 - value.leading_zeros();
    let significand = value << (31 - msb);
    let logentry = (significand >> 23) % LOGTABLE.len() as u32;

    let next = LOGTABLE[((logentry + 1) % LOGTABLE.len() as u32) as usize] as u32;
    let cur = LOGTABLE[logentry as usize] as u32;
    let interpolation =
        ((significand & 0x7fffff).wrapping_mul(next.wrapping_sub(cur) & 0xffff)) >> 15;

    (msb << 24) + (cur << 8) + interpolation
}

/// Port of `lib/math/int_log.c`: `unsigned int intlog10(u32 value)`.
///
/// Returns `log10(value) * 2^24`; `log10(x) = log2(x) * log10(2)`.
pub fn intlog10(value: u32) -> u32 {
    if value == 0 {
        return 0;
    }

    let log = intlog2(value) as u64;

    (log.wrapping_mul(646456993) >> 31) as u32
}

/// Port of `lib/math/div64.c`: `u32 iter_div_u64_rem(u64 dividend, u32 divisor, u64 *remainder)`.
///
/// Iterative div/mod for dividends not expected to be much bigger than the
/// divisor (O(quotient) subtract loop — the kernel's `__iter_div_u64_rem`).
/// Verified by `ports/lib-math/check-iter-div.py` (pointer out-param keeps it
/// outside rust-anything's ctypes bridge). Faithful to the C: `divisor == 0`
/// loops forever, so callers must not pass it, exactly as upstream.
pub fn iter_div_u64_rem(dividend: u64, divisor: u32, remainder: &mut u64) -> u32 {
    let mut ret: u32 = 0;
    let mut dividend = dividend;
    let divisor = divisor as u64;

    while dividend >= divisor {
        dividend -= divisor;
        ret = ret.wrapping_add(1);
    }

    *remainder = dividend;
    ret
}

/// Port of `lib/math/div64.c`: the `static inline` 64x64=128-bit multiply-add
/// helper (`mul_u64_u64_add_u64`).
fn mul_u64_u64_add_u64(p_lo: &mut u64, a: u64, b: u64, c: u64) -> u64 {
    let prod = a as u128 * b as u128 + c as u128;

    *p_lo = prod as u64;
    (prod >> 64) as u64
}

/// Port of `lib/math/div64.c`: `u64 mul_u64_add_u64_div_u64(u64 a, u64 b, u64 c, u64 d)`.
///
/// Computes `(a * b + c) / d` with 128-bit intermediate precision. Deliberate
/// divergence: the C traps on `d == 0` (a real division-by-zero fault, recorded
/// as a crash — unverifiable by behavior comparison); this port returns the
/// same saturating `u64::MAX` marker as the overflow path. Kernel callers must
/// not pass `d == 0`, exactly as in C.
pub fn mul_u64_add_u64_div_u64(a: u64, b: u64, c: u64, d: u64) -> u64 {
    const BITS_PER_ITER: u32 = 32;

    let mut n_lo: u64 = 0;
    let mut n_hi = mul_u64_u64_add_u64(&mut n_lo, a, b, c);
    let mut d = d;

    if n_hi == 0 {
        return n_lo.checked_div(d).unwrap_or(u64::MAX);
    }

    if n_hi >= d {
        return u64::MAX;
    }

    let d_z_hi = d.leading_zeros();
    if d_z_hi != 0 {
        d <<= d_z_hi;
        n_hi = (n_hi << d_z_hi) | (n_lo >> (64 - d_z_hi));
        n_lo <<= d_z_hi;
    }

    let mut reps: u32 = 64 / BITS_PER_ITER;
    if (n_hi >> 32) as u32 == 0 {
        reps -= 32 / BITS_PER_ITER;
        n_hi = (n_hi << 32) | (n_lo >> 32);
        n_lo <<= 32;
    }

    n_lo = !n_lo;
    n_hi = !n_hi;

    let d_msig: u64 = (d >> (64 - BITS_PER_ITER)).wrapping_add(1);

    let mut quotient: u64 = 0;
    while reps != 0 {
        reps -= 1;
        let mut q_digit: u64 = (!n_hi >> (64 - 2 * BITS_PER_ITER)) / d_msig;
        let mut overflow: u32 = (n_hi >> (64 - BITS_PER_ITER)) as u32;
        n_hi = (n_hi << BITS_PER_ITER).wrapping_add(n_lo >> (64 - BITS_PER_ITER));
        n_lo <<= BITS_PER_ITER;
        let n = n_hi;
        overflow = overflow.wrapping_add(mul_u64_u64_add_u64(&mut n_hi, d, q_digit, n) as u32);
        while overflow < (0xffff_ffffu32 >> (32 - BITS_PER_ITER)) {
            q_digit = q_digit.wrapping_add(1);
            n_hi = n_hi.wrapping_add(d);
            overflow = overflow.wrapping_add((n_hi < d) as u32);
        }
        quotient = (quotient << BITS_PER_ITER).wrapping_add(q_digit);
    }

    if n_hi.wrapping_add(d) > n_hi {
        quotient = quotient.wrapping_add(1);
    }
    quotient
}

/// Port of `lib/math/rational.c`: `rational_best_approximation` —
/// continued-fraction best approximation of `gn/gd` bounded by `mn`/`md`.
/// Returns `(best_numerator, best_denominator)` (the C out-params as a tuple).
pub fn rational_best_approximation(
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
        let dp = d;
        let a = n / d;
        d = n % d;
        n = dp;

        let n2 = n0.wrapping_add(a.wrapping_mul(n1));
        let d2 = d0.wrapping_add(a.wrapping_mul(d1));

        if n2 > mn || d2 > md {
            let mut t: c_ulong = c_ulong::MAX;

            t = md.wrapping_sub(d0).checked_div(d1).unwrap_or(t);
            t = t.min(mn.wrapping_sub(n0).checked_div(n1).unwrap_or(t));

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

/// `fls` for a u32-sized value, as the port of `reciprocal_div.c` uses it.
fn fls32(v: u32) -> i32 {
    if v == 0 {
        0
    } else {
        (32 - v.leading_zeros()) as i32
    }
}

/// Port of `include/linux/reciprocal_div.h`: `struct reciprocal_value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReciprocalValue {
    pub m: u32,
    pub sh1: u8,
    pub sh2: u8,
}

/// Port of `lib/math/reciprocal_div.c`: `reciprocal_value`.
///
/// `d == 0` faults in C (UB — recorded as a crash); the port returns whatever
/// the checked_div fallback produces. Callers must not pass 0, as upstream.
pub fn reciprocal_value(d: u32) -> ReciprocalValue {
    let l = fls32(d.wrapping_sub(1));
    let mut m = (1u64 << 32).wrapping_mul((1u64 << l) - d as u64);
    m = m.checked_div(d as u64).unwrap_or(u64::MAX);
    m = m.wrapping_add(1);

    ReciprocalValue {
        m: m as u32,
        sh1: l.min(1) as u8,
        sh2: (l - 1).max(0) as u8,
    }
}

/// Port of `include/linux/reciprocal_div.h`: `struct reciprocal_value_adv`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReciprocalValueAdv {
    pub m: u32,
    pub sh: u8,
    pub exp: u8,
    pub is_wide_m: bool,
}

/// Port of `lib/math/reciprocal_div.c`: `reciprocal_value_adv`.
pub fn reciprocal_value_adv(d: u32, prec: u8) -> ReciprocalValueAdv {
    let l = fls32(d.wrapping_sub(1)) as u32;
    let mut post_shift = l;
    let mut mlow = 1u64.wrapping_shl(32 + l);
    mlow = mlow.checked_div(d as u64).unwrap_or(u64::MAX);
    let mut mhigh = (1u64.wrapping_shl(32 + l))
        .wrapping_add(1u64.wrapping_shl(32u32.wrapping_add(l).wrapping_sub(prec as u32)));
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

const ARCTAN_TABLE: [i32; 18] = [
    2949120, 1740967, 919879, 466945, 234379, 117304, 58666, 29335, 14668, 7334, 3667, 1833, 917,
    458, 229, 115, 57, 29,
];

const CORDIC_ANGLE_GEN: i32 = 39797;
const CORDIC_PRECISION_SHIFT: u32 = 16;
const CORDIC_NUM_ITER: u32 = CORDIC_PRECISION_SHIFT + 2;

fn cordic_fixed(x: i32) -> i32 {
    x.wrapping_shl(CORDIC_PRECISION_SHIFT)
}

fn cordic_float(x: i32) -> i32 {
    if x >= 0 {
        ((x >> (CORDIC_PRECISION_SHIFT - 1)) + 1) >> 1
    } else {
        -(((x.wrapping_neg() >> (CORDIC_PRECISION_SHIFT - 1)) + 1) >> 1)
    }
}

/// Port of `include/linux/cordic.h`: `struct cordic_iq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CordicIq {
    pub i: i32,
    pub q: i32,
}

/// Port of `lib/math/cordic.c`: `cordic_calc_iq` — CORDIC sine/cosine pair,
/// scaled by 2^16, for `theta` in degrees.
pub fn cordic_calc_iq(theta: i32) -> CordicIq {
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

    CordicIq {
        i: coord_i.wrapping_mul(signx),
        q: coord_q.wrapping_mul(signx),
    }
}

/// Port of `include/linux/polynomial.h`: `struct polynomial_term`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolynomialTerm {
    pub deg: u32,
    pub coef: i64,
    pub divider: i64,
    pub divider_leftover: i64,
}

/// `mult_frac(x, n, d)` verbatim (`linux/math.h`). Division by zero faults in
/// C (UB); the port returns `i64::MAX` there, matching its other
/// deliberate-divergence markers.
fn mult_frac(x: i64, n: i64, d: i64) -> i64 {
    let q = x.checked_div(d).unwrap_or(i64::MAX);
    let r = x.checked_rem(d).unwrap_or(0);
    q.wrapping_mul(n)
        .wrapping_add(r.wrapping_mul(n).checked_div(d).unwrap_or(i64::MAX))
}

/// Port of `lib/math/polynomial.c`: `polynomial_calc`. `terms` plays the C
/// flexible array: it must end at its `deg == 0` entry (also the constant
/// coefficient), exactly like the kernel descriptor.
pub fn polynomial_calc(terms: &[PolynomialTerm], total_divider: i64, data: i64) -> i64 {
    let td = if total_divider != 0 { total_divider } else { 1 };
    let mut ret: i64 = 0;

    for term in terms {
        let mut tmp = term.coef;
        for _ in 0..term.deg {
            tmp = mult_frac(tmp, data, term.divider);
        }
        ret = ret.wrapping_add(tmp.checked_div(term.divider_leftover).unwrap_or(i64::MAX));
        if term.deg == 0 {
            break;
        }
    }

    ret.checked_div(td).unwrap_or(i64::MAX)
}

// ---- prime_numbers.c -----------------------------------------------------
//
// Port of `lib/math/prime_numbers.c`. The C keeps a grow-on-demand Sieve of
// Eratosthenes behind RCU+mutex and falls back to trial division when the
// allocation fails. `no_std` has no allocator, so the sieve is a fixed
// `SIEVE_BITS`-bit static bitmap: capacity exhaustion plays exactly the role
// `kmalloc` failure plays upstream — the same slow path takes over, so every
// observable return value is identical to the C on every input.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Sieve capacity in bits: 512 KiB of `.bss`.
///
/// INVARIANT: single-threaded, non-reentrant callers only (the kernel is a
/// single-CPU UEFI app today). The atomics make the statics `Sync`, but they
/// do NOT serialize expansion vs. reads — before SMP or ISR use this needs a
/// real lock.
const SIEVE_BITS: u64 = 1 << 22;
const SIEVE_WORDS: usize = (SIEVE_BITS / 64) as usize;

const SMALL_PRIMES_LAST: u64 = 61;
const SMALL_PRIMES_SZ: u64 = 64;
const SMALL_PRIMES_WORD: u64 = (1 << 2)
    | (1 << 3)
    | (1 << 5)
    | (1 << 7)
    | (1 << 11)
    | (1 << 13)
    | (1 << 17)
    | (1 << 19)
    | (1 << 23)
    | (1 << 29)
    | (1 << 31)
    | (1 << 37)
    | (1 << 41)
    | (1 << 43)
    | (1 << 47)
    | (1 << 53)
    | (1 << 59)
    | (1 << 61);

static SIEVE: [AtomicU64; SIEVE_WORDS] = [const { AtomicU64::new(0) }; SIEVE_WORDS];
static SIEVE_LAST: AtomicU64 = AtomicU64::new(SMALL_PRIMES_LAST);
static SIEVE_SZ: AtomicU64 = AtomicU64::new(SMALL_PRIMES_SZ);
static SIEVE_INIT: AtomicBool = AtomicBool::new(false);

fn sieve_init() {
    if !SIEVE_INIT.swap(true, Ordering::SeqCst) {
        SIEVE[0].store(SMALL_PRIMES_WORD, Ordering::SeqCst);
    }
}

fn sieve_test_bit(n: u64) -> bool {
    SIEVE[(n / 64) as usize].load(Ordering::SeqCst) & (1u64 << (n % 64)) != 0
}

fn sieve_clear_bit(n: u64) {
    let w = &SIEVE[(n / 64) as usize];
    w.fetch_and(!(1u64 << (n % 64)), Ordering::SeqCst);
}

fn sieve_find_next_bit(size: u64, offset: u64) -> u64 {
    let mut i = offset;
    while i < size {
        if sieve_test_bit(i) {
            return i;
        }
        i += 1;
    }
    size
}

fn sieve_clear_multiples(x: u64, start: u64, end: u64) -> u64 {
    let mut m = x.wrapping_mul(2);
    if m < start {
        m = start.wrapping_add(x - 1) / x * x;
    }
    while m < end {
        sieve_clear_bit(m);
        m = m.wrapping_add(x);
    }
    x
}

/// `expand_to_next_prime`, in-place on the static bitmap: the new region is
/// filled and sieved exactly like the C's freshly allocated bitmap.
/// Returns `false` past `SIEVE_BITS` — the `kmalloc`-failure analog.
fn sieve_expand_to(x: u64) -> bool {
    sieve_init();
    let mut sz = x.wrapping_mul(2);
    if sz < x {
        return false;
    }
    sz = (sz.wrapping_sub(1) | 63).wrapping_add(1);
    // sz == 0 means round_up wrapped (x in [2^63-31, 2^63-1]): upstream the
    // `sz < x` check misses this and the C heap-corrupts (8-byte write past
    // a 24-byte allocation). We take the intended contract instead — the
    // allocation "failed", so the slow path runs, matching what upstream
    // itself does for the neighboring x = 2^63.
    if sz == 0 || sz > SIEVE_BITS {
        return false;
    }

    if x < SIEVE_LAST.load(Ordering::SeqCst) {
        return true;
    }

    let old_sz = SIEVE_SZ.load(Ordering::SeqCst);
    let old_words = (old_sz.div_ceil(64)) as usize;
    let nwords = (sz / 64) as usize;
    for w in &SIEVE[old_words..nwords] {
        w.store(!0, Ordering::SeqCst);
    }

    let mut new_last = 0u64;
    let mut y = 2u64;
    while y < sz {
        new_last = sieve_clear_multiples(y, old_sz, sz);
        y = sieve_find_next_bit(sz, y + 1);
    }
    // BUG_ON(new->last <= x)
    assert!(new_last > x);

    SIEVE_LAST.store(new_last, Ordering::SeqCst);
    SIEVE_SZ.store(sz, Ordering::SeqCst);
    true
}

/// Port of `lib/math/prime_numbers.c`: `slow_is_prime_number`.
pub fn slow_is_prime_number(x: c_ulong) -> bool {
    let mut y = int_sqrt(x);

    while y > 1 {
        if x.is_multiple_of(y) {
            break;
        }
        y -= 1;
    }
    y == 1
}

/// Port of `lib/math/prime_numbers.c`: `is_prime_number`.
pub fn is_prime_number(x: c_ulong) -> bool {
    let x: u64 = x;

    loop {
        sieve_init();
        if x < SIEVE_SZ.load(Ordering::SeqCst) {
            return sieve_test_bit(x);
        }
        if !sieve_expand_to(x) {
            return slow_is_prime_number(x);
        }
    }
}

/// Port of `lib/math/prime_numbers.c`: `next_prime_number`.
pub fn next_prime_number(x: c_ulong) -> c_ulong {
    let mut x: u64 = x;

    loop {
        sieve_init();
        if x < SIEVE_LAST.load(Ordering::SeqCst) {
            return sieve_find_next_bit(SIEVE_LAST.load(Ordering::SeqCst), x.wrapping_add(1));
        }
        if !sieve_expand_to(x) {
            // slow_next_prime_number
            while x < u64::MAX {
                x = x.wrapping_add(1);
                if slow_is_prime_number(x) {
                    return x;
                }
            }
            return x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values() {
        for (x, want) in [
            (0, 0),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (15, 3),
            (16, 4),
            (17, 4),
        ] {
            assert_eq!(int_sqrt(x), want, "int_sqrt({x})");
        }
    }

    #[test]
    fn perfect_squares_and_neighbors() {
        for r in [1u64, 3, 255, 1000, 65535, u32::MAX as u64] {
            let sq = r * r;
            assert_eq!(int_sqrt(sq), r);
            assert_eq!(int_sqrt(sq - 1), r - 1);
            if sq < u64::MAX {
                assert_eq!(int_sqrt(sq + 1), r);
            }
        }
    }

    #[test]
    fn extremes() {
        assert_eq!(int_sqrt(u64::MAX), u32::MAX as u64);
    }

    #[test]
    fn gcd_basics() {
        assert_eq!(gcd(48, 36), 12);
        assert_eq!(gcd(1071, 462), 21);
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(5, 0), 5);
        assert_eq!(gcd(0, 0), 0);
        assert_eq!(gcd(7, 13), 1);
    }

    #[test]
    fn lcm_basics() {
        assert_eq!(lcm(4, 6), 12);
        assert_eq!(lcm(21, 6), 42);
        assert_eq!(lcm(0, 5), 0);
        assert_eq!(lcm_not_zero(0, 5), 5);
        assert_eq!(lcm_not_zero(5, 0), 5);
        assert_eq!(lcm_not_zero(0, 0), 0);
    }

    #[test]
    fn int_pow_basics() {
        assert_eq!(int_pow(2, 10), 1024);
        assert_eq!(int_pow(3, 0), 1);
        assert_eq!(int_pow(7, 13), 96889010407);
        assert_eq!(int_pow(0, 5), 0);
    }

    #[test]
    fn intlog_basics() {
        assert_eq!(intlog2(0), 0);
        assert_eq!(intlog10(0), 0);
        assert_eq!(intlog2(1), 0);
        assert_eq!(intlog2(255), 134123008);
        assert_eq!(intlog10(255), 40375048);
        assert_eq!(intlog2(2), 1 << 24);
    }

    #[test]
    fn mul_div_u64_basics() {
        assert_eq!(mul_u64_add_u64_div_u64(3, 4, 5, 2), 8);
        assert_eq!(
            mul_u64_add_u64_div_u64(1 << 63, 1 << 63, 0, 1 << 63),
            1 << 63
        );
        assert_eq!(mul_u64_add_u64_div_u64(u64::MAX, 2, 0, 2), u64::MAX);
        assert_eq!(mul_u64_add_u64_div_u64(5, 7, 1, 0), u64::MAX);
    }

    #[test]
    fn iter_div_basics() {
        let mut rem = 0;
        assert_eq!(iter_div_u64_rem(100, 7, &mut rem), 14);
        assert_eq!(rem, 2);
        assert_eq!(iter_div_u64_rem(0, 3, &mut rem), 0);
        assert_eq!(rem, 0);
        assert_eq!(iter_div_u64_rem(0xFFFF_FFFE, 0x7FFF_FFFF, &mut rem), 2);
        assert_eq!(rem, 0);
    }

    #[test]
    fn rational_basics() {
        // Values observed from the recorded harness: pi ≈ 22/7.
        assert_eq!(rational_best_approximation(31415, 10000, 255, 31), (22, 7));
        assert_eq!(rational_best_approximation(1, 2, 1, 1), (0, 1));
        assert_eq!(rational_best_approximation(0, 5, 10, 10), (0, 1));
    }

    #[test]
    fn reciprocal_basics() {
        let r = reciprocal_value(7);
        assert_eq!((r.m, r.sh1, r.sh2), (0x24924925, 1, 2));
        let r = reciprocal_value(1);
        assert_eq!((r.m, r.sh1, r.sh2), (1, 0, 0));
        let adv = reciprocal_value_adv(7, 32);
        assert_eq!(adv.exp, 3);
    }

    #[test]
    fn cordic_basics() {
        // cos(45°) == sin(45°) ≈ 0.707 * 2^16 = 46341 (one-ulp table error ok).
        let c = cordic_calc_iq(45);
        assert_eq!(c, CordicIq { i: 46342, q: 46341 });
        let c = cordic_calc_iq(0);
        assert_eq!(c, CordicIq { i: 65541, q: 0 });
        let c = cordic_calc_iq(-45);
        assert_eq!(
            c,
            CordicIq {
                i: 46342,
                q: -46342
            }
        );
        let c = cordic_calc_iq(180);
        assert_eq!(c, CordicIq { i: -65541, q: 0 });
    }

    #[test]
    fn polynomial_basics() {
        // x^2 + 2x + 3 at x=4 = 27, with unit dividers.
        let terms = [
            PolynomialTerm {
                deg: 2,
                coef: 1,
                divider: 1,
                divider_leftover: 1,
            },
            PolynomialTerm {
                deg: 1,
                coef: 2,
                divider: 1,
                divider_leftover: 1,
            },
            PolynomialTerm {
                deg: 0,
                coef: 3,
                divider: 1,
                divider_leftover: 1,
            },
        ];
        assert_eq!(polynomial_calc(&terms, 1, 4), 27);
        // total_divider 0 resolves to 1 (the ?: contract).
        assert_eq!(polynomial_calc(&terms, 0, 4), 27);
    }

    #[test]
    fn primes_basics() {
        // Values observed from the recorded harness.
        assert!(is_prime_number(2));
        assert!(is_prime_number(7919));
        assert!(!is_prime_number(561)); // Carmichael
        assert!(!is_prime_number(0));
        assert!(!is_prime_number(1));
        assert_eq!(next_prime_number(1), 2);
        assert_eq!(next_prime_number(60), 61);
        assert_eq!(next_prime_number(63), 67); // past small_primes boundary
        assert_eq!(next_prime_number(65535), 65537);
        // sieve fallback (x beyond capacity): still correct, trial division.
        assert_eq!(next_prime_number(u64::MAX), u64::MAX); // sentinel
    }
}
