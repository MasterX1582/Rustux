//! Sequential buffer writer — port of `lib/seq_buf.c` +
//! `include/linux/seq_buf.h`.
//!
//! `struct seq_buf` is [`SeqBuf`], borrowing a caller-owned backing
//! store. `size` is the logical capacity (`<= buf.len()`), `len` the
//! write position; `len > size` is the overflowed state. `char`-typed
//! return values use `i64` (the reference build's `char` is signed, so
//! `seq_buf_pop` sign-extends — that is upstream behaviour, not a bug).
//!
//! `seq_buf_printf`/`vprintf` run on [`crate::fmt::vsnprintf`], the
//! interim mini-printer — real kernel printf lands with `lib/vsprintf.c`.
//! `seq_buf_print_seq`, `seq_buf_path`, and `seq_buf_bprintf` are
//! unported: they need `seq_file`, `d_path`, and `CONFIG_BINARY_PRINTF`.

use crate::fmt::{vsnprintf, Arg};

/// `struct seq_buf`: a cursor over a caller-owned buffer.
pub struct SeqBuf<'a> {
    buf: &'a mut [u8],
    size: usize,
    len: usize,
}

fn cstrlen(b: &[u8]) -> usize {
    b.iter().position(|&c| c == 0).unwrap_or(b.len())
}

impl<'a> SeqBuf<'a> {
    /// `seq_buf_init`: start empty, capacity `buf.len()`.
    pub fn new(buf: &'a mut [u8]) -> Self {
        let mut s = SeqBuf {
            buf,
            size: 0,
            len: 0,
        };
        s.size = s.buf.len();
        s.clear();
        s
    }

    /// `seq_buf_init` with an explicit `size` (may be < buf.len()).
    pub fn with_size(buf: &'a mut [u8], size: usize) -> Self {
        assert!(size <= buf.len(), "seq_buf: size exceeds backing store");
        let mut s = SeqBuf { buf, size, len: 0 };
        s.clear();
        s
    }

    /// `seq_buf_clear`.
    pub fn clear(&mut self) {
        self.len = 0;
        if self.size != 0 {
            self.buf[0] = 0;
        }
    }

    /// `seq_buf_init` (re-init with a possibly different size).
    pub fn init(&mut self, size: usize) {
        assert!(size <= self.buf.len(), "seq_buf: size exceeds backing store");
        self.size = size;
        self.clear();
    }

    /// `seq_buf_has_overflowed`.
    pub fn has_overflowed(&self) -> bool {
        self.len > self.size
    }

    /// `seq_buf_set_overflow`: zero the tail and push len past size.
    pub fn set_overflow(&mut self) {
        if self.len < self.size {
            self.buf[self.len..self.size].fill(0);
        }
        self.len = self.size + 1;
    }

    /// `seq_buf_init_append`: initialise on a buffer that already holds
    /// a NUL-terminated prefix; writing continues after it. Unlike
    /// `init`/`new` this does NOT clear the buffer.
    pub fn init_append(buf: &'a mut [u8], size: usize) -> Self {
        assert!(size <= buf.len(), "seq_buf: size exceeds backing store");
        let mut s = SeqBuf { buf, size, len: 0 };
        s.len = cstrlen(&s.buf[..size]); // strnlen(buf, size)
        if s.len == size {
            s.set_overflow();
        }
        s
    }

