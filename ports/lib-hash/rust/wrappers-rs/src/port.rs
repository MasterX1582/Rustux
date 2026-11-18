//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.

#![forbid(unsafe_code)]

// ---- harness mirror --------------------------------------------------------

fn xs64(rng: &mut u64) -> u64 {
    let mut x = *rng;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *rng = x;
    x.wrapping_mul(2685821657736338717)
}

fn buf_len(len: i64) -> usize {
    len.unsigned_abs() as usize % 513
}

/// 520 = 512 data + 8 so `off <= 7` stays in bounds (C buf is buf[520]).
fn seeded(seed: u64, n: usize) -> [u8; 520] {
    let mut b = [0u8; 520];
    let mut r = seed | 1;
    for x in b[..n].iter_mut() {
        *x = xs64(&mut r) as u8;
    }
    b
}

fn seeded_words(seed: u64, n: usize) -> [u32; 128] {
    let mut w = [0u32; 128];
    let mut r = seed | 1;
    for x in w[..n].iter_mut() {
        *x = xs64(&mut r) as u32;
    }
    w
}

fn get_le64(d: &[u8]) -> u64 {
    u64::from_le_bytes(d[..8].try_into().unwrap())
}
fn get_le32(d: &[u8]) -> u32 {
    u32::from_le_bytes(d[..4].try_into().unwrap())
}
fn get_ne32(d: &[u8]) -> u32 {
    u32::from_ne_bytes(d[..4].try_into().unwrap())
}
fn pack32(a: u32, b: u32) -> u64 {
    (b as u64) << 32 | a as u64
}

// ---- siphash (lib/crypto/siphash.c) ----------------------------------------

macro_rules! sipround {
    ($v0:expr, $v1:expr, $v2:expr, $v3:expr) => {{
        $v0 = $v0.wrapping_add($v1);
        $v1 = $v1.rotate_left(13);
        $v1 ^= $v0;
        $v0 = $v0.rotate_left(32);
        $v2 = $v2.wrapping_add($v3);
        $v3 = $v3.rotate_left(16);
        $v3 ^= $v2;
        $v0 = $v0.wrapping_add($v3);
        $v3 = $v3.rotate_left(21);
        $v3 ^= $v0;
        $v2 = $v2.wrapping_add($v1);
        $v1 = $v1.rotate_left(17);
        $v1 ^= $v2;
        $v2 = $v2.rotate_left(32);
    }};
}

struct Sip {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    b: u64,
}

impl Sip {
    fn preamble(len: u64, k0: u64, k1: u64) -> Self {
        let mut s = Sip {
            v0: 0x736f6d6570736575,
            v1: 0x646f72616e646f6d,
            v2: 0x6c7967656e657261,
            v3: 0x7465646279746573,
            b: len << 56,
        };
        s.v3 ^= k1;
        s.v2 ^= k0;
        s.v1 ^= k1;
        s.v0 ^= k0;
        s
    }
    fn round(&mut self) {
        sipround!(self.v0, self.v1, self.v2, self.v3);
    }
    /// SipHash postamble: 2 XOR-rounds, 0xff marker, 4 final rounds.
    fn postamble(mut self) -> u64 {
        self.v3 ^= self.b;
        self.round();
        self.round();
        self.v0 ^= self.b;
        self.v2 ^= 0xff;
        for _ in 0..4 {
            self.round();
        }
        (self.v0 ^ self.v1) ^ (self.v2 ^ self.v3)
    }
    /// SipHash1-3 postamble (hsiphash on LP64): 1 round, marker, 3 rounds.
    fn hpostamble(mut self) -> u32 {
        self.v3 ^= self.b;
        self.round();
        self.v0 ^= self.b;
        self.v2 ^= 0xff;
        for _ in 0..3 {
            self.round();
        }
        ((self.v0 ^ self.v1) ^ (self.v2 ^ self.v3)) as u32
    }
}

/// Shared by the siphash_*u64/u32 scalars (rounds=2) and hsiphash_*u32
/// (rounds=1): `ms` are 8-byte message blocks, `extra_b` folds into `b`.
fn sip_run(len: u64, ms: &[u64], extra_b: u64, key: (u64, u64), rounds: u32) -> Sip {
    let mut s = Sip::preamble(len, key.0, key.1);
    for &m in ms {
        s.v3 ^= m;
        for _ in 0..rounds {
            s.round();
        }
        s.v0 ^= m;
    }
    s.b |= extra_b;
    s
}

