//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Mirrors of lib/kstrtox.c, lib/hexdump.c and lib/string_helpers.c on an
//! x86-64 LP64 target (unsigned long = u64, long = i64, little-endian) plus
//! the seeded-buffer harness.  Pointer-returning C functions return byte
//! offsets into their buffer; `NULL` becomes `Option::None` where the C code
//! can produce it.

#![forbid(unsafe_code)]
#![allow(non_snake_case)] // `wrap__kstrtoul` / `wrap__kstrtol` keep the C spelling

// ---- harness mirror ----------------------------------------------------------

const SSZ: usize = 640; // string buffers: body bytes + forced NUL + tail
const DSZ: usize = 1280; // destination buffer for mutating ops
const HBUF: usize = 96; // hexdump source blob (3 full 32-byte rows)
const LB: usize = 256; // hex_dump_to_buffer linebuf
const KLOG: usize = 8192; // captured printk output
const NSTR: usize = 128; // kstrtox input string
const PCANON: u64 = 0xffff888000000000; // canonical base for %p

fn xs64(rng: &mut u64) -> u64 {
    let mut x = *rng;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *rng = x;
    x.wrapping_mul(2685821657736338717)
}

fn blen(v: i64, cap: usize) -> usize {
    (v.unsigned_abs() % cap as u64) as usize
}

const FNV_INIT: u64 = 14695981039346656037;
const FNV_PRIME: u64 = 1099511628211;

fn fnv1a(b: &[u8]) -> u64 {
    let mut h = FNV_INIT;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Result mixer: every recorded output depends on all three inputs.
fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut h = a.wrapping_mul(0x9E3779B97F4A7C15);
    h = (h ^ b).wrapping_mul(0xC2B2AE3D27D4EB4F);
    h = (h ^ c).wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 32;
    h
}

/// Deterministic raw bytes (may contain zeros).
fn fill_raw(b: &mut [u8], seed: u64) {
    let mut r = seed | 1;
    for x in b.iter_mut() {
        *x = xs64(&mut r) as u8;
    }
}

/// Build a string in the first SSZ bytes of b: n non-zero body bytes + a
/// forced NUL at n, over a deterministic raw tail.  mode&1 plants an extra
/// NUL at a seeded position inside the body; mode&2 fills with 'A'+x%58
/// (letter-ish bytes) instead of x|1.
///
/// Returns the residual xorshift64 state; C keeps it in a file-static and
/// `wrap_strreplace` draws the `old`-plant positions from it.
fn make_str(b: &mut [u8], n: usize, seed: u64, mode: i64) -> u64 {
    fill_raw(&mut b[..SSZ], seed);
    let mut r = (seed ^ 0x5bd1e9959e3779b9) | 1;
    for x in b[..n].iter_mut() {
        let v = xs64(&mut r);
        *x = if mode & 2 != 0 {
            (b'A' as u64 + v % 58) as u8
        } else {
            (v | 1) as u8
        };
    }
    if mode & 1 != 0 && n != 0 {
        let pos = blen(xs64(&mut r) as i64, n);
        b[pos] = 0;
    }
    b[n] = 0;
    r
}

/// `strlen()`: offset of the first NUL.
fn cstrlen(b: &[u8]) -> usize {
    b.iter().position(|&c| c == 0).unwrap_or(b.len())
}

/// Byte read of a C string that tolerates reaching the end of the slice:
/// kernel readers always have the byte (the buffers are sized to cover every
/// offset they can reach); an absent byte is treated like the NUL tail.
fn sbyte(s: &[u8], i: usize) -> u8 {
    if i < s.len() { s[i] } else { 0 }
}