    /// Release the backing store.
    pub fn into_inner(self) -> &'a mut [u8] {
        self.buf
    }

    /// `seq_buf_buffer_left`.
    pub fn buffer_left(&self) -> usize {
        if self.has_overflowed() {
            0
        } else {
            self.size - self.len
        }
    }

    /// `seq_buf_used` — bytes written, clamped to size.
    pub fn used(&self) -> usize {
        self.len.min(self.size)
    }

    /// `seq_buf_len` — raw len (may exceed size when overflowed).
    /// No `is_empty`: emptiness in C terms is `seq_buf_used() == 0`.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `seq_buf_size` — capacity.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Borrow the written bytes (the `used` prefix).
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[..self.used()]
    }

    /// Borrow the whole backing store (like C reading `s->buffer`).
    pub fn buf(&self) -> &[u8] {
        self.buf
    }

    /// `__seq_buf_terminate`: write a NUL and return its offset.
    fn terminate_off(&mut self) -> usize {
        let end = if self.buffer_left() != 0 {
            self.len
        } else {
            self.size - 1
        };
        self.buf[end] = 0;
        end
    }

    /// `seq_buf_terminate`: NUL-terminate in place.
    pub fn terminate(&mut self) {
        if self.size != 0 {
            self.terminate_off();
        }
    }

    /// `seq_buf_strlen`: NUL-terminate like `seq_buf_str` and return
    /// the offset of the NUL (the string's length unless an earlier NUL
    /// is in the way). Returns 0 for a zero-sized buffer.
    pub fn strlen(&mut self) -> usize {
        if self.size == 0 {
            return 0;
        }
        self.terminate_off()
    }

    /// `seq_buf_str`: NUL-terminate and return the buffer contents up
    /// to and including the NUL (a usable C-string slice). A zero-sized
    /// buffer yields a lone NUL, like C's `""`.
    pub fn str(&mut self) -> &[u8] {
        if self.size == 0 {
            return b"\0";
        }
        let end = self.terminate_off();
        &self.buf[..end + 1]
    }

    /// `seq_buf_get_buf`: the writable tail `(offset, len)`, or None
    /// when full/overflowed. Mirrors C's `*bufp` + `*buflen` out-params;
    /// the caller writes then calls [`SeqBuf::commit`].
    pub fn get_buf(&self) -> Option<(usize, usize)> {
        if self.len < self.size {
            Some((self.len, self.size - self.len))
        } else {
            None
        }
    }

    /// Mutable access to the tail returned by [`SeqBuf::get_buf`].
    pub fn tail_mut(&mut self) -> Option<&mut [u8]> {
        if self.len < self.size {
            Some(&mut self.buf[self.len..self.size])
        } else {
            None
        }
    }

    /// `seq_buf_commit`: advance len by `num` after a `get_buf` write.
    /// C BUG_ONs when `len + num > size`; here that panics on the same
    /// contract violation. Negative num sets the overflow state.
    pub fn commit(&mut self, num: i64) {
        if num < 0 {
            self.set_overflow();
        } else {
            assert!(
                self.len + num as usize <= self.size,
                "seq_buf_commit: len + num > size"
            );
            self.len += num as usize;
        }
    }

    /// `seq_buf_pop`: drop the last byte, return it sign-extended like
    /// the C `(unsigned int)(char)` cast on a signed-char build; -1 on
    /// empty/overflowed.
    pub fn pop(&mut self) -> i64 {
        if self.len == 0 || self.has_overflowed() {
            return -1;
        }
        self.len -= 1;
        (self.buf[self.len] as i8) as i64
    }

    /// `seq_buf_can_fit`.
    pub fn can_fit(&self, len: usize) -> bool {
        // checked_add: C's size_t would wrap on absurd len; can't-fit is
        // the same outcome for every in-contract input.
        self.len.checked_add(len).is_some_and(|t| t <= self.size)
    }

    /// `seq_buf_putc`.
    pub fn putc(&mut self, c: u8) -> i64 {
        if self.can_fit(1) {
            self.buf[self.len] = c;
            self.len += 1;
            return 0;
        }
        self.set_overflow();
        -1
    }

    /// `seq_buf_puts`: write a C-string's bytes plus its NUL; NUL is not
    /// counted against len. `s` is the string bytes — if it contains a
    /// NUL the string ends there, otherwise the slice end is the
    /// implicit terminator (C would read a `const char *`; slicing a
    /// literal without NUL is a C contract violation this model makes
    /// safe). On overflow the tail gets a truncated `strscpy` result
    /// before the overflow state is set.
    pub fn puts(&mut self, s: &[u8]) -> i64 {
        // +1 for the trailing NUL which must be there
        let len = cstrlen(s) + 1;
        if self.can_fit(len) {
            self.buf[self.len..self.len + len - 1].copy_from_slice(&s[..len - 1]);
            self.buf[self.len + len - 1] = 0;
            self.len += len - 1; // don't count the NUL against capacity
            return 0;
        }
        // copy what fits, so the buffer never holds stale bytes (strscpy)
        if self.len < self.size {
            sized_strscpy(&mut self.buf[self.len..self.size], s);
            self.len = self.size;
        }
        self.set_overflow();
        -1
    }

    /// `seq_buf_putmem`.
    pub fn putmem(&mut self, mem: &[u8]) -> i64 {
        let len = mem.len();
        if self.can_fit(len) {
            self.buf[self.len..self.len + len].copy_from_slice(mem);
            self.len += len;
            return 0;
        }
        if self.len < self.size {
            let n = self.size - self.len;
            self.buf[self.len..self.len + n].copy_from_slice(&mem[..n]);
            self.len = self.size;
        }
        self.set_overflow();
        -1
    }

    /// `seq_buf_putmem_hex`: `data` in chunks of up to 8 bytes, each
    /// printed as a reversed hex group followed by a space.
    pub fn putmem_hex(&mut self, data: &[u8]) -> i64 {
        const MAX_MEMHEX_BYTES: usize = 8;
        const HEX_CHARS: usize = MAX_MEMHEX_BYTES * 2 + 1;
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
            hex[j] = b' ';
            j += 1;

            if self.putmem(&hex[..j]) != 0 {
                return -1;
            }
            if self.has_overflowed() {
                return -1;
            }
            len -= start_len;
            off += start_len;
        }
        0
    }

    /// `seq_buf_vprintf` on the interim formatter ([`crate::fmt`]).
    pub fn vprintf(&mut self, fmt: &[u8], args: &[Arg]) -> i64 {
        if self.len < self.size {
            let len = vsnprintf(&mut self.buf[self.len..self.size], fmt, args);
            if (self.len as i64 + len) < self.size as i64 {
                self.len += len as usize;
                return 0;
            }
            // vsnprintf wrote as much as fits, so none of it is stale
            self.len = self.size;
        }
        self.set_overflow();
        -1
    }

    /// `seq_buf_printf`.
    pub fn printf(&mut self, fmt: &[u8], args: &[Arg]) -> i64 {
        self.vprintf(fmt, args)
    }

    /// `seq_buf_to_user` modelled on a destination slice: copies
    /// `min(cnt, used - start, dst.len())` bytes at `buf[start..]`.
    /// Returns bytes copied, 0 for cnt==0, `-EBUSY` when `start` is past
    /// the used region, `-EFAULT` when nothing can be copied — including
    /// negative `cnt`, which a real `copy_to_user` fails outright.
    pub fn to_user(&self, dst: &mut [u8], start: usize, cnt: i64) -> i64 {
        if cnt == 0 {
            return 0;
        }
        let used = self.used();
        if start >= used {
            return -16; // -EBUSY
        }
        if cnt < 0 {
            return -14; // -EFAULT: negative count fails the copy in C
        }
        let c = (cnt as usize).min(used - start);
        // copy_to_user model: dst.len() is the accessible bound
        let ok = c.min(dst.len());
        dst[..ok].copy_from_slice(&self.buf[start..start + ok]);
        let ret = c - ok;
        if ret == c {
            return -14; // -EFAULT
        }
        (c - ret) as i64
    }

    /// `seq_buf_hex_dump`: render `buf` as hex lines. `prefix_type` is
    /// `DUMP_PREFIX_ADDRESS` (1), `DUMP_PREFIX_OFFSET` (2), or
    /// `DUMP_PREFIX_NONE` (0/other). `addr` is the base printed for
    /// prefix_type 1 (C prints `buf + i`).
    #[allow(clippy::too_many_arguments)] // mirrors the C signature
    pub fn hex_dump(
        &mut self,
        prefix: &[u8],
        prefix_type: i64,
        mut rowsize: i64,
        groupsize: i64,
        buf: &[u8],
        addr: u64,
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
            let linelen = remaining.min(rowsize) as usize;
            remaining -= rowsize;

            hex_dump_to_buffer(&buf[i..], linelen, rowsize, groupsize, &mut linebuf, ascii);

            let ret = match prefix_type {
                1 => {
                    // DUMP_PREFIX_ADDRESS: "%s%p: %s\n", prefix, ptr + i
                    self.vprintf(
                        b"%s%p: %s\n",
                        &[Arg::S(prefix), Arg::P(addr.wrapping_add(i as u64)), Arg::S(&linebuf)],
                    )
                }
                2 => {
                    // DUMP_PREFIX_OFFSET: "%s%.8x: %s\n"
                    self.vprintf(
                        b"%s%.8x: %s\n",
                        &[Arg::S(prefix), Arg::U(i as u64), Arg::S(&linebuf)],
                    )
                }
                _ => {
                    // DUMP_PREFIX_NONE and everything else: "%s%s\n"
                    self.vprintf(b"%s%s\n", &[Arg::S(prefix), Arg::S(&linebuf)])
                }
            };
            if ret != 0 {
                return ret;
            }
            i += rowsize as usize;
        }
        0
    }
}

