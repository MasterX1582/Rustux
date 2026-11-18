//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/seq_buf.c and the include/linux/seq_buf.h inline
//! layer, driven through the same seeded harness as the C wrappers.
//!
//! Pointer model: `s->buffer` is a fixed backing array inside `SeqBuf`;
//! putmem/hex_dump source data and printf/puts/prefix strings are offsets
//! into `data`/`strpool` pools, and `%p` arguments carry a plain `u64`
//! offset (the C shim prints `arg - data_base`, identical by construction).
//! `copy_to_user` is the deterministic fault model (kshim_uaccess_ok).
//!
//! The vsnprintf/vscnprintf family is a DETERMINISTIC MINI-PRINTER shared
//! verbatim with the C shim (linux/sprintf.h) — same supported subset,
//! same output bytes by construction; real printf formatting is a
//! separate unported unit.
//!
//! Out of scope, same as the C harness: seq_buf_print_seq (needs a live
//! seq_file), seq_buf_path (needs d_path/VFS), seq_buf_bprintf
//! (CONFIG_BINARY_PRINTF undefined).

#![forbid(unsafe_code)]
// pub fn names mirror the wrappers.c export names verbatim.
#![allow(non_snake_case)]

const BSZ: usize = 256;
const DSZ: usize = 192;
const SPSZ: usize = 512;
const USZ: usize = 128;
const KLOG: usize = 4096;

// ---- harness mirror --------------------------------------------------------

/// `struct seq_buf`: `buffer` is a fixed backing store (the C harness's
/// static `sbuf`); `size`/`len` mirror the C fields.
struct SeqBuf {
    buf: [u8; BSZ],
    size: usize,
    len: usize,
}

struct Rig {
    s: SeqBuf,
    data: [u8; DSZ],
    strpool: [u8; SPSZ],
    ubuf: [u8; USZ],
    klog: [u8; KLOG],
    klog_len: usize,
    uaccess_ok: usize, // kshim_uaccess_ok: usize::MAX = copy never faults
    rng: u64,
    h: u64,
}

fn fill(pool: &mut [u8], rng: &mut u64) {
    for b in pool.iter_mut() {
        *b = xs64(rng) as u8;
    }
}

/// Shared wrapper prologue: reset_all + rng = seed|1 + pool fills +
/// seq_buf_init(size). `strpool_first` covers wrap_seqbuf_printf's fill
/// order (strpool then data); the others fill data then strpool.
fn rig_seeded(seed: u64, size: usize, strpool_first: bool) -> Rig {
    let mut r = rig_new();
    reset_all(&mut r);
    r.rng = seed | 1;
    if strpool_first {
        fill(&mut r.strpool, &mut r.rng);
        r.strpool[SPSZ - 1] = 0;
        fill(&mut r.data, &mut r.rng);
    } else {
        fill(&mut r.data, &mut r.rng);
        fill(&mut r.strpool, &mut r.rng);
        r.strpool[SPSZ - 1] = 0;
    }
    seq_buf_init(&mut r.s, size);
    r
}

fn xs64(r: &mut u64) -> u64 {
    let mut x = *r;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *r = x;
    x.wrapping_mul(2685821657736338717)
}

fn iabs(v: i64) -> u64 {
    v.unsigned_abs()
}

fn fold(h: &mut u64, v: u64) {
    *h = (*h ^ v).wrapping_mul(0x100000001b3);
}