/// `strcmp() == 0` over NUL-terminated strings.
fn cstr_eq(a: &[u8], b: &[u8]) -> bool {
    let na = cstrlen(a);
    na == cstrlen(b) && a[..na] == b[..na]
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

/// `snprintf()` into a fixed buffer: writes min(len, avail-1) bytes plus a
/// NUL (when it fits the backing store), returns the would-be length.
fn snprintf_str(dst: &mut [u8], off: usize, avail: usize, s: &[u8]) -> usize {
    if avail == 0 {
        return s.len();
    }
    let w = s.len().min(avail - 1).min(dst.len().saturating_sub(off));
    dst[off..off + w].copy_from_slice(&s[..w]);
    if off + w < dst.len() {
        dst[off + w] = 0;
    }
    s.len()
}

// ---- linux/ctype.h (kernel table, byte-identical to lib/ctype.c) -------------

const C_U: u8 = 0x01;
const C_L: u8 = 0x02;
const C_D: u8 = 0x04;
const C_C: u8 = 0x08;
const C_P: u8 = 0x10;
const C_S: u8 = 0x20;
const C_X: u8 = 0x40;
const C_SP: u8 = 0x80;

#[rustfmt::skip]
const CTYPE: [u8; 256] = [
    C_C,C_C,C_C,C_C,C_C,C_C,C_C,C_C,                 // 0-7
    C_C,C_C|C_S,C_C|C_S,C_C|C_S,C_C|C_S,C_C|C_S,C_C,C_C, // 8-15
    C_C,C_C,C_C,C_C,C_C,C_C,C_C,C_C,                 // 16-23
    C_C,C_C,C_C,C_C,C_C,C_C,C_C,C_C,                 // 24-31
    C_S|C_SP,C_P,C_P,C_P,C_P,C_P,C_P,C_P,            // 32-39
    C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,                 // 40-47
    C_D,C_D,C_D,C_D,C_D,C_D,C_D,C_D,                 // 48-55
    C_D,C_D,C_P,C_P,C_P,C_P,C_P,C_P,                 // 56-63
    C_P,C_U|C_X,C_U|C_X,C_U|C_X,C_U|C_X,C_U|C_X,C_U|C_X,C_U, // 64-71
    C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,                 // 72-79
    C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,                 // 80-87
    C_U,C_U,C_U,C_P,C_P,C_P,C_P,C_P,                 // 88-95
    C_P,C_L|C_X,C_L|C_X,C_L|C_X,C_L|C_X,C_L|C_X,C_L|C_X,C_L, // 96-103
    C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,                 // 104-111
    C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,                 // 112-119
    C_L,C_L,C_L,C_P,C_P,C_P,C_P,C_C,                 // 120-127
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,                 // 128-143
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,                 // 144-159
    C_S|C_SP,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P, // 160-175
    C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P,C_P, // 176-191
    C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_U, // 192-207
    C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_P,C_U,C_U,C_U,C_U,C_U,C_U,C_U,C_L, // 208-223
    C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L, // 224-239
    C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_P,C_L,C_L,C_L,C_L,C_L,C_L,C_L,C_L, // 240-255
];

fn ismask(c: u8) -> u8 {
    CTYPE[c as usize]
}
/// The kernel's isdigit is the plain range check (sign-extended char is fine).
fn isdigit(c: i32) -> bool {
    (0x30..=0x39).contains(&c)
}
fn isxdigit(c: u8) -> bool {
    ismask(c) & (C_D | C_X) != 0
}
fn isspace(c: u8) -> bool {
    ismask(c) & C_S != 0
}
fn isprint(c: u8) -> bool {
    ismask(c) & (C_P | C_U | C_L | C_D | C_SP) != 0
}
fn isascii(c: u8) -> bool {
    c <= 0x7f
}
fn isodigit(c: u8) -> bool {
    (b'0'..=b'7').contains(&c)
}
fn islower_c(c: u8) -> bool {
    ismask(c) & C_L != 0
}
fn isupper_c(c: u8) -> bool {
    ismask(c) & C_U != 0
}
fn tolower_c(c: u8) -> u8 {
    if isupper_c(c) { c + 32 } else { c }
}
fn toupper_c(c: u8) -> u8 {
    if islower_c(c) { c - 32 } else { c }
}
/// `_tolower()`: unconditional bit-5 set.
fn lower_fast(c: u8) -> u8 {
    c | 0x20
}

// ---- errno -------------------------------------------------------------------

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;

// ---- seeded string generators -------------------------------------------------

#[rustfmt::skip]
const NUM_CASES: [&[u8]; 82] = [
    b"", b" ", b"+", b"-", b"+0", b"-0", b"0", b"00", b"0007", b"0x", b"0x0",
    b"0xg", b"0XFF", b"0x7fffffffffffffff", b"0x8000000000000000",
    b"0xffffffffffffffff", b"0x10000000000000000", b"0XaB",
    b"18446744073709551615", b"18446744073709551616",
    b"18446744073709551617", b"184467440737095516150",
    b"-9223372036854775808", b"9223372036854775807",
    b"-9223372036854775809", b"9223372036854775808",
    b"4294967295", b"4294967296", b"2147483647", b"2147483648",
    b"-2147483648", b"-2147483649",
    b"65535", b"65536", b"255", b"256", b"127", b"128",
    b"-127", b"-128", b"-129", b"32767", b"32768", b"-32768", b"-32769",
    b"01777777777777777777777", b"02000000000000000000000",
    b"01000000000000000000000", b"07777",
    b"1111111111111111111111111111111111111111111111111111111111111111",
    b"1000000000000000000000000000000000000000000000000000000000000000",
    b"11111111111111111111111111111111111111111111111111111111111111111",
    b"99999999999999999999", b"9999999999999999999", b"0xabcdef0123456789",
    b"123\n", b"123\n\n", b"123x", b"123 ", b" 123", b"12 3", b"1,234",
    b"+42", b"-42", b"+0x1f", b"-0x1f", b"0b101", b"0o17",
    b"g", b"1a2", b"12a", b"a12", b"\t1", b"1\t", b"\n", b"1\n2",
    b"++1", b"--1", b"+-1", b"-+1", b"0x-1", b"-0x-1",
];

/// Build a NUL-terminated numeric-string candidate into nbuf.
/// mode&0x400 selects a curated boundary string (seed picks which);
/// otherwise a seeded string assembled from whitespace/sign/prefix/digit
/// pool/trailer features.
fn mknum(nbuf: &mut [u8; NSTR], seed: u64, mode: i64) {
    const POOLS: [&[u8]; 8] = [
        b"0123456789",
        b"0123456789abcdef",
        b"0123456789ABCDEF",
        b"01234567",
        b"01",
        b"0123456789abcdefABCDEF",
        b"89",
        b"0123456789abcdefgG_",
    ];
    if mode & 0x400 != 0 {
        let s = NUM_CASES[(seed % NUM_CASES.len() as u64) as usize];
        fill_raw(nbuf, seed ^ 0xfeed5eed);
        nbuf[..s.len()].copy_from_slice(s);
        nbuf[s.len()] = 0;
        return;
    }

    fill_raw(nbuf, seed ^ 0xfeed5eed);
    let mut r = seed | 1;
    let mut i = 0usize;

    if mode & 1 != 0 {
        nbuf[i] = b' ';
        i += 1;
    }
    match (mode >> 1) & 3 {
        1 => {
            nbuf[i] = b'+';
            i += 1;
        }
        2 => {
            nbuf[i] = b'-';
            i += 1;
        }
        3 => {
            nbuf[i] = b' ';
            i += 1;
        }
        _ => {}
    }
    match (mode >> 3) & 3 {
        1 => {
            nbuf[i] = b'0';
            nbuf[i + 1] = b'x';
            i += 2;
        }
        2 => {
            nbuf[i] = b'0';
            nbuf[i + 1] = b'X';
            i += 2;
        }
        3 => {
            nbuf[i] = b'0';
            i += 1;
        }
        _ => {}
    }
    let ndig = 1 + xs64(&mut r) % 70;
    let pool = POOLS[((mode >> 5) & 7) as usize];
    let mut j = 0u64;
    while j < ndig && i < NSTR - 4 {
        let x = xs64(&mut r);
        nbuf[i] = pool[(x % pool.len() as u64) as usize];
        i += 1;
        j += 1;
    }
    if mode & 0x100 != 0 {
        nbuf[i] = b'g';
        i += 1;
    }
    match (mode >> 9) & 3 {
        1 => {
            nbuf[i] = b'\n';
            i += 1;
        }
        2 => {
            nbuf[i] = b'\n';
            nbuf[i + 1] = b'x';
            i += 2;
        }
        3 => {
            nbuf[i] = b' ';
            i += 1;
        }
        _ => {}
    }
    nbuf[i] = 0;
}

#[rustfmt::skip]
const DEC_CASES: [&[u8]; 32] = [
    b"0", b"0.0", b".5", b"1.", b"1.5", b"-3.141592", b"+2.75",
    b"123.45", b"0.999", b"18446744073709551615.0",
    b"18446744073709551616", b"9.223372036854775807",
    b"18446744073709551615", b"18446744073709551616",
    b"0.00000000000000000001", b"1.000000000000000000000",
    b".", b"-", b"", b"-.5", b"1.2x", b"1.2 ", b"1.2\n", b" 1.2",
    b"99999999999999999999.99999", b"1.234567890123456789012345",
    b"18446744073709551.615", b"18446744073709551.616",
    b"0.9999", b"-0.5", b"-18446744073709551615",
    b"-18446744073709551616",
];

/// Build a scaled-decimal candidate into nbuf: int digits, optional '.',
/// fraction digits, optional trailing newline/junk.
fn mkdec(nbuf: &mut [u8; NSTR], seed: u64, mode: i64) {
    if mode & 0x400 != 0 {
        let s = DEC_CASES[(seed % DEC_CASES.len() as u64) as usize];
        fill_raw(nbuf, seed ^ 0xfeed5eed);
        nbuf[..s.len()].copy_from_slice(s);
        nbuf[s.len()] = 0;
        return;
    }

    fill_raw(nbuf, seed ^ 0xfeed5eed);
    let mut r = seed | 1;
    let mut i = 0usize;

    if mode & 1 != 0 {
        nbuf[i] = b' ';
        i += 1;
    }
    match (mode >> 1) & 3 {
        1 => {
            nbuf[i] = b'+';
            i += 1;
        }
        2 => {
            nbuf[i] = b'-';
            i += 1;
        }
        _ => {}
    }
    let n = xs64(&mut r) % 22;
    let mut j = 0u64;
    while j < n && i < NSTR - 6 {
        nbuf[i] = b'0' + (xs64(&mut r) % 10) as u8;
        i += 1;
        j += 1;
    }
    if mode & 4 != 0 {
        nbuf[i] = b'.';
        i += 1;
    }
    let n = xs64(&mut r) % 24;
    let mut j = 0u64;
    while j < n && i < NSTR - 4 {
        nbuf[i] = b'0' + (xs64(&mut r) % 10) as u8;
        i += 1;
        j += 1;
    }
    match (mode >> 3) & 3 {
        1 => {
            nbuf[i] = b'\n';
            i += 1;
        }
        2 => {
            nbuf[i] = b'x';
            i += 1;
        }
        3 => {
            nbuf[i] = b' ';
            i += 1;
        }
        _ => {}
    }
    nbuf[i] = 0;
}

/// Build a kstrtobool candidate: one or two letters plus trailers.
fn mkbool(nbuf: &mut [u8; NSTR], seed: u64, mode: i64) {
    const POOL: &[u8] = b"EeYyTt1DdNnFf0oOxXbB"; // sizeof includes NUL: % 20
    const C2: &[u8] = b"nNfFxX";

    fill_raw(nbuf, seed ^ 0xfeed5eed);
    let mut r = seed | 1;
    let mut i = 0usize;

    if mode & 8 != 0 {
        nbuf[0] = 0;
        return;
    }
    let x = xs64(&mut r);
    nbuf[i] = POOL[(x % 20) as usize];
    i += 1;
    match (mode >> 1) & 3 {
        1 => {
            nbuf[i] = C2[(x % 6) as usize];
            i += 1;
        }
        2 => {
            nbuf[i] = (x | 1) as u8;
            i += 1;
        }
        3 => {
            nbuf[i] = b'\n';
            i += 1;
        }
        _ => {}
    }
    if mode & 4 != 0 {
        nbuf[i] = b'Z';
        i += 1;
    }
    nbuf[i] = 0;
}

/// The `only` dictionary for string_escape_*: selected by sel%5.
fn mkonly(sel: i64, seed: u64) -> Option<Vec<u8>> {
    match blen(sel, 5) {
        0 => None,
        1 => Some(Vec::new()),                  // "": is_dict false
        2 => Some(b"\\\"abc\x07\x1b".to_vec()), // specials + letters
        3 => Some(b"\n\r\t\x0b\x0c0".to_vec()), // spaces + NUL
        _ => {
            // seeded subset of all 256 bytes
            let mut r = seed | 1;
            let mut o = Vec::with_capacity(32);
            for i in 0..32 {
                let x = xs64(&mut r);
                o.push((x >> ((i % 7) * 8)) as u8);
            }
            Some(o)
        }
    }
}

/// Build an escape-sequence-heavy string for string_unescape: plain bytes
/// interleaved with \x, \NNN, \e-letter and trailing/empty backslashes.
fn mkesc(sbuf: &mut [u8; SSZ], n: usize, seed: u64, mode: i64) {
    const SP: &[u8] = b"nrtvf";
    const HX: &[u8] = b"0123456789abcdefABCDEFxg"; // % 24
    const SPEC: &[u8] = b"\\\"aeq";

    fill_raw(sbuf, seed ^ 0xfeed5eed);
    let mut r = seed | 1;
    let mut i = 0usize;
    let mut j = 0usize;
    while j < n && i < SSZ - 6 {
        let x = xs64(&mut r);
        match x % 8 {
            0 => {
                sbuf[i] = b'\\';
                sbuf[i + 1] = SP[(x % 5) as usize];
                i += 2;
            }
            1 => {
                sbuf[i] = b'\\';
                sbuf[i + 1] = b'x';
                sbuf[i + 2] = HX[(x % 24) as usize];
                i += 3;
                if x & 0x100 != 0 {
                    sbuf[i] = HX[((x >> 8) % 24) as usize];
                    i += 1;
                }
            }
            2 => {
                sbuf[i] = b'\\';
                sbuf[i + 1] = b'0' + (x % 9) as u8;
                i += 2;
                if x & 0x100 != 0 {
                    sbuf[i] = b'0' + ((x >> 8) % 8) as u8;
                    i += 1;
                }
                if x & 0x200 != 0 {
                    sbuf[i] = b'0' + ((x >> 16) % 8) as u8;
                    i += 1;
                }
            }
            3 => {
                sbuf[i] = b'\\';
                sbuf[i + 1] = SPEC[(x % 5) as usize];
                i += 2;
            }
            4 => {
                sbuf[i] = b'\\';
                sbuf[i + 1] = ((x >> 8) | 1) as u8;
                i += 2;
            }
            5 => {
                sbuf[i] = b'\\';
                i += 1;
            }
            _ => {
                sbuf[i] = (x | 1) as u8;
                i += 1;
            }
        }
        j += 1;
    }
    if mode & 2 != 0 {
        sbuf[i] = b'\\';
        i += 1;
    }
    sbuf[i] = 0;
}

const ARR_N: usize = 16;

/// The `arr` model: `Some(i)` points at `astr[i]`, `None` is a NULL entry.
fn mkarray(
    astr: &mut [[u8; 96]; ARR_N],
    arr: &mut [Option<usize>; ARR_N + 1],
    seed: u64,
    mode: i64,
) {
    let mut r = seed | 1;
    for i in 0..ARR_N {
        let n = 1 + (xs64(&mut r) % 40) as usize;
        for x in astr[i].iter_mut().take(n) {
            *x = b'a' + (xs64(&mut r) % 58) as u8;
        }
        astr[i][n] = 0;
        arr[i] = Some(i);
    }
    arr[ARR_N] = None;
    if mode & 1 != 0 {
        // plant a NULL terminator mid-array (first NULL stops scan)
        let k = blen(xs64(&mut r) as i64, ARR_N);
        arr[k] = None;
    }
}

// ---- lib/kstrtox.c -------------------------------------------------------------

const KSTRTOX_OVERFLOW: u32 = 1 << 31;

/// `_parse_integer_fixup_radix`: adjusts `base` for 0/0x prefixes and returns
/// the number of chars consumed (0 or 2).
fn parse_integer_fixup_radix(s: &[u8], base: &mut u32) -> usize {
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
/// starting from `init`; returns `(count | KSTRTOX_OVERFLOW, result)`.
fn parse_integer_limit(s: &[u8], base: u32, max_chars: u64, init: u64) -> (u32, u64) {
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
fn parse_integer(s: &[u8], base: u32) -> (u32, u64) {
    parse_integer_limit(s, base, i32::MAX as u64, 0)
}

fn kstrtoull_core(s: &[u8], base: u32) -> Result<u64, i32> {
    let mut base = base;
    let off = parse_integer_fixup_radix(s, &mut base);
    let s = &s[off.min(s.len())..];
    let (rv, res) = parse_integer(s, base);
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

fn kstrtoull(s: &[u8], base: u32) -> Result<u64, i32> {
    if sbyte(s, 0) == b'+' {
        return kstrtoull_core(&s[1..], base);
    }
    kstrtoull_core(s, base)
}

fn kstrtoll(s: &[u8], base: u32) -> Result<i64, i32> {
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

/// `_kstrtoul` on LP64: `unsigned long` is u64, the C narrowing test
/// `tmp != (unsigned long)tmp` is vacuous and never returns -ERANGE.
fn kstrtoul_l(s: &[u8], base: u32) -> Result<u64, i32> {
    kstrtoull(s, base)
}

/// `_kstrtol` on LP64: `long` is i64, the C narrowing test
/// `tmp != (long)tmp` is vacuous and never returns -ERANGE.
fn kstrtol_l(s: &[u8], base: u32) -> Result<i64, i32> {
    kstrtoll(s, base)
}

/// The shared body of kstrtou16/kstrtou8/kstrtouint/kstrtou32.
fn kstrto_unsigned(s: &[u8], base: u32, max: u64) -> Result<u64, i32> {
    let tmp = kstrtoull(s, base)?;
    if tmp != (tmp & max) {
        return Err(-ERANGE);
    }
    Ok(tmp)
}

/// The shared body of kstrtos16/kstrtos8/kstrtoint/kstrtos32.
fn kstrto_signed(s: &[u8], base: u32, lo: i64, hi: i64) -> Result<i64, i32> {
    let tmp = kstrtoll(s, base)?;
    if tmp < lo || tmp > hi {
        return Err(-ERANGE);
    }
    Ok(tmp)
}

fn kstrtobool(s: Option<&[u8]>) -> Result<bool, i32> {
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

/// `int_pow` — base^exp with wrapping multiply, like lib/math/int_pow.c.
fn int_pow(mut base: u64, mut exp: u32) -> u64 {
    let mut result = 1u64;
    while exp != 0 {
        if exp & 1 != 0 {
            result = result.wrapping_mul(base);
        }
        exp >>= 1;
        base = base.wrapping_mul(base);
    }
    result
}

fn kstrtoudec64_core(s: &[u8], scale: u32) -> Result<u64, i32> {
    let (rv_int, mut res) = parse_integer(s, 10);
    if rv_int & KSTRTOX_OVERFLOW != 0 {
        return Err(-ERANGE);
    }
    let mut i = rv_int as usize;
    if sbyte(s, i) == b'.' {
        i += 1;
    }
    let (rv_frac, r2) = parse_integer_limit(&s[i.min(s.len())..], 10, scale as u64, res);
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

fn kstrtoudec64(s: &[u8], scale: u32) -> Result<u64, i32> {
    if sbyte(s, 0) == b'+' {
        return kstrtoudec64_core(&s[1..], scale);
    }
    kstrtoudec64_core(s, scale)
}

fn kstrtodec64(s: &[u8], scale: u32) -> Result<i64, i32> {
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

/// The kstrto*_from_user bodies: `buf[cap]` gets min(count, cap-1) copied
/// bytes plus a NUL; copy_from_user never faults in the harness.
fn from_user_buf(src: &[u8], count: i64, bufsize: usize) -> Vec<u8> {
    let n = ((count as u64).min((bufsize - 1) as u64)) as usize;
    let mut buf = vec![0u8; bufsize];
    buf[..n].copy_from_slice(&src[..n]);
    buf[n] = 0;
    buf
}

// ---- lib/hexdump.c -------------------------------------------------------------

const HEX_ASC: &[u8; 16] = b"0123456789abcdef";

fn hex_to_bin(ch: u8) -> i32 {
    let c = ch as i32;
    let cu = (ch & 0xdf) as i32;
    let d = ((c - 0x39 - 1) & (0x30 - 1 - c)) as u32 >> 8;
    let e = ((cu - 0x46 - 1) & (0x41 - 1 - cu)) as u32 >> 8;
    (-1i32)
        .wrapping_add((c - 0x30 + 1) & (d as i32))
        .wrapping_add((cu - 0x41 + 11) & (e as i32))
}

fn hex2bin(dst: &mut [u8], src: &[u8], count: usize) -> i32 {
    let mut si = 0usize;
    let mut di = 0usize;
    let mut count = count;
    while count > 0 {
        count -= 1;
        let hi = hex_to_bin(sbyte(src, si));
        si += 1;
        if hi < 0 {
            return -EINVAL;
        }
        let lo = hex_to_bin(sbyte(src, si));
        si += 1;
        if lo < 0 {
            return -EINVAL;
        }
        if di < dst.len() {
            dst[di] = ((hi << 4) | lo) as u8;
        }
        di += 1;
    }
    0
}

/// `bin2hex`: returns the number of hex chars written (r - dst).
fn bin2hex(dst: &mut [u8], src: &[u8], count: usize) -> usize {
    let mut di = 0usize;
    for &b in src.iter().take(count) {
        if di + 1 < dst.len() {
            dst[di] = HEX_ASC[(b >> 4) as usize];
            dst[di + 1] = HEX_ASC[(b & 0x0f) as usize];
        }
        di += 2;
    }
    di
}

/// `is_power_of_2(n)` on int: `n != 0 && (n & (n - 1)) == 0` — INT_MIN counts.
fn is_power_of_2(n: i32) -> bool {
    n != 0 && (n & n.wrapping_sub(1)) == 0
}

/// `le64/32/16_at`: unaligned little-endian loads.
fn le64_at(b: &[u8], i: usize) -> u64 {
    let mut v = 0u64;
    for k in 0..8 {
        v |= (sbyte(b, i + k) as u64) << (8 * k);
    }
    v
}
fn le32_at(b: &[u8], i: usize) -> u32 {
    let mut v = 0u32;
    for k in 0..4 {
        v |= (sbyte(b, i + k) as u32) << (8 * k);
    }
    v
}
fn le16_at(b: &[u8], i: usize) -> u16 {
    (sbyte(b, i) as u16) | ((sbyte(b, i + 1) as u16) << 8)
}

/// hex_dump_to_buffer's `overflow1` return value.
fn hdb_overflow(ascii: bool, ascii_column: i32, len: usize, groupsize: i32, ngroups: i32) -> i32 {
    if ascii {
        // `ascii_column + len` is int + size_t -> size_t -> int.
        (ascii_column as i64 + len as i64) as i32
    } else {
        groupsize
            .wrapping_mul(2)
            .wrapping_add(1)
            .wrapping_mul(ngroups)
            .wrapping_sub(1)
    }
}

#[allow(clippy::too_many_arguments)]
fn hex_dump_to_buffer(
    buf: &[u8],
    len: usize,
    rowsize: i32,
    groupsize: i32,
    linebuf: &mut [u8],
    linebuflen: usize,
    ascii: bool,
) -> i32 {
    let mut rowsize = rowsize;
    let mut groupsize = groupsize;
    let mut len = len;

    if rowsize != 16 && rowsize != 32 {
        rowsize = 16;
    }
    if len > rowsize as usize {
        len = rowsize as usize;
    }
    if !is_power_of_2(groupsize) || groupsize > 8 {
        groupsize = 1;
    }
    // `len % groupsize`: groupsize is int -> size_t (sign extension).
    if !(len as u64).is_multiple_of(groupsize as i64 as u64) {
        groupsize = 1;
    }

    // `ngroups` is int in C; the division is size_t / size_t.
    let ngroups = ((len as u64) / (groupsize as i64 as u64)) as i32;
    let ascii_column = rowsize * 2 + rowsize / groupsize + 1;

    let mut lx = 0usize;

    if linebuflen == 0 {
        return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
    }
    if len == 0 {
        linebuf[lx] = 0;
        return lx as i32;
    }

    if groupsize == 8 {
        for j in 0..ngroups as usize {
            let s = format!(
                "{}{:016x}",
                if j != 0 { " " } else { "" },
                le64_at(buf, j * 8)
            );
            let avail = linebuflen - lx;
            let ret = snprintf_str(linebuf, lx, avail, s.as_bytes());
            if ret >= avail {
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            lx += ret;
        }
    } else if groupsize == 4 {
        for j in 0..ngroups as usize {
            let s = format!(
                "{}{:08x}",
                if j != 0 { " " } else { "" },
                le32_at(buf, j * 4)
            );
            let avail = linebuflen - lx;
            let ret = snprintf_str(linebuf, lx, avail, s.as_bytes());
            if ret >= avail {
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            lx += ret;
        }
    } else if groupsize == 2 {
        for j in 0..ngroups as usize {
            let s = format!(
                "{}{:04x}",
                if j != 0 { " " } else { "" },
                le16_at(buf, j * 2)
            );
            let avail = linebuflen - lx;
            let ret = snprintf_str(linebuf, lx, avail, s.as_bytes());
            if ret >= avail {
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            lx += ret;
        }
    } else {
        let mut j = 0usize;
        while j < len {
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            let ch = buf[j];
            linebuf[lx] = HEX_ASC[(ch >> 4) as usize];
            lx += 1;
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            linebuf[lx] = HEX_ASC[(ch & 0x0f) as usize];
            lx += 1;
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
            }
            linebuf[lx] = b' ';
            lx += 1;
            j += 1;
        }
        if j != 0 {
            lx -= 1;
        }
    }
    if !ascii {
        linebuf[lx] = 0;
        return lx as i32;
    }
    while lx < ascii_column as usize {
        if linebuflen < lx + 2 {
            linebuf[lx] = 0;
            return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
        }
        linebuf[lx] = b' ';
        lx += 1;
    }
    for &ch in buf.iter().take(len) {
        if linebuflen < lx + 2 {
            linebuf[lx] = 0;
            return hdb_overflow(ascii, ascii_column, len, groupsize, ngroups);
        }
        linebuf[lx] = if isascii(ch) && isprint(ch) { ch } else { b'.' };
        lx += 1;
    }
    linebuf[lx] = 0;
    lx as i32
}

/// Append to the captured printk log (`kemit`): capped at KLOG bytes.
fn kemit(klog: &mut Vec<u8>, s: &[u8]) {
    for &c in s {
        if klog.len() < KLOG {
            klog.push(c);
        }
    }
}

const DUMP_PREFIX_NONE: i32 = 0;
const DUMP_PREFIX_ADDRESS: i32 = 1;
const DUMP_PREFIX_OFFSET: i32 = 2;

#[allow(clippy::too_many_arguments)]
fn print_hex_dump(
    level: &[u8],
    prefix_str: &[u8],
    prefix_type: i32,
    rowsize: i32,
    groupsize: i32,
    buf: &[u8],
    len: usize,
    ascii: bool,
    klog: &mut Vec<u8>,
) {
    let mut rowsize = rowsize;
    if rowsize != 16 && rowsize != 32 {
        rowsize = 16;
    }
    let mut remaining = len as i32;
    let mut i = 0usize;
    while i < len {
        let linelen = remaining.min(rowsize);
        remaining -= rowsize;

        let mut linebuf = [0u8; 32 * 3 + 2 + 32 + 1];
        hex_dump_to_buffer(
            &buf[i..],
            linelen as usize,
            rowsize,
            groupsize,
            &mut linebuf,
            32 * 3 + 2 + 32 + 1,
            ascii,
        );

        kemit(klog, level);
        kemit(klog, prefix_str);
        match prefix_type {
            DUMP_PREFIX_ADDRESS => {
                // printk("%s%s%p: %s\n"): %p canonicalizes to PCANON + offset.
                kemit(klog, format!("{:016x}: ", PCANON + i as u64).as_bytes());
            }
            DUMP_PREFIX_OFFSET => {
                kemit(klog, format!("{:08x}: ", i).as_bytes());
            }
            _ => {}
        }
        kemit(klog, &linebuf[..cstrlen(&linebuf)]);
        kemit(klog, b"\n");

        i += rowsize as usize;
    }
    let _ = DUMP_PREFIX_NONE;
}

// ---- lib/string_helpers.c -----------------------------------------------------

/// `string_get_size`: Napier's algorithm, 3 significant figures.
fn string_get_size(size: u64, blk_size: u64, units: i32, buf: &mut [u8], len: i32) -> i64 {
    const UNITS_10: [&str; 9] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"];
    const UNITS_2: [&str; 9] = ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi", "Yi"];
    const ROUNDING: [u32; 3] = [500, 50, 5];
    let units_base = (units & 1) as usize; // STRING_UNITS_MASK
    let divisor: u64 = if units_base == 0 { 1000 } else { 1024 };
    let mut i = 0usize;
    let mut remainder = 0u32;
    let mut size = size;
    let mut blk_size = blk_size;
    let mut tmp = String::new();

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
            remainder = remainder.wrapping_mul(1000);
            remainder >>= 10;
        }
        remainder = remainder.wrapping_add(ROUNDING[j]);
        if remainder >= 1000 {
            remainder -= 1000;
            size += 1;
        }
        if j != 0 {
            // snprintf(tmp, 12, ".%03u"); tmp[j+1] = '\0';
            let t = format!(".{:03}", remainder);
            tmp = t[..(j + 1).min(t.len())].to_string();
        }
    }

    let unit = if i >= UNITS_2.len() {
        "UNK"
    } else {
        (if units_base == 0 { UNITS_10 } else { UNITS_2 })[i]
    };
    let s = format!(
        "{}{}{}{}{}",
        size as u32,
        tmp,
        if units & (1 << 30) != 0 { "" } else { " " }, // STRING_UNITS_NO_SPACE
        unit,
        if units & (1 << 31) != 0 { "" } else { "B" } // STRING_UNITS_NO_BYTES
    );
    // snprintf(buf, len, ...): len is int -> size_t.  A negative int becomes
    // a size_t > PTRDIFF_MAX, for which the libc snprintf used by the
    // reference build has a quirk: it writes the formatted output minus its
    // last character plus a NUL (as if the size had been the needed length).
    // Positive sizes behave normally (write min(ret, size-1) + NUL).
    let sb = s.as_bytes();
    if len < 0 {
        let w = sb.len().saturating_sub(1).min(buf.len());
        buf[..w].copy_from_slice(&sb[..w]);
        if w < buf.len() {
            buf[w] = 0;
        }
    } else {
        snprintf_str(buf, 0, len as usize, sb);
    }
    s.len() as i64
}

const UNESCAPE_SPACE: u32 = 1 << 0;
const UNESCAPE_OCTAL: u32 = 1 << 1;
const UNESCAPE_HEX: u32 = 1 << 2;
const UNESCAPE_SPECIAL: u32 = 1 << 3;

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

/// `string_unescape`: the output byte at `di` is written only when it fits the
/// destination region the caller exposed (`dbuf + 700` can tail into
/// unobserved storage in C); the count is always exact.
fn string_unescape(src: &[u8], dst: &mut [u8], size: usize, flags: u32) -> usize {
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

const ESCAPE_SPACE: u32 = 1 << 0;
const ESCAPE_SPECIAL: u32 = 1 << 1;
const ESCAPE_NULL: u32 = 1 << 2;
const ESCAPE_OCTAL: u32 = 1 << 3;
const ESCAPE_NP: u32 = 1 << 4;
const ESCAPE_HEX: u32 = 1 << 5;
const ESCAPE_NA: u32 = 1 << 6;
const ESCAPE_NAP: u32 = 1 << 7;
const ESCAPE_APPEND: u32 = 1 << 8;

/// `escape_passthrough` and friends: emit into dst while `pos < dst.len()`
/// (`out < end`), counting always.
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

fn string_escape_mem(
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

        if !(is_append || in_dict) && is_dict && escape_passthrough(c, dst, &mut pos) {
            continue;
        }
        if !(is_append && in_dict)
            && isascii(c)
            && isprint(c)
            && flags & ESCAPE_NAP != 0
            && escape_passthrough(c, dst, &mut pos)
        {
            continue;
        }
        if !(is_append && in_dict)
            && isprint(c)
            && flags & ESCAPE_NP != 0
            && escape_passthrough(c, dst, &mut pos)
        {
            continue;
        }
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

/// `skip_spaces`: offset of the first non-whitespace char (kernel isspace,
/// which includes 0xa0 via the _ctype table).
fn skip_spaces(s: &[u8]) -> usize {
    let mut i = 0usize;
    while isspace(sbyte(s, i)) {
        i += 1;
    }
    i
}

/// `strim`: trims trailing whitespace in place, returns the skip_spaces offset.
fn strim(s: &mut [u8]) -> usize {
    let size = cstrlen(s);
    if size == 0 {
        return 0;
    }
    let mut e = size as i64 - 1;
    while e >= 0 && isspace(s[e as usize]) {
        e -= 1;
    }
    s[(e + 1) as usize] = 0;
    skip_spaces(s)
}

/// `sysfs_streq`: equal modulo one trailing newline on either side.
fn sysfs_streq(s1: &[u8], s2: &[u8]) -> bool {
    let (mut a, mut b) = (0usize, 0usize);
    while sbyte(s1, a) != 0 && sbyte(s1, a) == sbyte(s2, b) {
        a += 1;
        b += 1;
    }
    if sbyte(s1, a) == sbyte(s2, b) {
        return true;
    }
    if sbyte(s1, a) == 0 && sbyte(s2, b) == b'\n' && sbyte(s2, b + 1) == 0 {
        return true;
    }
    if sbyte(s1, a) == b'\n' && sbyte(s1, a + 1) == 0 && sbyte(s2, b) == 0 {
        return true;
    }
    false
}

/// `strreplace` in place over the NUL-terminated string.
fn strreplace(s: &mut [u8], old: u8, new: u8) {
    let mut i = 0usize;
    while sbyte(s, i) != 0 {
        if s[i] == old {
            s[i] = new;
        }
        i += 1;
    }
}

/// `match_string`: index of `q` in the array, or -EINVAL.
fn match_string(
    astr: &[[u8; 96]; ARR_N],
    arr: &[Option<usize>; ARR_N + 1],
    n: usize,
    q: &[u8],
) -> i32 {
    let mut index = 0usize;
    while index < n {
        let item = match arr[index] {
            None => break,
            Some(i) => &astr[i][..],
        };
        if cstr_eq(item, q) {
            return index as i32;
        }
        index += 1;
    }
    -EINVAL
}

/// `__sysfs_match_string`: same scan with sysfs_streq.
fn sysfs_match_string(
    astr: &[[u8; 96]; ARR_N],
    arr: &[Option<usize>; ARR_N + 1],
    n: usize,
    q: &[u8],
) -> i32 {
    let mut index = 0usize;
    while index < n {
        let item = match arr[index] {
            None => break,
            Some(i) => &astr[i][..],
        };
        if sysfs_streq(item, q) {
            return index as i32;
        }
        index += 1;
    }
    -EINVAL
}

/// `memcpy_and_pad`.
fn memcpy_and_pad(dest: &mut [u8], dest_len: usize, src: &[u8], count: usize, pad: u8) {
    if dest_len > count {
        dest[..count].copy_from_slice(&src[..count]);
        for x in &mut dest[count..dest_len] {
            *x = pad;
        }
    } else {
        dest[..dest_len].copy_from_slice(&src[..dest_len]);
    }
}

// ==== kstrtox wrappers =========================================================

/// Port of `uint64_t wrap_kstrtoull(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:519`).
pub fn wrap_kstrtoull(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoull(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtoll(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:528`).
pub fn wrap_kstrtoll(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoll(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap__kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:537`).
pub fn wrap__kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoul_l(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap__kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:546`).
pub fn wrap__kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtol_l(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtouint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:555`).
pub fn wrap_kstrtouint(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_unsigned(&nbuf, base as u32, u32::MAX as u64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtoint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:564`).
pub fn wrap_kstrtoint(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_signed(&nbuf, base as u32, i32::MIN as i64, i32::MAX as i64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtou16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:573`).
pub fn wrap_kstrtou16(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_unsigned(&nbuf, base as u32, u16::MAX as u64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtos16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:582`).
pub fn wrap_kstrtos16(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_signed(&nbuf, base as u32, i16::MIN as i64, i16::MAX as i64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtou8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:591`).
pub fn wrap_kstrtou8(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_unsigned(&nbuf, base as u32, u8::MAX as u64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtos8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:600`).
pub fn wrap_kstrtos8(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_signed(&nbuf, base as u32, i8::MIN as i64, i8::MAX as i64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtou64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:609`).
pub fn wrap_kstrtou64(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoull(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtos64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:618`).
pub fn wrap_kstrtos64(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoll(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtou32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:627`).
pub fn wrap_kstrtou32(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_unsigned(&nbuf, base as u32, u32::MAX as u64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtos32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:636`).
pub fn wrap_kstrtos32(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrto_signed(&nbuf, base as u32, i32::MIN as i64, i32::MAX as i64);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:645`).
pub fn wrap_kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtoul_l(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:654`).
pub fn wrap_kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let r = kstrtol_l(&nbuf, base as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, base as u64)
}

/// Port of `uint64_t wrap_parse_integer_limit(uint64_t seed, int64_t base, int64_t maxc, int64_t init, int64_t mode)` (`wrappers.c:663`).
pub fn wrap_parse_integer_limit(seed: u64, base: i64, maxc: i64, init: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let (rv, res) = parse_integer_limit(&nbuf, base as u32, maxc as u64, init as u64);
    mix(rv as u64, res, base as u64)
}

/// Port of `uint64_t wrap_parse_integer_fixup_radix(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:675`).
pub fn wrap_parse_integer_fixup_radix(seed: u64, base: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let mut b = base as u32;
    let off = parse_integer_fixup_radix(&nbuf, &mut b);
    mix(off as u64, b as u64, 0)
}

/// Port of `uint64_t wrap_kstrtobool(uint64_t seed, int64_t mode)` (`wrappers.c:686`).
pub fn wrap_kstrtobool(seed: u64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mkbool(&mut nbuf, seed, mode);
    let r = kstrtobool(if mode & 0x40 != 0 { None } else { Some(&nbuf) });
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(false);
    mix(((ret + 16) as i64) as u64, res as u64, 0)
}

/// Shared body of the unsigned kstrto*_from_user wrappers.
fn wrap_u_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
    bufsize: usize,
    max: u64,
    sentinel: u64,
) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let buf = from_user_buf(&nbuf, count, bufsize);
    let r = kstrto_unsigned(&buf, base as u32, max);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(sentinel);
    mix(((ret + 16) as i64) as u64, res, count as u64)
}

/// Shared body of the signed kstrto*_from_user wrappers.
#[allow(clippy::too_many_arguments)]
fn wrap_s_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
    bufsize: usize,
    lo: i64,
    hi: i64,
    sentinel: i64,
) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mknum(&mut nbuf, seed, mode);
    let buf = from_user_buf(&nbuf, count, bufsize);
    let r = kstrto_signed(&buf, base as u32, lo, hi);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(sentinel) as u64;
    mix(((ret + 16) as i64) as u64, res, count as u64)
}

/// Port of `uint64_t wrap_kstrtoull_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:698`).
pub fn wrap_kstrtoull_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_u_from_user(
        seed,
        count,
        base,
        mode,
        1 + 64 + 1 + 1,
        u64::MAX,
        0x5a5a5a5a5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtoll_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:709`).
pub fn wrap_kstrtoll_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_s_from_user(
        seed,
        count,
        base,
        mode,
        1 + 64 + 1 + 1,
        i64::MIN,
        i64::MAX,
        0x5a5a5a5a5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtoul_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:720`).
pub fn wrap_kstrtoul_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_u_from_user(
        seed,
        count,
        base,
        mode,
        1 + 64 + 1 + 1,
        u64::MAX,
        0x5a5a5a5a5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtol_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:731`).
pub fn wrap_kstrtol_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_s_from_user(
        seed,
        count,
        base,
        mode,
        1 + 64 + 1 + 1,
        i64::MIN,
        i64::MAX,
        0x5a5a5a5a5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtouint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:742`).
pub fn wrap_kstrtouint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_u_from_user(
        seed,
        count,
        base,
        mode,
        1 + 32 + 1 + 1,
        u32::MAX as u64,
        0x5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtoint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:753`).
pub fn wrap_kstrtoint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_s_from_user(
        seed,
        count,
        base,
        mode,
        1 + 32 + 1 + 1,
        i32::MIN as i64,
        i32::MAX as i64,
        0x5a5a5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtou16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:764`).
pub fn wrap_kstrtou16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_u_from_user(
        seed,
        count,
        base,
        mode,
        1 + 16 + 1 + 1,
        u16::MAX as u64,
        0x5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtos16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:775`).
pub fn wrap_kstrtos16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_s_from_user(
        seed,
        count,
        base,
        mode,
        1 + 16 + 1 + 1,
        i16::MIN as i64,
        i16::MAX as i64,
        0x5a5a,
    )
}

/// Port of `uint64_t wrap_kstrtou8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:786`).
pub fn wrap_kstrtou8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_u_from_user(seed, count, base, mode, 1 + 8 + 1 + 1, u8::MAX as u64, 0x5a)
}

/// Port of `uint64_t wrap_kstrtos8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:797`).
pub fn wrap_kstrtos8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    wrap_s_from_user(
        seed,
        count,
        base,
        mode,
        1 + 8 + 1 + 1,
        i8::MIN as i64,
        i8::MAX as i64,
        0x5a,
    )
}

/// Port of `uint64_t wrap_kstrtobool_from_user(uint64_t seed, int64_t count, int64_t mode)` (`wrappers.c:808`).
pub fn wrap_kstrtobool_from_user(seed: u64, count: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mkbool(&mut nbuf, seed, mode);
    let buf = from_user_buf(&nbuf, count, 4);
    let r = kstrtobool(Some(&buf));
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(false);
    mix(((ret + 16) as i64) as u64, res as u64, count as u64)
}

/// Port of `uint64_t wrap_kstrtoudec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:820`).
pub fn wrap_kstrtoudec64(seed: u64, scale: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mkdec(&mut nbuf, seed, mode);
    let r = kstrtoudec64(&nbuf, scale as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a);
    mix(((ret + 16) as i64) as u64, res, scale as u64)
}

/// Port of `uint64_t wrap_kstrtodec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:830`).
pub fn wrap_kstrtodec64(seed: u64, scale: i64, mode: i64) -> u64 {
    let mut nbuf = [0u8; NSTR];
    mkdec(&mut nbuf, seed, mode);
    let r = kstrtodec64(&nbuf, scale as u32);
    let ret = r.err().unwrap_or(0);
    let res = r.unwrap_or(0x5a5a5a5a5a5a5a5a) as u64;
    mix(((ret + 16) as i64) as u64, res, scale as u64)
}

// ==== hexdump wrappers ==========================================================

/// Port of `int64_t wrap_hex_to_bin(int64_t c)` (`wrappers.c:843`).
pub fn wrap_hex_to_bin(c: i64) -> i64 {
    hex_to_bin(c as u8) as i64
}

/// Port of `uint64_t wrap_hex2bin(uint64_t seed, int64_t slen, int64_t cnt, int64_t mode)` (`wrappers.c:848`).
pub fn wrap_hex2bin(seed: u64, slen: i64, cnt: i64, mode: i64) -> u64 {
    const HD: &[u8] = b"0123456789abcdefABCDEF"; // % 22
    let n = blen(slen, 300);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0u8; DSZ];

    fill_raw(&mut sbuf, seed);
    let mut r = (seed ^ 0x5bd1e9959e3779b9) | 1;
    for x in sbuf.iter_mut().take(n) {
        *x = HD[(xs64(&mut r) % 22) as usize];
    }
    if mode & 1 != 0 && n != 0 {
        let p = blen(xs64(&mut r) as i64, n);
        sbuf[p] = b'q';
    }
    fill_raw(&mut dbuf, seed ^ 0x9e3779b97f4a7c15);
    let ret = hex2bin(&mut dbuf, &sbuf, blen(cnt, 300));
    mix(((ret + 16) as i64) as u64, fnv1a(&dbuf), n as u64)
}

/// Port of `uint64_t wrap_bin2hex(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:866`).
pub fn wrap_bin2hex(seed: u64, slen: i64, mode: i64) -> u64 {
    let n = blen(slen, HBUF);
    let mut hbuf = [0u8; HBUF];
    let mut dbuf = [0u8; DSZ];

    fill_raw(&mut hbuf, seed);
    fill_raw(&mut dbuf, seed ^ 0x9e3779b97f4a7c15);
    let r = bin2hex(&mut dbuf, &hbuf, n);
    mix(r as u64, fnv1a(&dbuf), mode as u64)
}

/// Port of `uint64_t wrap_hex_dump_to_buffer(uint64_t seed, int64_t len, int64_t rowsize, int64_t groupsize, int64_t linebuflen, int64_t ascii)` (`wrappers.c:878`).
pub fn wrap_hex_dump_to_buffer(
    seed: u64,
    len: i64,
    rowsize: i64,
    groupsize: i64,
    linebuflen: i64,
    ascii: i64,
) -> u64 {
    let l = blen(len, HBUF + 1);
    let lblen = blen(linebuflen, LB + 1);
    let mut hbuf = [0u8; HBUF];
    let mut linebuf = [0xa5u8; LB];

    fill_raw(&mut hbuf, seed);
    let ret = hex_dump_to_buffer(
        &hbuf,
        l,
        rowsize as i32,
        groupsize as i32,
        &mut linebuf,
        lblen,
        ascii & 1 != 0,
    );
    mix(((ret + 16) as i64) as u64, fnv1a(&linebuf), l as u64)
}

/// Port of `uint64_t wrap_print_hex_dump(uint64_t seed, int64_t ptype, int64_t rowsize, int64_t groupsize, int64_t len, int64_t ascii, int64_t mode)` (`wrappers.c:894`).
pub fn wrap_print_hex_dump(
    seed: u64,
    ptype: i64,
    rowsize: i64,
    groupsize: i64,
    len: i64,
    ascii: i64,
    mode: i64,
) -> u64 {
    const LEVELS: [&[u8]; 5] = [b"\x017", b"\x016", b"\x014", b"", b"X"];
    const PFXS: [&[u8]; 3] = [b"", b"pfx: ", b"data "];
    let mut hbuf = [0u8; HBUF];
    let mut klog = Vec::new();

    fill_raw(&mut hbuf, seed);
    print_hex_dump(
        LEVELS[blen(mode, 5)],
        PFXS[blen(mode / 8, 3)],
        ptype as i32,
        rowsize as i32,
        groupsize as i32,
        &hbuf,
        blen(len, HBUF + 1),
        ascii & 1 != 0,
        &mut klog,
    );
    mix(klog.len() as u64, fnv1a(&klog), 0)
}

// ==== string_helpers wrappers ===================================================

/// Port of `uint64_t wrap_string_get_size(int64_t size, int64_t blk, int64_t units, int64_t len)` (`wrappers.c:914`).
pub fn wrap_string_get_size(size: i64, blk: i64, units: i64, len: i64) -> u64 {
    let mut buf = [0xa5u8; 64];
    let ret = string_get_size(size as u64, blk as u64, units as i32, &mut buf, len as i32);
    mix((ret + 16) as u64, fnv1a(&buf), (size as u64) ^ (blk as u64))
}

/// Port of `uint64_t wrap_string_escape_mem(uint64_t seed, int64_t isz, int64_t osz, int64_t flags, int64_t only)` (`wrappers.c:928`).
pub fn wrap_string_escape_mem(seed: u64, isz: i64, osz: i64, flags: i64, only: i64) -> u64 {
    let is = blen(isz, SSZ + 1);
    let os = blen(osz, DSZ + 1);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    fill_raw(&mut sbuf, seed);
    let ret = string_escape_mem(
        &sbuf,
        is,
        &mut dbuf[..os],
        flags as u32,
        mkonly(only, seed).as_deref(),
    );
    mix(((ret + 16) as i64) as u64, fnv1a(&dbuf), is as u64)
}

/// Port of `uint64_t wrap_string_escape_str(uint64_t seed, int64_t slen, int64_t osz, int64_t flags, int64_t only, int64_t mode)` (`wrappers.c:943`).
pub fn wrap_string_escape_str(
    seed: u64,
    slen: i64,
    osz: i64,
    flags: i64,
    only: i64,
    mode: i64,
) -> u64 {
    let n = blen(slen, SSZ - 1);
    let os = blen(osz, DSZ + 1);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    // string_escape_str() = string_escape_mem(src, strlen(src), ...)
    let ret = string_escape_mem(
        &sbuf,
        cstrlen(&sbuf),
        &mut dbuf[..os],
        flags as u32,
        mkonly(only, seed).as_deref(),
    );
    mix(((ret + 16) as i64) as u64, fnv1a(&dbuf), n as u64)
}

/// Port of `uint64_t wrap_string_unescape(uint64_t seed, int64_t slen, int64_t size, int64_t flags, int64_t mode)` (`wrappers.c:958`).
pub fn wrap_string_unescape(seed: u64, slen: i64, size: i64, flags: i64, mode: i64) -> u64 {
    let n = blen(slen, 100);
    let sz = if mode & 8 != 0 {
        0
    } else {
        blen(size, SSZ + 1)
    };
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    mkesc(&mut sbuf, n, seed, mode);
    sbuf2.copy_from_slice(&sbuf);
    let r1 = string_unescape(&sbuf, &mut dbuf, sz, flags as u32) as i32;
    let r2 = if mode & 4 != 0 {
        // in-place: writes trail reads, so a cloned source is equivalent
        let tmp = sbuf2;
        string_unescape(&tmp, &mut sbuf2, sz, flags as u32) as i32
    } else {
        string_unescape(&sbuf2, &mut dbuf[700..], sz, flags as u32) as i32
    };
    mix(
        ((r1 + 16) as i64 as u64) ^ ((r2.wrapping_shl(20)) as i64 as u64),
        fnv1a(&dbuf),
        sz as u64,
    )
}

/// Port of `uint64_t wrap_skip_spaces(uint64_t seed, int64_t mode)` (`wrappers.c:979`).
pub fn wrap_skip_spaces(seed: u64, mode: i64) -> u64 {
    let n = 1 + blen(seed as i64, 60);
    let mut sbuf = [0u8; SSZ];

    make_str(&mut sbuf, n, seed, 0);
    let lead = if mode & 1 != 0 {
        n
    } else {
        blen((seed >> 8) as i64, n)
    };
    for x in sbuf.iter_mut().take(lead) {
        *x = b' ';
    }
    let r = skip_spaces(&sbuf);
    mix(r as u64, fnv1a(&sbuf), n as u64)
}

/// Port of `uint64_t wrap_strim(uint64_t seed, int64_t mode)` (`wrappers.c:993`).
pub fn wrap_strim(seed: u64, mode: i64) -> u64 {
    let n = 1 + blen(seed as i64, 60);
    let mut sbuf = [0u8; SSZ];

    make_str(&mut sbuf, n, seed, 0);
    let lead = if mode & 1 != 0 {
        n
    } else {
        blen((seed >> 8) as i64, n)
    };
    let trail = if mode & 2 != 0 {
        n - lead
    } else {
        blen((seed >> 16) as i64, n - lead + 1)
    };
    for x in sbuf.iter_mut().take(lead) {
        *x = b' ';
    }
    for i in 0..trail {
        sbuf[n - 1 - i] = b' ';
    }
    let r = strim(&mut sbuf);
    mix(r as u64, fnv1a(&sbuf), n as u64)
}

/// Port of `uint64_t wrap_sysfs_streq(uint64_t seed, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:1010`).
pub fn wrap_sysfs_streq(seed: u64, la: i64, lb: i64, mode: i64) -> u64 {
    let na = blen(la, SSZ - 2);
    let nb = blen(lb, SSZ - 2);
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];

    make_str(&mut sbuf, na, seed, 0);
    if mode & 1 != 0 {
        sbuf2.copy_from_slice(&sbuf); // identical
    } else {
        make_str(&mut sbuf2, nb, seed ^ 0x9e3779b97f4a7c15, 0);
    }
    if mode & 2 != 0 {
        let k = cstrlen(&sbuf2); // s2 becomes "s2\n\0"
        sbuf2[k] = b'\n';
        sbuf2[k + 1] = 0;
    }
    if mode & 4 != 0 {
        let k = cstrlen(&sbuf); // s1 becomes "s1\n\0"
        sbuf[k] = b'\n';
        sbuf[k + 1] = 0;
    }
    let r = sysfs_streq(&sbuf, &sbuf2);
    mix(r as u64, fnv1a(&sbuf[..na + 2]), fnv1a(&sbuf2[..nb + 2]))
}

/// Port of `uint64_t wrap_strreplace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1037`).
pub fn wrap_strreplace(seed: u64, slen: i64, old: i64, new: i64, mode: i64) -> u64 {
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];

    // The `old`-plants draw from make_str's residual xorshift64 state.
    let mut r = make_str(&mut sbuf, n, seed, mode & 3);
    if mode & 4 != 0 {
        for _ in 0..3 {
            if n != 0 {
                let p = blen(xs64(&mut r) as i64, n);
                sbuf[p] = old as u8;
            }
        }
    }
    strreplace(&mut sbuf, old as u8, new as u8);
    mix(0, fnv1a(&sbuf), n as u64)
}

/// Port of `uint64_t wrap_memcpy_and_pad(uint64_t seed, int64_t dest_len, int64_t count, int64_t pad)` (`wrappers.c:1054`).
pub fn wrap_memcpy_and_pad(seed: u64, dest_len: i64, count: i64, pad: i64) -> u64 {
    let dl = blen(dest_len, DSZ + 1);
    let ct = blen(count, SSZ + 1);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    fill_raw(&mut sbuf, seed);
    memcpy_and_pad(&mut dbuf, dl, &sbuf, ct, pad as u8);
    mix(dl as u64, fnv1a(&dbuf), ct as u64)
}

/// Port of `uint64_t wrap_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1090`).
pub fn wrap_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    let mut astr = [[0u8; 96]; ARR_N];
    let mut arr = [None; ARR_N + 1];
    let mut sbuf = [0u8; SSZ];

    mkarray(&mut astr, &mut arr, seed, mode);
    let ret = if mode & 2 != 0 {
        let k = blen(strsel, ARR_N);
        let q = astr[k];
        match_string(&astr, &arr, blen(n, ARR_N + 2), &q)
    } else {
        let mut r = (seed ^ 0x5bd1e9959e3779b9) | 1;
        for x in sbuf.iter_mut().take(40) {
            *x = b'a' + (xs64(&mut r) % 58) as u8;
        }
        sbuf[40] = 0;
        match_string(&astr, &arr, blen(n, ARR_N + 2), &sbuf)
    };
    mix(((ret + 4) as i64) as u64, blen(n, ARR_N + 2) as u64, 0)
}

/// Port of `uint64_t wrap_sysfs_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1111`).
pub fn wrap_sysfs_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    let mut astr = [[0u8; 96]; ARR_N];
    let mut arr = [None; ARR_N + 1];
    let mut sbuf = [0u8; SSZ];

    mkarray(&mut astr, &mut arr, seed, mode);
    if mode & 2 != 0 {
        let k = blen(strsel, ARR_N);
        sbuf[..96].copy_from_slice(&astr[k]);
        if mode & 4 != 0 {
            let l = cstrlen(&sbuf); // strcat(sbuf, "\n")
            sbuf[l] = b'\n';
            sbuf[l + 1] = 0;
        }
    } else {
        let mut r = (seed ^ 0x5bd1e9959e3779b9) | 1;
        for x in sbuf.iter_mut().take(40) {
            *x = b'a' + (xs64(&mut r) % 58) as u8;
        }
        sbuf[40] = 0;
        if mode & 4 != 0 {
            let l = cstrlen(&sbuf);
            sbuf[l] = b'\n';
            sbuf[l + 1] = 0;
        }
    }
    if mode & 8 != 0 {
        // array element carries a trailing newline too
        let k = blen(strsel >> 8, ARR_N);
        let l = cstrlen(&astr[k]);
        astr[k][l] = b'\n';
        astr[k][l + 1] = 0;
    }
    let ret = sysfs_match_string(&astr, &arr, blen(n, ARR_N + 2), &sbuf);
    mix(((ret + 4) as i64) as u64, blen(n, ARR_N + 2) as u64, 0)
}

/// Port of `uint64_t wrap_string_is_terminated(uint64_t seed, int64_t slen, int64_t len, int64_t mode)` (`wrappers.c:1145`).
pub fn wrap_string_is_terminated(seed: u64, slen: i64, len: i64, mode: i64) -> u64 {
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    let r = sbuf[..blen(len, SSZ)].contains(&0); // memchr(s, '\0', len)
    mix(r as u64, n as u64, 0)
}

/// Port of `uint64_t wrap_string_upper(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1156`).
pub fn wrap_string_upper(seed: u64, slen: i64, mode: i64) -> u64 {
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    // do { *dst++ = toupper(*src) } while (*src++);
    let mut i = 0usize;
    loop {
        let c = sbuf[i];
        dbuf[i] = toupper_c(c);
        if c == 0 {
            break;
        }
        i += 1;
    }
    mix(n as u64, fnv1a(&dbuf), 0)
}

/// Port of `uint64_t wrap_string_lower(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1166`).
pub fn wrap_string_lower(seed: u64, slen: i64, mode: i64) -> u64 {
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0xa5u8; DSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    let mut i = 0usize;
    loop {
        let c = sbuf[i];
        dbuf[i] = tolower_c(c);
        if c == 0 {
            break;
        }
        i += 1;
    }
    mix(n as u64, fnv1a(&dbuf), 0)
}

/// Port of `uint64_t wrap_kstrdup_quotable(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1176`).
pub fn wrap_kstrdup_quotable(seed: u64, slen: i64, mode: i64) -> u64 {
    const ESC: &[u8] = b"\x0c\n\r\t\x0b\x07\x1b\\\""; // "\f\n\r\t\v\a\e\\\""
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    if mode & 8 != 0 {
        // kstrdup_quotable(NULL) -> NULL: h = 0, r != NULL is false
        return mix(n as u64, 0, 0);
    }
    let sl = cstrlen(&sbuf);
    let dlen = string_escape_mem(&sbuf[..sl], sl, &mut [], ESCAPE_HEX, Some(ESC));
    let mut dst = vec![0u8; dlen + 1];
    string_escape_mem(&sbuf[..sl], sl, &mut dst[..dlen], ESCAPE_HEX, Some(ESC));
    dst[dlen] = 0;
    let h = fnv1a(&dst[..cstrlen(&dst) + 1]);
    mix(n as u64, h, 1)
}

/// Port of `uint64_t wrap_kstrdup_and_replace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1189`).
pub fn wrap_kstrdup_and_replace(seed: u64, slen: i64, old: i64, new: i64, mode: i64) -> u64 {
    let n = blen(slen, SSZ - 1);
    let mut sbuf = [0u8; SSZ];

    make_str(&mut sbuf, n, seed, mode & 3);
    let sl = cstrlen(&sbuf);
    let mut dst = sbuf[..sl + 1].to_vec(); // kstrdup
    strreplace(&mut dst, old as u8, new as u8);
    let h = fnv1a(&dst[..cstrlen(&dst) + 1]);
    mix(n as u64, h, 1)
}
