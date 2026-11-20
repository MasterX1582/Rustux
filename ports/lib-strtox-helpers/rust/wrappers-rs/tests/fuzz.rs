// Scoped security-audit harness: panic/robustness fuzz of every wrapper
// with pseudo-random i64/u64 arguments (including extremes). Not part of the
// shipped crate surface; used only for the audit run.
use std::panic;
use wrappers_rs::port;

fn xs(state: &mut u64) -> i64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x as i64
}

fn interesting(i: u64) -> i64 {
    const V: [i64; 12] = [
        0,
        1,
        -1,
        2,
        -2,
        42,
        255,
        -2147483648,
        2147483647,
        i64::MIN,
        i64::MAX,
        -100,
    ];
    if i.is_multiple_of(4) {
        V[(i / 4) as usize % 12]
    } else {
        i as i64
    }
}

#[test]
fn fuzz_no_panics() {
    panic::set_hook(Box::new(|_| {}));
    let mut st = 0x9e3779b97f4a7c15u64;
    let mut panics = 0u32;
    let iters = 2000u64;
    for i in 0..iters {
        let a = interesting(i).wrapping_add(xs(&mut st));
        let b = interesting(i + 1).wrapping_add(xs(&mut st));
        let c = interesting(i + 2).wrapping_add(xs(&mut st));
        let d = interesting(i + 3).wrapping_add(xs(&mut st));
        let e = interesting(i + 4).wrapping_add(xs(&mut st));
        let f = interesting(i + 5).wrapping_add(xs(&mut st));
        let g = interesting(i + 6).wrapping_add(xs(&mut st));
        macro_rules! t {
            ($f:expr) => {
                if panic::catch_unwind(|| $f).is_err() {
                    panics += 1;
                }
            };
        }
        t!(port::wrap_kstrtoull(a as u64, b, c));
        t!(port::wrap_kstrtoll(a as u64, b, c));
        t!(port::wrap__kstrtoul(a as u64, b, c));
        t!(port::wrap__kstrtol(a as u64, b, c));
        t!(port::wrap_kstrtouint(a as u64, b, c));
        t!(port::wrap_kstrtoint(a as u64, b, c));
        t!(port::wrap_kstrtou16(a as u64, b, c));
        t!(port::wrap_kstrtos16(a as u64, b, c));
        t!(port::wrap_kstrtou8(a as u64, b, c));
        t!(port::wrap_kstrtos8(a as u64, b, c));
        t!(port::wrap_kstrtou64(a as u64, b, c));
        t!(port::wrap_kstrtos64(a as u64, b, c));
        t!(port::wrap_kstrtou32(a as u64, b, c));
        t!(port::wrap_kstrtos32(a as u64, b, c));
        t!(port::wrap_kstrtoul(a as u64, b, c));
        t!(port::wrap_kstrtol(a as u64, b, c));
        t!(port::wrap_parse_integer_limit(a as u64, b, c, d, e));
        t!(port::wrap_parse_integer_fixup_radix(a as u64, b, c));
        t!(port::wrap_kstrtobool(a as u64, b));
        t!(port::wrap_kstrtoull_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtoll_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtoul_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtol_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtouint_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtoint_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtou16_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtos16_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtou8_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtos8_from_user(a as u64, b, c, d));
        t!(port::wrap_kstrtobool_from_user(a as u64, b, c));
        t!(port::wrap_kstrtoudec64(a as u64, b, c));
        t!(port::wrap_kstrtodec64(a as u64, b, c));
        t!(port::wrap_hex_to_bin(a));
        t!(port::wrap_hex2bin(a as u64, b, c, d));
        t!(port::wrap_bin2hex(a as u64, b, c));
        t!(port::wrap_hex_dump_to_buffer(a as u64, b, c, d, e, f));
        t!(port::wrap_print_hex_dump(a as u64, b, c, d, e, f, g));
        t!(port::wrap_string_get_size(a, b, c, d));
        t!(port::wrap_string_escape_mem(a as u64, b, c, d, e));
        t!(port::wrap_string_escape_str(a as u64, b, c, d, e, f));
        t!(port::wrap_string_unescape(a as u64, b, c, d, e));
        t!(port::wrap_skip_spaces(a as u64, b));
        t!(port::wrap_strim(a as u64, b));
        t!(port::wrap_sysfs_streq(a as u64, b, c, d));
        t!(port::wrap_strreplace(a as u64, b, c, d, e));
        t!(port::wrap_memcpy_and_pad(a as u64, b, c, d));
        t!(port::wrap_match_string(a as u64, b, c, d));
        t!(port::wrap_sysfs_match_string(a as u64, b, c, d));
        t!(port::wrap_string_is_terminated(a as u64, b, c, d));
        t!(port::wrap_string_upper(a as u64, b, c));
        t!(port::wrap_string_lower(a as u64, b, c));
        t!(port::wrap_kstrdup_quotable(a as u64, b, c));
        t!(port::wrap_kstrdup_and_replace(a as u64, b, c, d, e));
    }
    let _ = panic::take_hook();
    assert_eq!(panics, 0);
}
