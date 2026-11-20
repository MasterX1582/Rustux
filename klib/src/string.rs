//! String/memory routines, ported from Linux `lib/string.c`, plus the
//! string helpers upstream keeps in `lib/string_helpers.c` and
//! `include/linux/string_helpers.h` (`sysfs_streq`, `match_string`,
//! `__sysfs_match_string`, `strreplace`, `skip_spaces`, `strim`,
//! `string_upper`, `string_lower`, `string_is_terminated`,
//! `memcpy_and_pad` — the `ports/lib-strtox-helpers` overlap).
//!
//! C pointers become `&[u8]` slices or `usize` offsets into them; NUL
//! pointers become `None` / [`Option<usize>`] offsets. Functions that scan
//! for NUL (`strlen`, `strcmp`, `strcpy`, ...) read within the slice bounds
//! and panic if no terminator is present — where C would over-read the
//! buffer, the caller's contract is "the terminator lies inside the slice"
//! (`sized_strscpy` is the exception: it treats the slice end as an
//! implicit NUL, matching C's `load_unaligned_zeropad` behavior).
//! `int` returns map to `i32`, `ssize_t` to `isize`.

const E2BIG: isize = 7;
const EINVAL: i32 = 22;
const WAAT_ONES: u64 = 0x0101010101010101;
const WAAT_HIGHS: u64 = 0x8080808080808080;
const ALLBUTLAST_BYTE_MASK: u64 = u64::MAX >> 8; /* little-endian */

fn tolower_c(c: u8) -> u8 {
    if c.is_ascii_uppercase() { c + 32 } else { c }
}

pub fn strncasecmp(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    let (mut i, mut j) = (0usize, 0usize);
    let (mut c1, mut c2);
    if len == 0 {
        return 0;
    }
    let mut len = len;
    loop {
        c1 = s1[i];
        i += 1;
        c2 = s2[j];
        j += 1;
        if c1 == 0 || c2 == 0 {
            break;
        }
        if c1 != c2 {
            c1 = tolower_c(c1);
            c2 = tolower_c(c2);
            if c1 != c2 {
                break;
            }
        }
        len -= 1;
        if len == 0 {
            break;
        }
    }
    c1 as i32 - c2 as i32
}

pub fn strcasecmp(s1: &[u8], s2: &[u8]) -> i32 {
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let c1 = tolower_c(s1[i]);
        i += 1;
        let c2 = tolower_c(s2[j]);
        j += 1;
        if !(c1 == c2 && c1 != 0) {
            return c1 as i32 - c2 as i32;
        }
    }
}

pub fn strcpy(dest: &mut [u8], src: &[u8]) {
    let mut i = 0;
    loop {
        dest[i] = src[i];
        if src[i] == 0 {
            break;
        }
        i += 1;
    }
}

fn has_zero(a: u64) -> u64 {
    a.wrapping_sub(WAAT_ONES) & !a & WAAT_HIGHS
}

/// prep_zero_mask and create_zero_mask are the identity on x86-64;
/// zero_bytemask folds the has_zero mask to a per-byte 0/0xff mask.
fn zero_bytemask(bits: u64) -> u64 {
    let bits = bits.wrapping_sub(1) & !bits;
    bits >> 7
}

/// find_zero(bits) = __ffs(bits) >> 3 on x86-64.
fn find_zero(bits: u64) -> u64 {
    (bits.trailing_zeros() >> 3) as u64
}

