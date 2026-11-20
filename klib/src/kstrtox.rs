//! `klib::kstrtox` — Linux `lib/kstrtox.c` + `lib/kstrtox.h`, ported
//! faithfully and verified against the C implementation
//! (`ports/lib-strtox-helpers`, 11667/11667 recorded differential cases).
//!
//! C strings become `&[u8]` slices. Byte reads past the slice end return 0
//! (the verified port's `sbyte` model — reaching the end reads like the
//! NUL tail), so a non-terminated input parses as if terminated at the
//! slice end rather than panicking; in-contract callers still pass a
//! NUL-terminated string inside the slice, like `klib::string`.
//!
//! `int` return codes become `Result<_, i32>`: `Ok` carries the converted
//! value (C's `*res` out-param), `Err` carries the negative errno
//! (`-EINVAL` / `-ERANGE` / `-EFAULT`). `unsigned long long` is `u64` and
//! `long long` is `i64`; `long`/`unsigned long` are LP64 (`i64`/`u64`), so
//! the `_kstrtoul`/`_kstrtol` narrowing tests are vacuous exactly like the
//! x86-64 reference build.

#![forbid(unsafe_code)]

use crate::math::int_pow;

/// `_parse_integer_limit`'s overflow bit, ORed into the returned count.
pub const KSTRTOX_OVERFLOW: u32 = 1 << 31;
pub const EINVAL: i32 = 22;
pub const ERANGE: i32 = 34;
pub const EFAULT: i32 = 14;

/// Byte read of a C string that tolerates reaching the end of the slice:
/// an absent byte reads as the NUL tail.
fn sbyte(s: &[u8], i: usize) -> u8 {
    if i < s.len() { s[i] } else { 0 }
}

/// The kernel's isdigit is the plain range check; `unsigned int c = *s`
/// sign-extends `char` on the reference build, so bytes >= 0x80 (which
/// sign-extend negative) are not digits either way.
fn isdigit(c: i32) -> bool {
    (0x30..=0x39).contains(&c)
}

/// `_tolower()` — the unconditional bit-5 set (NOT table-based tolower).
fn lower_fast(c: u8) -> u8 {
    c | 0x20
}

/// kernel `isxdigit` — the `_ctype` `_D|_X` set is exactly ASCII hex digits.
fn isxdigit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

/// `_parse_integer_fixup_radix`: adjusts `base` for `0`/`0x` prefixes and
/// returns the number of chars consumed (0 or 2).
pub fn _parse_integer_fixup_radix(s: &[u8], base: &mut u32) -> usize {
    if *base == 0 {
        if sbyte(s, 0) == b'0' {
            if lower_fast(sbyte(s, 1)) == b'x' && isxdigit(sbyte(s, 2)) {
                *base = 16;
            } else {
                *base = 8;
            }
        } else {
            *base = 10;
        }
    }
    if *base == 16 && sbyte(s, 0) == b'0' && lower_fast(sbyte(s, 1)) == b'x' {
        2
    } else {
        0
    }
}

/// `_parse_integer_limit`: digit-accumulate in `base` up to `max_chars`,
/// starting from `init`. Returns `(count | KSTRTOX_OVERFLOW, result)` —
/// the overflow bit is set when the accumulation exceeds u64.
pub fn _parse_integer_limit(s: &[u8], base: u32, max_chars: u64, init: u64) -> (u32, u64) {
    let mut res = init;
    let mut overflow = 0u32;
    let mut rv = 0u32;
    while (rv as u64) < max_chars {
        // `unsigned int c = *s` on a signed-char target sign-extends.
        let c = sbyte(s, rv as usize) as i8 as i32;
        let lc = c | 0x20; // _tolower()
        let val = if isdigit(c) {
            (c - 0x30) as u32
        } else if (0x61..=0x66).contains(&lc) {
            (lc - 0x61 + 10) as u32
        } else {
            break;
        };
        if val >= base {
            break;
        }
        // Check for overflow only when within range of it in the max base.
        if res & (u64::MAX << 60) != 0 {
            let (m, o1) = res.overflowing_mul(base as u64);
            let (a, o2) = m.overflowing_add(val as u64);
            res = a;
            if o1 || o2 {
                res = u64::MAX;
                overflow = KSTRTOX_OVERFLOW;
            }
        } else {
            res = res.wrapping_mul(base as u64).wrapping_add(val as u64);
        }
        rv += 1;
    }
    (rv | overflow, res)
}

