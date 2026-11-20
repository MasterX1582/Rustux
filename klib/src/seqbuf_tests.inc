use super::*;
use crate::fmt::Arg;

#[test]
fn init_and_empty() {
    let mut backing = [0u8; 16];
    let s = SeqBuf::new(&mut backing);
    assert_eq!(s.size(), 16);
    assert_eq!(s.len(), 0);
    assert_eq!(s.used(), 0);
    assert_eq!(s.buffer_left(), 16);
    assert!(!s.has_overflowed());
}

#[test]
fn putc_puts() {
    let mut backing = [0u8; 16];
    let mut s = SeqBuf::new(&mut backing);
    assert_eq!(s.putc(b'a'), 0);
    assert_eq!(s.puts(b"bc\0ignored"), 0);
    assert_eq!(s.len(), 3);
    // puts wrote the NUL but doesn't count it
    assert_eq!(s.as_slice(), b"abc");
    assert_eq!(s.buffer_left(), 13);
}

#[test]
fn exact_fit_then_overflow() {
    let mut backing = [0u8; 4];
    let mut s = SeqBuf::new(&mut backing);
    assert_eq!(s.putmem(b"abcd"), 0);
    assert_eq!(s.used(), 4);
    assert_eq!(s.putc(b'x'), -1); // no room
    assert!(s.has_overflowed());
    assert_eq!(s.buffer_left(), 0);
    assert_eq!(s.used(), 4); // used clamps at size
}

#[test]
fn puts_overflow_strscpy() {
    let mut backing = [0u8; 4];
    let mut s = SeqBuf::new(&mut backing);
    // "hello" + NUL = 6 > 4: strscpy writes "hel\0" then overflow
    assert_eq!(s.puts(b"hello"), -1);
    assert!(s.has_overflowed());
    assert_eq!(s.as_slice(), b"hel\0");
}

#[test]
fn putmem_partial() {
    let mut backing = [0u8; 6];
    let mut s = SeqBuf::new(&mut backing);
    s.putmem(b"ab");
    assert_eq!(s.putmem(b"cdefgh"), -1); // 6 > 4 left: writes "cdef"
    assert_eq!(s.as_slice(), b"abcdef");
    assert!(s.has_overflowed());
}

#[test]
fn get_buf_commit() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    let (off, n) = s.get_buf().unwrap();
    assert_eq!((off, n), (0, 8));
    s.tail_mut().unwrap()[..3].copy_from_slice(b"xyz");
    s.commit(3);
    assert_eq!(s.as_slice(), b"xyz");
    s.commit(-1); // error signal -> overflow
    assert!(s.has_overflowed());
    assert!(s.get_buf().is_none());
}

#[test]
#[should_panic]
fn commit_past_end_panics() {
    // C BUG_ON(len + num > size) — same contract violation
    let mut backing = [0u8; 4];
    let mut s = SeqBuf::new(&mut backing);
    s.commit(5);
}

#[test]
fn pop() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    assert_eq!(s.pop(), -1); // empty
    s.putmem(&[b'a', 0x80]);
    // upstream (unsigned int)(char) sign-extends: 0x80 -> -128
    assert_eq!(s.pop(), -128);
    assert_eq!(s.pop(), b'a' as i64);
    assert_eq!(s.pop(), -1);
}

#[test]
fn terminate_str_strlen() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    s.puts(b"hi\0");
    assert_eq!(s.strlen(), 2);
    assert_eq!(s.str(), b"hi\0");
    // full buffer: NUL steals the last byte
    let mut b2 = [0u8; 4];
    let mut s2 = SeqBuf::new(&mut b2);
    s2.putmem(b"abcd");
    assert_eq!(s2.strlen(), 3);
    assert_eq!(s2.str(), b"abc\0");
    assert_eq!(s2.used(), 4); // the lost byte still counts as used
}

#[test]
fn init_append() {
    let mut backing = *b"old\0xxxxx";
    let mut s = SeqBuf::init_append(&mut backing[..], 9);
    assert_eq!(s.len(), 3); // strnlen "old"
    s.puts(b"!\0");
    assert_eq!(s.str(), b"old!\0");
    // no NUL within size -> overflow
    let mut b2 = *b"full";
    let s2 = SeqBuf::init_append(&mut b2[..], 4);
    assert!(s2.has_overflowed());
}

