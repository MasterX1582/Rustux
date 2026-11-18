//! The Rust port of `div64.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

/// `BITS_PER_ITER` on LP64 (`__LONG_WIDTH__ >= 64`).
const BITS_PER_ITER: u32 = 32;

/// Port of the `static inline uint64_t mul_u64_u64_add_u64(uint64_t *p_lo, ...)`
/// helper (`div64.c:51`): native 64x64=128 bits multiplication.
fn mul_u64_u64_add_u64(p_lo: &mut u64, a: u64, b: u64, c: u64) -> u64 {
    let prod = a as u128 * b as u128 + c as u128;

    *p_lo = prod as u64;
    (prod >> 64) as u64
}

/// Port of `u32 iter_div_u64_rem(u64 dividend, u32 divisor, u64 *remainder)`
/// (`div64.c:42`), i.e. `<vdso/math64.h>`'s `__iter_div_u64_rem` loop.
///
/// The pointer signature keeps this function out of rust-anything's ctypes
/// bridge (`js_only`); it is verified by `ports/lib-math/check-iter-div.py`
/// instead. Faithful to the C: `divisor == 0` loops forever, exactly like the
/// original's `while (dividend >= divisor)` with `OPTIMIZER_HIDE_VAR`.
pub fn iter_div_u64_rem(dividend: u64, divisor: u32, remainder: &mut u64) -> u32 {
    let mut ret: u32 = 0;
    let mut dividend = dividend;
    let divisor = divisor as u64;

    while dividend >= divisor {
        // OPTIMIZER_HIDE_VAR(dividend): the C defeats strength reduction of the
        // subtraction loop; the port keeps the plain loop, which Rust cannot
        // strength-reduce to a modulo either way.
        dividend -= divisor;
        ret = ret.wrapping_add(1);
    }

    *remainder = dividend;
    ret
}

/// Port of `uint64_t mul_u64_add_u64_div_u64(uint64_t a, uint64_t b, uint64_t c, uint64_t d)`
/// (`div64.c:67`): computes `(a * b + c) / d` with 128-bit intermediate precision.
pub fn mul_u64_add_u64_div_u64(a: u64, b: u64, c: u64, d: u64) -> u64 {
    let mut n_lo: u64 = 0;
    let mut n_hi = mul_u64_u64_add_u64(&mut n_lo, a, b, c);
    let mut d = d;

    if n_hi == 0 {
        // div64_u64(n_lo, d): C traps on d == 0; the port cannot fault, so it
        // reports the same "unrepresentable" saturating value as overflow.
        return n_lo.checked_div(d).unwrap_or(u64::MAX);
    }

    if n_hi >= d {
        // d == 0: C deliberately traps (~0UL/zero). Unrepresentable either way.
        return u64::MAX;
    }

    // Left align the divisor, shifting the dividend to match.
    let d_z_hi = d.leading_zeros();
    if d_z_hi != 0 {
        d <<= d_z_hi;
        n_hi = (n_hi << d_z_hi) | (n_lo >> (64 - d_z_hi));
        n_lo <<= d_z_hi;
    }

    let mut reps: u32 = 64 / BITS_PER_ITER;
    // Optimise loop count for small dividends.
    if (n_hi >> 32) as u32 == 0 {
        reps -= 32 / BITS_PER_ITER;
        n_hi = (n_hi << 32) | (n_lo >> 32);
        n_lo <<= 32;
    }

    // Invert the dividend so we can use add instead of subtract.
    n_lo = !n_lo;
    n_hi = !n_hi;

    // Most significant BITS_PER_ITER bits of the divisor, for the low "guestimate".
    let d_msig: u64 = (d >> (64 - BITS_PER_ITER)).wrapping_add(1);

    let mut quotient: u64 = 0;
    while reps != 0 {
        reps -= 1;
        // 64 - 2 * BITS_PER_ITER == 0 on LP64: q_digit = ~n_hi / d_msig
        let mut q_digit: u64 = (!n_hi >> (64 - 2 * BITS_PER_ITER)) / d_msig;
        // Shift 'n' left to align with the product q_digit * d.
        let mut overflow: u32 = (n_hi >> (64 - BITS_PER_ITER)) as u32;
        n_hi = (n_hi << BITS_PER_ITER).wrapping_add(n_lo >> (64 - BITS_PER_ITER));
        n_lo <<= BITS_PER_ITER;
        // Add product to negated dividend (mul_u64_long_add_u64 == mul_u64_u64_add_u64).
        let n = n_hi;
        overflow = overflow.wrapping_add(mul_u64_u64_add_u64(&mut n_hi, d, q_digit, n) as u32);
        // Adjust for the q_digit 'guestimate' being low.
        while overflow < (0xffff_ffffu32 >> (32 - BITS_PER_ITER)) {
            q_digit = q_digit.wrapping_add(1);
            n_hi = n_hi.wrapping_add(d);
            overflow = overflow.wrapping_add((n_hi < d) as u32);
        }
        quotient = (quotient << BITS_PER_ITER).wrapping_add(q_digit);
    }

    // The remainder may still admit one more copy of the divisor.
    if n_hi.wrapping_add(d) > n_hi {
        quotient = quotient.wrapping_add(1);
    }
    quotient
}
