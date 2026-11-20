// klib::string_helpers tests — sizes, escapes, unescape, kstrdup_* model.

#[test]
fn get_size_basics() {
    let mut buf = [0u8; 32];
    let n = string_get_size(0, 0, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(n, 3);
    assert_eq!(&buf[..4], b"0 B\0");
    // 3 significant figures: a bare 1 comes out "1.00 B", not "1 B"
    let n = string_get_size(1, 1, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"1.00 B");
    let n = string_get_size(512, 512, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"256 KiB");
    let n = string_get_size(2, 1024, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"2.00 KiB");
    let n = string_get_size(1, 1500, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"1.46 KiB");
    // decimal units
    let n = string_get_size(2, 1024, STRING_UNITS_10, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"2.05 kB");
    // NO_SPACE | NO_BYTES
    let n = string_get_size(
        1,
        1,
        STRING_UNITS_2 | STRING_UNITS_NO_SPACE | STRING_UNITS_NO_BYTES,
        &mut buf,
        32,
    );
    assert_eq!(&buf[..n as usize], b"1.00");
    let n = string_get_size(1, 1024, STRING_UNITS_2 | STRING_UNITS_NO_SPACE, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"1.00KiB");
    // rounding pushes the remainder over: 1536*1024-ish -> 1.50 MiB class
    let n = string_get_size(1536, 1024, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"1.50 MiB");
    // rounding carry: 1000 rounds up into the next digit
    let n = string_get_size(999, 1024, STRING_UNITS_2, &mut buf, 32);
    assert_eq!(&buf[..n as usize], b"999 KiB");
}

#[test]
fn get_size_glibc_len_quirk() {
    // `int len` becomes size_t: a negative len is a huge buffer size to
    // glibc snprintf, which on this libc writes `ret - 1` chars + NUL
    // (verified quirk — kept verbatim from the differential).
    let mut buf = [0u8; 32];
    let n = string_get_size(1, 1, STRING_UNITS_2, &mut buf, -1);
    assert_eq!(n, 6); // "1.00 B" is the would-be length
    assert_eq!(&buf[..6], b"1.00 \0"); // 5 chars + NUL — last char dropped
    // truncation via a small positive len still returns would-be length
    let mut buf2 = [0u8; 3];
    let n = string_get_size(1, 1500, STRING_UNITS_2, &mut buf2, 3);
    assert_eq!(n, 8); // "1.46 KiB"
    assert_eq!(&buf2, b"1.\0"); // 2 chars + NUL
}

#[test]
fn escape_basics() {
    let mut dst = [0u8; 64];
    // ESCAPE_SPACE: only whitespace escapes
    let n = string_escape_str(b"a\nb\0", &mut dst, ESCAPE_SPACE, None);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b"a\\nb");
    // ESCAPE_ANY_NP: the common "log it" set. NP's printable check runs
    // before special/octal, so '"' passes through literally and only the
    // newline and DEL take escapes: a \n b " \177 -> 9
    let n = string_escape_str_any_np(b"a\nb\"\x7f\0", &mut dst, None);
    assert_eq!(n, 9);
    assert_eq!(&dst[..9], b"a\\nb\"\\177");
    // ESCAPE_OCTAL escapes everything (no printable passthrough)
    let n = string_escape_str(b"ab\0", &mut dst, ESCAPE_OCTAL, None);
    assert_eq!(n, 8);
    assert_eq!(&dst[..8], b"\\141\\142");
    // ESCAPE_HEX
    let n = string_escape_str(b"\xde\0", &mut dst, ESCAPE_HEX, None);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b"\\xde");
    // the return is always the would-be length
    let mut tiny = [0u8; 3];
    let n = string_escape_str(b"abcd\0", &mut tiny, ESCAPE_SPACE, None);
    assert_eq!(n, 4);
    assert_eq!(&tiny, b"abc");
    // ESCAPE_NULL alone handles the interior NUL via isz, not strlen
    let n = string_escape_mem(b"a\0b", 3, &mut dst, ESCAPE_NULL, None);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b"a\\0b");
    // ...while ESCAPE_ANY's OCTAL leg catches 'a'/'b' too
    let n = string_escape_mem(b"a\0b", 3, &mut dst, ESCAPE_ANY, None);
    assert_eq!(n, 10);
    assert_eq!(&dst[..10], b"\\141\\0\\142");
    // `only` dictionary: chars in it escape, others pass through
    let n = string_escape_mem(b"abcabc", 6, &mut dst, ESCAPE_HEX, Some(b"a\0"));
    assert_eq!(n, 12); // \x61 b c \x61 b c
    assert_eq!(&dst[..12], b"\\x61bc\\x61bc");
    // ESCAPE_APPEND: the dict gate is bypassed, so the flag pipeline
    // applies to EVERY byte (dict members just lose their printable
    // exemption) — all six escape as hex
    let n = string_escape_mem(b"abcabc", 6, &mut dst, ESCAPE_HEX | ESCAPE_APPEND, Some(b"a\0"));
    assert_eq!(n, 24);
    assert_eq!(&dst[..24], b"\\x61\\x62\\x63\\x61\\x62\\x63");
    // NAP: printable ASCII passes, DEL/high bytes escape
    let n = string_escape_mem(b"a\x7f\x80", 3, &mut dst, ESCAPE_OCTAL | ESCAPE_NAP, None);
    assert_eq!(n, 1 + 4 + 4);
    assert_eq!(&dst[..9], b"a\\177\\200");
    // NA: ASCII passes (printable or not), high bytes escape
    let n = string_escape_mem(b"a\x01\x80", 3, &mut dst, ESCAPE_OCTAL | ESCAPE_NA, None);
    assert_eq!(n, 1 + 1 + 4);
    assert_eq!(&dst[..6], b"a\x01\\200");
    // NP: kernel isprint covers 0xa0-0xff too
    let n = string_escape_mem(b"a\xe9\x7f", 3, &mut dst, ESCAPE_OCTAL | ESCAPE_NP, None);
    assert_eq!(n, 1 + 1 + 4);
    assert_eq!(&dst[..6], b"a\xe9\\177");
    // ESCAPE_SPECIAL: ", \\, \a, \e
    let n = string_escape_str(b"a\"b\\\x07\0", &mut dst, ESCAPE_SPECIAL, None);
    assert_eq!(n, 8);
    assert_eq!(&dst[..8], b"a\\\"b\\\\\\a");
}