#[test]
fn set_overflow_clears_tail() {
    let mut backing = [0xffu8; 8];
    let mut s = SeqBuf::new(&mut backing);
    s.putmem(b"ab");
    s.set_overflow();
    assert!(s.has_overflowed());
    // bytes len..size zeroed
    assert_eq!(&s.buf()[2..8], &[0; 6]);
}

#[test]
fn clear_resets() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    s.putmem(b"abcd");
    s.set_overflow();
    s.clear();
    assert_eq!(s.len(), 0);
    assert!(!s.has_overflowed());
    assert_eq!(s.buf()[0], 0);
}

#[test]
fn putmem_hex() {
    let mut backing = [0u8; 64];
    let mut s = SeqBuf::new(&mut backing);
    // little-endian: bytes reversed per chunk
    assert_eq!(s.putmem_hex(&[0x01, 0x02, 0x03]), 0);
    assert_eq!(s.as_slice(), b"030201 ");
}

#[test]
fn vprintf_printf() {
    let mut backing = [0u8; 64];
    let mut s = SeqBuf::new(&mut backing);
    assert_eq!(s.printf(b"%s=%d\n", &[Arg::S(b"x"), Arg::I(-42)]), 0);
    assert_eq!(s.as_slice(), b"x=-42\n");
    // overflow: result doesn't fit -> overflow set
    let mut b2 = [0u8; 5];
    let mut s2 = SeqBuf::new(&mut b2);
    assert_eq!(s2.printf(b"%s", &[Arg::S(b"toolong")]), -1);
    assert!(s2.has_overflowed());
}

#[test]
fn to_user() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    s.putmem(b"abcdef");
    let mut dst = [0u8; 8];
    assert_eq!(s.to_user(&mut dst, 0, 0), 0); // cnt==0
    assert_eq!(s.to_user(&mut dst, 0, 3), 3);
    assert_eq!(&dst[..3], b"abc");
    assert_eq!(s.to_user(&mut dst, 2, 10), 4); // clamped to used-start
    assert_eq!(&dst[..4], b"cdef");
    assert_eq!(s.to_user(&mut dst, 6, 4), -16); // -EBUSY past used
    let mut empty = [0u8; 0];
    assert_eq!(s.to_user(&mut empty, 0, 2), -14); // -EFAULT nothing copied
    // short dst: partial copy counted
    let mut tiny = [0u8; 2];
    assert_eq!(s.to_user(&mut tiny, 0, 6), 2);
}

#[test]
fn hex_dump_to_buffer_groups() {
    let mut lb = [0u8; 131];
    let data = [0x11u8, 0x22, 0x33, 0x44];
    // groupsize 1: "11 22 33 44"
    let n = hex_dump_to_buffer(&data, 4, 16, 1, &mut lb, false);
    assert_eq!(&lb[..n as usize], b"11 22 33 44");
    // groupsize 2 (LE): u16 LE loads -> "2211 4433"
    let mut lb2 = [0u8; 131];
    let n = hex_dump_to_buffer(&data, 4, 16, 2, &mut lb2, false);
    assert_eq!(&lb2[..n as usize], b"2211 4433");
    // groupsize 4 (LE): "44332211"
    let mut lb3 = [0u8; 131];
    let n = hex_dump_to_buffer(&data, 4, 16, 4, &mut lb3, false);
    assert_eq!(&lb3[..n as usize], b"44332211");
}

#[test]
fn hex_dump_to_buffer_ascii() {
    let mut lb = [0u8; 131];
    let data = [0x41u8, 0x42, 0x01];
    let n = hex_dump_to_buffer(&data, 3, 16, 1, &mut lb, true);
    let line = &lb[..n as usize];
    assert!(line.starts_with(b"41 42 01 "));
    assert!(line.ends_with(b"AB."));
    // ascii_column = 16*2 + 16/1 + 1 = 49, + 3 ascii chars
    assert_eq!(line.len(), 49 + 3);
}

