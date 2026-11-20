// klib::kstrtox tests — EINVAL vs ERANGE is the point.

#[test]
fn u64_all_bases() {
    assert_eq!(kstrtoull(b"0\0", 10), Ok(0));
    assert_eq!(kstrtoull(b"0007\0", 10), Ok(7));
    assert_eq!(kstrtoull(b"12345\0", 10), Ok(12345));
    // base 0 auto-detect: 0x hex (either case), leading 0 octal, else 10
    assert_eq!(kstrtoull(b"0x1f\0", 0), Ok(31));
    assert_eq!(kstrtoull(b"0XFF\0", 0), Ok(255));
    assert_eq!(kstrtoull(b"0XaB\0", 0), Ok(0xab));
    assert_eq!(kstrtoull(b"017\0", 0), Ok(0o17));
    assert_eq!(kstrtoull(b"0\0", 0), Ok(0));
    assert_eq!(kstrtoull(b"17\0", 0), Ok(17));
    // explicit base 16 still skips a 0x prefix
    assert_eq!(kstrtoull(b"0xff\0", 16), Ok(255));
    assert_eq!(kstrtoull(b"ff\0", 16), Ok(255));
    assert_eq!(kstrtoull(b"FF\0", 16), Ok(255));
    assert_eq!(kstrtoull(b"1111\0", 2), Ok(15));
    assert_eq!(kstrtoull(b"18446744073709551615\0", 10), Ok(u64::MAX));
    assert_eq!(kstrtoull(b"0xffffffffffffffff\0", 16), Ok(u64::MAX));
    // '+' accepted, '-'\ rejected for unsigned
    assert_eq!(kstrtoull(b"+42\0", 10), Ok(42));
    assert_eq!(kstrtoull(b"+0x1f\0", 0), Ok(31));
    // a single trailing newline is consumed; nothing else may follow
    assert_eq!(kstrtoull(b"123\n\0", 10), Ok(123));
}