#[test]
fn unescape_basics() {
    let mut dst = [0u8; 64];
    let n = string_unescape_any(b"a\\nb\\r\\t\\v\\f\0", &mut dst, 0);
    assert_eq!(n, 7);
    assert_eq!(&dst[..8], b"a\nb\r\t\x0b\x0c\0");
    // octal escapes: \123 = 83 = 'S'
    let n = string_unescape_any(b"a\\123b\0", &mut dst, 0);
    assert_eq!(n, 3);
    assert_eq!(&dst[..3], b"aSb");
    let n = string_unescape_any(b"\\0\0", &mut dst, 0);
    assert_eq!(n, 1);
    assert_eq!(&dst[..2], b"\0\0");
    // hex escapes: \x then up to 2 digits; \x5g emits 0x05 then 'g'
    let n = string_unescape_any(b"\\x20\\x0a\\x5g\0", &mut dst, 0);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b" \n\x05g");
    // special escapes
    let n = string_unescape_any(b"\\\\\\a\\e\\\"x\0", &mut dst, 0);
    assert_eq!(n, 5);
    assert_eq!(&dst[..5], b"\\\x07\x1b\"x");
    // flags filter: OCTAL only leaves \n literal (backslash kept)
    let n = string_unescape(b"a\\n\\123\0", &mut dst, 0, UNESCAPE_OCTAL);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b"a\\nS");
    // unrecognised escape keeps backslash + char
    let n = string_unescape_any(b"a\\zb\0", &mut dst, 0);
    assert_eq!(n, 4);
    assert_eq!(&dst[..4], b"a\\zb");
    // trailing lone backslash: \ at end stays literal
    let n = string_unescape_any(b"ab\\\0", &mut dst, 0);
    assert_eq!(n, 3);
    assert_eq!(&dst[..3], b"ab\\");
    // size bound: 0 = unlimit; C's `--size` loop stops one char early
    let n = string_unescape_any(b"abcde\0", &mut dst, 3);
    assert_eq!(n, 2);
    assert_eq!(&dst[..2], b"ab");
    // dst bounds writes; count is still exact
    let mut tiny = [0u8; 2];
    let n = string_unescape_any(b"abcd\0", &mut tiny, 0);
    assert_eq!(n, 4);
    assert_eq!(&tiny, b"ab");
}

#[test]
fn kstrdup_model() {
    // kstrdup_quotable: ESCAPE_HEX over the "\f\n\r\t\v\a\e\\\"" set
    let mut dst = [0u8; 64];
    let n = kstrdup_quotable(Some(b"hi\n\"x\0"), &mut dst).unwrap();
    assert_eq!(&dst[..n], b"hi\\x0a\\x22x");
    assert_eq!(dst[n], 0); // NUL written after the escaped text
    let n = kstrdup_quotable(Some(b"plain\0"), &mut dst).unwrap();
    assert_eq!(&dst[..n], b"plain");
    // NULL source -> NULL result
    assert!(kstrdup_quotable(None, &mut dst).is_none());
    // dst too small -> the kmalloc-failure/NULL model
    let mut tiny = [0u8; 4];
    assert!(kstrdup_quotable(Some(b"hi\n\0"), &mut tiny).is_none());
    // kstrdup_and_replace: copy + strreplace
    let n = kstrdup_and_replace(Some(b"a,b,c\0"), &mut dst, b',', b';').unwrap();
    assert_eq!(n, 5);
    assert_eq!(&dst[..6], b"a;b;c\0");
    assert!(kstrdup_and_replace(None, &mut dst, b',', b';').is_none());
    assert!(kstrdup_and_replace(Some(b"toolong\0"), &mut tiny, b'x', b'y').is_none());
}
