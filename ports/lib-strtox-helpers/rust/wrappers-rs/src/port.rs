//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Replace each `unimplemented!` with the port of the C function. C has no error channel:
//! return what the C code returns, and never panic (a panic aborts at the C boundary).

#![forbid(unsafe_code)]

/// Port of `uint64_t wrap_kstrtoull(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:519`).
pub fn wrap_kstrtoull(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtoull` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoll(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:528`).
pub fn wrap_kstrtoll(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtoll` from wrappers.c")
}

/// Port of `uint64_t wrap__kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:537`).
pub fn wrap__kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap__kstrtoul` from wrappers.c")
}

/// Port of `uint64_t wrap__kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:546`).
pub fn wrap__kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap__kstrtol` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtouint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:555`).
pub fn wrap_kstrtouint(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtouint` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:564`).
pub fn wrap_kstrtoint(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtoint` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:573`).
pub fn wrap_kstrtou16(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtou16` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:582`).
pub fn wrap_kstrtos16(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtos16` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:591`).
pub fn wrap_kstrtou8(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtou8` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:600`).
pub fn wrap_kstrtos8(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtos8` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:609`).
pub fn wrap_kstrtou64(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtou64` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:618`).
pub fn wrap_kstrtos64(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtos64` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:627`).
pub fn wrap_kstrtou32(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtou32` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:636`).
pub fn wrap_kstrtos32(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtos32` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:645`).
pub fn wrap_kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtoul` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:654`).
pub fn wrap_kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_kstrtol` from wrappers.c")
}

/// Port of `uint64_t wrap_parse_integer_limit(uint64_t seed, int64_t base, int64_t maxc, int64_t init, int64_t mode)` (`wrappers.c:663`).
pub fn wrap_parse_integer_limit(seed: u64, base: i64, maxc: i64, init: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &maxc, &init, &mode);
    unimplemented!("port `wrap_parse_integer_limit` from wrappers.c")
}

/// Port of `uint64_t wrap_parse_integer_fixup_radix(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:675`).
pub fn wrap_parse_integer_fixup_radix(seed: u64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &base, &mode);
    unimplemented!("port `wrap_parse_integer_fixup_radix` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtobool(uint64_t seed, int64_t mode)` (`wrappers.c:686`).
pub fn wrap_kstrtobool(seed: u64, mode: i64) -> u64 {
    let _ = (&seed, &mode);
    unimplemented!("port `wrap_kstrtobool` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoull_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:698`).
pub fn wrap_kstrtoull_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtoull_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoll_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:709`).
pub fn wrap_kstrtoll_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtoll_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoul_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:720`).
pub fn wrap_kstrtoul_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtoul_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtol_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:731`).
pub fn wrap_kstrtol_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtol_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtouint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:742`).
pub fn wrap_kstrtouint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtouint_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:753`).
pub fn wrap_kstrtoint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtoint_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:764`).
pub fn wrap_kstrtou16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtou16_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:775`).
pub fn wrap_kstrtos16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtos16_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtou8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:786`).
pub fn wrap_kstrtou8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtou8_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtos8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:797`).
pub fn wrap_kstrtos8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &base, &mode);
    unimplemented!("port `wrap_kstrtos8_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtobool_from_user(uint64_t seed, int64_t count, int64_t mode)` (`wrappers.c:808`).
pub fn wrap_kstrtobool_from_user(seed: u64, count: i64, mode: i64) -> u64 {
    let _ = (&seed, &count, &mode);
    unimplemented!("port `wrap_kstrtobool_from_user` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtoudec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:820`).
pub fn wrap_kstrtoudec64(seed: u64, scale: i64, mode: i64) -> u64 {
    let _ = (&seed, &scale, &mode);
    unimplemented!("port `wrap_kstrtoudec64` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrtodec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:830`).
pub fn wrap_kstrtodec64(seed: u64, scale: i64, mode: i64) -> u64 {
    let _ = (&seed, &scale, &mode);
    unimplemented!("port `wrap_kstrtodec64` from wrappers.c")
}

/// Port of `int64_t wrap_hex_to_bin(int64_t c)` (`wrappers.c:843`).
pub fn wrap_hex_to_bin(c: i64) -> i64 {
    let _ = &c;
    unimplemented!("port `wrap_hex_to_bin` from wrappers.c")
}

/// Port of `uint64_t wrap_hex2bin(uint64_t seed, int64_t slen, int64_t cnt, int64_t mode)` (`wrappers.c:848`).
pub fn wrap_hex2bin(seed: u64, slen: i64, cnt: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &cnt, &mode);
    unimplemented!("port `wrap_hex2bin` from wrappers.c")
}

/// Port of `uint64_t wrap_bin2hex(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:866`).
pub fn wrap_bin2hex(seed: u64, slen: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &mode);
    unimplemented!("port `wrap_bin2hex` from wrappers.c")
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
    let _ = (&seed, &len, &rowsize, &groupsize, &linebuflen, &ascii);
    unimplemented!("port `wrap_hex_dump_to_buffer` from wrappers.c")
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
    let _ = (&seed, &ptype, &rowsize, &groupsize, &len, &ascii, &mode);
    unimplemented!("port `wrap_print_hex_dump` from wrappers.c")
}

