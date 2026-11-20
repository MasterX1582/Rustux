//! `klib::string_helpers` — Linux `lib/string_helpers.c` +
//! `include/linux/string_helpers.h`, ported faithfully and verified
//! against the C implementation (`ports/lib-strtox-helpers`,
//! 11667/11667 recorded differential cases).
//!
//! Destination buffers are caller-owned `&mut [u8]` windows: writes past
//! the window are dropped while the would-be count is still returned
//! exactly (C's `p - dst` semantics — the caller is expected to compare
//! the return against the window). `NULL` becomes `Option::None` where C
//! can produce it (`only` dictionaries, `kstrdup_*` sources).
//!
//! `kstrdup_*` allocate upstream; the model here is the verified port's:
//! the caller supplies the destination and `None` plays the NULL return
//! (NULL source or a `dst` too small for the escaped string + NUL). The
//! `*_inplace` unescape inlines are not expressible on disjoint slices —
//! callers copy first (in-place writes trail reads, so a copied source
//! is equivalent).

#![forbid(unsafe_code)]

use crate::seqbuf::hex_to_bin;
use crate::string::strlen;

// ---- enum string_size_units (include/linux/string_helpers.h) -----------

/// use powers of 10^3 (standard SI)
pub const STRING_UNITS_10: i32 = 0;
/// use binary powers of 2^10
pub const STRING_UNITS_2: i32 = 1;
pub const STRING_UNITS_MASK: i32 = 1;
pub const STRING_UNITS_NO_SPACE: i32 = 1 << 30;
/// BIT(31) — the sign bit; `units & STRING_UNITS_NO_BYTES` is the test.
pub const STRING_UNITS_NO_BYTES: i32 = 1 << 31;

/// `string_get_size` — Napier's algorithm, 3 significant figures.
/// `buf`/`len` model the `snprintf(buf, len, ...)` call at the end:
/// the return value is the would-be length, which may exceed `len`.
pub fn string_get_size(size: u64, blk_size: u64, units: i32, buf: &mut [u8], len: i32) -> i32 {
    const UNITS_10: [&[u8]; 9] = [b"", b"k", b"M", b"G", b"T", b"P", b"E", b"Z", b"Y"];
    const UNITS_2: [&[u8]; 9] = [b"", b"Ki", b"Mi", b"Gi", b"Ti", b"Pi", b"Ei", b"Zi", b"Yi"];
    const ROUNDING: [u32; 3] = [500, 50, 5];
    let units_base = (units & STRING_UNITS_MASK) as usize;
    let divisor: u64 = if units_base == 0 { 1000 } else { 1024 };
    let mut i = 0usize;
    let mut remainder = 0u32;
    let mut size = size;
    let mut blk_size = blk_size;
    // `char tmp[12]` in C holds ".%03u" truncated to j+1 chars; '.', then
    // up to 3 digits — 4 bytes is enough (j <= 3 always: sf_cap < divisor).
    let mut frac = [0u8; 4];
    let mut frac_len = 0usize;

    if blk_size == 0 {
        size = 0;
    }
    if size != 0 {
        while blk_size >> 32 != 0 {
            blk_size /= divisor;
            i += 1;
        }
        while size >> 32 != 0 {
            size /= divisor;
            i += 1;
        }
        size = size.wrapping_mul(blk_size);
        while size >= divisor {
            remainder = (size % divisor) as u32;
            size /= divisor;
            i += 1;
        }

        let mut sf_cap = size;
        let mut j = 0usize;
        while sf_cap * 10 < 1000 {
            sf_cap *= 10;
            j += 1;
        }
        if units_base == 1 {
            // express the remainder as a decimal (denominator is 1 << 10)
            remainder = remainder.wrapping_mul(1000);
            remainder >>= 10;
        }
        remainder = remainder.wrapping_add(ROUNDING[j]);
        if remainder >= 1000 {
            remainder -= 1000;
            size += 1;
        }
        if j != 0 {
            // snprintf(tmp, 12, ".%03u"); tmp[j+1] = '\0'
            frac[0] = b'.';
            frac[1] = b'0' + ((remainder / 100) % 10) as u8;
            frac[2] = b'0' + ((remainder / 10) % 10) as u8;
            frac[3] = b'0' + (remainder % 10) as u8;
            frac_len = j + 1;
        }
    }

    let unit: &[u8] = if i >= UNITS_2.len() {
        b"UNK"
    } else if units_base == 0 {
        UNITS_10[i]
    } else {
        UNITS_2[i]
    };

    // snprintf(buf, len, "%u%s%s%s%s") — worst case is
    // 10 + 4 + 1 + 3 + 1 = 19 bytes.
    let mut s = [0u8; 24];
    let mut p = push_u32(&mut s, size as u32);
    s[p..p + frac_len].copy_from_slice(&frac[..frac_len]);
    p += frac_len;
    if units & STRING_UNITS_NO_SPACE == 0 {
        s[p] = b' ';
        p += 1;
    }
    s[p..p + unit.len()].copy_from_slice(unit);
    p += unit.len();
    if units & STRING_UNITS_NO_BYTES == 0 {
        s[p] = b'B';
        p += 1;
    }
    let sb = &s[..p];

    // snprintf's `int len` converts to size_t: a negative len becomes a
    // size_t > PTRDIFF_MAX, for which the libc snprintf used by the
    // reference build has a quirk — it writes the formatted output minus
    // its last character plus a NUL (as if the size had been the needed
    // length). Positive sizes behave normally (write min(ret, size-1)
    // chars + NUL). Keep the quirk verbatim: it was part of the verified
    // differential.
    if len < 0 {
        let w = sb.len().saturating_sub(1).min(buf.len());
        buf[..w].copy_from_slice(&sb[..w]);
        if w < buf.len() {
            buf[w] = 0;
        }
    } else {
        snprintf_str(buf, len as usize, sb);
    }
    sb.len() as i32
}