/// The non-DCACHE_WORD_ACCESS switch tail: little-endian pack of the
/// 0..=7 leftover bytes — identical to the C fallthrough chain, which
/// never overlaps the len<<56 field (left<=7 covers bits 0..55).
fn sip_tail(tail: &[u8]) -> u64 {
    tail.iter()
        .enumerate()
        .fold(0u64, |v, (i, &b)| v | (b as u64) << (8 * i))
}

/// `__siphash_{aligned,unaligned}` and `__hsiphash_{aligned,unaligned}` —
/// the C variants differ only in the load primitive; byte-identical here.
fn sip_data(data: &[u8], key: (u64, u64), rounds: u32) -> Sip {
    let len = data.len();
    let (blocks, tail) = data.split_at(len - (len % 8));
    let mut s = Sip::preamble(len as u64, key.0, key.1);
    for m in blocks.as_chunks::<8>().0 {
        let m = u64::from_le_bytes(*m);
        s.v3 ^= m;
        for _ in 0..rounds {
            s.round();
        }
        s.v0 ^= m;
    }
    s.b |= sip_tail(tail);
    s
}

// ---- xxhash (lib/xxhash.c) -------------------------------------------------

const P32_1: u32 = 2654435761;
const P32_2: u32 = 2246822519;
const P32_3: u32 = 3266489917;
const P32_4: u32 = 668265263;
const P32_5: u32 = 374761393;

const P64_1: u64 = 11400714785074694791;
const P64_2: u64 = 14029467366897019727;
const P64_3: u64 = 1609587929392839161;
const P64_4: u64 = 9650029242287828579;
const P64_5: u64 = 2870177450012600261;

fn xxh32_round(seed: u32, input: u32) -> u32 {
    seed.wrapping_add(input.wrapping_mul(P32_2))
        .rotate_left(13)
        .wrapping_mul(P32_1)
}

fn xxh32(input: &[u8], seed: u32) -> u32 {
    let len = input.len();
    let mut p = 0usize;
    let mut h32 = if len >= 16 {
        let mut v = [
            seed.wrapping_add(P32_1).wrapping_add(P32_2),
            seed.wrapping_add(P32_2),
            seed,
            seed.wrapping_sub(P32_1),
        ];
        loop {
            for (i, x) in v.iter_mut().enumerate() {
                *x = xxh32_round(*x, get_le32(&input[p + 4 * i..]));
            }
            p += 16;
            if p > len - 16 {
                break;
            }
        }
        v[0].rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18))
    } else {
        seed.wrapping_add(P32_5)
    };

    h32 = h32.wrapping_add(len as u32);
    while p + 4 <= len {
        h32 = h32.wrapping_add(get_le32(&input[p..]).wrapping_mul(P32_3));
        h32 = h32.rotate_left(17).wrapping_mul(P32_4);
        p += 4;
    }
    while p < len {
        h32 = h32.wrapping_add((input[p] as u32).wrapping_mul(P32_5));
        h32 = h32.rotate_left(11).wrapping_mul(P32_1);
        p += 1;
    }
    h32 ^= h32 >> 15;
    h32 = h32.wrapping_mul(P32_2);
    h32 ^= h32 >> 13;
    h32 = h32.wrapping_mul(P32_3);
    h32 ^ (h32 >> 16)
}

fn xxh64_round(acc: u64, input: u64) -> u64 {
    acc.wrapping_add(input.wrapping_mul(P64_2))
        .rotate_left(31)
        .wrapping_mul(P64_1)
}

fn xxh64_merge_round(acc: u64, val: u64) -> u64 {
    (acc ^ xxh64_round(0, val))
        .wrapping_mul(P64_1)
        .wrapping_add(P64_4)
}

fn xxh64_merge_all(v: &[u64; 4]) -> u64 {
    let mut h = v[0]
        .rotate_left(1)
        .wrapping_add(v[1].rotate_left(7))
        .wrapping_add(v[2].rotate_left(12))
        .wrapping_add(v[3].rotate_left(18));
    for &x in v {
        h = xxh64_merge_round(h, x);
    }
    h
}