/// Port of `uint64_t wrap_string_get_size(int64_t size, int64_t blk, int64_t units, int64_t len)` (`wrappers.c:914`).
pub fn wrap_string_get_size(size: i64, blk: i64, units: i64, len: i64) -> u64 {
    let _ = (&size, &blk, &units, &len);
    unimplemented!("port `wrap_string_get_size` from wrappers.c")
}

/// Port of `uint64_t wrap_string_escape_mem(uint64_t seed, int64_t isz, int64_t osz, int64_t flags, int64_t only)` (`wrappers.c:928`).
pub fn wrap_string_escape_mem(seed: u64, isz: i64, osz: i64, flags: i64, only: i64) -> u64 {
    let _ = (&seed, &isz, &osz, &flags, &only);
    unimplemented!("port `wrap_string_escape_mem` from wrappers.c")
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
    let _ = (&seed, &slen, &osz, &flags, &only, &mode);
    unimplemented!("port `wrap_string_escape_str` from wrappers.c")
}

/// Port of `uint64_t wrap_string_unescape(uint64_t seed, int64_t slen, int64_t size, int64_t flags, int64_t mode)` (`wrappers.c:958`).
pub fn wrap_string_unescape(seed: u64, slen: i64, size: i64, flags: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &size, &flags, &mode);
    unimplemented!("port `wrap_string_unescape` from wrappers.c")
}

/// Port of `uint64_t wrap_skip_spaces(uint64_t seed, int64_t mode)` (`wrappers.c:979`).
pub fn wrap_skip_spaces(seed: u64, mode: i64) -> u64 {
    let _ = (&seed, &mode);
    unimplemented!("port `wrap_skip_spaces` from wrappers.c")
}

/// Port of `uint64_t wrap_strim(uint64_t seed, int64_t mode)` (`wrappers.c:993`).
pub fn wrap_strim(seed: u64, mode: i64) -> u64 {
    let _ = (&seed, &mode);
    unimplemented!("port `wrap_strim` from wrappers.c")
}

/// Port of `uint64_t wrap_sysfs_streq(uint64_t seed, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:1010`).
pub fn wrap_sysfs_streq(seed: u64, la: i64, lb: i64, mode: i64) -> u64 {
    let _ = (&seed, &la, &lb, &mode);
    unimplemented!("port `wrap_sysfs_streq` from wrappers.c")
}

/// Port of `uint64_t wrap_strreplace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1037`).
pub fn wrap_strreplace(seed: u64, slen: i64, old: i64, new: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &old, &new, &mode);
    unimplemented!("port `wrap_strreplace` from wrappers.c")
}

/// Port of `uint64_t wrap_memcpy_and_pad(uint64_t seed, int64_t dest_len, int64_t count, int64_t pad)` (`wrappers.c:1054`).
pub fn wrap_memcpy_and_pad(seed: u64, dest_len: i64, count: i64, pad: i64) -> u64 {
    let _ = (&seed, &dest_len, &count, &pad);
    unimplemented!("port `wrap_memcpy_and_pad` from wrappers.c")
}

/// Port of `uint64_t wrap_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1090`).
pub fn wrap_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    let _ = (&seed, &n, &strsel, &mode);
    unimplemented!("port `wrap_match_string` from wrappers.c")
}

/// Port of `uint64_t wrap_sysfs_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1111`).
pub fn wrap_sysfs_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    let _ = (&seed, &n, &strsel, &mode);
    unimplemented!("port `wrap_sysfs_match_string` from wrappers.c")
}

/// Port of `uint64_t wrap_string_is_terminated(uint64_t seed, int64_t slen, int64_t len, int64_t mode)` (`wrappers.c:1145`).
pub fn wrap_string_is_terminated(seed: u64, slen: i64, len: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &len, &mode);
    unimplemented!("port `wrap_string_is_terminated` from wrappers.c")
}

/// Port of `uint64_t wrap_string_upper(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1156`).
pub fn wrap_string_upper(seed: u64, slen: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &mode);
    unimplemented!("port `wrap_string_upper` from wrappers.c")
}

/// Port of `uint64_t wrap_string_lower(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1166`).
pub fn wrap_string_lower(seed: u64, slen: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &mode);
    unimplemented!("port `wrap_string_lower` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrdup_quotable(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1176`).
pub fn wrap_kstrdup_quotable(seed: u64, slen: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &mode);
    unimplemented!("port `wrap_kstrdup_quotable` from wrappers.c")
}

/// Port of `uint64_t wrap_kstrdup_and_replace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1189`).
pub fn wrap_kstrdup_and_replace(seed: u64, slen: i64, old: i64, new: i64, mode: i64) -> u64 {
    let _ = (&seed, &slen, &old, &new, &mode);
    unimplemented!("port `wrap_kstrdup_and_replace` from wrappers.c")
}