fn fnv1a(b: &[u8]) -> u64 {
    let mut h = 14695981039346656037_u64;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn rig_new() -> Rig {
    Rig {
        s: SeqBuf {
            buf: [0; BSZ],
            size: 0,
            len: 0,
        },
        data: [0; DSZ],
        strpool: [0; SPSZ],
        ubuf: [0; USZ],
        klog: [0; KLOG],
        klog_len: 0,
        uaccess_ok: usize::MAX,
        rng: 0,
        h: 0x9e3779b97f4a7c15,
    }
}

/// C `reset_all()`: memset pools, deterministic stale sbuf bytes, fold seed.
fn reset_all(r: &mut Rig) {
    r.rng = 0;
    r.data = [0; DSZ];
    r.strpool = [0; SPSZ];
    r.ubuf = [0; USZ];
    r.klog = [0; KLOG];
    r.klog_len = 0;
    r.uaccess_ok = usize::MAX;
    r.h = 0x9e3779b97f4a7c15;
    for (i, b) in r.s.buf.iter_mut().enumerate() {
        *b = (0x40 + (i * 37 % 59)) as u8;
    }
}

/// C `lay_str()`: n seeded body bytes + NUL at `off`, returns off.
/// Draws one xs64 per body byte — keep the draw count identical.
fn lay_str(r: &mut Rig, off: usize, n: usize) -> usize {
    for i in 0..n {
        r.strpool[off + i] = b'A' + (xs64(&mut r.rng) % 58) as u8;
    }
    r.strpool[off + n] = 0;
    off
}

/// strlen over a pool slice: index of first NUL, or slice len (the C
/// pools always hold a NUL within bounds, so this stays in-contract).
fn cstrlen(b: &[u8]) -> usize {
    b.iter().position(|&c| c == 0).unwrap_or(b.len())
}

// ---- deterministic mini-printer (linux/sprintf.h, shared semantics) ------

/// Format argument: models the promoted varargs the harness call sites
/// pass. `S` is a byte slice containing a NUL (read like `const char *`);
/// `P` is the %p arg — already a pool offset (C prints `p - data_base`).
#[derive(Clone, Copy)]
enum Arg<'a> {
    I(i64),
    U(u64),
    C(i64),
    S(&'a [u8]),
    P(u64),
}

struct FmtBuf<'a> {
    buf: &'a mut [u8], // dst; size = buf.len() like C's (buf, size) pair
    pos: usize,        // would-be length
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
    let v = match args[*i] {
        Arg::I(v) | Arg::C(v) => v,
        Arg::U(v) => v as i64,
        Arg::S(_) | Arg::P(_) => 0,
    };
    *i += 1;
    v
}

fn peek(s: &[u8], p: usize) -> u8 {
    s.get(p).copied().unwrap_or(0)
}

/// Least-significant digits of m in `base` into tmp, returns count.
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