/// Shared avalanche + tail walker used by xxh64() and xxh64_digest().
fn xxh64_final(mut h64: u64, b: &[u8], total: u64) -> u64 {
    h64 = h64.wrapping_add(total);
    let mut i = 0usize;
    while i + 8 <= b.len() {
        h64 ^= xxh64_round(0, get_le64(&b[i..]));
        h64 = h64.rotate_left(27).wrapping_mul(P64_1).wrapping_add(P64_4);
        i += 8;
    }
    if i + 4 <= b.len() {
        h64 ^= (get_le32(&b[i..]) as u64).wrapping_mul(P64_1);
        h64 = h64.rotate_left(23).wrapping_mul(P64_2).wrapping_add(P64_3);
        i += 4;
    }
    while i < b.len() {
        h64 ^= (b[i] as u64).wrapping_mul(P64_5);
        h64 = h64.rotate_left(11).wrapping_mul(P64_1);
        i += 1;
    }
    h64 ^= h64 >> 33;
    h64 = h64.wrapping_mul(P64_2);
    h64 ^= h64 >> 29;
    h64 = h64.wrapping_mul(P64_3);
    h64 ^ (h64 >> 32)
}

fn xxh64_block(v: &mut [u64; 4], input: &[u8], mut p: usize) -> usize {
    loop {
        for (i, x) in v.iter_mut().enumerate() {
            *x = xxh64_round(*x, get_le64(&input[p + 8 * i..]));
        }
        p += 32;
        if p > input.len() - 32 {
            break;
        }
    }
    p
}

fn xxh64_seeded(seed: u64) -> [u64; 4] {
    [
        seed.wrapping_add(P64_1).wrapping_add(P64_2),
        seed.wrapping_add(P64_2),
        seed,
        seed.wrapping_sub(P64_1),
    ]
}

fn xxh64(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut v = xxh64_seeded(seed);
    let (h64, off) = if len >= 32 {
        let off = xxh64_block(&mut v, input, 0);
        (xxh64_merge_all(&v), off)
    } else {
        (seed.wrapping_add(P64_5), 0)
    };
    xxh64_final(h64, &input[off..], len as u64)
}

struct Xxh64State {
    total_len: u64,
    v: [u64; 4],
    mem64: [u8; 32],
    memsize: usize,
}

impl Xxh64State {
    fn reset(seed: u64) -> Self {
        Xxh64State {
            total_len: 0,
            v: xxh64_seeded(seed),
            mem64: [0; 32],
            memsize: 0,
        }
    }

    /// The C returns -EINVAL on NULL input (unreachable through the
    /// wrapper); slices are never null, so this always succeeds.
    fn update(&mut self, input: &[u8]) {
        let len = input.len();
        self.total_len = self.total_len.wrapping_add(len as u64);

        if self.memsize + len < 32 {
            self.mem64[self.memsize..self.memsize + len].copy_from_slice(input);
            self.memsize += len;
            return;
        }

        let mut p = 0usize;
        if self.memsize != 0 {
            let gap = 32 - self.memsize;
            self.mem64[self.memsize..32].copy_from_slice(&input[..gap]);
            for i in 0..4 {
                self.v[i] = xxh64_round(self.v[i], get_le64(&self.mem64[8 * i..]));
            }
            p += gap;
            self.memsize = 0;
        }

        if p + 32 <= len {
            p = xxh64_block(&mut self.v, input, p);
        }
        if p < len {
            self.mem64[..len - p].copy_from_slice(&input[p..]);
            self.memsize = len - p;
        }
    }

    fn digest(&self) -> u64 {
        let h64 = if self.total_len >= 32 {
            xxh64_merge_all(&self.v)
        } else {
            self.v[2].wrapping_add(P64_5)
        };
        xxh64_final(h64, &self.mem64[..self.memsize], self.total_len)
    }
}

// ---- jhash (include/linux/jhash.h) -----------------------------------------

const JHASH_INITVAL: u32 = 0xdeadbeef;

macro_rules! jhash_mix {
    ($a:expr, $b:expr, $c:expr) => {{
        $a = $a.wrapping_sub($c);
        $a ^= $c.rotate_left(4);
        $c = $c.wrapping_add($b);
        $b = $b.wrapping_sub($a);
        $b ^= $a.rotate_left(6);
        $a = $a.wrapping_add($c);
        $c = $c.wrapping_sub($b);
        $c ^= $b.rotate_left(8);
        $b = $b.wrapping_add($a);
        $a = $a.wrapping_sub($c);
        $a ^= $c.rotate_left(16);
        $c = $c.wrapping_add($b);
        $b = $b.wrapping_sub($a);
        $b ^= $a.rotate_left(19);
        $a = $a.wrapping_add($c);
        $c = $c.wrapping_sub($b);
        $c ^= $b.rotate_left(4);
        $b = $b.wrapping_add($a);
    }};
}

