//! Minimal `snprintf`-subset formatter — interim shim semantics.
//!
//! This is NOT a port of `lib/vsprintf.c` (that unit is still unported).
//! It is the deterministic mini-printer that the seq_buf differential
//! harness validated end-to-end: libc `snprintf` semantics for
//! `%c %s %p %d %i %u %x %X %o %%` with `-0+ #` flags, `*`/numeric
//! width and precision, and `l ll h hh z t` length modifiers consumed.
//! Until the real vsprintf lands, `seq_buf_printf` and friends run on
//! this engine.

/// Format argument — models the promoted varargs the C call sites
/// pass. `S` is a byte slice treated like `const char *` (stops at the
/// first NUL or the slice end). `P` is the `%p` argument.
#[derive(Clone, Copy)]
pub enum Arg<'a> {
    /// `%d`/`%i`/`%u`/`%o`/`%x`/`%X`/`%c` numeric argument.
    I(i64),
    /// unsigned numeric argument.
    U(u64),
    /// `%c` character argument.
    C(i64),
    /// `%s` string argument (NUL-terminated convention).
    S(&'a [u8]),
    /// `%p` pointer argument, printed as `0x<hex>`.
    P(u64),
}

struct FmtBuf<'a> {
    buf: &'a mut [u8],
    pos: usize, // would-be length
}

impl<'a> FmtBuf<'a> {
    fn putc(&mut self, c: u8) {
        if self.pos + 1 < self.buf.len() {
            self.buf[self.pos] = c;
        }
        self.pos += 1;
    }
    fn emitn(&mut self, c: u8, n: i64) {
        for _ in 0..n.max(0) {
            self.putc(c);
        }
    }
    fn emitstr(&mut self, s: &[u8], prec: i64) {
        let mut i = 0usize;
        while i < s.len() && s[i] != 0 && (prec < 0 || (i as i64) < prec) {
            let c = s[i];
            self.putc(c);
            i += 1;
        }
    }
}

fn arg_i(args: &[Arg], i: &mut usize) -> i64 {
    let v = match args.get(*i) {
        Some(Arg::I(v)) | Some(Arg::C(v)) => *v,
        Some(Arg::U(v)) => *v as i64,
        _ => 0,
    };
    *i += 1;
    v
}

fn peek(s: &[u8], p: usize) -> u8 {
    s.get(p).copied().unwrap_or(0)
}

/// Kernel clamps from lib/vsprintf.c (`set_field_width`/`set_precision`).
const FIELD_WIDTH_MAX: i64 = (1 << 23) - 1;
const PRECISION_MAX: i64 = (1 << 15) - 1;

/// Least-significant digits of `m` in `base` into `tmp`; returns count.
fn to_digits(mut m: u64, base: u64, digits: &[u8], tmp: &mut [u8]) -> usize {
    let mut n = 0;
    loop {
        tmp[n] = digits[(m % base) as usize];
        n += 1;
        m /= base;
        if m == 0 {
            return n;
        }
    }
}