pub fn sized_strscpy(dest: &mut [u8], src: &[u8], count: usize) -> isize {
    let mut max = count;
    let mut res = 0usize;
    if count == 0 || count > i32::MAX as usize {
        return -E2BIG;
    }
    let mut count = count;
    while max >= 8 {
        // load_unaligned_zeropad: a full unaligned word load when in bounds;
        // bytes past the slice end read as zero (an implicit terminator),
        // which reproduces C's observable result whenever the real source
        // string's NUL lies within the slice.
        let c = if res + 8 <= src.len() {
            u64::from_ne_bytes(src[res..res + 8].try_into().unwrap())
        } else {
            let mut w = [0u8; 8];
            w[..src.len() - res].copy_from_slice(&src[res..]);
            u64::from_ne_bytes(w)
        };
        let data = has_zero(c);
        if data != 0 {
            let out = c & zero_bytemask(data);
            dest[res..res + 8].copy_from_slice(&out.to_ne_bytes());
            return (res + find_zero(data) as usize) as isize;
        }
        count -= 8;
        if count == 0 {
            let c = c & ALLBUTLAST_BYTE_MASK;
            dest[res..res + 8].copy_from_slice(&c.to_ne_bytes());
            return -E2BIG;
        }
        dest[res..res + 8].copy_from_slice(&c.to_ne_bytes());
        res += 8;
        max -= 8;
    }
    while count > 1 {
        // Same implicit-NUL model: bytes past the slice end read as zero.
        let c = src.get(res).copied().unwrap_or(0);
        dest[res] = c;
        if c == 0 {
            return res as isize;
        }
        res += 1;
        count -= 1;
    }
    dest[res] = 0;
    if src.get(res).copied().unwrap_or(0) != 0 { -E2BIG } else { res as isize }
}

pub fn stpcpy(dest: &mut [u8], src: &[u8]) -> usize {
    let mut i = 0;
    loop {
        dest[i] = src[i];
        if src[i] == 0 {
            return i;
        }
        i += 1;
    }
}

pub fn strcat(dest: &mut [u8], src: &[u8]) {
    let mut i = strlen(dest);
    let mut j = 0;
    loop {
        dest[i] = src[j];
        if src[j] == 0 {
            break;
        }
        i += 1;
        j += 1;
    }
}

pub fn strncat(dest: &mut [u8], src: &[u8], count: usize) {
    if count == 0 {
        return;
    }
    let mut i = strlen(dest);
    let mut j = 0;
    let mut count = count;
    loop {
        dest[i] = src[j];
        i += 1;
        if src[j] == 0 {
            break;
        }
        j += 1;
        count -= 1;
        if count == 0 {
            dest[i] = 0;
            break;
        }
    }
}

pub fn strlcat(dest: &mut [u8], src: &[u8], count: usize) -> usize {
    let dsize = strlen(dest);
    let mut len = strlen(src);
    let res = dsize + len;
    // BUG_ON(dsize >= count): the wrapper always keeps count > dsize.
    let count = count - dsize;
    if len >= count {
        len = count - 1;
    }
    dest[dsize..dsize + len].copy_from_slice(&src[..len]);
    dest[dsize + len] = 0;
    res
}

pub fn strcmp(s1: &[u8], s2: &[u8]) -> i32 {
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let c1 = s1[i];
        i += 1;
        let c2 = s2[j];
        j += 1;
        if c1 != c2 {
            return if c1 < c2 { -1 } else { 1 };
        }
        if c1 == 0 {
            return 0;
        }
    }
}

pub fn strncmp(s1: &[u8], s2: &[u8], count: usize) -> i32 {
    let (mut i, mut j) = (0usize, 0usize);
    let mut count = count;
    while count != 0 {
        let c1 = s1[i];
        i += 1;
        let c2 = s2[j];
        j += 1;
        if c1 != c2 {
            return if c1 < c2 { -1 } else { 1 };
        }
        if c1 == 0 {
            break;
        }
        count -= 1;
    }
    0
}

pub fn strchr(s: &[u8], c: u8) -> Option<usize> {
    let mut i = 0;
    loop {
        if s[i] == c {
            return Some(i);
        }
        if s[i] == 0 {
            return None;
        }
        i += 1;
    }
}

pub fn strchrnul(s: &[u8], c: u8) -> usize {
    let mut i = 0;
    while s[i] != 0 && s[i] != c {
        i += 1;
    }
    i
}

pub fn strnchrnul(s: &[u8], count: usize, c: u8) -> usize {
    let mut i = 0;
    let mut count = count;
    while count != 0 {
        count -= 1;
        if s[i] == 0 || s[i] == c {
            break;
        }
        i += 1;
    }
    i
}