#[test]
fn einval_not_erange() {
    // -EINVAL: no digits, or trailing junk after the digits
    assert_eq!(kstrtoull(b"\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"g\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"0x\0", 0), Err(-EINVAL)); // "0x" alone
    assert_eq!(kstrtoull(b"0xg\0", 16), Err(-EINVAL));
    assert_eq!(kstrtoull(b"123x\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"12a\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"1a2\0", 10), Err(-EINVAL));
    // kstrtox does NOT skip whitespace — leading or trailing space fails
    assert_eq!(kstrtoull(b" 123\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"123 \0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"12 3\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"\t1\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"1\t\0", 10), Err(-EINVAL));
    // only ONE trailing newline is tolerated
    assert_eq!(kstrtoull(b"123\n\n\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"1\n2\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"123\nx\0", 10), Err(-EINVAL));
    // sign abuse
    assert_eq!(kstrtoull(b"-1\0", 10), Err(-EINVAL)); // no '-' for unsigned
    assert_eq!(kstrtoull(b"++1\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"+-1\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoull(b"0x-1\0", 10), Err(-EINVAL));
    // non-standard prefixes stay literal: "0b"/"0o" parse the leading 0
    assert_eq!(kstrtoull(b"0b101\0", 0), Err(-EINVAL));
    assert_eq!(kstrtoull(b"0o17\0", 0), Err(-EINVAL));
    assert_eq!(kstrtoull(b"1,234\0", 10), Err(-EINVAL));
}

#[test]
fn erange_not_einval() {
    // -ERANGE: digits parsed fine but the value overflowed
    assert_eq!(kstrtoull(b"18446744073709551616\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoull(b"18446744073709551617\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoull(b"184467440737095516150\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoull(b"0x10000000000000000\0", 16), Err(-ERANGE));
    assert_eq!(kstrtoull(b"99999999999999999999\0", 10), Err(-ERANGE));
    assert_eq!(
        kstrtoull(b"10000000000000000000000000000000000000000000000000000000000000000\0", 2),
        Err(-ERANGE)
    );
    assert_eq!(kstrtoull(b"02000000000000000000000\0", 0), Err(-ERANGE));
    // octal max fits — boundary, not overflow
    assert_eq!(kstrtoull(b"01777777777777777777777\0", 0), Ok(u64::MAX));
}

#[test]
fn signed_family() {
    assert_eq!(kstrtoll(b"-42\0", 10), Ok(-42));
    assert_eq!(kstrtoll(b"+42\0", 10), Ok(42));
    assert_eq!(kstrtoll(b"-0x1f\0", 0), Ok(-31));
    assert_eq!(kstrtoll(b"9223372036854775807\0", 10), Ok(i64::MAX));
    assert_eq!(kstrtoll(b"-9223372036854775808\0", 10), Ok(i64::MIN));
    assert_eq!(kstrtoll(b"9223372036854775808\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoll(b"-9223372036854775809\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoll(b"-\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoll(b"--1\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoll(b"-+1\0", 10), Err(-EINVAL));
    assert_eq!(kstrtoll(b"-0x-1\0", 0), Err(-EINVAL));
    // LP64: kstrtoul/kstrtol are u64/i64, narrowing is vacuous
    assert_eq!(kstrtoul(b"18446744073709551615\0", 10), Ok(u64::MAX));
    assert_eq!(kstrtol(b"-9223372036854775808\0", 10), Ok(i64::MIN));
    // u8..u64/s8..s64 narrowing boundaries
    assert_eq!(kstrtou8(b"255\0", 10), Ok(255));
    assert_eq!(kstrtou8(b"256\0", 10), Err(-ERANGE));
    assert_eq!(kstrtos8(b"127\0", 10), Ok(127));
    assert_eq!(kstrtos8(b"128\0", 10), Err(-ERANGE));
    assert_eq!(kstrtos8(b"-128\0", 10), Ok(-128));
    assert_eq!(kstrtos8(b"-129\0", 10), Err(-ERANGE));
    assert_eq!(kstrtou16(b"65535\0", 10), Ok(65535));
    assert_eq!(kstrtou16(b"65536\0", 10), Err(-ERANGE));
    assert_eq!(kstrtos16(b"32767\0", 10), Ok(32767));
    assert_eq!(kstrtos16(b"32768\0", 10), Err(-ERANGE));
    assert_eq!(kstrtos16(b"-32768\0", 10), Ok(-32768));
    assert_eq!(kstrtos16(b"-32769\0", 10), Err(-ERANGE));
    assert_eq!(kstrtouint(b"4294967295\0", 10), Ok(u32::MAX));
    assert_eq!(kstrtouint(b"4294967296\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoint(b"2147483647\0", 10), Ok(i32::MAX));
    assert_eq!(kstrtoint(b"2147483648\0", 10), Err(-ERANGE));
    assert_eq!(kstrtoint(b"-2147483648\0", 10), Ok(i32::MIN));
    assert_eq!(kstrtoint(b"-2147483649\0", 10), Err(-ERANGE));
    // macro aliases share the implementations
    assert_eq!(kstrtou64(b"18446744073709551615\0", 10), Ok(u64::MAX));
    assert_eq!(kstrtos64(b"-9223372036854775808\0", 10), Ok(i64::MIN));
    assert_eq!(kstrtou32(b"4294967295\0", 10), Ok(u32::MAX));
    assert_eq!(kstrtos32(b"-2147483648\0", 10), Ok(i32::MIN));
}

#[test]
fn bool_family() {
    assert_eq!(kstrtobool(Some(b"1\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"0\0")), Ok(false));
    assert_eq!(kstrtobool(Some(b"y\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"Y\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"n\0")), Ok(false));
    assert_eq!(kstrtobool(Some(b"t\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"f\0")), Ok(false));
    assert_eq!(kstrtobool(Some(b"E\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"D\0")), Ok(false));
    assert_eq!(kstrtobool(Some(b"on\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"On\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"oFf\0")), Ok(false));
    // first chars decide; trailing junk is ignored
    assert_eq!(kstrtobool(Some(b"yes anything\0")), Ok(true));
    assert_eq!(kstrtobool(Some(b"off!\0")), Ok(false));
    assert_eq!(kstrtobool(Some(b"o\0")), Err(-EINVAL));
    assert_eq!(kstrtobool(Some(b"ox\0")), Err(-EINVAL));
    assert_eq!(kstrtobool(Some(b"2\0")), Err(-EINVAL));
    assert_eq!(kstrtobool(Some(b"\0")), Err(-EINVAL));
    assert_eq!(kstrtobool(None), Err(-EINVAL));
}

#[test]
fn dec64_scaled() {
    assert_eq!(kstrtoudec64(b"0\0", 0), Ok(0));
    assert_eq!(kstrtoudec64(b"0.0\0", 1), Ok(0));
    assert_eq!(kstrtoudec64(b"1.5\0", 1), Ok(15));
    assert_eq!(kstrtoudec64(b"123.45\0", 2), Ok(12345));
    assert_eq!(kstrtoudec64(b"+2.75\0", 2), Ok(275));
    assert_eq!(kstrtoudec64(b"0.999\0", 3), Ok(999));
    // digits past scale are truncated, not rounded
    assert_eq!(kstrtoudec64(b"1.239\0", 2), Ok(123));
    assert_eq!(kstrtoudec64(b"1.\0", 0), Ok(1));
    // ".5" with scale 0 parses as 0 (documented upstream case)
    assert_eq!(kstrtoudec64(b".5\0", 0), Ok(0));
    assert_eq!(kstrtoudec64(b".\0", 0), Err(-EINVAL));
    assert_eq!(kstrtoudec64(b"\0", 0), Err(-EINVAL));
    assert_eq!(kstrtoudec64(b"1.2x\0", 1), Err(-EINVAL));
    assert_eq!(kstrtoudec64(b"1.2 \0", 1), Err(-EINVAL));
    assert_eq!(kstrtoudec64(b" 1.2\0", 1), Err(-EINVAL));
    assert_eq!(kstrtoudec64(b"1.2\n\0", 1), Ok(12));
    assert_eq!(kstrtoudec64(b"18446744073709551616\0", 0), Err(-ERANGE));
    assert_eq!(kstrtoudec64(b"99999999999999999999.99999\0", 5), Err(-ERANGE));
    // 2^64 boundary through the scale multiplier
    assert_eq!(kstrtoudec64(b"18446744073709551.615\0", 3), Ok(u64::MAX));
    assert_eq!(kstrtoudec64(b"18446744073709551.616\0", 3), Err(-ERANGE));
    assert_eq!(kstrtodec64(b"-3.141592\0", 4), Ok(-31415));
    assert_eq!(kstrtodec64(b"-0.5\0", 1), Ok(-5));
    // 19 digits need scale 18: 9*10^18 + 223372036854775807 = i64::MAX
    assert_eq!(kstrtodec64(b"9.223372036854775807\0", 18), Ok(i64::MAX));
    assert_eq!(kstrtodec64(b"9.223372036854775808\0", 18), Err(-ERANGE));
    // digits past scale truncate: scale 15 reads only the first 15
    assert_eq!(kstrtodec64(b"9.223372036854775807\0", 15), Ok(9223372036854775));
    // -u64::MAX wraps to +1 > 0 upstream -> -ERANGE, not the wrapped 1
    assert_eq!(kstrtodec64(b"-18446744073709551615\0", 0), Err(-ERANGE));
    assert_eq!(kstrtodec64(b"-18446744073709551616\0", 0), Err(-ERANGE));
}

#[test]
fn parse_integer_internals() {
    // fixup_radix: base 0 auto-detect; explicit base 16 consumes "0x"
    let mut base = 0u32;
    assert_eq!(_parse_integer_fixup_radix(b"0x1f\0", &mut base), 2);
    assert_eq!(base, 16);
    let mut base = 0u32;
    assert_eq!(_parse_integer_fixup_radix(b"077\0", &mut base), 0);
    assert_eq!(base, 8);
    let mut base = 0u32;
    assert_eq!(_parse_integer_fixup_radix(b"42\0", &mut base), 0);
    assert_eq!(base, 10);
    // explicit base 8 keeps the "0x" literal (fails to parse later)
    let mut base = 8u32;
    assert_eq!(_parse_integer_fixup_radix(b"0x1f\0", &mut base), 0);
    assert_eq!(base, 8);
    // "0x" with no hex digit after is NOT hex for base-0 detection
    let mut base = 0u32;
    assert_eq!(_parse_integer_fixup_radix(b"0xg\0", &mut base), 0);
    assert_eq!(base, 8);

    let (rv, res) = _parse_integer_limit(b"ff\0", 16, 64, 0);
    assert_eq!((rv, res), (2, 255));
    // max_chars bounds the count; init seeds the accumulation
    let (rv, res) = _parse_integer_limit(b"12345", 10, 3, 0);
    assert_eq!((rv, res), (3, 123));
    let (_rv, res) = _parse_integer_limit(b"23\0", 10, 10, 1);
    assert_eq!(res, 123);
    // 17 f's -> overflow bit + saturating u64::MAX
    let (rv, res) = _parse_integer(b"fffffffffffffffff\0", 16);
    assert_eq!(rv, 17 | KSTRTOX_OVERFLOW);
    assert_eq!(res, u64::MAX);
    // digits >= base stop the scan without error
    let (rv, res) = _parse_integer(b"128\0", 8);
    assert_eq!((rv, res), (2, 0o12));
    // bytes >= 0x80 are not digits (sign-extended char semantics)
    let (rv, _res) = _parse_integer(b"1\xe9\0", 16);
    assert_eq!(rv, 1);
}

#[test]
fn from_user_family() {
    assert_eq!(kstrtoull_from_user(Some(b"123\0"), 4, 10), Ok(123));
    // count caps what is copied: "12" parses, trailing "\0" ends it
    assert_eq!(kstrtoull_from_user(Some(b"123\0"), 2, 10), Ok(12));
    // copy_from_user fails when the window is short -> -EFAULT
    assert_eq!(kstrtoull_from_user(Some(b"123\0"), 100, 10), Err(-14));
    assert_eq!(kstrtoull_from_user(None, 5, 10), Err(-14));
    // but count <= window is fine even with NUL right after
    assert_eq!(kstrtouint_from_user(Some(b"ff\0"), 3, 16), Ok(255));
    assert_eq!(kstrtouint_from_user(Some(b"4294967296\0"), 11, 10), Err(-34));
    assert_eq!(kstrtos8_from_user(Some(b"-128\0"), 5, 10), Ok(-128));
    assert_eq!(kstrtos8_from_user(Some(b"128\0"), 4, 10), Err(-34));
    assert_eq!(kstrtobool_from_user(Some(b"on\0"), 3), Ok(true));
    assert_eq!(kstrtobool_from_user(Some(b"of\0"), 3), Ok(false));
    // buf[4]: "of" fits, "off" truncated still reads 'o','f' -> false
    assert_eq!(kstrtobool_from_user(Some(b"off\0"), 4), Ok(false));
    assert_eq!(kstrtobool_from_user(Some(b"o\0"), 1), Err(-22));
    assert_eq!(kstrtobool_from_user(None, 2), Err(-14));
    assert_eq!(kstrtol_from_user(Some(b"-7\0"), 3, 10), Ok(-7));
    assert_eq!(kstrtou16_from_user(Some(b"65535\0"), 6, 10), Ok(65535));
    assert_eq!(kstrtos16_from_user(Some(b"-9\0"), 3, 10), Ok(-9));
    assert_eq!(kstrtou8_from_user(Some(b"255\0"), 4, 10), Ok(255));
    assert_eq!(kstrtoul_from_user(Some(b"255\0"), 4, 10), Ok(255));
}