/// Port of kshim_vsnprintf(): libc snprintf semantics for the shim
/// subset — at most dst.len()-1 chars + NUL (when dst nonempty),
/// returns the would-be length. Length modifiers are consumed but
/// unneeded: each Arg already carries the width the C va_arg would read.
fn kshim_vsnprintf(dst: &mut [u8], fmt: &[u8], args: &[Arg]) -> i64 {
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
        // width
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
                width = width * 10 + (fmt[p] - b'0') as i64;
                p += 1;
            }
        }
        // precision
        let mut prec: i64 = -1;
        if peek(fmt, p) == b'.' {
            p += 1;
            if peek(fmt, p) == b'*' {
                prec = arg_i(args, &mut ai) as i32 as i64;
                p += 1;
            } else {
                prec = 0;
                while peek(fmt, p).is_ascii_digit() {
                    prec = prec * 10 + (fmt[p] - b'0') as i64;
                    p += 1;
                }
            }
        }
        // length modifiers: l ll h hh z t — skip (see doc comment)
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
                let s = match args[ai] {
                    Arg::S(s) => s,
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
                let mag = match args[ai] {
                    Arg::P(v) => v,
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
                    let v = match args[ai] {
                        Arg::U(v) => v,
                        Arg::I(v) | Arg::C(v) => v as u64,
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
                    if base == 16 { 2i64 } else { 1 }
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
            0 => break, // trailing % at NUL: C's p-- just lets the loop exit
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

/// Port of kshim_printk(): format into the klog sink, bounded like C.
fn kshim_printk(klog: &mut [u8], klog_len: &mut usize, fmt: &[u8], args: &[Arg]) -> i64 {
    let rem = KLOG - *klog_len;
    let ret = kshim_vsnprintf(&mut klog[*klog_len..], fmt, args);
    let add = if (ret as usize) < rem {
        ret as usize
    } else if rem != 0 {
        rem - 1
    } else {
        0
    };
    *klog_len += add;
    ret
}

// ---- lib/seq_buf.c + include/linux/seq_buf.h ------------------------------

fn seq_buf_clear(s: &mut SeqBuf) {
    s.len = 0;
    if s.size != 0 {
        s.buf[0] = 0;
    }
}

fn seq_buf_init(s: &mut SeqBuf, size: usize) {
    // s.buffer is the fixed backing store; seq_buf_init sets size + clear.
    s.size = size;
    seq_buf_clear(s);
}

fn seq_buf_has_overflowed(s: &SeqBuf) -> bool {
    s.len > s.size
}

fn seq_buf_set_overflow(s: &mut SeqBuf) {
    if s.len < s.size {
        s.buf[s.len..s.size].fill(0);
    }
    s.len = s.size + 1;
}

fn seq_buf_init_append(s: &mut SeqBuf, size: usize) {
    s.size = size;
    s.len = cstrlen(&s.buf[..size]); // strnlen(buf, size)
    if s.len == size {
        seq_buf_set_overflow(s);
    }
}

fn seq_buf_buffer_left(s: &SeqBuf) -> u64 {
    if seq_buf_has_overflowed(s) {
        0
    } else {
        (s.size - s.len) as u64
    }
}

fn seq_buf_used(s: &SeqBuf) -> u64 {
    s.len.min(s.size) as u64
}

/// __seq_buf_terminate: write the NUL, return its offset. Callers guard
/// s.size != 0 exactly like the C inline wrappers do.
fn seq_buf_terminate_off(s: &mut SeqBuf) -> usize {
    let end = if seq_buf_buffer_left(s) != 0 {
        s.len
    } else {
        s.size - 1
    };
    s.buf[end] = 0;
    end
}

fn seq_buf_get_buf(s: &SeqBuf) -> Option<(usize, usize)> {
    // WARN_ON(len > size + 1) — never fires in-contract.
    if s.len < s.size {
        Some((s.len, s.size - s.len))
    } else {
        None
    }
}

fn seq_buf_commit(s: &mut SeqBuf, num: i64) {
    if num < 0 {
        seq_buf_set_overflow(s);
    } else {
        // BUG_ON(len + num > size) — harness stays in contract.
        s.len += num as usize;
    }
}

fn seq_buf_pop(s: &mut SeqBuf) -> i64 {
    if s.len == 0 || seq_buf_has_overflowed(s) {
        return -1;
    }
    s.len -= 1;
    // upstream: (unsigned int)s->buffer[s->len] — buffer is char*, and
    // char is signed on the reference build, so the byte sign-extends.
    (s.buf[s.len] as i8) as i64
}

fn seq_buf_can_fit(s: &SeqBuf, len: usize) -> bool {
    s.len + len <= s.size
}

fn seq_buf_putc(s: &mut SeqBuf, c: u8) -> i64 {
    if seq_buf_can_fit(s, 1) {
        s.buf[s.len] = c;
        s.len += 1;
        return 0;
    }
    seq_buf_set_overflow(s);
    -1
}

/// sized_strscpy(dst, src, dst.len()): copies the NUL-terminated src,
/// at most dst.len() bytes with the last forced NUL. Return value is
/// unused by seq_buf_puts (it only cares what landed in the buffer).
fn sized_strscpy(dst: &mut [u8], src: &[u8]) {
    let count = dst.len();
    if count == 0 {
        return; // C: -E2BIG, return value unused by the caller
    }
    let mut res = 0usize;
    while res + 1 < count {
        let c = src.get(res).copied().unwrap_or(0);
        dst[res] = c;
        if c == 0 {
            return;
        }
        res += 1;
    }
    dst[res] = 0;
}

fn seq_buf_puts(s: &mut SeqBuf, str_: &[u8]) -> i64 {
    // +1 for the trailing NUL which must be there
    let len = cstrlen(str_) + 1;
    if seq_buf_can_fit(s, len) {
        s.buf[s.len..s.len + len].copy_from_slice(&str_[..len]);
        s.len += len - 1; // don't count the NUL against capacity
        return 0;
    }
    // copy what fits, so the buffer never holds stale bytes (strscpy)
    if s.len < s.size {
        sized_strscpy(&mut s.buf[s.len..s.size], str_);
        s.len = s.size;
    }
    seq_buf_set_overflow(s);
    -1
}

fn seq_buf_putmem(s: &mut SeqBuf, mem: &[u8]) -> i64 {
    let len = mem.len();
    if seq_buf_can_fit(s, len) {
        s.buf[s.len..s.len + len].copy_from_slice(mem);
        s.len += len;
        return 0;
    }
    if s.len < s.size {
        let n = s.size - s.len;
        s.buf[s.len..s.len + n].copy_from_slice(&mem[..n]);
        s.len = s.size;
    }
    seq_buf_set_overflow(s);
    -1
}

fn hex_asc_hi(x: u8) -> u8 {
    b"0123456789abcdef"[(x >> 4) as usize]
}

fn hex_asc_lo(x: u8) -> u8 {
    b"0123456789abcdef"[(x & 0x0f) as usize]
}

const MAX_MEMHEX_BYTES: usize = 8;
const HEX_CHARS: usize = MAX_MEMHEX_BYTES * 2 + 1;

fn seq_buf_putmem_hex(s: &mut SeqBuf, data: &[u8]) -> i64 {
    let mut hex = [0u8; HEX_CHARS];
    let mut off = 0usize;
    let mut len = data.len();

    while len != 0 {
        let start_len = len.min(MAX_MEMHEX_BYTES);
        let mut j = 0usize;
        // little-endian build: bytes reversed within the chunk
        for i in (0..start_len).rev() {
            hex[j] = hex_asc_hi(data[off + i]);
            j += 1;
            hex[j] = hex_asc_lo(data[off + i]);
            j += 1;
        }
        // WARN_ON_ONCE(j == 0 || j/2 > len) — never fires in-contract
        hex[j] = b' ';
        j += 1;

        if seq_buf_putmem(s, &hex[..j]) != 0 {
            return -1;
        }
        if seq_buf_has_overflowed(s) {
            return -1;
        }
        len -= start_len;
        off += start_len;
    }
    0
}

fn seq_buf_vprintf(s: &mut SeqBuf, fmt: &[u8], args: &[Arg]) -> i64 {
    // WARN_ON(s->size == 0) — statement, no control flow.
    if s.len < s.size {
        let len = kshim_vsnprintf(&mut s.buf[s.len..s.size], fmt, args);
        if (s.len as i64 + len) < s.size as i64 {
            s.len += len as usize;
            return 0;
        }
        // vsnprintf wrote as much as fits, so none of it is stale
        s.len = s.size;
    }
    seq_buf_set_overflow(s);
    -1
}

fn seq_buf_printf(s: &mut SeqBuf, fmt: &[u8], args: &[Arg]) -> i64 {
    seq_buf_vprintf(s, fmt, args)
}

fn seq_buf_to_user(s: &SeqBuf, ubuf: &mut [u8], start: usize, cnt: i64, uaccess_ok: usize) -> i64 {
    if cnt == 0 {
        return 0;
    }
    let mut len = seq_buf_used(s) as i64;
    if len <= start as i64 {
        return -16; // -EBUSY
    }
    len -= start as i64;
    let mut c = cnt;
    if c > len {
        c = len;
    }
    // copy_to_user: copies uaccess_ok bytes max, returns uncopied count
    let ok = (c as usize).min(uaccess_ok);
    ubuf[..ok].copy_from_slice(&s.buf[start..start + ok]);
    let ret = c as usize - ok;
    if ret == c as usize {
        return -14; // -EFAULT
    }
    c - ret as i64
}

// ---- hex shim (linux/hex.h): hex_dump_to_buffer -----------------------------

fn is_power_of_2(n: u64) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

// isascii(ch) && isprint(ch): printable implies ASCII on the u8 range.
fn isprint(c: u8) -> bool {
    (0x20..=0x7e).contains(&c)
}

/// Port of hex_dump_to_buffer(): same control flow as the C shim; buf is
/// the data slice (ptr), linebuf the caller's 131-byte scratch.
fn hex_dump_to_buffer(
    buf: &[u8],
    mut len: usize,
    mut rowsize: i64,
    mut groupsize: i64,
    linebuf: &mut [u8],
    ascii: bool,
) -> i64 {
    let linebuflen = linebuf.len();
    let mut lx: usize = 0;

    if rowsize != 16 && rowsize != 32 {
        rowsize = 16;
    }
    if len > rowsize as usize {
        len = rowsize as usize; // limit to one line at a time
    }
    if !is_power_of_2(groupsize as u64) || groupsize > 8 {
        groupsize = 1;
    }
    if (len as i64 % groupsize) != 0 {
        groupsize = 1; // no mixed size output
    }

    let ngroups = len as i64 / groupsize;
    let ascii_column = rowsize * 2 + rowsize / groupsize + 1;

    // overflow1: the C goto target's return value
    let overflow = |ascii: bool| -> i64 {
        if ascii {
            ascii_column + len as i64
        } else {
            (groupsize * 2 + 1) * ngroups - 1
        }
    };

    if linebuflen == 0 {
        return overflow(ascii);
    }
    if len == 0 {
        // nil
        linebuf[lx] = 0;
        return lx as i64;
    }

    if matches!(groupsize, 8 | 4 | 2) {
        let (gs, fmt): (usize, &[u8]) = match groupsize {
            8 => (8, b"%s%16.16llx"),
            4 => (4, b"%s%8.8x"),
            _ => (2, b"%s%4.4x"),
        };
        for j in 0..ngroups as usize {
            let o = j * gs;
            // get_unaligned: native LE load (x86-64 reference build)
            let v = match gs {
                8 => u64::from_le_bytes(buf[o..o + 8].try_into().unwrap_or([0; 8])),
                4 => u32::from_le_bytes(buf[o..o + 4].try_into().unwrap_or([0; 4])) as u64,
                _ => u16::from_le_bytes(buf[o..o + 2].try_into().unwrap_or([0; 2])) as u64,
            };
            let ret = kshim_vsnprintf(
                &mut linebuf[lx..],
                fmt,
                &[Arg::S(if j != 0 { b" " } else { b"" }), Arg::U(v)],
            );
            if ret as usize >= linebuflen - lx {
                return overflow(ascii); // overflow1: no NUL write
            }
            lx += ret as usize;
        }
    } else {
        let mut j = 0usize;
        while j < len {
            // each write checks overflow2: NUL at lx, then overflow1's return
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return overflow(ascii);
            }
            let ch = buf[j];
            linebuf[lx] = hex_asc_hi(ch);
            lx += 1;
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return overflow(ascii);
            }
            linebuf[lx] = hex_asc_lo(ch);
            lx += 1;
            if linebuflen < lx + 2 {
                linebuf[lx] = 0;
                return overflow(ascii);
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
        return lx as i64;
    }
    while (lx as i64) < ascii_column {
        if linebuflen < lx + 2 {
            linebuf[lx] = 0;
            return overflow(ascii);
        }
        linebuf[lx] = b' ';
        lx += 1;
    }
    for &ch in buf.iter().take(len) {
        if linebuflen < lx + 2 {
            linebuf[lx] = 0;
            return overflow(ascii);
        }
        linebuf[lx] = if isprint(ch) { ch } else { b'.' };
        lx += 1;
    }
    linebuf[lx] = 0;
    lx as i64
}

/// Port of seq_buf_hex_dump(). `buf` is the data slice; `prefix` the
/// prefix string bytes; `data_off` is the pool offset of `buf` (for %p).
#[allow(clippy::too_many_arguments)] // mirrors the C signature verbatim
fn seq_buf_hex_dump(
    s: &mut SeqBuf,
    prefix: &[u8],
    prefix_type: i64,
    mut rowsize: i64,
    groupsize: i64,
    buf: &[u8],
    data_off: usize,
    len: usize,
    ascii: bool,
) -> i64 {
    let mut linebuf = [0u8; 32 * 3 + 2 + 32 + 1];
    if rowsize != 16 && rowsize != 32 {
        rowsize = 16;
    }
    let mut remaining = len as i64;
    let mut i = 0usize;
    while (i as i64) < len as i64 {
        let linelen = (remaining.min(rowsize)) as usize;
        remaining -= rowsize;

        hex_dump_to_buffer(&buf[i..], linelen, rowsize, groupsize, &mut linebuf, ascii);

        let ret = match prefix_type {
            1 => {
                // DUMP_PREFIX_ADDRESS: "%s%p: %s\n", prefix, ptr + i, linebuf
                seq_buf_printf(
                    s,
                    b"%s%p: %s\n",
                    &[
                        Arg::S(prefix),
                        Arg::P((data_off + i) as u64),
                        Arg::S(&linebuf),
                    ],
                )
            }
            2 => {
                // DUMP_PREFIX_OFFSET: "%s%.8x: %s\n"
                seq_buf_printf(
                    s,
                    b"%s%.8x: %s\n",
                    &[Arg::S(prefix), Arg::U(i as u64), Arg::S(&linebuf)],
                )
            }
            _ => {
                // DUMP_PREFIX_NONE and everything else: "%s%s\n"
                seq_buf_printf(s, b"%s%s\n", &[Arg::S(prefix), Arg::S(&linebuf)])
            }
        };
        if ret != 0 {
            return ret;
        }
        i += rowsize as usize;
    }
    0
}

// ---- fold helpers ----------------------------------------------------------

fn fold_state(r: &mut Rig) {
    let (len, size) = (r.s.len as u64, r.s.size as u64);
    fold(&mut r.h, len);
    fold(&mut r.h, size);
    let ov = seq_buf_has_overflowed(&r.s) as u64;
    fold(&mut r.h, ov);
    let left = seq_buf_buffer_left(&r.s);
    fold(&mut r.h, left);
    let used = seq_buf_used(&r.s);
    fold(&mut r.h, used);
}

fn fold_all(r: &mut Rig) {
    fold_state(r);
    let h1 = fnv1a(&r.s.buf);
    fold(&mut r.h, h1);
    let h2 = fnv1a(&r.ubuf);
    fold(&mut r.h, h2);
    let h3 = fnv1a(&r.klog[..r.klog_len]);
    fold(&mut r.h, h3);
    let kl = r.klog_len as u64;
    fold(&mut r.h, kl);
}

/// C `do_printf()`: the 12 format call sites, same arg types/order.
/// `p` is seq_buf_printf, `sp` the strpool slice at `soff`.
fn do_printf(s: &mut SeqBuf, strpool: &[u8], fidx: u64, a: u64, b: u64, soff: usize) -> i64 {
    let sp = &strpool[soff..];
    let (i, u) = (a as i32 as i64, b as u32 as u64); // common (int)a, (unsigned)b
    match fidx % 12 {
        0 => seq_buf_printf(s, b"x=%d y=%u", &[Arg::I(i), Arg::U(u)]),
        1 => seq_buf_printf(s, b"s=%s n=%x", &[Arg::S(sp), Arg::U(u)]),
        2 => seq_buf_printf(
            s,
            b"%5d|%-8x|%c",
            &[Arg::I(i), Arg::U(u), Arg::C((a & 0x7f) as i64)],
        ),
        3 => seq_buf_printf(
            s,
            b"%.3s:%.*s",
            &[Arg::S(sp), Arg::I((b % 40) as i64), Arg::S(sp)],
        ),
        4 => seq_buf_printf(s, b"p=%p", &[Arg::P(a % DSZ as u64)]),
        5 => seq_buf_printf(s, b"%08lx", &[Arg::U(a | 1)]),
        6 => seq_buf_printf(s, b"%%done %04u", &[Arg::U(b % 10000)]),
        7 => seq_buf_printf(s, b"%*d", &[Arg::I((b % 12) as i64), Arg::I(i)]),
        8 => seq_buf_printf(s, b"%-+6i", &[Arg::I(i)]),
        9 => seq_buf_printf(s, b"%#x %#o", &[Arg::U(a as u32 as u64), Arg::U(u)]),
        10 => seq_buf_printf(s, b"%llu/%llx", &[Arg::U(a), Arg::U(b)]),
        _ => seq_buf_printf(s, b"%.8x:%-10.5s|", &[Arg::U(u), Arg::S(sp)]),
    }
}

// ---- exported wrappers -----------------------------------------------------

/// Port of `wrap_seqbuf_seq`: seeded op-mix driver.
pub fn wrap_seqbuf_seq(seed: u64, bsz_i: i64, nops_i: i64) -> u64 {
    let size = (iabs(bsz_i) % (BSZ as u64 + 1)) as usize;
    let nops = (iabs(nops_i) % 150) as usize;
    let mut r = rig_seeded(seed, size, false);

    for _ in 0..nops {
        let x = xs64(&mut r.rng);
        let op = (x % 16) as u32;

        match op {
            0 | 1 => {
                // putc
                let c = (x >> 8) as u8;
                let v = seq_buf_putc(&mut r.s, c);
                fold(&mut r.h, v as u64);
            }
            2 => {
                // puts
                let n = ((x >> 8) % 40) as usize;
                let off = lay_str(&mut r, ((x >> 16) as usize) % (SPSZ - 64), n);
                let v = seq_buf_puts(&mut r.s, &r.strpool[off..]);
                fold(&mut r.h, v as u64);
            }
            3 | 4 => {
                // putmem
                let off = ((x >> 8) % DSZ as u64) as usize;
                let mut n = ((x >> 20) % 24) as usize;
                if n > DSZ - off {
                    n = DSZ - off;
                }
                let v = seq_buf_putmem(&mut r.s, &r.data[off..off + n]);
                fold(&mut r.h, v as u64);
            }
            5 => {
                // putmem_hex
                let off = ((x >> 8) % DSZ as u64) as usize;
                let mut n = ((x >> 20) % 20) as usize;
                if n > DSZ - off {
                    n = DSZ - off;
                }
                let v = seq_buf_putmem_hex(&mut r.s, &r.data[off..off + n]);
                fold(&mut r.h, v as u64);
            }
            6 => {
                // printf. C is `do_printf(idx, xs64(), xs64(), off)` and GCC
                // evaluates call args right-to-left: the param-a xs64() draws
                // SECOND. Match: b draws first, then a.
                let off = lay_str(
                    &mut r,
                    ((x >> 8) as usize) % (SPSZ - 64),
                    ((x >> 20) % 40) as usize,
                );
                let b = xs64(&mut r.rng);
                let a = xs64(&mut r.rng);
                let v = do_printf(&mut r.s, &r.strpool, (x >> 28) % 12, a, b, off);
                fold(&mut r.h, v as u64);
            }
            7 => {
                // get_buf + commit
                let gb = seq_buf_get_buf(&r.s);
                let (n, poff) = match gb {
                    Some((o, n)) => (n, o as u64),
                    None => (0, 0xffff),
                };
                fold(&mut r.h, n as u64);
                fold(&mut r.h, poff);
                let k = if n != 0 {
                    ((x >> 8) % (n as u64 + 1)) as usize
                } else {
                    0
                };
                if let Some((o, _)) = gb {
                    for j in 0..k {
                        r.s.buf[o + j] = xs64(&mut r.rng) as u8;
                    }
                }
                if seq_buf_has_overflowed(&r.s) || (x & 0x20 != 0) {
                    seq_buf_commit(&mut r.s, -1);
                } else {
                    seq_buf_commit(&mut r.s, k as i64);
                }
            }
            8 => {
                let v = seq_buf_pop(&mut r.s);
                fold(&mut r.h, v as u64);
            }
            9 => {
                seq_buf_clear(&mut r.s);
                fold_state(&mut r);
            }
            10 => {
                // init_append with seeded content
                let k = ((x >> 8) % (size as u64 + 2)) as usize;
                for (j, b) in r.s.buf.iter_mut().enumerate().take(size) {
                    *b = b'a' + ((x + j as u64) % 26) as u8;
                }
                if k < size {
                    r.s.buf[k] = 0;
                }
                seq_buf_init_append(&mut r.s, size);
                fold_state(&mut r);
            }
            11 => {
                if x & 0x40 != 0 {
                    seq_buf_set_overflow(&mut r.s);
                }
                let a = seq_buf_has_overflowed(&r.s) as u64;
                fold(&mut r.h, a);
                let b = seq_buf_buffer_left(&r.s);
                fold(&mut r.h, b);
                let c = seq_buf_used(&r.s);
                fold(&mut r.h, c);
            }
            12 => match (x >> 8) % 3 {
                0 => {
                    // seq_buf_str
                    if r.s.size == 0 {
                        let e = fnv1a(b"");
                        fold(&mut r.h, e);
                        fold(&mut r.h, 0);
                    } else {
                        seq_buf_terminate_off(&mut r.s);
                        let n = cstrlen(&r.s.buf[..r.s.size]);
                        let fh = fnv1a(&r.s.buf[..n]);
                        fold(&mut r.h, fh);
                        fold(&mut r.h, n as u64);
                    }
                }
                1 => {
                    // seq_buf_strlen
                    let v = if r.s.size == 0 {
                        0
                    } else {
                        seq_buf_terminate_off(&mut r.s)
                    };
                    fold(&mut r.h, v as u64);
                }
                _ => {
                    if r.s.size != 0 {
                        seq_buf_terminate_off(&mut r.s);
                    }
                    fold(&mut r.h, 0x5e);
                }
            },
            13 => {
                // to_user
                let start = ((x >> 8) % (size as u64 + 2)) as usize;
                let cnt = ((x >> 20) % (USZ as u64 + 1)) as i64;
                if x & 0x40 != 0 {
                    r.uaccess_ok = ((x >> 28) % (cnt as u64 + 1)) as usize;
                } else {
                    r.uaccess_ok = usize::MAX;
                }
                let v = seq_buf_to_user(&r.s, &mut r.ubuf, start, cnt, r.uaccess_ok);
                r.uaccess_ok = usize::MAX;
                fold(&mut r.h, v as u64);
                let fh = fnv1a(&r.ubuf);
                fold(&mut r.h, fh);
            }
            14 => {
                // hex_dump
                const ROWS: [i64; 4] = [16, 32, 8, 24];
                const GRPS: [i64; 6] = [1, 2, 4, 8, 3, 16];
                let off = ((x >> 8) % DSZ as u64) as usize;
                let mut n = ((x >> 20) % 48) as usize;
                let poff = lay_str(&mut r, ((x >> 32) as usize) % (SPSZ - 64), 4);
                if n > DSZ - off {
                    n = DSZ - off;
                }
                let v = seq_buf_hex_dump(
                    &mut r.s,
                    &r.strpool[poff..],
                    ((x >> 40) % 4) as i64,
                    ROWS[((x >> 44) % 4) as usize],
                    GRPS[((x >> 48) % 6) as usize],
                    &r.data[off..off + n],
                    off,
                    n,
                    (x >> 52) & 1 != 0,
                );
                fold(&mut r.h, v as u64);
            }
            _ => {
                // do_printk + state
                lay_str(&mut r, 0, 4); // lvl string
                // seq_buf_do_printk: line-split the terminated buffer.
                if r.s.size != 0 && r.s.len != 0 {
                    seq_buf_terminate_off(&mut r.s);
                    let mut start = 0usize;
                    loop {
                        // strchr(start, '\n'): stops at NUL too — find the
                        // first byte that is 0 or '\n' and match only '\n'.
                        let lf = {
                            let mut hit = None;
                            for (i, &c) in r.s.buf[start..].iter().enumerate() {
                                if c == 0 {
                                    break;
                                }
                                if c == b'\n' {
                                    hit = Some(start + i);
                                    break;
                                }
                            }
                            hit
                        };
                        match lf {
                            Some(l) => {
                                let llen = l - start + 1;
                                kshim_printk(
                                    &mut r.klog,
                                    &mut r.klog_len,
                                    b"%s%.*s",
                                    &[
                                        Arg::S(&r.strpool[..]),
                                        Arg::I(llen as i64),
                                        Arg::S(&r.s.buf[start..start + llen]),
                                    ],
                                );
                                start = l + 1;
                            }
                            None => break,
                        }
                    }
                    if r.s.buf[start] != 0 {
                        kshim_printk(
                            &mut r.klog,
                            &mut r.klog_len,
                            b"%s%s\n",
                            &[Arg::S(&r.strpool[..]), Arg::S(&r.s.buf[start..])],
                        );
                    }
                }
                let fh = fnv1a(&r.klog[..r.klog_len]);
                fold(&mut r.h, fh);
                fold_state(&mut r);
            }
        }
        if op == 12 {
            fold_state(&mut r);
        }
    }
    fold_all(&mut r);
    r.h
}

/// Port of `wrap_seqbuf_printf`: printf/puts mix over the format table.
pub fn wrap_seqbuf_printf(seed: u64, bsz_i: i64) -> u64 {
    let size = (iabs(bsz_i) % (BSZ as u64 + 1)) as usize;
    let mut r = rig_seeded(seed, size, true);

    for i in 0..24usize {
        let a = xs64(&mut r.rng);
        let b = xs64(&mut r.rng);
        let off = lay_str(&mut r, i * 17 % (SPSZ - 64), (b % 48) as usize);
        let v = do_printf(&mut r.s, &r.strpool, a % 12, a, b, off);
        fold(&mut r.h, v as u64);
        let v = seq_buf_puts(&mut r.s, &r.strpool[i * 13 % (SPSZ - 64)..]);
        fold(&mut r.h, v as u64);
    }
    fold_all(&mut r);
    r.h
}

/// Port of `wrap_seqbuf_hexdump`: all prefixes/row/group/ascii combos.
pub fn wrap_seqbuf_hexdump(seed: u64, bsz_i: i64, n_i: i64) -> u64 {
    let size = (iabs(bsz_i) % (BSZ as u64 + 1)) as usize;
    let n = (iabs(n_i) % 24) as usize;
    let mut r = rig_seeded(seed, size, false);

    const ROWS: [i64; 4] = [16, 32, 8, 48];
    const GRPS: [i64; 6] = [1, 2, 4, 8, 3, 16];
    for i in 0..n {
        let x = xs64(&mut r.rng);
        let off = ((x >> 8) % DSZ as u64) as usize;
        let mut len = ((x >> 20) % 80) as usize;
        let poff = lay_str(
            &mut r,
            ((x >> 32) as usize) % (SPSZ - 64),
            ((x >> 52) % 12) as usize,
        );
        if len > DSZ - off {
            len = DSZ - off;
        }
        let v = seq_buf_hex_dump(
            &mut r.s,
            &r.strpool[poff..],
            (i % 4) as i64,
            ROWS[((x >> 40) % 4) as usize],
            GRPS[((x >> 44) % 6) as usize],
            &r.data[off..off + len],
            off,
            len,
            (x >> 48) & 1 != 0,
        );
        fold(&mut r.h, v as u64);
    }
    fold_all(&mut r);
    r.h
}