/// "%u" of a u32 into `out` starting at byte 0; returns the end position.
fn push_u32(out: &mut [u8; 24], mut v: u32) -> usize {
    let mut rev = [0u8; 10];
    let mut n = 0usize;
    loop {
        rev[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let mut p = 0usize;
    while n != 0 {
        n -= 1;
        out[p] = rev[n];
        p += 1;
    }
    p
}

/// `snprintf()` into a fixed buffer: writes min(len, avail-1) bytes plus a
/// NUL (when it fits), returns the would-be length.
fn snprintf_str(dst: &mut [u8], avail: usize, s: &[u8]) -> usize {
    if avail == 0 {
        return s.len();
    }
    let w = s.len().min(avail - 1).min(dst.len());
    dst[..w].copy_from_slice(&s[..w]);
    if w < dst.len() {
        dst[w] = 0;
    }
    s.len()
}

/// Byte read that tolerates reaching the end of the slice: an absent byte
/// reads as the NUL tail (the verified port's `sbyte` model).
fn sbyte(s: &[u8], i: usize) -> u8 {
    if i < s.len() { s[i] } else { 0 }
}

/// `strchr(s, c)` on a NUL-terminated string; `strchr(s, 0)` finds the
/// terminator, so `c == 0` is always a hit.
fn cstr_contains(s: &[u8], c: u8) -> bool {
    for &b in s {
        if b == c {
            return true;
        }
        if b == 0 {
            return c == 0;
        }
    }
    c == 0
}

// ---- kernel ctype predicates (the `_ctype` mask sets) -------------------

/// kernel `isprint` — `_P|_U|_L|_D|_SP` covers 0x20-0x7e plus all of
/// 0xa0-0xff in the kernel table (0x80-0x9f are 0).
fn isprint(c: u8) -> bool {
    matches!(c, 0x20..=0x7e | 0xa0..=0xff)
}

fn isascii(c: u8) -> bool {
    c <= 0x7f
}

/// `isodigit` — the upstream inline is a plain range check.
fn isodigit(c: u8) -> bool {
    (b'0'..=b'7').contains(&c)
}

// ---- string_unescape (lib/string_helpers.c) ------------------------------

pub const UNESCAPE_SPACE: u32 = 1 << 0;
pub const UNESCAPE_OCTAL: u32 = 1 << 1;
pub const UNESCAPE_HEX: u32 = 1 << 2;
pub const UNESCAPE_SPECIAL: u32 = 1 << 3;
pub const UNESCAPE_ALL_MASK: u32 = 0xf;
pub const UNESCAPE_ANY: u32 = UNESCAPE_SPACE | UNESCAPE_OCTAL | UNESCAPE_HEX | UNESCAPE_SPECIAL;

fn unescape_space(src: &[u8], si: &mut usize, dst: &mut [u8], di: &mut usize) -> bool {
    let b = match sbyte(src, *si) {
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'v' => 0x0b,
        b'f' => 0x0c,
        _ => return false,
    };
    if *di < dst.len() {
        dst[*di] = b;
    }
    *di += 1;
    *si += 1;
    true
}

fn unescape_octal(src: &[u8], si: &mut usize, dst: &mut [u8], di: &mut usize) -> bool {
    let start = *si;
    let mut q = *si;
    if !isodigit(sbyte(src, q)) {
        return false;
    }
    let mut num = sbyte(src, q) & 7;
    q += 1;
    while num < 32 && isodigit(sbyte(src, q)) && q - start < 3 {
        num = (num << 3).wrapping_add(sbyte(src, q) & 7);
        q += 1;
    }
    if *di < dst.len() {
        dst[*di] = num;
    }
    *di += 1;
    *si = q;
    true
}

fn unescape_hex(src: &[u8], si: &mut usize, dst: &mut [u8], di: &mut usize) -> bool {
    let mut q = *si;
    if sbyte(src, q) != b'x' {
        return false;
    }
    q += 1;
    let d1 = hex_to_bin(sbyte(src, q));
    q += 1;
    if d1 < 0 {
        return false;
    }
    let mut num = d1 as u8;
    let d2 = hex_to_bin(sbyte(src, q));
    if d2 >= 0 {
        q += 1;
        num = (num << 4) | (d2 as u8);
    }
    if *di < dst.len() {
        dst[*di] = num;
    }
    *di += 1;
    *si = q;
    true
}

fn unescape_special(src: &[u8], si: &mut usize, dst: &mut [u8], di: &mut usize) -> bool {
    let b = match sbyte(src, *si) {
        b'"' => b'"',
        b'\\' => b'\\',
        b'a' => 0x07,
        b'e' => 0x1b,
        _ => return false,
    };
    if *di < dst.len() {
        dst[*di] = b;
    }
    *di += 1;
    *si += 1;
    true
}

/// `string_unescape` — unquote `\` escapes per `flags`. `size` is the C
/// destination bound (0 = unlimit); `dst` bounds the actual writes and the
/// NUL terminator (the count is always the exact `out - dst` upstream
/// would produce). Returns bytes processed excluding the trailing NUL.
pub fn string_unescape(src: &[u8], dst: &mut [u8], size: usize, flags: u32) -> usize {
    let mut out = 0usize;
    let mut size = if size == 0 { usize::MAX } else { size };
    let mut si = 0usize;

    loop {
        if sbyte(src, si) == 0 {
            break;
        }
        size -= 1;
        if size == 0 {
            break;
        }
        if sbyte(src, si) == b'\\' && sbyte(src, si + 1) != 0 && size > 1 {
            si += 1;
            size -= 1;

            if flags & UNESCAPE_SPACE != 0 && unescape_space(src, &mut si, dst, &mut out) {
                continue;
            }
            if flags & UNESCAPE_OCTAL != 0 && unescape_octal(src, &mut si, dst, &mut out) {
                continue;
            }
            if flags & UNESCAPE_HEX != 0 && unescape_hex(src, &mut si, dst, &mut out) {
                continue;
            }
            if flags & UNESCAPE_SPECIAL != 0 && unescape_special(src, &mut si, dst, &mut out) {
                continue;
            }
            if out < dst.len() {
                dst[out] = b'\\';
            }
            out += 1;
        }
        if out < dst.len() {
            dst[out] = sbyte(src, si);
        }
        out += 1;
        si += 1;
    }
    if out < dst.len() {
        dst[out] = 0;
    }
    out
}

/// `string_unescape_any` — all escape classes.
pub fn string_unescape_any(src: &[u8], dst: &mut [u8], size: usize) -> usize {
    string_unescape(src, dst, size, UNESCAPE_ANY)
}

// ---- string_escape_* (lib/string_helpers.c) ------------------------------

pub const ESCAPE_SPACE: u32 = 1 << 0;
pub const ESCAPE_SPECIAL: u32 = 1 << 1;
pub const ESCAPE_NULL: u32 = 1 << 2;
pub const ESCAPE_OCTAL: u32 = 1 << 3;
pub const ESCAPE_ANY: u32 = ESCAPE_SPACE | ESCAPE_SPECIAL | ESCAPE_NULL | ESCAPE_OCTAL;
pub const ESCAPE_NP: u32 = 1 << 4;
pub const ESCAPE_ANY_NP: u32 = ESCAPE_ANY | ESCAPE_NP;
pub const ESCAPE_HEX: u32 = 1 << 5;
pub const ESCAPE_NA: u32 = 1 << 6;
pub const ESCAPE_NAP: u32 = 1 << 7;
pub const ESCAPE_APPEND: u32 = 1 << 8;
pub const ESCAPE_ALL_MASK: u32 = 0x1ff;

const HEX_ASC: &[u8; 16] = b"0123456789abcdef";

/// `escape_passthrough` and friends: emit into `dst` while `pos < dst.len()`
/// (`p < end` upstream), counting always.
fn esc_emit(dst: &mut [u8], pos: &mut usize, b: u8) {
    if *pos < dst.len() {
        dst[*pos] = b;
    }
    *pos += 1;
}

fn escape_passthrough(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    esc_emit(dst, pos, c);
    true
}

fn escape_space(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    let to = match c {
        b'\n' => b'n',
        b'\r' => b'r',
        b'\t' => b't',
        0x0b => b'v',
        0x0c => b'f',
        _ => return false,
    };
    esc_emit(dst, pos, b'\\');
    esc_emit(dst, pos, to);
    true
}

fn escape_special(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    let to = match c {
        b'\\' => b'\\',
        0x07 => b'a',
        0x1b => b'e',
        b'"' => b'"',
        _ => return false,
    };
    esc_emit(dst, pos, b'\\');
    esc_emit(dst, pos, to);
    true
}

fn escape_null(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    if c != 0 {
        return false;
    }
    esc_emit(dst, pos, b'\\');
    esc_emit(dst, pos, b'0');
    true
}

fn escape_octal(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    esc_emit(dst, pos, b'\\');
    esc_emit(dst, pos, b'0' + ((c >> 6) & 0x07));
    esc_emit(dst, pos, b'0' + ((c >> 3) & 0x07));
    esc_emit(dst, pos, b'0' + (c & 0x07));
    true
}

fn escape_hex(c: u8, dst: &mut [u8], pos: &mut usize) -> bool {
    esc_emit(dst, pos, b'\\');
    esc_emit(dst, pos, b'x');
    esc_emit(dst, pos, HEX_ASC[(c >> 4) as usize]);
    esc_emit(dst, pos, HEX_ASC[(c & 0x0f) as usize]);
    true
}

/// `string_escape_mem` — escape `isz` bytes of `src` per `flags`/`only`
/// into `dst` (the `osz` window). The return is always the total size the
/// output would take; truncation shows as `ret > dst.len()`. `only` is the
/// optional NUL-terminated dictionary (`None` = NULL).
pub fn string_escape_mem(
    src: &[u8],
    isz: usize,
    dst: &mut [u8],
    flags: u32,
    only: Option<&[u8]>,
) -> usize {
    let mut pos = 0usize;
    let is_dict = only.is_some_and(|o| !o.is_empty());
    let is_append = flags & ESCAPE_APPEND != 0;
    let only_b = only.unwrap_or(&[]);
    let mut isz = isz;
    let mut i = 0usize;

    while isz > 0 {
        isz -= 1;
        let c = src[i];
        i += 1;
        let in_dict = is_dict && cstr_contains(only_b, c);

        // - `only` supplied and does not contain c -> passthrough
        if !(is_append || in_dict) && is_dict && escape_passthrough(c, dst, &mut pos) {
            continue;
        }
        // - printable and ASCII, when ESCAPE_NAP
        if !(is_append && in_dict)
            && isascii(c)
            && isprint(c)
            && flags & ESCAPE_NAP != 0
            && escape_passthrough(c, dst, &mut pos)
        {
            continue;
        }
        // - printable, when ESCAPE_NP
        if !(is_append && in_dict)
            && isprint(c)
            && flags & ESCAPE_NP != 0
            && escape_passthrough(c, dst, &mut pos)
        {
            continue;
        }
        // - ASCII, when ESCAPE_NA
        if !(is_append && in_dict)
            && isascii(c)
            && flags & ESCAPE_NA != 0
            && escape_passthrough(c, dst, &mut pos)
        {
            continue;
        }
        if flags & ESCAPE_SPACE != 0 && escape_space(c, dst, &mut pos) {
            continue;
        }
        if flags & ESCAPE_SPECIAL != 0 && escape_special(c, dst, &mut pos) {
            continue;
        }
        if flags & ESCAPE_NULL != 0 && escape_null(c, dst, &mut pos) {
            continue;
        }
        // ESCAPE_OCTAL and ESCAPE_HEX always go last
        if flags & ESCAPE_OCTAL != 0 && escape_octal(c, dst, &mut pos) {
            continue;
        }
        if flags & ESCAPE_HEX != 0 && escape_hex(c, dst, &mut pos) {
            continue;
        }
        escape_passthrough(c, dst, &mut pos);
    }
    pos
}

/// `string_escape_mem_any_np` — the header inline: ESCAPE_ANY_NP.
pub fn string_escape_mem_any_np(
    src: &[u8],
    isz: usize,
    dst: &mut [u8],
    only: Option<&[u8]>,
) -> usize {
    string_escape_mem(src, isz, dst, ESCAPE_ANY_NP, only)
}

/// `string_escape_str` — `string_escape_mem(src, strlen(src), ...)`.
pub fn string_escape_str(src: &[u8], dst: &mut [u8], flags: u32, only: Option<&[u8]>) -> usize {
    string_escape_mem(src, strlen(src), dst, flags, only)
}

/// `string_escape_str_any_np` — the header inline.
pub fn string_escape_str_any_np(src: &[u8], dst: &mut [u8], only: Option<&[u8]>) -> usize {
    string_escape_str(src, dst, ESCAPE_ANY_NP, only)
}

// ---- kstrdup_* (lib/string_helpers.c) -------------------------------------

/// `kstrdup_quotable` — escapes `src` with ESCAPE_HEX over the
/// `"\f\n\r\t\v\a\e\\\""` dictionary so it is safe to log in quotes.
/// `dst` replaces the kmalloc: returns `Some(len)` with `dst[..len]` plus
/// a NUL written, or `None` for a NULL source / too-small `dst` (the two
/// upstream NULL cases).
pub fn kstrdup_quotable(src: Option<&[u8]>, dst: &mut [u8]) -> Option<usize> {
    const ESC: &[u8] = b"\x0c\n\r\t\x0b\x07\x1b\\\"";
    let src = src?;
    let sl = strlen(src);
    let dlen = string_escape_mem(&src[..sl], sl, &mut [], ESCAPE_HEX, Some(ESC));
    if dst.len() < dlen + 1 {
        return None;
    }
    let n = string_escape_mem(&src[..sl], sl, &mut dst[..dlen], ESCAPE_HEX, Some(ESC));
    debug_assert_eq!(n, dlen); // WARN_ON upstream
    dst[dlen] = 0;
    Some(dlen)
}

/// `kstrdup_and_replace` — duplicate `src` (incl. the NUL) into `dst`,
/// then `strreplace` every `old` with `new`. Same `Option` model as
/// [`kstrdup_quotable`]: `None` is the NULL result.
pub fn kstrdup_and_replace(src: Option<&[u8]>, dst: &mut [u8], old: u8, new: u8) -> Option<usize> {
    let src = src?;
    let sl = strlen(src);
    if dst.len() < sl + 1 {
        return None;
    }
    dst[..sl + 1].copy_from_slice(&src[..sl + 1]);
    crate::string::strreplace(&mut dst[..sl + 1], old, new);
    Some(sl)
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("string_helpers_tests.inc");
}
