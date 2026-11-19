//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Source-faithful model of `lib/crypto/siphash.c` (SipHash-2-4 plus the
//! LP64 `hsiphash`, which is SipHash-1-3 keyed on `unsigned long[2]`).
//! The C `siphash()`/`hsiphash()` entry points dispatch on pointer
//! alignment between `__*_{aligned,unaligned}`; both compute identical
//! results on the same bytes (only the load primitive differs), so the
//! port keeps a single byte-level implementation, like the other verified
//! ports. The `CONFIG_DCACHE_WORD_ACCESS` tail is compiled out in the
//! recorded x86-64 harness; the port mirrors the generic LE-pack tail.

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

fn ramp_len(len: i64) -> usize {
    len.unsigned_abs() as usize % 65
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

fn ramp(n: usize) -> [u8; 520] {
    let mut b = [0u8; 520];
    for (i, x) in b[..n].iter_mut().enumerate() {
        *x = i as u8;
    }
    b
}

fn pack32(a: u32, b: u32) -> u64 {
    (b as u64) << 32 | a as u64
}

// ---- siphash (lib/crypto/siphash.c) ----------------------------------------

/// `SIPHASH_PERMUTATION(v0, v1, v2, v3)` — one SipRound.
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

/// The PREAMBLE state: v0..v3 keyed, b = len << 56.
struct Sip {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    b: u64,
}

impl Sip {
    /// `PREAMBLE(len)` — SIPHASH_CONST_* XORed with key->key[0..1].
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
    /// `POSTAMBLE` — SipHash-2-4: two rounds on b, 0xff marker, four rounds.
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
    /// `HPOSTAMBLE` — SipHash-1-3 (hsiphash on LP64): one round, marker,
    /// three rounds; the C function returns u32.
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

/// Shared by the siphash_*u64/u32 scalars (rounds=2) and the hsiphash_*u32
/// scalars (rounds=1): `ms` are the 8-byte message blocks each absorbed
/// with `v3 ^= m; rounds*SIPROUND; v0 ^= m`, `extra_b` folds into `b`.
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

fn hsiphash(data: &[u8], key: (u64, u64)) -> u32 {
    sip_data(data, key, 1).hpostamble()
}

// ---- exported wrappers -----------------------------------------------------

pub fn wrap_siphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    let off = (off as u64 & 7) as usize;
    let buf = seeded(seed, n + off);
    sip_data(&buf[off..off + n], (k0, k1), 2).postamble()
}

pub fn wrap_siphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    let n = ramp_len(len);
    let off = (off as u64 & 7) as usize;
    let buf = ramp(n + off);
    sip_data(&buf[off..off + n], (k0, k1), 2).postamble()
}

pub fn wrap_siphash_1u64(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[a], 0, (k0, k1), 2).postamble()
}

pub fn wrap_siphash_2u64(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(16, &[a, b], 0, (k0, k1), 2).postamble()
}

pub fn wrap_siphash_3u64(a: u64, b: u64, c: u64, k0: u64, k1: u64) -> u64 {
    sip_run(24, &[a, b, c], 0, (k0, k1), 2).postamble()
}

pub fn wrap_siphash_4u64(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    sip_run(32, &[a, b, c, d], 0, (k0, k1), 2).postamble()
}

pub fn wrap_siphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(4, &[], a as u32 as u64, (k0, k1), 2).postamble()
}

pub fn wrap_siphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[pack32(a as u32, b as u32)], 0, (k0, k1), 2).postamble()
}

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

pub fn wrap_siphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    sip_run(
        16,
        &[pack32(a as u32, b as u32), pack32(c as u32, d as u32)],
        0,
        (k0, k1),
        2,
    )
    .postamble()
}

pub fn wrap_hsiphash(k0: u64, k1: u64, off: i64, seed: u64, len: i64) -> u64 {
    let n = buf_len(len);
    let off = (off as u64 & 7) as usize;
    let buf = seeded(seed, n + off);
    hsiphash(&buf[off..off + n], (k0, k1)) as u64
}

pub fn wrap_hsiphash_ramp(k0: u64, k1: u64, off: i64, len: i64) -> u64 {
    let n = ramp_len(len);
    let off = (off as u64 & 7) as usize;
    let buf = ramp(n + off);
    hsiphash(&buf[off..off + n], (k0, k1)) as u64
}

pub fn wrap_hsiphash_1u32(a: u64, k0: u64, k1: u64) -> u64 {
    sip_run(4, &[], a as u32 as u64, (k0, k1), 1).hpostamble() as u64
}

pub fn wrap_hsiphash_2u32(a: u64, b: u64, k0: u64, k1: u64) -> u64 {
    sip_run(8, &[pack32(a as u32, b as u32)], 0, (k0, k1), 1).hpostamble() as u64
}

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

pub fn wrap_hsiphash_4u32(a: u64, b: u64, c: u64, d: u64, k0: u64, k1: u64) -> u64 {
    sip_run(
        16,
        &[pack32(a as u32, b as u32), pack32(c as u32, d as u32)],
        0,
        (k0, k1),
        1,
    )
    .hpostamble() as u64
}