/// `vsnprintf(dst, fmt, args)` — writes at most `dst.len()-1` chars plus
/// a NUL (when `dst` is nonempty) and returns the would-be length, like
/// the C library function. `fmt` is read as a C string (stops at NUL).
/// Out-of-args reads yield zero; unknown conversions print literally.
pub fn vsnprintf(dst: &mut [u8], fmt: &[u8], args: &[Arg]) -> i64 {
    let mut f = FmtBuf { buf: dst, pos: 0 };
    let mut ai = 0usize;
    let mut p = 0usize;

    while p < fmt.len() && fmt[p] != 0 {
        if fmt[p] != b'%' {
            f.putc(fmt[p]);
            p += 1;
            continue;
        }
        p += 1;
        // flags
        let (mut left, mut zero, mut plus, mut space, mut alt) =
            (false, false, false, false, false);
        while p < fmt.len() {
            match fmt[p] {
                b'-' => left = true,
                b'0' => zero = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'#' => alt = true,
                _ => break,
            }
            p += 1;
        }
        // width (kernel clamps to FIELD_WIDTH_MAX via set_field_width)
        let mut width: i64 = 0;
        if peek(fmt, p) == b'*' {
            width = arg_i(args, &mut ai) as i32 as i64;
            if width < 0 {
                left = true;
                width = -width;
            }
            p += 1;
        } else {
            while peek(fmt, p).is_ascii_digit() {
                width = width
                    .saturating_mul(10)
                    .saturating_add((fmt[p] - b'0') as i64);
                p += 1;
            }
        }
        width = width.min(FIELD_WIDTH_MAX);
        // precision: kernel's set_precision clamps to [0, PRECISION_MAX];
        // this shim keeps negative `*` precision as "omitted" (glibc
        // semantics), clamping only the upper end.
        let mut prec: i64 = -1;
        if peek(fmt, p) == b'.' {
            p += 1;
            if peek(fmt, p) == b'*' {
                prec = arg_i(args, &mut ai) as i32 as i64;
                p += 1;
            } else {
                prec = 0;
                while peek(fmt, p).is_ascii_digit() {
                    prec = prec
                        .saturating_mul(10)
                        .saturating_add((fmt[p] - b'0') as i64);
                    p += 1;
                }
            }
            prec = prec.min(PRECISION_MAX);
        }
        // length modifiers: l ll h hh z t — consumed, Arg carries width
        match peek(fmt, p) {
            b'l' => {
                p += 1;
                if peek(fmt, p) == b'l' {
                    p += 1;
                }
            }
            b'h' => {
                p += 1;
                if peek(fmt, p) == b'h' {
                    p += 1;
                }
            }
            b'z' | b't' => p += 1,
            _ => {}
        }
        let conv = peek(fmt, p);
        p += 1;
        match conv {
            b'c' => {
                let c = arg_i(args, &mut ai);
                if !left {
                    f.emitn(b' ', width - 1);
                }
                f.putc(c as i32 as u8);
                if left {
                    f.emitn(b' ', width - 1);
                }
            }
            b's' => {
                let s = match args.get(ai) {
                    Some(Arg::S(s)) => *s,
                    _ => &[],
                };
                ai += 1;
                let mut slen = 0usize;
                while slen < s.len() && s[slen] != 0 && (prec < 0 || (slen as i64) < prec) {
                    slen += 1;
                }
                if !left {
                    f.emitn(b' ', width - slen as i64);
                }
                f.emitstr(s, prec);
                if left {
                    f.emitn(b' ', width - slen as i64);
                }
            }
            b'p' => {
                let mag = match args.get(ai) {
                    Some(Arg::P(v)) => *v,
                    _ => 0,
                };
                ai += 1;
                let mut tmp = [0u8; 32];
                let ndig = to_digits(mag, 16, b"0123456789abcdef", &mut tmp);
                if !left {
                    f.emitn(b' ', width - ndig as i64 - 2);
                }
                f.putc(b'0');
                f.putc(b'x');
                for i in (0..ndig).rev() {
                    f.putc(tmp[i]);
                }
                if left {
                    f.emitn(b' ', width - ndig as i64 - 2);
                }
            }
            b'd' | b'i' | b'u' | b'x' | b'X' | b'o' => {
                let signed = conv == b'd' || conv == b'i';
                let base: u64 = match conv {
                    b'o' => 8,
                    b'd' | b'i' | b'u' => 10,
                    _ => 16,
                };
                let digits: &[u8] = if conv == b'X' {
                    b"0123456789ABCDEF"
                } else {
                    b"0123456789abcdef"
                };
                let (mag, sign): (u64, u8) = if signed {
                    let v = arg_i(args, &mut ai);
                    let neg = v < 0;
                    let sgn = if neg {
                        b'-'
                    } else if plus {
                        b'+'
                    } else if space {
                        b' '
                    } else {
                        0
                    };
                    (
                        if neg {
                            (v as u64).wrapping_neg()
                        } else {
                            v as u64
                        },
                        sgn,
                    )
                } else {
                    let v = match args.get(ai) {
                        Some(Arg::U(v)) => *v,
                        Some(Arg::I(v)) | Some(Arg::C(v)) => *v as u64,
                        _ => 0,
                    };
                    ai += 1;
                    (v, if alt && v != 0 { b'#' } else { 0 })
                };
                let mut tmp = [0u8; 32];
                let ndig = to_digits(mag, base, digits, &mut tmp);
                let pad = (prec - ndig as i64).max(0);
                // '#' means 0x/0X/0 prefix for x/X/o
                let prefix = if sign == b'#' {
                    if base == 16 {
                        2i64
                    } else {
                        1
                    }
                } else {
                    (sign != 0) as i64
                };
                let total = ndig as i64 + pad + prefix;
                if !left && !(zero && prec < 0) {
                    f.emitn(b' ', width - total);
                }
                if sign == b'#' {
                    f.putc(b'0');
                    if base == 16 {
                        f.putc(conv); // 'x' or 'X'
                    }
                } else if sign != 0 {
                    f.putc(sign);
                }
                if !left && zero && prec < 0 {
                    f.emitn(b'0', width - total);
                }
                f.emitn(b'0', pad);
                for i in (0..ndig).rev() {
                    f.putc(tmp[i]);
                }
                if left {
                    f.emitn(b' ', width - total);
                }
            }
            b'%' => f.putc(b'%'),
            0 => break, // trailing % at NUL: loop exits like C's p--
            _ => {
                // unknown conversion: emit it literally
                f.putc(b'%');
                f.putc(conv);
            }
        }
    }
    if !f.buf.is_empty() {
        let end = if f.pos < f.buf.len() {
            f.pos
        } else {
            f.buf.len() - 1
        };
        f.buf[end] = 0;
    }
    f.pos as i64
}