/// `sized_strscpy(dst, src, dst.len())` — copies the NUL-terminated src,
/// at most dst.len() bytes with the last forced NUL.
fn sized_strscpy(dst: &mut [u8], src: &[u8]) {
    let count = dst.len();
    if count == 0 {
        return;
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

fn hex_asc_hi(x: u8) -> u8 {
    b"0123456789abcdef"[(x >> 4) as usize]
}

fn hex_asc_lo(x: u8) -> u8 {
    b"0123456789abcdef"[(x & 0x0f) as usize]
}

fn is_power_of_2(n: u64) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

/// `isascii(ch) && isprint(ch)`: printable implies ASCII on u8.
fn isprint(c: u8) -> bool {
    (0x20..=0x7e).contains(&c)
}

/// `hex_dump_to_buffer()` — format one row of hex into `linebuf`.
/// `rowsize` is clamped to 16/32, `groupsize` to a power of two <= 8.
/// Returns the line length, or the would-be length on truncation.
/// `buf` must hold at least `len` bytes (`len` is clamped to rowsize).
pub fn hex_dump_to_buffer(
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
        len = rowsize as usize;
    }
    // C's groupsize is `int`; any negative sign-extends to a non-pow2
    // unsigned long upstream, so `<= 0` resets to 1 there too.
    if groupsize <= 0 || !is_power_of_2(groupsize as u64) || groupsize > 8 {
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
        linebuf[lx] = 0;
        return lx as i64;
    }

    if matches!(groupsize, 8 | 4 | 2) {
        for j in 0..ngroups as usize {
            let o = j * groupsize as usize;
            // get_unaligned: native LE load (x86-64 reference build).
            // In contract len <= buf.len(), so o + groupsize <= len and
            // the unwrap can't fail; a short buf panics on the slice
            // first, like C's over-read.
            let v = match groupsize {
                8 => u64::from_le_bytes(buf[o..o + 8].try_into().unwrap()),
                4 => u32::from_le_bytes(buf[o..o + 4].try_into().unwrap()) as u64,
                _ => u16::from_le_bytes(buf[o..o + 2].try_into().unwrap()) as u64,
            };
            let ret = vsnprintf(
                &mut linebuf[lx..],
                match groupsize {
                    8 => b"%s%16.16llx",
                    4 => b"%s%8.8x",
                    _ => b"%s%4.4x",
                },
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

#[cfg(test)]
mod tests {
    include!("seqbuf_tests.inc");
}