/// `_parse_integer(s, base, res)` == `_parse_integer_limit(s, base, res, INT_MAX, 0)`.
pub fn _parse_integer(s: &[u8], base: u32) -> (u32, u64) {
    _parse_integer_limit(s, base, i32::MAX as u64, 0)
}

/// `_kstrtoull` — the shared core behind kstrtoull/kstrtoll.
fn kstrtoull_core(s: &[u8], base: u32) -> Result<u64, i32> {
    let mut base = base;
    let off = _parse_integer_fixup_radix(s, &mut base);
    let s = &s[off.min(s.len())..];
    let (rv, res) = _parse_integer(s, base);
    if rv & KSTRTOX_OVERFLOW != 0 {
        return Err(-ERANGE);
    }
    if rv == 0 {
        return Err(-EINVAL);
    }
    let mut i = rv as usize;
    if sbyte(s, i) == b'\n' {
        i += 1;
    }
    if sbyte(s, i) != 0 {
        return Err(-EINVAL);
    }
    Ok(res)
}

/// `kstrtoull` — a single optional `+`, no `-`; trailing `\n` allowed once.
pub fn kstrtoull(s: &[u8], base: u32) -> Result<u64, i32> {
    if sbyte(s, 0) == b'+' {
        return kstrtoull_core(&s[1..], base);
    }
    kstrtoull_core(s, base)
}

/// `kstrtoll` — accepts `-`; negated overflow maps to `-ERANGE`.
pub fn kstrtoll(s: &[u8], base: u32) -> Result<i64, i32> {
    if sbyte(s, 0) == b'-' {
        let tmp = kstrtoull_core(&s[1..], base)?;
        if (tmp.wrapping_neg() as i64) > 0 {
            return Err(-ERANGE);
        }
        Ok(tmp.wrapping_neg() as i64)
    } else {
        let tmp = kstrtoull(s, base)?;
        if (tmp as i64) < 0 {
            return Err(-ERANGE);
        }
        Ok(tmp as i64)
    }
}

/// `_kstrtoul`/`kstrtoul` on LP64: `unsigned long` is u64, so the C
/// narrowing test `tmp != (unsigned long)tmp` is vacuous and the function
/// is identical to kstrtoull — like the reference build.
pub fn kstrtoul(s: &[u8], base: u32) -> Result<u64, i32> {
    kstrtoull(s, base)
}

/// `_kstrtol`/`kstrtol` on LP64: `long` is i64 — identical to kstrtoll.
pub fn kstrtol(s: &[u8], base: u32) -> Result<i64, i32> {
    kstrtoll(s, base)
}

/// The shared body of kstrtou16/kstrtou8/kstrtouint/kstrtou32:
/// `tmp != (type)tmp` for an unsigned `type` is `tmp != tmp & MAX`.
fn kstrto_unsigned(s: &[u8], base: u32, max: u64) -> Result<u64, i32> {
    let tmp = kstrtoull(s, base)?;
    if tmp != (tmp & max) {
        return Err(-ERANGE);
    }
    Ok(tmp)
}

/// The shared body of kstrtos16/kstrtos8/kstrtoint/kstrtos32:
/// `tmp != (type)tmp` for a signed `type` is `tmp < lo || tmp > hi`.
fn kstrto_signed(s: &[u8], base: u32, lo: i64, hi: i64) -> Result<i64, i32> {
    let tmp = kstrtoll(s, base)?;
    if tmp < lo || tmp > hi {
        return Err(-ERANGE);
    }
    Ok(tmp)
}

pub fn kstrtouint(s: &[u8], base: u32) -> Result<u32, i32> {
    kstrto_unsigned(s, base, u32::MAX as u64).map(|v| v as u32)
}

pub fn kstrtoint(s: &[u8], base: u32) -> Result<i32, i32> {
    kstrto_signed(s, base, i32::MIN as i64, i32::MAX as i64).map(|v| v as i32)
}

pub fn kstrtou16(s: &[u8], base: u32) -> Result<u16, i32> {
    kstrto_unsigned(s, base, u16::MAX as u64).map(|v| v as u16)
}