#[test]
fn hex_dump_to_buffer_linebuf_too_small() {
    let data = [0x11u8; 16];
    let mut tiny = [0u8; 4];
    // overflow1 return: (1*2+1)*16-1 = 47 for gs=1 non-ascii
    let n = hex_dump_to_buffer(&data, 16, 16, 1, &mut tiny, false);
    assert_eq!(n, 47);
}

#[test]
fn hex_dump_prefix_none() {
    let mut backing = [0u8; 256];
    let mut s = SeqBuf::new(&mut backing);
    let data = [0xdeu8, 0xad];
    assert_eq!(s.hex_dump(b"", 0, 16, 1, &data, 0, 2, false), 0);
    assert_eq!(s.as_slice(), b"de ad\n");
}

#[test]
fn hex_dump_prefix_offset() {
    let mut backing = [0u8; 256];
    let mut s = SeqBuf::new(&mut backing);
    let data = [0xdeu8; 32];
    assert_eq!(s.hex_dump(b"pfx ", 2, 16, 1, &data, 0, 32, false), 0);
    let out = s.as_slice();
    assert!(out.starts_with(b"pfx 00000000: de de"));
    // line 1 is 14 + 47 + 1 = 62 bytes; line 2 starts with offset 0x10
    assert_eq!(&out[62..76], b"pfx 00000010: ");
}

#[test]
fn to_user_negative_cnt() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    s.putmem(b"abcdef");
    let mut dst = [0u8; 8];
    // negative cnt: real copy_to_user fails outright -> -EFAULT
    assert_eq!(s.to_user(&mut dst, 0, -1), -14);
    assert_eq!(dst, [0; 8]);
    // but -EBUSY (start past used) still wins ordering like C
    assert_eq!(s.to_user(&mut dst, 9, -1), -16);
}

#[test]
fn puts_unterminated_slice() {
    let mut backing = [0u8; 8];
    let mut s = SeqBuf::new(&mut backing);
    // slice end is the implicit terminator (C contract needs the NUL)
    assert_eq!(s.puts(b"hi"), 0);
    assert_eq!(s.as_slice(), b"hi");
    assert_eq!(s.buf()[2], 0);
}

#[test]
fn can_fit_huge_len() {
    let mut backing = [0u8; 8];
    let s = SeqBuf::new(&mut backing);
    assert!(!s.can_fit(usize::MAX)); // no wrap to true
    assert!(s.can_fit(8));
}

#[test]
fn groupsize_negative_resets() {
    let mut lb = [0u8; 131];
    let data = [0xabu8, 0xcd];
    // C's int groupsize can't produce sign-extended pow2 negatives;
    // all negatives reset to 1 like upstream
    let n = hex_dump_to_buffer(&data, 2, 16, i64::MIN, &mut lb, false);
    assert_eq!(&lb[..n as usize], b"ab cd");
    let mut lb2 = [0u8; 131];
    let n2 = hex_dump_to_buffer(&data, 2, 16, -4, &mut lb2, false);
    assert_eq!(&lb2[..n2 as usize], b"ab cd");
}

#[test]
fn vprintf_huge_width_clamped() {
    // kernel clamps field width to FIELD_WIDTH_MAX (2^23-1): a hostile
    // fmt can't overflow the parse or hang in an unbounded pad loop.
    let mut dst = [0u8; 64];
    let n = crate::fmt::vsnprintf(&mut dst, b"%9999999999999999999d", &[Arg::I(7)]);
    assert_eq!(n, (1 << 23) - 1); // would-be length = clamped width
    // buffer is filled and NUL-terminated, no panic
    assert_eq!(dst[63], 0);
    let mut dst2 = [0u8; 64];
    let n2 = crate::fmt::vsnprintf(&mut dst2, b"%.9999999999999999999s", &[Arg::S(b"abc")]);
    assert_eq!(n2, 3); // precision clamp only affects the pad bound
}

