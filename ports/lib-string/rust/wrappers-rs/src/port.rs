//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Mirrors of lib/string.c (x86-64 config: DCACHE_WORD_ACCESS, 64BIT,
//! HAVE_EFFICIENT_UNALIGNED_ACCESS, ARCH_HAS_FAST_MULTIPLIER) plus the
//! seeded-buffer harness.  Pointers become slice indices; pointer-returning
//! functions return `Option<usize>` offsets.

#![forbid(unsafe_code)]

// ---- harness mirror --------------------------------------------------------

const SSZ: usize = 640;
const DSZ: usize = 1280;

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

fn fnv1a_u16(w: &[u16]) -> u64 {
    let mut h = FNV_INIT;
    for &x in w {
        for b in x.to_ne_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

fn fnv1a_u32(w: &[u32]) -> u64 {
    let mut h = FNV_INIT;
    for &x in w {
        for b in x.to_ne_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

fn fnv1a_u64(w: &[u64]) -> u64 {
    let mut h = FNV_INIT;
    for &x in w {
        for b in x.to_ne_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
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

fn fill_typed(w16: &mut [u16; 640], w32: &mut [u32; 640], w64: &mut [u64; 160], seed: u64) {
    let mut r = seed | 1;
    for i in 0..640 {
        w16[i] = xs64(&mut r) as u16;
        w32[i] = xs64(&mut r) as u32;
    }
    for x in w64.iter_mut() {
        *x = xs64(&mut r);
    }
}

/// Build a string in the first SSZ bytes of b: n non-zero body bytes + a
/// forced NUL at n, over a deterministic raw tail.  mode&1 plants an extra
/// NUL at a seeded position inside the body; mode&2 fills with 'A'+x%58
/// (case-fold collisions) instead of x|1.
fn make_str(b: &mut [u8], n: usize, seed: u64, mode: i64) -> usize {
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
    n
}

/// String destination in dbuf: deterministic fill of the whole DSZ buffer
/// under the string content.
fn make_dstr(d: &mut [u8; DSZ], n: usize, seed: u64, mode: i64) {
    fill_raw(d, seed ^ 0x9e3779b97f4a7c15);
    make_str(d, n, seed, mode);
}

/// s1 in sbuf, s2 in sbuf2; mode&16 makes s2 a verbatim copy of s1.
fn two_strs(
    s1: &mut [u8; SSZ],
    s2: &mut [u8; SSZ],
    sa: u64,
    sb: u64,
    na: usize,
    nb: usize,
    mode: i64,
) {
    make_str(s1, na, sa, mode & 3);
    if mode & 16 != 0 {
        s2.copy_from_slice(s1);
    } else {
        make_str(s2, nb, sb, (mode >> 2) & 3);
    }
}

/// Byte to search for inside a C string: mode&8 -> 0; mode&4 -> a byte
/// that is definitely in the string; otherwise a raw byte.
fn pick_str_c(csel: i64, s: &[u8], mode: i64) -> i64 {
    let n = strlen_c(s);
    if mode & 8 != 0 {
        return 0;
    }
    if mode & 4 != 0 && n != 0 {
        return s[blen(csel, n)] as i64;
    }
    csel & 0xff
}

/// Byte to search for inside a raw memory region of n bytes.
fn pick_mem_c(csel: i64, b: &[u8], n: usize, mode: i64) -> i64 {
    if mode & 4 != 0 && n != 0 {
        return b[blen(csel, n)] as i64;
    }
    csel & 0xff
}

// ---- lib/string.c ports -----------------------------------------------------

const E2BIG: i64 = 7;
const WAAT_ONES: u64 = 0x0101010101010101;
const WAAT_HIGHS: u64 = 0x8080808080808080;
const ALLBUTLAST_BYTE_MASK: u64 = u64::MAX >> 8; /* little-endian */

fn tolower_c(c: u8) -> u8 {
    if c.is_ascii_uppercase() { c + 32 } else { c }
}

fn strncasecmp_c(s1: &[u8], s2: &[u8], len: usize) -> i64 {
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
    c1 as i64 - c2 as i64
}

fn strcasecmp_c(s1: &[u8], s2: &[u8]) -> i64 {
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let c1 = tolower_c(s1[i]);
        i += 1;
        let c2 = tolower_c(s2[j]);
        j += 1;
        if !(c1 == c2 && c1 != 0) {
            return c1 as i64 - c2 as i64;
        }
    }
}

fn strcpy_c(dest: &mut [u8], src: &[u8]) {
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

fn sized_strscpy(dest: &mut [u8], src: &[u8], count: usize) -> i64 {
    let mut max = count;
    let mut res = 0usize;
    if count == 0 || count > i32::MAX as usize {
        return -E2BIG;
    }
    let mut count = count;
    while max >= 8 {
        // load_unaligned_zeropad: plain unaligned word load in userspace.
        let c = u64::from_ne_bytes(src[res..res + 8].try_into().unwrap());
        let data = has_zero(c);
        if data != 0 {
            let out = c & zero_bytemask(data);
            dest[res..res + 8].copy_from_slice(&out.to_ne_bytes());
            return res as i64 + find_zero(data) as i64;
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
        let c = src[res];
        dest[res] = c;
        if c == 0 {
            return res as i64;
        }
        res += 1;
        count -= 1;
    }
    dest[res] = 0;
    if src[res] != 0 { -E2BIG } else { res as i64 }
}

fn stpcpy_c(dest: &mut [u8], src: &[u8]) -> usize {
    let mut i = 0;
    loop {
        dest[i] = src[i];
        if src[i] == 0 {
            return i;
        }
        i += 1;
    }
}

fn strcat_c(dest: &mut [u8], src: &[u8]) {
    let mut i = strlen_c(dest);
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

fn strncat_c(dest: &mut [u8], src: &[u8], count: usize) {
    if count == 0 {
        return;
    }
    let mut i = strlen_c(dest);
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

fn strlcat_c(dest: &mut [u8], src: &[u8], count: usize) -> usize {
    let dsize = strlen_c(dest);
    let mut len = strlen_c(src);
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

fn strcmp_c(s1: &[u8], s2: &[u8]) -> i64 {
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

fn strncmp_c(s1: &[u8], s2: &[u8], count: usize) -> i64 {
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

fn strchr_c(s: &[u8], c: u8) -> Option<usize> {
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

fn strchrnul_c(s: &[u8], c: u8) -> usize {
    let mut i = 0;
    while s[i] != 0 && s[i] != c {
        i += 1;
    }
    i
}

fn strnchrnul_c(s: &[u8], count: usize, c: u8) -> usize {
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

fn strrchr_c(s: &[u8], c: u8) -> Option<usize> {
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

fn strnchr_c(s: &[u8], count: usize, c: u8) -> Option<usize> {
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

fn strlen_c(s: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        i += 1;
    }
    i
}

fn strnlen_c(s: &[u8], count: usize) -> usize {
    let mut i = 0;
    let mut count = count;
    while count != 0 && s[i] != 0 {
        i += 1;
        count -= 1;
    }
    i
}

fn strspn_c(s: &[u8], accept: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        if strchr_c(accept, s[i]).is_none() {
            break;
        }
        i += 1;
    }
    i
}

fn strcspn_c(s: &[u8], reject: &[u8]) -> usize {
    let mut i = 0;
    while s[i] != 0 {
        if strchr_c(reject, s[i]).is_some() {
            break;
        }
        i += 1;
    }
    i
}

fn strpbrk_c(s: &[u8], ct: &[u8]) -> Option<usize> {
    let mut i = 0;
    while s[i] != 0 {
        if strchr_c(ct, s[i]).is_some() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `p` is the `char **` cursor modelled as an offset into `buf`.
fn strsep_c(p: &mut Option<usize>, buf: &mut [u8], ct: &[u8]) -> Option<usize> {
    let sbegin = (*p)?;
    if let Some(e) = strpbrk_c(&buf[sbegin..], ct) {
        let e = sbegin + e;
        buf[e] = 0;
        *p = Some(e + 1);
    } else {
        *p = None;
    }
    Some(sbegin)
}

fn memset_c(d: &mut [u8], c: u8, n: usize) {
    for x in d[..n].iter_mut() {
        *x = c;
    }
}

fn memset16_c(w: &mut [u16], v: u16, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

fn memset32_c(w: &mut [u32], v: u32, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

fn memset64_c(w: &mut [u64], v: u64, n: usize) {
    for x in w[..n].iter_mut() {
        *x = v;
    }
}

fn memcpy_c(d: &mut [u8], s: &[u8], n: usize) {
    d[..n].copy_from_slice(&s[..n]);
}

/// Single-buffer memmove: forward when doff <= soff, backward otherwise.
fn memmove_c(d: &mut [u8], doff: usize, soff: usize, n: usize) {
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

fn memcmp_c(a: &[u8], b: &[u8], n: usize) -> i64 {
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
    res as i64
}

fn memscan_c(s: &[u8], c: u8, size: usize) -> usize {
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

fn strstr_c(s1: &[u8], s2: &[u8]) -> Option<usize> {
    let l2 = strlen_c(s2);
    if l2 == 0 {
        return Some(0);
    }
    let mut l1 = strlen_c(s1);
    let mut i = 0;
    while l1 >= l2 {
        l1 -= 1;
        if memcmp_c(&s1[i..], s2, l2) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn strnstr_c(s1: &[u8], s2: &[u8], len: usize) -> Option<usize> {
    let l2 = strlen_c(s2);
    if l2 == 0 {
        return Some(0);
    }
    let mut len = len;
    let mut i = 0;
    while len >= l2 {
        len -= 1;
        if memcmp_c(&s1[i..], s2, l2) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn memchr_c(s: &[u8], c: u8, n: usize) -> Option<usize> {
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
fn memchr_inv_c(start: &[u8], c: i64, bytes: usize) -> Option<usize> {
    let value = c as u8;
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
        if let Some(r) = check_bytes8(&start[..prefix.min(start.len())], value, prefix) {
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

// ---- exported wrapper mirrors ------------------------------------------------

/// Port of `int64_t wrap_strncasecmp(...)` (`wrappers.c`).
pub fn wrap_strncasecmp(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strncasecmp_c(&sbuf, &sbuf2, blen(cnt, SSZ))
}

/// Port of `int64_t wrap_strcasecmp(...)`.
pub fn wrap_strcasecmp(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strcasecmp_c(&sbuf, &sbuf2)
}

/// Port of `uint64_t wrap_strcpy(...)`.
pub fn wrap_strcpy(sa: u64, la: i64, mode: i64) -> u64 {
    let na = blen(la, 600);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0u8; DSZ];
    make_str(&mut sbuf, na, sa, mode & 3);
    fill_raw(&mut dbuf, sa ^ 0x9e3779b97f4a7c15);
    strcpy_c(&mut dbuf, &sbuf);
    mix(strlen_c(&dbuf) as u64, fnv1a(&dbuf), na as u64)
}

/// Port of `uint64_t wrap_sized_strscpy(...)`.
pub fn wrap_sized_strscpy(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    let na = blen(la, SSZ - 1);
    let count = if mode & 4 != 0 {
        i32::MAX as usize + 1
    } else {
        blen(cnt, SSZ)
    };
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0u8; DSZ];
    make_str(&mut sbuf, na, sa, mode & 3);
    fill_raw(&mut dbuf, sa ^ 0x9e3779b97f4a7c15);
    let res = sized_strscpy(&mut dbuf, &sbuf, count);
    mix((res + 16) as u64, fnv1a(&dbuf), count as u64)
}

/// Port of `uint64_t wrap_stpcpy(...)`.
pub fn wrap_stpcpy(sa: u64, la: i64, mode: i64) -> u64 {
    let na = blen(la, 600);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0u8; DSZ];
    make_str(&mut sbuf, na, sa, mode & 3);
    fill_raw(&mut dbuf, sa ^ 0x9e3779b97f4a7c15);
    let r = stpcpy_c(&mut dbuf, &sbuf);
    mix(r as u64, fnv1a(&dbuf), na as u64)
}

/// Port of `uint64_t wrap_strcat(...)`.
pub fn wrap_strcat(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    let mut dbuf = [0u8; DSZ];
    let mut sbuf = [0u8; SSZ];
    make_dstr(&mut dbuf, blen(la, 600), sa, mode & 3);
    make_str(&mut sbuf, blen(lb, SSZ - 1), sb, (mode >> 2) & 3);
    strcat_c(&mut dbuf, &sbuf);
    mix(strlen_c(&dbuf) as u64, fnv1a(&dbuf), 0)
}

/// Port of `uint64_t wrap_strncat(...)`.
pub fn wrap_strncat(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> u64 {
    let count = blen(cnt, 600);
    let mut dbuf = [0u8; DSZ];
    let mut sbuf = [0u8; SSZ];
    make_dstr(&mut dbuf, blen(la, 600), sa, mode & 3);
    make_str(&mut sbuf, blen(lb, SSZ - 1), sb, (mode >> 2) & 3);
    strncat_c(&mut dbuf, &sbuf, count);
    mix(strlen_c(&dbuf) as u64, fnv1a(&dbuf), count as u64)
}

/// Port of `uint64_t wrap_strlcat(...)`.
pub fn wrap_strlcat(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> u64 {
    let na = blen(la, 600);
    let mut dbuf = [0u8; DSZ];
    let mut sbuf = [0u8; SSZ];
    make_dstr(&mut dbuf, na, sa, mode & 3);
    make_str(&mut sbuf, blen(lb, SSZ - 1), sb, (mode >> 2) & 3);
    let count = na + 1 + blen(cnt, DSZ - na - 1);
    let res = strlcat_c(&mut dbuf, &sbuf, count);
    mix(res as u64, fnv1a(&dbuf), count as u64)
}

/// Port of `int64_t wrap_strcmp(...)`.
pub fn wrap_strcmp(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strcmp_c(&sbuf, &sbuf2)
}

/// Port of `int64_t wrap_strncmp(...)`.
pub fn wrap_strncmp(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strncmp_c(&sbuf, &sbuf2, blen(cnt, SSZ))
}

/// Port of `int64_t wrap_strchr(...)`.
pub fn wrap_strchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    let cv = pick_str_c(c, &sbuf, mode) as u8;
    match strchr_c(&sbuf, cv) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `int64_t wrap_strchrnul(...)`.
pub fn wrap_strchrnul(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    let cv = pick_str_c(c, &sbuf, mode) as u8;
    strchrnul_c(&sbuf, cv) as i64
}

/// Port of `int64_t wrap_strnchrnul(...)`.
pub fn wrap_strnchrnul(sa: u64, la: i64, c: i64, cnt: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    let cv = pick_str_c(c, &sbuf, mode) as u8;
    strnchrnul_c(&sbuf, blen(cnt, SSZ), cv) as i64
}

/// Port of `int64_t wrap_strrchr(...)`.
pub fn wrap_strrchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    let cv = pick_str_c(c, &sbuf, mode) as u8;
    match strrchr_c(&sbuf, cv) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `int64_t wrap_strnchr(...)`.
pub fn wrap_strnchr(sa: u64, la: i64, c: i64, cnt: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    let cv = pick_str_c(c, &sbuf, mode) as u8;
    match strnchr_c(&sbuf, blen(cnt, SSZ), cv) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `uint64_t wrap_strlen(...)`.
pub fn wrap_strlen(sa: u64, la: i64, mode: i64) -> u64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    strlen_c(&sbuf) as u64
}

/// Port of `uint64_t wrap_strnlen(...)`.
pub fn wrap_strnlen(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    let mut sbuf = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    strnlen_c(&sbuf, blen(cnt, SSZ)) as u64
}

/// Port of `uint64_t wrap_strspn(...)`.
pub fn wrap_strspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strspn_c(&sbuf, &sbuf2) as u64
}

/// Port of `uint64_t wrap_strcspn(...)`.
pub fn wrap_strcspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    strcspn_c(&sbuf, &sbuf2) as u64
}

/// Port of `int64_t wrap_strpbrk(...)`.
pub fn wrap_strpbrk(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    two_strs(
        &mut sbuf,
        &mut sbuf2,
        sa,
        sb,
        blen(la, SSZ - 1),
        blen(lb, SSZ - 1),
        mode,
    );
    match strpbrk_c(&sbuf, &sbuf2) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `uint64_t wrap_strsep(...)`: `char **` modelled as an offset
/// cursor into dbuf, 1..4 calls per invocation.
pub fn wrap_strsep(sa: u64, sb: u64, la: i64, lb: i64, cur: i64, mode: i64) -> u64 {
    let na = blen(la, 1000);
    let nb = blen(lb, 300);
    let mut dbuf = [0u8; DSZ];
    let mut sbuf2 = [0u8; SSZ];
    let mut acc = 0u64;
    let reps = 1 + ((mode >> 5).unsigned_abs() & 3) as usize;
    make_dstr(&mut dbuf, na, sa, mode & 3);
    make_str(&mut sbuf2, nb, sb, (mode >> 2) & 3);
    let mut p = if mode & 16 != 0 {
        None
    } else {
        Some(blen(cur, na + 1))
    };
    for _ in 0..reps {
        let tok = strsep_c(&mut p, &mut dbuf, &sbuf2);
        acc = mix(
            acc,
            tok.map(|t| t as u64 + 1).unwrap_or(0),
            p.map(|t| t as u64 + 1).unwrap_or(0),
        );
    }
    mix(acc, fnv1a(&dbuf), na as u64)
}

/// Port of `uint64_t wrap_memset(...)`.
pub fn wrap_memset(sb: u64, c: i64, len: i64, off: i64, mode: i64) -> u64 {
    let mut o = blen(off, 640);
    let mut n = blen(len, 640);
    let mut dbuf = [0u8; DSZ];
    fill_raw(&mut dbuf, sb);
    if mode & 1 != 0 {
        o = 0;
        n = DSZ;
    }
    memset_c(&mut dbuf[o..], c as u8, n);
    mix(
        o as u64,
        fnv1a(&dbuf),
        (((c & 0xff) as u64) << 16) | n as u64,
    )
}

/// Port of `uint64_t wrap_memset16(...)`.
pub fn wrap_memset16(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    let mut o = blen(off, 320);
    let mut n = blen(len, 320);
    let mut w16 = [0u16; 640];
    let mut w32 = [0u32; 640];
    let mut w64 = [0u64; 160];
    fill_typed(&mut w16, &mut w32, &mut w64, sb);
    if mode & 1 != 0 {
        o = 0;
        n = 640;
    }
    memset16_c(&mut w16[o..], v as u16, n);
    mix(
        o as u64,
        fnv1a_u16(&w16),
        (((v & 0xffff) as u64) << 16) | n as u64,
    )
}

/// Port of `uint64_t wrap_memset32(...)`.
pub fn wrap_memset32(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    let mut o = blen(off, 320);
    let mut n = blen(len, 320);
    let mut w16 = [0u16; 640];
    let mut w32 = [0u32; 640];
    let mut w64 = [0u64; 160];
    fill_typed(&mut w16, &mut w32, &mut w64, sb);
    if mode & 1 != 0 {
        o = 0;
        n = 640;
    }
    memset32_c(&mut w32[o..], v as u32, n);
    mix(
        o as u64,
        fnv1a_u32(&w32),
        (((v & 0xffffff) as u64) << 16) | n as u64,
    )
}

/// Port of `uint64_t wrap_memset64(...)`.
pub fn wrap_memset64(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    let mut o = blen(off, 80);
    let mut n = blen(len, 80);
    let mut w16 = [0u16; 640];
    let mut w32 = [0u32; 640];
    let mut w64 = [0u64; 160];
    fill_typed(&mut w16, &mut w32, &mut w64, sb);
    if mode & 1 != 0 {
        o = 0;
        n = 160;
    }
    memset64_c(&mut w64[o..], v as u64, n);
    mix(o as u64, fnv1a_u64(&w64), n as u64)
}

/// Port of `uint64_t wrap_memcpy(...)`.
pub fn wrap_memcpy(sa: u64, sb: u64, doff: i64, soff: i64, len: i64, mode: i64) -> u64 {
    let so = blen(soff, 320);
    let mut n = blen(len, 320);
    let dof = blen(doff, DSZ - 320);
    let mut sbuf = [0u8; SSZ];
    let mut dbuf = [0u8; DSZ];
    fill_raw(&mut sbuf, sa);
    fill_raw(&mut dbuf, sb);
    if mode & 1 != 0 {
        n = 0;
    }
    memcpy_c(&mut dbuf[dof..], &sbuf[so..], n);
    mix(dof as u64, fnv1a(&dbuf), n as u64)
}

/// Port of `uint64_t wrap_memmove(...)`: overlapping moves inside dbuf.
pub fn wrap_memmove(sb: u64, doff: i64, soff: i64, len: i64, mode: i64) -> u64 {
    let so = blen(soff, 640);
    let mut dof = blen(doff, 640);
    let n = blen(len, 640);
    let mut dbuf = [0u8; DSZ];
    fill_raw(&mut dbuf, sb);
    if mode & 1 != 0 {
        dof = so;
    }
    if mode & 2 != 0 {
        dof = so + 1;
    }
    memmove_c(&mut dbuf, dof, so, n);
    mix(dof as u64, fnv1a(&dbuf), ((dof as u64) << 16) | so as u64)
}

/// Port of `int64_t wrap_memcmp(...)`.
pub fn wrap_memcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    let n = blen(len, SSZ);
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    fill_raw(&mut sbuf, sa);
    fill_raw(&mut sbuf2, sb);
    if mode & 16 != 0 {
        sbuf2[..n].copy_from_slice(&sbuf[..n]);
    }
    if mode & 32 != 0 && n != 0 {
        sbuf2[blen(len ^ 0x5a, n)] ^= 0xff;
    }
    memcmp_c(&sbuf, &sbuf2, n)
}

/// Port of `int64_t wrap_bcmp(...)`.
pub fn wrap_bcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    let n = blen(len, SSZ);
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    fill_raw(&mut sbuf, sa);
    fill_raw(&mut sbuf2, sb);
    if mode & 16 != 0 {
        sbuf2[..n].copy_from_slice(&sbuf[..n]);
    }
    if mode & 32 != 0 && n != 0 {
        sbuf2[blen(len ^ 0x5a, n)] ^= 0xff;
    }
    memcmp_c(&sbuf, &sbuf2, n)
}

/// Port of `int64_t wrap_memscan(...)`.
pub fn wrap_memscan(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    let n = blen(len, SSZ);
    let mut sbuf = [0u8; SSZ];
    fill_raw(&mut sbuf, sb);
    let cv = pick_mem_c(c, &sbuf, n, mode) as u8;
    memscan_c(&sbuf, cv, n) as i64
}

/// Port of `int64_t wrap_memchr(...)`.
pub fn wrap_memchr(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    let n = blen(len, SSZ);
    let mut sbuf = [0u8; SSZ];
    fill_raw(&mut sbuf, sb);
    let cv = pick_mem_c(c, &sbuf, n, mode) as u8;
    match memchr_c(&sbuf, cv, n) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `int64_t wrap_memchr_inv(...)`.
pub fn wrap_memchr_inv(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    let off = blen(mode >> 4, 8);
    let n = blen(len, SSZ - 8);
    let cv = c as u8;
    let mut dbuf = [0u8; DSZ];
    fill_raw(&mut dbuf, sb);
    match mode & 3 {
        1 => memset_c(&mut dbuf[off..], cv, n),
        2 => {
            let k = if n != 0 { blen(len ^ 0x33, n) } else { 0 };
            memset_c(&mut dbuf[off..], cv, n);
            dbuf[off + k] = cv.wrapping_add(1);
        }
        _ => {}
    }
    match memchr_inv_c(&dbuf[off..], cv as i64, n) {
        Some(r) => (off + r) as i64,
        None => -1,
    }
}

/// Port of `int64_t wrap_strstr(...)`.
pub fn wrap_strstr(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    make_str(&mut sbuf2, blen(lb, SSZ - 1), sb, (mode >> 2) & 3);
    if mode & 16 != 0 {
        let l1 = strlen_c(&sbuf);
        let l2 = strlen_c(&sbuf2);
        if l2 != 0 && l1 >= l2 {
            let pos = blen(mode >> 8, l1 - l2 + 1);
            sbuf[pos..pos + l2].copy_from_slice(&sbuf2[..l2]);
        }
    }
    match strstr_c(&sbuf, &sbuf2) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// Port of `int64_t wrap_strnstr(...)`.
pub fn wrap_strnstr(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    let mut sbuf = [0u8; SSZ];
    let mut sbuf2 = [0u8; SSZ];
    make_str(&mut sbuf, blen(la, SSZ - 1), sa, mode & 3);
    make_str(&mut sbuf2, blen(lb, SSZ - 1), sb, (mode >> 2) & 3);
    if mode & 16 != 0 {
        let l1 = strlen_c(&sbuf);
        let l2 = strlen_c(&sbuf2);
        if l2 != 0 && l1 >= l2 {
            let pos = blen(mode >> 8, l1 - l2 + 1);
            sbuf[pos..pos + l2].copy_from_slice(&sbuf2[..l2]);
        }
    }
    match strnstr_c(&sbuf, &sbuf2, blen(cnt, SSZ)) {
        Some(i) => i as i64,
        None => -1,
    }
}