pub fn kstrtos16(s: &[u8], base: u32) -> Result<i16, i32> {
    kstrto_signed(s, base, i16::MIN as i64, i16::MAX as i64).map(|v| v as i16)
}

pub fn kstrtou8(s: &[u8], base: u32) -> Result<u8, i32> {
    kstrto_unsigned(s, base, u8::MAX as u64).map(|v| v as u8)
}

pub fn kstrtos8(s: &[u8], base: u32) -> Result<i8, i32> {
    kstrto_signed(s, base, i8::MIN as i64, i8::MAX as i64).map(|v| v as i8)
}

/// `kstrtou64` — upstream macro alias of kstrtoull.
pub fn kstrtou64(s: &[u8], base: u32) -> Result<u64, i32> {
    kstrtoull(s, base)
}

/// `kstrtos64` — upstream macro alias of kstrtoll.
pub fn kstrtos64(s: &[u8], base: u32) -> Result<i64, i32> {
    kstrtoll(s, base)
}

/// `kstrtou32` — upstream macro alias of kstrtouint.
pub fn kstrtou32(s: &[u8], base: u32) -> Result<u32, i32> {
    kstrtouint(s, base)
}

/// `kstrtos32` — upstream macro alias of kstrtoint.
pub fn kstrtos32(s: &[u8], base: u32) -> Result<i32, i32> {
    kstrtoint(s, base)
}

/// `kstrtobool` — first char decides: `y/Y/t/T/1/e/E` true, `n/N/f/F/0/d/D`
/// false, `on/off` (either case) long forms, everything else -EINVAL.
/// `None` is the NULL input upstream rejects with -EINVAL.
pub fn kstrtobool(s: Option<&[u8]>) -> Result<bool, i32> {
    let s = match s {
        None => return Err(-EINVAL),
        Some(s) => s,
    };
    match sbyte(s, 0) {
        b'e' | b'E' | b'y' | b'Y' | b't' | b'T' | b'1' => Ok(true),
        b'd' | b'D' | b'n' | b'N' | b'f' | b'F' | b'0' => Ok(false),
        b'o' | b'O' => match sbyte(s, 1) {
            b'n' | b'N' => Ok(true),
            b'f' | b'F' => Ok(false),
            _ => Err(-EINVAL),
        },
        _ => Err(-EINVAL),
    }
}

/// `_kstrtoudec64` — scaled-decimal core behind kstrtoudec64/kstrtodec64.
fn kstrtoudec64_core(s: &[u8], scale: u32) -> Result<u64, i32> {
    let (rv_int, mut res) = _parse_integer(s, 10);
    if rv_int & KSTRTOX_OVERFLOW != 0 {
        return Err(-ERANGE);
    }
    let mut i = rv_int as usize;
    if sbyte(s, i) == b'.' {
        i += 1;
    }
    let (rv_frac, r2) = _parse_integer_limit(&s[i.min(s.len())..], 10, scale as u64, res);
    if rv_frac & KSTRTOX_OVERFLOW != 0 {
        return Err(-ERANGE);
    }
    res = r2;
    i += rv_frac as usize;

    // Cover cases like ".5" with scale 0, parsed as 0.
    if rv_int == 0 && rv_frac == 0 && !isdigit(sbyte(s, i) as i8 as i32) {
        return Err(-EINVAL);
    }
    while isdigit(sbyte(s, i) as i8 as i32) {
        i += 1;
    }
    if sbyte(s, i) == b'\n' {
        i += 1;
    }
    if sbyte(s, i) != 0 {
        return Err(-EINVAL);
    }

    if res != 0 {
        // `scale - rv_frac` is unsigned-int arithmetic: rv_frac > scale wraps.
        let diff = scale.wrapping_sub(rv_frac);
        if diff > 19 {
            return Err(-ERANGE);
        }
        let (m, o) = res.overflowing_mul(int_pow(10, diff));
        if o {
            return Err(-ERANGE);
        }
        res = m;
    }
    Ok(res)
}

/// `kstrtoudec64` — `+` allowed, `-` not; digits past `scale` are truncated.
pub fn kstrtoudec64(s: &[u8], scale: u32) -> Result<u64, i32> {
    if sbyte(s, 0) == b'+' {
        return kstrtoudec64_core(&s[1..], scale);
    }
    kstrtoudec64_core(s, scale)
}