#[test]
fn hex_dump_to_buffer_source_deltas() {
    // groupsize 8: native u64 LE load per group
    let mut lb = [0u8; 131];
    let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let n = hex_dump_to_buffer(&data, 8, 16, 8, &mut lb, false);
    assert_eq!(&lb[..n as usize], b"0807060504030201");
    // non-power-of-two groupsize resets to 1 upstream
    let mut lb2 = [0u8; 131];
    let n = hex_dump_to_buffer(&data[..4], 4, 16, 3, &mut lb2, false);
    assert_eq!(&lb2[..n as usize], b"01 02 03 04");
    // groupsize 16 > 8 resets to 1
    let mut lb3 = [0u8; 131];
    let n = hex_dump_to_buffer(&data[..4], 4, 16, 16, &mut lb3, false);
    assert_eq!(&lb3[..n as usize], b"01 02 03 04");
    // len % groupsize != 0 disables grouping ("no mixed size output")
    let mut lb4 = [0u8; 131];
    let n = hex_dump_to_buffer(&data[..6], 6, 16, 4, &mut lb4, false);
    assert_eq!(&lb4[..n as usize], b"01 02 03 04 05 06");
    // len > rowsize clamps to the row
    let mut lb5 = [0u8; 131];
    let big = [0x5au8; 32];
    let n = hex_dump_to_buffer(&big, 20, 16, 1, &mut lb5, false);
    assert_eq!(n, 47);
    // bad rowsize resets to 16
    let mut lb6 = [0u8; 131];
    let n = hex_dump_to_buffer(&data, 8, 8, 1, &mut lb6, false);
    assert_eq!(&lb6[..n as usize], b"01 02 03 04 05 06 07 08");
    // rowsize 32 with an ASCII column: 32*2 + 32/1 + 1 = 97 + 8
    let mut lb7 = [0u8; 160];
    let n = hex_dump_to_buffer(&data, 8, 32, 1, &mut lb7, true);
    assert_eq!(n, 97 + 8);
    assert_eq!(&lb7[97..105], b"........"); // 0x01-0x08 aren't printable
    // zero length: lone NUL, return 0
    let mut lb8 = [0u8; 16];
    let n = hex_dump_to_buffer(&data, 0, 16, 1, &mut lb8, false);
    assert_eq!(n, 0);
    assert_eq!(lb8[0], 0);
    // zero-size linebuf still returns the would-be length
    let n = hex_dump_to_buffer(&data, 8, 16, 1, &mut [], false);
    assert_eq!(n, (2 + 1) * 8 - 1);
}

#[test]
fn hex_to_bin_table() {
    for d in 0..10u8 {
        assert_eq!(hex_to_bin(b'0' + d), d as i32);
    }
    for d in 0..6u8 {
        assert_eq!(hex_to_bin(b'a' + d), (10 + d) as i32);
        assert_eq!(hex_to_bin(b'A' + d), (10 + d) as i32);
    }
    assert_eq!(hex_to_bin(b'g'), -1);
    assert_eq!(hex_to_bin(b'/'), -1);
    assert_eq!(hex_to_bin(b':'), -1);
    assert_eq!(hex_to_bin(0xff), -1);
    let mut dst = [0u8; 4];
    assert_eq!(hex2bin(&mut dst, b"deadbeef", 4), 0);
    assert_eq!(dst, [0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(hex2bin(&mut dst, b"zz", 1), -22);
    let n = bin2hex(&mut dst, &[0xab, 0x12], 2);
    assert_eq!(n, 4);
    assert_eq!(&dst, b"ab12");
    assert_eq!(HEX_ASC[10], b'a');
    assert_eq!(hex_asc_hi(0xab), b'a');
    assert_eq!(hex_asc_lo(0xab), b'b');
}

#[test]
fn hex_dump_prefix_address() {
    let mut backing = [0u8; 256];
    let mut s = SeqBuf::new(&mut backing);
    let data = [0xabu8; 4];
    assert_eq!(s.hex_dump(b"", 1, 16, 1, &data, 0x1000, 4, false), 0);
    assert_eq!(s.as_slice(), b"0x1000: ab ab ab ab\n");
}