pub fn strrchr(s: &[u8], c: u8) -> Option<usize> {
    let mut last = None;
    let mut i = 0;
    loop {
        if s[i] == c {
            last = Some(i);
        }
        if s[i] == 0 {
            break;
        }
        i += 1;
    }
    last
}

pub fn strnchr(s: &[u8], count: usize, c: u8) -> Option<usize> {
    let mut i = 0;
    let mut count = count;
    while count != 0 {
        count -= 1;
        if s[i] == c {
            return Some(i);
        }
        if s[i] == 0 {
            break;
        }
        i += 1;
    }
    None
}

pub fn strlen(s: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        i += 1;
    }
    i
}

pub fn strnlen(s: &[u8], count: usize) -> usize {
    let mut i = 0;
    let mut count = count;
    while count != 0 && s[i] != 0 {
        i += 1;
        count -= 1;
    }
    i
}

pub fn strspn(s: &[u8], accept: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        if strchr(accept, s[i]).is_none() {
            break;
        }
        i += 1;
    }
    i
}

pub fn strcspn(s: &[u8], reject: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        if strchr(reject, s[i]).is_some() {
            break;
        }
        i += 1;
    }
    i
}

pub fn strpbrk(s: &[u8], ct: &[u8]) -> Option<usize> {
    let mut i = 0;
    while s[i] != 0 {
        if strchr(ct, s[i]).is_some() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `p` is the `char **` cursor modelled as an offset into `buf`.
pub fn strsep(p: &mut Option<usize>, buf: &mut [u8], ct: &[u8]) -> Option<usize> {
    let sbegin = (*p)?;
    if let Some(e) = strpbrk(&buf[sbegin..], ct) {
        let e = sbegin + e;
        buf[e] = 0;
        *p = Some(e + 1);
    } else {
        *p = None;
    }
    Some(sbegin)
}

pub fn memset(d: &mut [u8], c: u8, n: usize) {
    for x in d[..n].iter_mut() {
        *x = c;
    }
}

pub fn memset16(w: &mut [u16], v: u16, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

pub fn memset32(w: &mut [u32], v: u32, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

pub fn memset64(w: &mut [u64], v: u64, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

pub fn memcpy(d: &mut [u8], s: &[u8], n: usize) {
    d[..n].copy_from_slice(&s[..n]);
}

/// Single-buffer memmove: forward when doff <= soff, backward otherwise.
pub fn memmove(d: &mut [u8], doff: usize, soff: usize, n: usize) {
    if doff <= soff {
        for k in 0..n {
            d[doff + k] = d[soff + k];
        }
    } else {
        let mut k = n;
        while k != 0 {
            k -= 1;
            d[doff + k] = d[soff + k];
        }
    }
}

pub fn memcmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    let (mut i, mut j) = (0usize, 0usize);
    let mut count = n;
    // CONFIG_HAVE_EFFICIENT_UNALIGNED_ACCESS word loop.
    if count >= 8 {
        loop {
            if u64::from_ne_bytes(a[i..i + 8].try_into().unwrap())
                != u64::from_ne_bytes(b[j..j + 8].try_into().unwrap())
            {
                break;
            }
            i += 8;
            j += 8;
            count -= 8;
            if count < 8 {
                break;
            }
        }
    }
    let mut res = 0i32;
    while count > 0 {
        res = a[i] as i32 - b[j] as i32;
        if res != 0 {
            break;
        }
        i += 1;
        j += 1;
        count -= 1;
    }
    res
}

/// `bcmp` — tail call to `memcmp` (upstream contract: only zero vs non-zero
/// is meaningful).
pub fn bcmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    memcmp(a, b, n)
}

pub fn memscan(s: &[u8], c: u8, size: usize) -> usize {
    let mut i = 0;
    let mut size = size;
    while size != 0 {
        if s[i] == c {
            return i;
        }
        i += 1;
        size -= 1;
    }
    i
}

pub fn strstr(s1: &[u8], s2: &[u8]) -> Option<usize> {
    let l2 = strlen(s2);
    if l2 == 0 {
        return Some(0);
    }
    let mut l1 = strlen(s1);
    let mut i = 0;
    while l1 >= l2 {
        l1 -= 1;
        if memcmp(&s1[i..], s2, l2) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn strnstr(s1: &[u8], s2: &[u8], len: usize) -> Option<usize> {
    let l2 = strlen(s2);
    if l2 == 0 {
        return Some(0);
    }
    let mut len = len;
    let mut i = 0;
    while len >= l2 {
        len -= 1;
        if memcmp(&s1[i..], s2, l2) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn memchr(s: &[u8], c: u8, n: usize) -> Option<usize> {
    let mut i = 0;
    let mut n = n;
    while n != 0 {
        n -= 1;
        if s[i] == c {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn check_bytes8(s: &[u8], value: u8, bytes: usize) -> Option<usize> {
    let mut i = 0;
    let mut bytes = bytes;
    while bytes != 0 {
        if s[i] != value {
            return Some(i);
        }
        i += 1;
        bytes -= 1;
    }
    None
}

/// Returns the offset of the first byte other than `value`, like the C
/// pointer result relative to `start`.  The `% 8` prefix mirrors the C
/// address check; every path yields the same offset.
pub fn memchr_inv(start: &[u8], c: u8, bytes: usize) -> Option<usize> {
    let value = c;
    if bytes <= 16 {
        return check_bytes8(start, value, bytes);
    }
    // CONFIG_ARCH_HAS_FAST_MULTIPLIER && BITS_PER_LONG == 64.
    let value64 = (value as u64).wrapping_mul(0x0101010101010101);
    let mut pos = 0usize;
    let mut bytes = bytes;
    let prefix = (start.as_ptr() as usize) % 8;
    if prefix != 0 {
        let prefix = 8 - prefix;
        if let Some(r) = check_bytes8(&start[..prefix], value, prefix) {
            return Some(r);
        }
        pos += prefix;
        bytes -= prefix;
    }
    let mut words = bytes / 8;
    while words != 0 {
        let w = u64::from_ne_bytes(start[pos..pos + 8].try_into().unwrap());
        if w != value64 {
            return check_bytes8(&start[pos..], value, 8).map(|r| pos + r);
        }
        pos += 8;
        words -= 1;
    }
    check_bytes8(&start[pos..], value, bytes % 8).map(|r| pos + r)
}

// ---- merged: lib/string_helpers.c + include/linux/string_helpers.h ------
// (the ports/lib-strtox-helpers overlap — composites over the primitives
// above, verified in the 11667-case differential)

/// kernel `isspace` — the `_ctype` `_S` set: \t-\r, ' ', and 0xa0.
fn isspace(c: u8) -> bool {
    matches!(c, 0x09..=0x0d | 0x20 | 0xa0)
}

/// kernel `toupper` — `_ctype` `_L` is a-z plus Latin-1 0xdf-0xf6 and
/// 0xf8-0xff (so `string_upper` maps e.g. 0xe1 to 0xc1, like upstream).
fn toupper(c: u8) -> u8 {
    if matches!(c, 0x61..=0x7a | 0xdf..=0xf6 | 0xf8..=0xff) { c - 32 } else { c }
}

/// kernel `tolower` — `_ctype` `_U` is A-Z plus Latin-1 0xc0-0xd6 and
/// 0xd8-0xde. (`tolower_c` above stays the ASCII subset the strncasecmp
/// family was adopted with.)
fn tolower(c: u8) -> u8 {
    if matches!(c, 0x41..=0x5a | 0xc0..=0xd6 | 0xd8..=0xde) { c + 32 } else { c }
}

/// `skip_spaces` — offset of the first non-whitespace char.
pub fn skip_spaces(s: &[u8]) -> usize {
    let mut i = 0;
    while isspace(s[i]) {
        i += 1;
    }
    i
}

/// `strim` — trim trailing whitespace in place, return the `skip_spaces`
/// offset of the (still NUL-terminated) remainder.
pub fn strim(s: &mut [u8]) -> usize {
    let size = strlen(s);
    if size == 0 {
        return 0;
    }
    // C walks `end` back from s + size - 1; `e` is the exclusive end.
    let mut e = size;
    while e > 0 && isspace(s[e - 1]) {
        e -= 1;
    }
    s[e] = 0;
    skip_spaces(s)
}

/// `sysfs_streq` — true iff the strings are equal, treating NUL and
/// newline-then-NUL as equivalent terminations on either side.
pub fn sysfs_streq(s1: &[u8], s2: &[u8]) -> bool {
    let (mut a, mut b) = (0usize, 0usize);
    while s1[a] != 0 && s1[a] == s2[b] {
        a += 1;
        b += 1;
    }
    if s1[a] == s2[b] {
        return true;
    }
    if s1[a] == 0 && s2[b] == b'\n' && s2[b + 1] == 0 {
        return true;
    }
    if s1[a] == b'\n' && s1[a + 1] == 0 && s2[b] == 0 {
        return true;
    }
    false
}

/// `match_string` — index of `s` in `array`, or -EINVAL. `array` is the
/// `char * const *` model: `None` entries are C NULLs and the first one
/// stops the scan; `n` bounds it independently (callers pass
/// `array.len()`, or a larger n with a NULL terminator).
pub fn match_string(array: &[Option<&[u8]>], n: usize, s: &[u8]) -> i32 {
    let mut index = 0usize;
    while index < n {
        let item = match array[index] {
            None => break,
            Some(item) => item,
        };
        if strcmp(item, s) == 0 {
            return index as i32;
        }
        index += 1;
    }
    -EINVAL
}

/// `__sysfs_match_string` — like `match_string` but with `sysfs_streq`.
pub fn __sysfs_match_string(array: &[Option<&[u8]>], n: usize, s: &[u8]) -> i32 {
    let mut index = 0usize;
    while index < n {
        let item = match array[index] {
            None => break,
            Some(item) => item,
        };
        if sysfs_streq(item, s) {
            return index as i32;
        }
        index += 1;
    }
    -EINVAL
}

/// `strreplace` — replace every `old` with `new` in the NUL-terminated
/// string, in place (upstream returns `str`, which the slice already is).
pub fn strreplace(s: &mut [u8], old: u8, new: u8) {
    let mut i = 0;
    while s[i] != 0 {
        if s[i] == old {
            s[i] = new;
        }
        i += 1;
    }
}

/// `memcpy_and_pad` — copy `count` bytes then pad to `dest_len`, or copy
/// `dest_len` bytes when `count >= dest_len`.
pub fn memcpy_and_pad(dest: &mut [u8], dest_len: usize, src: &[u8], count: usize, pad: u8) {
    if dest_len > count {
        memcpy(dest, src, count);
        memset(&mut dest[count..], pad, dest_len - count);
    } else {
        memcpy(dest, src, dest_len);
    }
}

/// `string_is_terminated` — `memchr(s, '\0', len) != NULL`. Upstream takes
/// `int len`; `usize` keeps the safe model (`len <= s.len()` contract,
/// like the other slice functions).
pub fn string_is_terminated(s: &[u8], len: usize) -> bool {
    memchr(s, 0, len).is_some()
}

/// `string_upper` — copy `src` to `dst` uppercased, including the NUL.
pub fn string_upper(dst: &mut [u8], src: &[u8]) {
    let mut i = 0;
    loop {
        let c = src[i];
        dst[i] = toupper(c);
        if c == 0 {
            break;
        }
        i += 1;
    }
}

/// `string_lower` — copy `src` to `dst` lowercased, including the NUL.
pub fn string_lower(dst: &mut [u8], src: &[u8]) {
    let mut i = 0;
    loop {
        let c = src[i];
        dst[i] = tolower(c);
        if c == 0 {
            break;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_family() {
        assert_eq!(strcmp(b"abc\0", b"abc\0"), 0);
        assert!(strcmp(b"abc\0", b"abd\0") < 0);
        assert!(strcmp(b"abd\0", b"abc\0") > 0);
        // comparison stops at NUL — trailing bytes are not part of the string
        assert_eq!(strcmp(b"abc\0", b"abc\0extra"), 0);
        assert_eq!(strncmp(b"abc", b"abd", 2), 0);
        assert_eq!(strncasecmp(b"AbC", b"aBc", 3), 0);
        assert_ne!(strncasecmp(b"AbD", b"aBc", 3), 0);
        assert_eq!(strcasecmp(b"Hello\0", b"heLLo\0"), 0);
        // kernel strcmp returns a normalized sign (signed char __res), not
        // the byte difference like glibc
        assert_eq!(strcmp(b"a\xff\0", b"a\x01\0"), 1);
    }

    #[test]
    fn length_and_search() {
        assert_eq!(strlen(b"hello\0world"), 5);
        assert_eq!(strlen(b"\0"), 0);
        assert_eq!(strnlen(b"hello\0world", 100), 5);
        assert_eq!(strnlen(b"hello\0world", 3), 3);
        assert_eq!(strchr(b"hello\0", b'l'), Some(2));
        assert_eq!(strchr(b"hello\0", b'z'), None);
        assert_eq!(strchr(b"hello\0", 0), Some(5)); // NUL itself is findable
        assert_eq!(strchrnul(b"hello\0", b'z'), 5);
        assert_eq!(strrchr(b"hello\0", b'l'), Some(3));
        assert_eq!(strnchr(b"aaaab\0", 3, b'a'), Some(0));
        assert_eq!(strnchr(b"aaaab\0", 3, b'b'), None);
        assert_eq!(strnchrnul(b"abc\0", 2, b'd'), 2);
        assert_eq!(strspn(b"aabbc\0", b"ab\0"), 4);
        assert_eq!(strcspn(b"aabbc\0", b"c\0"), 4);
        assert_eq!(strpbrk(b"hello\0", b"ol\0"), Some(2));
        assert_eq!(strstr(b"hello world\0", b"o w\0"), Some(4));
        assert_eq!(strstr(b"hello\0", b"hello world\0"), None);
        assert_eq!(strnstr(b"hello world\0", b"o w\0", 6), None);
        assert_eq!(strnstr(b"hello world\0", b"o w\0", 7), Some(4));
    }

    #[test]
    fn copy_family() {
        let mut d = [0u8; 32];
        strcpy(&mut d, b"hello\0");
        assert_eq!(&d[..6], b"hello\0");
        assert_eq!(stpcpy(&mut d, b"abc\0"), 3);
        let mut d = [0u8; 32];
        strcat(&mut d, b"hello\0");
        strcat(&mut d, b" world\0");
        assert_eq!(&d[..12], b"hello world\0");
        let mut d = [0u8; 32];
        strncat(&mut d, b"ab\0", 10);
        strncat(&mut d, b"cdefgh\0", 3);
        assert_eq!(&d[..6], b"abcde\0");
        let mut d = [b'x'; 8];
        d[0] = 0;
        assert_eq!(strlcat(&mut d, b"abc\0", 8), 3);
        assert_eq!(&d[..4], b"abc\0");
        // truncation: returns intended length
        let mut d = [b'x'; 4];
        d[0] = 0;
        assert_eq!(strlcat(&mut d, b"abcdef\0", 4), 6);
        assert_eq!(&d[..4], b"abc\0");
    }

    #[test]
    fn strscpy_short_src_zeroes() {
        // short-but-terminated src: C's masked word store zeroes dest bytes
        // after the NUL within the final word — the implicit-NUL load
        // reproduces it exactly.
        let mut d = [0x55u8; 16];
        assert_eq!(sized_strscpy(&mut d, b"hi\0", 16), 2);
        assert_eq!(&d[..8], b"hi\0\0\0\0\0\0");
        // unterminated-in-slice src: implicit NUL at slice end, deterministic
        let mut d = [0x55u8; 16];
        assert_eq!(sized_strscpy(&mut d, b"hi", 16), 2);
        assert_eq!(&d[..8], b"hi\0\0\0\0\0\0");
    }

    #[test]
    fn sized_strscpy_returns() {
        let mut d = [0u8; 16];
        assert_eq!(sized_strscpy(&mut d, b"hello\0", 16), 5);
        assert_eq!(&d[..6], b"hello\0");
        let mut d = [0u8; 16];
        assert_eq!(sized_strscpy(&mut d, b"hello world\0", 4), -7); // -E2BIG
        assert_eq!(&d[..4], b"hel\0");
        assert_eq!(sized_strscpy(&mut d, b"x\0", 0), -7);
    }

    #[test]
    fn strsep_tokens() {
        let mut buf = *b"a,b,c\0";
        let mut cur = Some(0usize);
        assert_eq!(strsep(&mut cur, &mut buf, b",\0"), Some(0));
        assert_eq!(&buf[0..2], b"a\0");
        assert_eq!(strsep(&mut cur, &mut buf, b",\0"), Some(2));
        assert_eq!(strsep(&mut cur, &mut buf, b",\0"), Some(4));
        assert_eq!(cur, None);
        assert_eq!(strsep(&mut cur, &mut buf, b",\0"), None);
    }

    #[test]
    fn mem_family() {
        let mut d = [0u8; 16];
        memset(&mut d, 0xab, 8);
        assert_eq!(&d[..8], &[0xab; 8]);
        let mut d16 = [0u16; 8];
        memset16(&mut d16, 0x1234, 4);
        assert_eq!(d16[..4], [0x1234; 4]);
        let mut d32 = [0u32; 4];
        memset32(&mut d32, 0xdeadbeef, 2);
        assert_eq!(d32[..2], [0xdeadbeef; 2]);
        let mut d64 = [0u64; 2];
        memset64(&mut d64, 0xffffffffffffffff, 1);
        assert_eq!(d64[0], u64::MAX);
        let src = *b"0123456789abcdef";
        let mut d = [0u8; 16];
        memcpy(&mut d, &src, 16);
        assert_eq!(d, src);
        // memmove overlap both directions
        let mut b = *b"0123456789abcdef";
        memmove(&mut b, 2, 0, 10); // dst > src, copies "0123456789" right
        assert_eq!(&b, b"010123456789cdef");
        let mut b = *b"0123456789abcdef";
        memmove(&mut b, 0, 2, 10); // dst < src, copies "23456789ab" left
        assert_eq!(&b, b"23456789ababcdef");
    }

    #[test]
    fn memcmp_bcmp_scan() {
        assert_eq!(memcmp(b"abc", b"abc", 3), 0);
        assert_eq!(memcmp(b"abc", b"abd", 3), -1);
        // C returns byte difference
        assert_eq!(memcmp(b"\xff", b"\x01", 1), 254);
        assert_eq!(memcmp(b"abcdefghijklmnopq", b"abcdefghijklmnopr", 17), -1);
        assert_eq!(bcmp(b"abc", b"abc", 3), 0);
        assert_ne!(bcmp(b"abc", b"abd", 3), 0);
        assert_eq!(memscan(b"xxax", b'a', 4), 2);
        assert_eq!(memscan(b"xxxx", b'a', 4), 4); // past end = "not found" addr
    }

    #[test]
    fn memchr_family() {
        assert_eq!(memchr(b"hello", b'l', 5), Some(2));
        assert_eq!(memchr(b"hello", b'z', 5), None);
        let all7 = [7u8; 40];
        assert_eq!(memchr_inv(&all7, 7, 40), None);
        let mut b = [7u8; 40];
        b[19] = 9;
        assert_eq!(memchr_inv(&b, 7, 40), Some(19));
        b[1] = 3;
        assert_eq!(memchr_inv(&b, 7, 40), Some(1));
        // small buffers take check_bytes8 path
        let mut s = [5u8; 10];
        s[9] = 1;
        assert_eq!(memchr_inv(&s, 5, 10), Some(9));
    }

    #[test]
    fn sysfs_streq_rules() {
        assert!(sysfs_streq(b"abc\0", b"abc\0"));
        assert!(sysfs_streq(b"abc\0", b"abc\n\0"));
        assert!(sysfs_streq(b"abc\n\0", b"abc\0"));
        assert!(sysfs_streq(b"abc\n\0", b"abc\n\0"));
        assert!(sysfs_streq(b"\0", b"\n\0"));
        assert!(sysfs_streq(b"\n\0", b"\0"));
        assert!(!sysfs_streq(b"abc\0", b"abd\n\0"));
        assert!(!sysfs_streq(b"ab\0", b"abc\n\0"));
        assert!(!sysfs_streq(b"abc\n\0", b"abd\0"));
        // an interior newline is a real character, not a terminator
        assert!(!sysfs_streq(b"ab\ncd\0", b"ab\0"));
        // ...but a newline before the terminator matches like upstream:
        // as C strings these are "ab\n" and "ab" — 'x' past the NUL is
        // unreachable, same as for the C char* it models
        assert!(sysfs_streq(b"ab\n\0x", b"ab\0"));
    }

    #[test]
    fn match_string_family() {
        let arr: [Option<&[u8]>; 4] = [
            Some(b"one\0"),
            Some(b"two\0"),
            Some(b"three\0"),
            None,
        ];
        assert_eq!(match_string(&arr, 4, b"two\0"), 1);
        assert_eq!(match_string(&arr, 4, b"four\0"), -EINVAL);
        // the n bound stops the scan before the NULL terminator
        assert_eq!(match_string(&arr, 2, b"three\0"), -EINVAL);
        // n = -1 upstream: an unbounded scan stops at the first NULL
        assert_eq!(match_string(&arr, usize::MAX, b"three\0"), 2);
        let arr2: [Option<&[u8]>; 3] = [Some(b"a\0"), None, Some(b"c\0")];
        assert_eq!(match_string(&arr2, 3, b"c\0"), -EINVAL);
        // sysfs variant: newline-terminated query matches too
        assert_eq!(__sysfs_match_string(&arr, 4, b"two\n\0"), 1);
        assert_eq!(match_string(&arr, 4, b"two\n\0"), -EINVAL);
        // and a newline on the array side
        let arr3: [Option<&[u8]>; 2] = [Some(b"on\n\0"), None];
        assert_eq!(__sysfs_match_string(&arr3, 2, b"on\0"), 0);
    }

    #[test]
    fn trim_and_case() {
        assert_eq!(skip_spaces(b"  \t\nabc\0"), 4);
        assert_eq!(skip_spaces(b"abc\0"), 0);
        // kernel isspace covers 0xa0 via the _ctype _S flag
        assert_eq!(skip_spaces(b"\xa0 x\0"), 2);
        let mut s = *b"  hi  \0";
        assert_eq!(strim(&mut s), 2);
        assert_eq!(&s[..5], b"  hi\0");
        let mut all = *b"   \0";
        assert_eq!(strim(&mut all), 0);
        assert_eq!(&all[..1], b"\0");
        let mut empty = *b"\0";
        assert_eq!(strim(&mut empty), 0);
        let mut d = [0u8; 8];
        string_upper(&mut d, b"Ab3\0");
        assert_eq!(&d[..4], b"AB3\0");
        string_lower(&mut d, b"Ab3\0");
        assert_eq!(&d[..4], b"ab3\0");
        // kernel case mapping covers Latin-1: 0xe1 -> 0xc1, 0xd8 -> 0xf8
        string_upper(&mut d, b"\xe1\0");
        assert_eq!(&d[..2], b"\xc1\0");
        string_lower(&mut d, b"\xd8\0");
        assert_eq!(&d[..2], b"\xf8\0");
    }

    #[test]
    fn replace_pad_terminated() {
        let mut s = *b"a,b,c\0";
        strreplace(&mut s, b',', b';');
        assert_eq!(&s[..6], b"a;b;c\0");
        let mut d = [0u8; 8];
        memcpy_and_pad(&mut d, 6, b"ab", 2, b'.');
        assert_eq!(&d[..6], b"ab....");
        memcpy_and_pad(&mut d, 3, b"wxyz", 5, b'.');
        assert_eq!(&d[..3], b"wxy");
        assert!(string_is_terminated(b"ab\0cd", 3));
        assert!(!string_is_terminated(b"ab\0cd", 2));
        assert!(!string_is_terminated(b"abc", 3));
    }
}