macro_rules! jhash_final {
    ($a:expr, $b:expr, $c:expr) => {{
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(14));
        $a ^= $c;
        $a = $a.wrapping_sub($c.rotate_left(11));
        $b ^= $a;
        $b = $b.wrapping_sub($a.rotate_left(25));
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(16));
        $a ^= $c;
        $a = $a.wrapping_sub($c.rotate_left(4));
        $b ^= $a;
        $b = $b.wrapping_sub($a.rotate_left(14));
        $c ^= $b;
        $c = $c.wrapping_sub($b.rotate_left(24));
    }};
}

fn jhash(key: &[u8], initval: u32) -> u32 {
    let mut length = key.len();
    let mut a = JHASH_INITVAL
        .wrapping_add(length as u32)
        .wrapping_add(initval);
    let (mut b, mut c) = (a, a);
    let mut k = 0usize;

    while length > 12 {
        a = a.wrapping_add(get_ne32(&key[k..]));
        b = b.wrapping_add(get_ne32(&key[k + 4..]));
        c = c.wrapping_add(get_ne32(&key[k + 8..]));
        jhash_mix!(a, b, c);
        length -= 12;
        k += 12;
    }
    // Fallthrough tail: k[8..len)->c, k[4..8)->b, k[0..4)->a, LE-packed.
    let kb = &key[k..];
    if length > 0 {
        for (i, &x) in kb.iter().enumerate() {
            let v = (x as u32) << (8 * (i & 3));
            match i / 4 {
                0 => a = a.wrapping_add(v),
                1 => b = b.wrapping_add(v),
                _ => c = c.wrapping_add(v),
            }
        }
        jhash_final!(a, b, c);
    }
    c
}

fn jhash2(k: &[u32], initval: u32) -> u32 {
    let mut length = k.len();
    let mut a = JHASH_INITVAL
        .wrapping_add((length as u32) << 2)
        .wrapping_add(initval);
    let (mut b, mut c) = (a, a);
    let mut i = 0usize;

    while length > 3 {
        a = a.wrapping_add(k[i]);
        b = b.wrapping_add(k[i + 1]);
        c = c.wrapping_add(k[i + 2]);
        jhash_mix!(a, b, c);
        length -= 3;
        i += 3;
    }
    if length > 0 {
        if length >= 3 {
            c = c.wrapping_add(k[i + 2]);
        }
        if length >= 2 {
            b = b.wrapping_add(k[i + 1]);
        }
        a = a.wrapping_add(k[i]);
        jhash_final!(a, b, c);
    }
    c
}

fn jhash_nwords(a: u32, b: u32, c: u32, initval: u32) -> u32 {
    let mut a = a.wrapping_add(initval);
    let mut b = b.wrapping_add(initval);
    let mut c = c.wrapping_add(initval);
    jhash_final!(a, b, c);
    c
}

// ---- exported wrapper mirrors ----------------------------------------------

/// `wrap_siphash` (`wrappers.c:55`)
pub fn wrap_siphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    let off = (off & 7) as usize;
    let buf = seeded(seed, n + off);
    sip_data(&buf[off..off + n], (k0, k1), 2).postamble()
}

/// `wrap_siphash_1u64` (`wrappers.c:67`)
pub fn wrap_siphash_1u64(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[a], 0, (k0, k1), 2).postamble()
}

/// `wrap_siphash_2u64` (`wrappers.c:73`)
pub fn wrap_siphash_2u64(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(16, &[a, b], 0, (k0, k1), 2).postamble()
}

/// `wrap_siphash_3u64` (`wrappers.c:79`)
pub fn wrap_siphash_3u64(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    sip_run(24, &[a, b, c], 0, (k0, k1), 2).postamble()
}

/// `wrap_siphash_4u64` (`wrappers.c:86`)
pub fn wrap_siphash_4u64(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    sip_run(32, &[a, b, c, d], 0, (k0, k1), 2).postamble()
}

/// `wrap_siphash_1u32` (`wrappers.c:93`)
pub fn wrap_siphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(4, &[], a as u32 as u64, (k0, k1), 2).postamble()
}