/// `kstrtodec64` — the signed variant; `-` mirrors kstrtoll's negation.
pub fn kstrtodec64(s: &[u8], scale: u32) -> Result<i64, i32> {
    if sbyte(s, 0) == b'-' {
        let tmp = kstrtoudec64_core(&s[1..], scale)?;
        if (tmp.wrapping_neg() as i64) > 0 {
            return Err(-ERANGE);
        }
        Ok(tmp.wrapping_neg() as i64)
    } else {
        let tmp = kstrtoudec64(s, scale)?;
        if (tmp as i64) < 0 {
            return Err(-ERANGE);
        }
        Ok(tmp as i64)
    }
}

/// The `kstrto_from_user` shared body: `char buf[1 + bits + 1 + 1]`
/// (`buf[4]` for kstrtobool). `s` models the accessible user window:
/// `None` is a NULL user pointer, and a (capped) `count` larger than the
/// window both map to `copy_from_user`'s -EFAULT.
fn kstrto_from_user_buf<const N: usize>(s: Option<&[u8]>, count: usize) -> Result<[u8; N], i32> {
    let s = s.ok_or(-EFAULT)?;
    let n = count.min(N - 1);
    if n > s.len() {
        return Err(-EFAULT);
    }
    let mut buf = [0u8; N];
    buf[..n].copy_from_slice(&s[..n]);
    Ok(buf) // buf[n] = '\0' already
}

/// `kstrtoull_from_user` — buf is `1 + 64 + 1 + 1` bytes upstream.
pub fn kstrtoull_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<u64, i32> {
    kstrtoull(&kstrto_from_user_buf::<67>(s, count)?, base)
}

/// `kstrtoll_from_user` — buf is `1 + 64 + 1 + 1` bytes upstream.
pub fn kstrtoll_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<i64, i32> {
    kstrtoll(&kstrto_from_user_buf::<67>(s, count)?, base)
}

/// `kstrtoul_from_user` — buf is `1 + 64 + 1 + 1` bytes upstream (LP64).
pub fn kstrtoul_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<u64, i32> {
    kstrtoul(&kstrto_from_user_buf::<67>(s, count)?, base)
}

/// `kstrtol_from_user` — buf is `1 + 64 + 1 + 1` bytes upstream (LP64).
pub fn kstrtol_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<i64, i32> {
    kstrtol(&kstrto_from_user_buf::<67>(s, count)?, base)
}

/// `kstrtouint_from_user` — buf is `1 + 32 + 1 + 1` bytes upstream.
pub fn kstrtouint_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<u32, i32> {
    kstrtouint(&kstrto_from_user_buf::<35>(s, count)?, base)
}

/// `kstrtoint_from_user` — buf is `1 + 32 + 1 + 1` bytes upstream.
pub fn kstrtoint_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<i32, i32> {
    kstrtoint(&kstrto_from_user_buf::<35>(s, count)?, base)
}

/// `kstrtou16_from_user` — buf is `1 + 16 + 1 + 1` bytes upstream.
pub fn kstrtou16_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<u16, i32> {
    kstrtou16(&kstrto_from_user_buf::<19>(s, count)?, base)
}

/// `kstrtos16_from_user` — buf is `1 + 16 + 1 + 1` bytes upstream.
pub fn kstrtos16_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<i16, i32> {
    kstrtos16(&kstrto_from_user_buf::<19>(s, count)?, base)
}

/// `kstrtou8_from_user` — buf is `1 + 8 + 1 + 1` bytes upstream.
pub fn kstrtou8_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<u8, i32> {
    kstrtou8(&kstrto_from_user_buf::<11>(s, count)?, base)
}

/// `kstrtos8_from_user` — buf is `1 + 8 + 1 + 1` bytes upstream.
pub fn kstrtos8_from_user(s: Option<&[u8]>, count: usize, base: u32) -> Result<i8, i32> {
    kstrtos8(&kstrto_from_user_buf::<11>(s, count)?, base)
}

/// `kstrtobool_from_user` — open-coded `char buf[4]` upstream (no base).
pub fn kstrtobool_from_user(s: Option<&[u8]>, count: usize) -> Result<bool, i32> {
    kstrtobool(Some(&kstrto_from_user_buf::<4>(s, count)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("kstrtox_tests.inc");
}