/// `wrap_siphash_2u32` (`wrappers.c:99`)
pub fn wrap_siphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[pack32(a as u32, b as u32)], 0, (k0, k1), 2).postamble()
}

/// `wrap_siphash_3u32` (`wrappers.c:105`)
pub fn wrap_siphash_3u32(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    sip_run(
        12,
        &[pack32(a as u32, b as u32)],
        c as u32 as u64,
        (k0, k1),
        2,
    )
    .postamble()
}

/// `wrap_siphash_4u32` (`wrappers.c:112`)
pub fn wrap_siphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    let ms = [pack32(a as u32, b as u32), pack32(c as u32, d as u32)];
    sip_run(16, &ms, 0, (k0, k1), 2).postamble()
}

/// `wrap_hsiphash` (`wrappers.c:121`)
pub fn wrap_hsiphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    let off = (off & 7) as usize;
    let buf = seeded(seed, n + off);
    sip_data(&buf[off..off + n], (k0, k1), 1).hpostamble() as u64
}

/// `wrap_hsiphash_1u32` (`wrappers.c:130`)
pub fn wrap_hsiphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(4, &[], a as u32 as u64, (k0, k1), 1).hpostamble() as u64
}

/// `wrap_hsiphash_2u32` (`wrappers.c:136`)
pub fn wrap_hsiphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[pack32(a as u32, b as u32)], 0, (k0, k1), 1).hpostamble() as u64
}

/// `wrap_hsiphash_3u32` (`wrappers.c:142`)
pub fn wrap_hsiphash_3u32(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    sip_run(
        12,
        &[pack32(a as u32, b as u32)],
        c as u32 as u64,
        (k0, k1),
        1,
    )
    .hpostamble() as u64
}

/// `wrap_hsiphash_4u32` (`wrappers.c:149`)
pub fn wrap_hsiphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    let ms = [pack32(a as u32, b as u32), pack32(c as u32, d as u32)];
    sip_run(16, &ms, 0, (k0, k1), 1).hpostamble() as u64
}

/// `wrap_xxh32` (`wrappers.c:158`)
pub fn wrap_xxh32(seed: u64, bseed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    xxh32(&seeded(bseed, n)[..n], seed as u32) as u64
}

/// `wrap_xxh64` (`wrappers.c:165`)
pub fn wrap_xxh64(seed: u64, bseed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    xxh64(&seeded(bseed, n)[..n], seed)
}

/// `wrap_xxh64_stream` (`wrappers.c:174`)
pub fn wrap_xxh64_stream(seed: u64, bseed: u64, len: i64, chunk: i64) -> u64 {
    let n = buf_len(len);
    let c = chunk.unsigned_abs() % 64 + 1;
    let buf = seeded(bseed, n);
    let mut st = Xxh64State::reset(seed);
    let mut i = 0usize;
    while i < n {
        let step = (n - i).min(c as usize);
        st.update(&buf[i..i + step]);
        i += step;
    }
    st.digest()
}

/// `wrap_jhash` (`wrappers.c:196`)
pub fn wrap_jhash(initval: u64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    jhash(&seeded(seed, n)[..n], initval as u32) as u64
}

/// `wrap_jhash2` (`wrappers.c:203`)
pub fn wrap_jhash2(initval: u64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len) % 129;
    let w = seeded_words(seed, n);
    jhash2(&w[..n], initval as u32) as u64
}

/// `wrap_jhash_1word` (`wrappers.c:210`)
pub fn wrap_jhash_1word(a: u64, initval: u64) -> u64 {
    let iv = (initval as u32)
        .wrapping_add(JHASH_INITVAL)
        .wrapping_add(1 << 2);
    jhash_nwords(a as u32, 0, 0, iv) as u64
}

/// `wrap_jhash_2words` (`wrappers.c:215`)
pub fn wrap_jhash_2words(a: u64, b: u64, initval: u64) -> u64 {
    let iv = (initval as u32)
        .wrapping_add(JHASH_INITVAL)
        .wrapping_add(2 << 2);
    jhash_nwords(a as u32, b as u32, 0, iv) as u64
}

/// `wrap_jhash_3words` (`wrappers.c:220`)
pub fn wrap_jhash_3words(a: u64, b: u64, c: u64, initval: u64) -> u64 {
    let iv = (initval as u32)
        .wrapping_add(JHASH_INITVAL)
        .wrapping_add(3 << 2);
    jhash_nwords(a as u32, b as u32, c as u32, iv) as u64
}
