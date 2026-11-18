//! Verified Rust port of the SHA family from Linux `lib/crypto/`:
//! `sha1.c`, `sha256.c`, `sha512.c`.
//!
//! Covers the full freestanding surface of `include/crypto/sha1.h` +
//! `include/crypto/sha2.h`: block transforms, init/update/final,
//! one-shot digests, `sha256_finup_2x` (generic sequential path),
//! the complete HMAC family (prepared-key, raw-key, streaming,
//! one-shot), and the `*_zero_message_hash` published constants.
//!
//! Behavior-verified against the byte-identical C units over 6174
//! recorded cases (ports/lib-sha) — one-shot, chunked-streaming,
//! block/padding-boundary lengths, HMAC keys up to >blocksize,
//! prepared vs raw-key agreement, and empty-digest equality.
//!
//! API notes:
//! - Functions keep C's `(slice, len)` dual parameters; `len` must be
//!   `<= data.len()` (panics otherwise), matching C's pointer+len.
//! - Digest `out` params are `&mut [u8]` of exactly `*_DIGEST_SIZE`.
//! - `*Ctx::default()`/`Default` yields a ZEROED context, not an
//!   IV-initialized one — always call `sha*_init` before update/final,
//!   exactly as a C caller must call `*_init` on a memset ctx.
//! - `*_blocks`/`sha*_block_generic` are the raw transforms (kernel
//!   drivers/shash use them); `sha256_finup_2x_arch` always returns
//!   false — the arch hook is a no-op on this generic build, matching
//!   the recorded generic path.

#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_rotate)]

pub const SHA1_ZERO_MESSAGE_HASH: [u8; SHA1_DIGEST_SIZE] = [0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d, 0x32, 0x55, 0xbf, 0xef, 0x95, 0x60, 0x18, 0x90, 0xaf, 0xd8, 0x07, 0x09,];

pub const SHA224_ZERO_MESSAGE_HASH: [u8; SHA224_DIGEST_SIZE] = [0xd1, 0x4a, 0x02, 0x8c, 0x2a, 0x3a, 0x2b, 0xc9, 0x47, 0x61, 0x02, 0xbb, 0x28, 0x82, 0x34, 0xc4, 0x15, 0xa2, 0xb0, 0x1f, 0x82, 0x8e, 0xa6, 0x2a, 0xc5, 0xb3, 0xe4, 0x2f,];

pub const SHA256_ZERO_MESSAGE_HASH: [u8; SHA256_DIGEST_SIZE] = [0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,];

pub const SHA384_ZERO_MESSAGE_HASH: [u8; SHA384_DIGEST_SIZE] = [0x38, 0xb0, 0x60, 0xa7, 0x51, 0xac, 0x96, 0x38, 0x4c, 0xd9, 0x32, 0x7e, 0xb1, 0xb1, 0xe3, 0x6a, 0x21, 0xfd, 0xb7, 0x11, 0x14, 0xbe, 0x07, 0x43, 0x4c, 0x0c, 0xc7, 0xbf, 0x63, 0xf6, 0xe1, 0xda, 0x27, 0x4e, 0xde, 0xbf, 0xe7, 0x6f, 0x65, 0xfb, 0xd5, 0x1a, 0xd2, 0xf1, 0x48, 0x98, 0xb9, 0x5b,];

pub const SHA512_ZERO_MESSAGE_HASH: [u8; SHA512_DIGEST_SIZE] = [0xcf, 0x83, 0xe1, 0x35, 0x7e, 0xef, 0xb8, 0xbd, 0xf1, 0x54, 0x28, 0x50, 0xd6, 0x6d, 0x80, 0x07, 0xd6, 0x20, 0xe4, 0x05, 0x0b, 0x57, 0x15, 0xdc, 0x83, 0xf4, 0xa9, 0x21, 0xd3, 0x6c, 0xe9, 0xce, 0x47, 0xd0, 0xd1, 0x3c, 0x5d, 0x85, 0xf2, 0xb0, 0xff, 0x83, 0x18, 0xd2, 0x87, 0x7e, 0xec, 0x2f, 0x63, 0xb9, 0x31, 0xbd, 0x47, 0x41, 0x7a, 0x81, 0xa5, 0x38, 0x32, 0x7a, 0xf9, 0x27, 0xda, 0x3e,];

// ---- kernel glue (linux/*.h shims) -----------------------------------------

fn rol32(word: u32, shift: u32) -> u32 {
    (word << (shift & 31)) | (word >> (shift.wrapping_neg() & 31))
}

fn ror32(word: u32, shift: u32) -> u32 {
    (word >> (shift & 31)) | (word << (shift.wrapping_neg() & 31))
}

fn ror64(word: u64, shift: u32) -> u64 {
    (word >> (shift & 63)) | (word << (shift.wrapping_neg() & 63))
}

fn get_unaligned_be32(d: &[u8]) -> u32 {
    u32::from_be_bytes(d[..4].try_into().unwrap())
}

fn get_unaligned_be64(d: &[u8]) -> u64 {
    u64::from_be_bytes(d[..8].try_into().unwrap())
}

fn put_unaligned_be32(v: u32, out: &mut [u8]) {
    out[..4].copy_from_slice(&v.to_be_bytes());
}

fn put_unaligned_be64(v: u64, out: &mut [u8]) {
    out[..8].copy_from_slice(&v.to_be_bytes());
}

/* memset + barrier; on the Rust side a plain fill is unelidable across
 * the FFI boundary anyway, and zeroization is what is observable. */
fn memzero_explicit(s: &mut [u8]) {
    s.fill(0);
}

pub const HMAC_IPAD_VALUE: u8 = 0x36;
pub const HMAC_OPAD_VALUE: u8 = 0x5c;

/* The C code unions derived_key.b[] with unsigned long w[] and xors each
 * word with REPEAT_BYTE(x) == 0x01...01 * x; on LP64-LE that is exactly a
 * byte-wise ^= x over the whole block, which is what the ports do. */

// ---- SHA-1 (lib/crypto/sha1.c) ----------------------------------------------

pub const SHA1_DIGEST_SIZE: usize = 20;
pub const SHA1_BLOCK_SIZE: usize = 64;

pub const SHA1_H0: u32 = 0x67452301;
pub const SHA1_H1: u32 = 0xefcdab89;
pub const SHA1_H2: u32 = 0x98badcfe;
pub const SHA1_H3: u32 = 0x10325476;
pub const SHA1_H4: u32 = 0xc3d2e1f0;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha1BlockState {
    pub h: [u32; 5],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sha1Ctx {
    pub state: Sha1BlockState,
    pub bytecount: u64,
    pub buf: [u8; SHA1_BLOCK_SIZE],
}

impl Default for Sha1Ctx {
    fn default() -> Self {
        Sha1Ctx {
            state: Sha1BlockState::default(),
            bytecount: 0,
            buf: [0; SHA1_BLOCK_SIZE],
        }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha1Key {
    pub istate: Sha1BlockState,
    pub ostate: Sha1BlockState,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha1Ctx {
    pub sha_ctx: Sha1Ctx,
    pub ostate: Sha1BlockState,
}

const SHA1_IV: Sha1BlockState = Sha1BlockState {
    h: [SHA1_H0, SHA1_H1, SHA1_H2, SHA1_H3, SHA1_H4],
};

pub const SHA1_WORKSPACE_WORDS: usize = 16;

/* SHA_SRC(t) / SHA_MIX(t) / SHA_ROUND(t, ...) / T_*(), written per
 * range exactly like the C loops.  W(x) is workspace[x & 15]. */
pub fn sha1_block_generic(state: &mut Sha1BlockState, data: &[u8], workspace: &mut [u32; 16]) {
    let [mut a, mut b, mut c, mut d, mut e] = state.h;

    let mut i = 0usize;
    /* Round 1 - iterations 0-15 take their input from 'data' */
    while i < 16 {
        let temp = get_unaligned_be32(&data[i * 4..]);
        workspace[i & 15] = temp;
        e = e
            .wrapping_add(temp)
            .wrapping_add(rol32(a, 5))
            .wrapping_add((c ^ d) & b ^ d)
            .wrapping_add(0x5a827999);
        b = ror32(b, 2);
        let t = e;
        e = d;
        d = c;
        c = b;
        b = a;
        a = t;
        i += 1;
    }
    /* Round 1 - tail. Input from 512-bit mixing array */
    while i < 20 {
        let temp = rol32(
            workspace[(i + 13) & 15]
                ^ workspace[(i + 8) & 15]
                ^ workspace[(i + 2) & 15]
                ^ workspace[i & 15],
            1,
        );
        workspace[i & 15] = temp;
        e = e
            .wrapping_add(temp)
            .wrapping_add(rol32(a, 5))
            .wrapping_add((c ^ d) & b ^ d)
            .wrapping_add(0x5a827999);
        b = ror32(b, 2);
        let t = e;
        e = d;
        d = c;
        c = b;
        b = a;
        a = t;
        i += 1;
    }
    /* Round 2 */
    while i < 40 {
        let temp = rol32(
            workspace[(i + 13) & 15]
                ^ workspace[(i + 8) & 15]
                ^ workspace[(i + 2) & 15]
                ^ workspace[i & 15],
            1,
        );
        workspace[i & 15] = temp;
        e = e
            .wrapping_add(temp)
            .wrapping_add(rol32(a, 5))
            .wrapping_add(b ^ c ^ d)
            .wrapping_add(0x6ed9eba1);
        b = ror32(b, 2);
        let t = e;
        e = d;
        d = c;
        c = b;
        b = a;
        a = t;
        i += 1;
    }
    /* Round 3 */
    while i < 60 {
        let temp = rol32(
            workspace[(i + 13) & 15]
                ^ workspace[(i + 8) & 15]
                ^ workspace[(i + 2) & 15]
                ^ workspace[i & 15],
            1,
        );
        workspace[i & 15] = temp;
        e = e
            .wrapping_add(temp)
            .wrapping_add(rol32(a, 5))
            .wrapping_add((b & c).wrapping_add(d & (b ^ c)))
            .wrapping_add(0x8f1bbcdc);
        b = ror32(b, 2);
        let t = e;
        e = d;
        d = c;
        c = b;
        b = a;
        a = t;
        i += 1;
    }
    /* Round 4 */
    while i < 80 {
        let temp = rol32(
            workspace[(i + 13) & 15]
                ^ workspace[(i + 8) & 15]
                ^ workspace[(i + 2) & 15]
                ^ workspace[i & 15],
            1,
        );
        workspace[i & 15] = temp;
        e = e
            .wrapping_add(temp)
            .wrapping_add(rol32(a, 5))
            .wrapping_add(b ^ c ^ d)
            .wrapping_add(0xca62c1d6);
        b = ror32(b, 2);
        let t = e;
        e = d;
        d = c;
        c = b;
        b = a;
        a = t;
        i += 1;
    }

    state.h[0] = state.h[0].wrapping_add(a);
    state.h[1] = state.h[1].wrapping_add(b);
    state.h[2] = state.h[2].wrapping_add(c);
    state.h[3] = state.h[3].wrapping_add(d);
    state.h[4] = state.h[4].wrapping_add(e);
}

/* sha1_blocks: CONFIG_CRYPTO_LIB_SHA1_ARCH unset -> generic */
pub fn sha1_blocks(state: &mut Sha1BlockState, mut data: &[u8], mut nblocks: usize) {
    let mut workspace = [0u32; SHA1_WORKSPACE_WORDS];
    loop {
        sha1_block_generic(state, data, &mut workspace);
        data = &data[SHA1_BLOCK_SIZE..];
        nblocks -= 1;
        if nblocks == 0 {
            break;
        }
    }
    workspace.fill(0); /* memzero_explicit */
}

pub fn sha1_init(ctx: &mut Sha1Ctx) {
    ctx.state = SHA1_IV;
    ctx.bytecount = 0;
}

pub fn sha1_update(ctx: &mut Sha1Ctx, data_in: &[u8], len_in: usize) {
    let mut data = &data_in[..len_in];
    let mut len = len_in;
    let mut partial = (ctx.bytecount % SHA1_BLOCK_SIZE as u64) as usize;

    ctx.bytecount = ctx.bytecount.wrapping_add(len as u64);

    if partial + len >= SHA1_BLOCK_SIZE {
        if partial != 0 {
            let l = SHA1_BLOCK_SIZE - partial;
            ctx.buf[partial..partial + l].copy_from_slice(&data[..l]);
            data = &data[l..];
            len -= l;
            sha1_blocks(&mut ctx.state, &ctx.buf, 1);
        }
        let nblocks = len / SHA1_BLOCK_SIZE;
        len %= SHA1_BLOCK_SIZE;
        if nblocks != 0 {
            sha1_blocks(&mut ctx.state, &data[..nblocks * SHA1_BLOCK_SIZE], nblocks);
            data = &data[nblocks * SHA1_BLOCK_SIZE..];
        }
        partial = 0;
    }
    if len != 0 {
        ctx.buf[partial..partial + len].copy_from_slice(&data[..len]);
    }
}

pub fn __sha1_final(ctx: &mut Sha1Ctx, out: &mut [u8]) {
    let bitcount = ctx.bytecount.wrapping_shl(3);
    let mut partial = (ctx.bytecount % SHA1_BLOCK_SIZE as u64) as usize;

    ctx.buf[partial] = 0x80;
    partial += 1;
    if partial > SHA1_BLOCK_SIZE - 8 {
        ctx.buf[partial..SHA1_BLOCK_SIZE].fill(0);
        sha1_blocks(&mut ctx.state, &ctx.buf, 1);
        partial = 0;
    }
    ctx.buf[partial..SHA1_BLOCK_SIZE - 8].fill(0);
    ctx.buf[SHA1_BLOCK_SIZE - 8..SHA1_BLOCK_SIZE].copy_from_slice(&bitcount.to_be_bytes());
    sha1_blocks(&mut ctx.state, &ctx.buf, 1);

    let mut i = 0usize;
    while i < SHA1_DIGEST_SIZE {
        put_unaligned_be32(ctx.state.h[i / 4], &mut out[i..]);
        i += 4;
    }
}

pub fn sha1_final(ctx: &mut Sha1Ctx, out: &mut [u8]) {
    __sha1_final(ctx, out);
    *ctx = Sha1Ctx::default(); /* memzero_explicit */
}

pub fn sha1(data: &[u8], len: usize, out: &mut [u8]) {
    let mut ctx = Sha1Ctx::default();
    sha1_init(&mut ctx);
    sha1_update(&mut ctx, data, len);
    sha1_final(&mut ctx, out);
}

fn __hmac_sha1_preparekey(
    istate: &mut Sha1BlockState,
    ostate: &mut Sha1BlockState,
    raw_key: &[u8],
    raw_key_len: usize,
) {
    let mut derived_key = [0u8; SHA1_BLOCK_SIZE];

    if raw_key_len > SHA1_BLOCK_SIZE {
        sha1(raw_key, raw_key_len, &mut derived_key[..SHA1_DIGEST_SIZE]);
    } else {
        derived_key[..raw_key_len].copy_from_slice(&raw_key[..raw_key_len]);
    }

    /* derived_key.w[i] ^= REPEAT_BYTE(HMAC_IPAD_VALUE) per u64 lane ==
     * byte-wise ^= HMAC_IPAD_VALUE on LP64-LE. */
    for b in derived_key.iter_mut() {
        *b ^= HMAC_IPAD_VALUE;
    }
    *istate = SHA1_IV;
    sha1_blocks(istate, &derived_key, 1);

    for b in derived_key.iter_mut() {
        *b ^= HMAC_OPAD_VALUE ^ HMAC_IPAD_VALUE;
    }
    *ostate = SHA1_IV;
    sha1_blocks(ostate, &derived_key, 1);

    memzero_explicit(&mut derived_key);
}

pub fn hmac_sha1_preparekey(key: &mut HmacSha1Key, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha1_preparekey(&mut key.istate, &mut key.ostate, raw_key, raw_key_len);
}

pub fn hmac_sha1_init(ctx: &mut HmacSha1Ctx, key: &HmacSha1Key) {
    ctx.sha_ctx.state = key.istate;
    ctx.sha_ctx.bytecount = SHA1_BLOCK_SIZE as u64;
    ctx.ostate = key.ostate;
}

pub fn hmac_sha1_init_usingrawkey(ctx: &mut HmacSha1Ctx, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha1_preparekey(
        &mut ctx.sha_ctx.state,
        &mut ctx.ostate,
        raw_key,
        raw_key_len,
    );
    ctx.sha_ctx.bytecount = SHA1_BLOCK_SIZE as u64;
}

/* header inline: hmac_sha1_update -> sha1_update */
pub fn hmac_sha1_update(ctx: &mut HmacSha1Ctx, data: &[u8], data_len: usize) {
    sha1_update(&mut ctx.sha_ctx, data, data_len);
}

/* header inline: hmac_sha1_zeroize_ctx -> memzero_explicit(ctx) */
pub fn hmac_sha1_zeroize_ctx(ctx: &mut HmacSha1Ctx) {
    *ctx = HmacSha1Ctx::default();
}

pub fn hmac_sha1_final(ctx: &mut HmacSha1Ctx, out: &mut [u8]) {
    /* Generate the padded input for the outer hash in ctx->sha_ctx.buf. */
    let mut tmp = ctx.sha_ctx.buf;
    __sha1_final(&mut ctx.sha_ctx, &mut tmp);
    ctx.sha_ctx.buf = tmp;
    ctx.sha_ctx.buf[SHA1_DIGEST_SIZE..SHA1_BLOCK_SIZE].fill(0);
    ctx.sha_ctx.buf[SHA1_DIGEST_SIZE] = 0x80;
    ctx.sha_ctx.buf[SHA1_BLOCK_SIZE - 4..SHA1_BLOCK_SIZE]
        .copy_from_slice(&(8 * (SHA1_BLOCK_SIZE + SHA1_DIGEST_SIZE) as u32).to_be_bytes());

    /* Compute the outer hash, which gives the HMAC value. */
    sha1_blocks(&mut ctx.ostate, &ctx.sha_ctx.buf, 1);
    let mut i = 0usize;
    while i < SHA1_DIGEST_SIZE {
        put_unaligned_be32(ctx.ostate.h[i / 4], &mut out[i..]);
        i += 4;
    }

    hmac_sha1_zeroize_ctx(ctx);
}

pub fn hmac_sha1(key: &HmacSha1Key, data: &[u8], data_len: usize, out: &mut [u8]) {
    let mut ctx = HmacSha1Ctx::default();
    hmac_sha1_init(&mut ctx, key);
    hmac_sha1_update(&mut ctx, data, data_len);
    hmac_sha1_final(&mut ctx, out);
}

pub fn hmac_sha1_usingrawkey(
    raw_key: &[u8],
    raw_key_len: usize,
    data: &[u8],
    data_len: usize,
    out: &mut [u8],
) {
    let mut ctx = HmacSha1Ctx::default();
    hmac_sha1_init_usingrawkey(&mut ctx, raw_key, raw_key_len);
    hmac_sha1_update(&mut ctx, data, data_len);
    hmac_sha1_final(&mut ctx, out);
}

// ---- SHA-224/SHA-256 (lib/crypto/sha256.c) -----------------------------------

pub const SHA224_DIGEST_SIZE: usize = 28;
pub const SHA224_BLOCK_SIZE: usize = 64;
pub const SHA256_DIGEST_SIZE: usize = 32;
pub const SHA256_BLOCK_SIZE: usize = 64;
pub const SHA256_STATE_WORDS: usize = 8;

pub const SHA224_H0: u32 = 0xc1059ed8;
pub const SHA224_H1: u32 = 0x367cd507;
pub const SHA224_H2: u32 = 0x3070dd17;
pub const SHA224_H3: u32 = 0xf70e5939;
pub const SHA224_H4: u32 = 0xffc00b31;
pub const SHA224_H5: u32 = 0x68581511;
pub const SHA224_H6: u32 = 0x64f98fa7;
pub const SHA224_H7: u32 = 0xbefa4fa4;

pub const SHA256_H0: u32 = 0x6a09e667;
pub const SHA256_H1: u32 = 0xbb67ae85;
pub const SHA256_H2: u32 = 0x3c6ef372;
pub const SHA256_H3: u32 = 0xa54ff53a;
pub const SHA256_H4: u32 = 0x510e527f;
pub const SHA256_H5: u32 = 0x9b05688c;
pub const SHA256_H6: u32 = 0x1f83d9ab;
pub const SHA256_H7: u32 = 0x5be0cd19;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha256BlockState {
    pub h: [u32; 8],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __Sha256Ctx {
    pub state: Sha256BlockState,
    pub bytecount: u64,
    pub buf: [u8; SHA256_BLOCK_SIZE],
}

impl Default for __Sha256Ctx {
    fn default() -> Self {
        __Sha256Ctx {
            state: Sha256BlockState::default(),
            bytecount: 0,
            buf: [0; SHA256_BLOCK_SIZE],
        }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha224Ctx {
    pub ctx: __Sha256Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha256Ctx {
    pub ctx: __Sha256Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct __HmacSha256Key {
    pub istate: Sha256BlockState,
    pub ostate: Sha256BlockState,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct __HmacSha256Ctx {
    pub sha_ctx: __Sha256Ctx,
    pub ostate: Sha256BlockState,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha224Key {
    pub key: __HmacSha256Key,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha224Ctx {
    pub ctx: __HmacSha256Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha256Key {
    pub key: __HmacSha256Key,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha256Ctx {
    pub ctx: __HmacSha256Ctx,
}

/* static, not const: the C code compares `iv == &sha224_iv` by address,
 * so the IVs need stable addresses for core::ptr::eq to mirror it. */
static SHA224_IV: Sha256BlockState = Sha256BlockState {
    h: [
        SHA224_H0, SHA224_H1, SHA224_H2, SHA224_H3, SHA224_H4, SHA224_H5, SHA224_H6, SHA224_H7,
    ],
};

static SHA256_IV: Sha256BlockState = Sha256BlockState {
    h: [
        SHA256_H0, SHA256_H1, SHA256_H2, SHA256_H3, SHA256_H4, SHA256_H5, SHA256_H6, SHA256_H7,
    ],
};

/* initial_sha256_ctx: state == sha256_iv, bytecount == 0 */
static INITIAL_SHA256_CTX: Sha256Ctx = Sha256Ctx {
    ctx: __Sha256Ctx {
        state: Sha256BlockState {
            h: [
                SHA256_H0, SHA256_H1, SHA256_H2, SHA256_H3, SHA256_H4, SHA256_H5, SHA256_H6,
                SHA256_H7,
            ],
        },
        bytecount: 0,
        buf: [0; SHA256_BLOCK_SIZE],
    },
};

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[inline(always)]
fn ch32(x: u32, y: u32, z: u32) -> u32 {
    z ^ (x & (y ^ z))
}

#[inline(always)]
fn maj32(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (z & (x | y))
}

#[inline(always)]
fn e0_32(x: u32) -> u32 {
    ror32(x, 2) ^ ror32(x, 13) ^ ror32(x, 22)
}

#[inline(always)]
fn e1_32(x: u32) -> u32 {
    ror32(x, 6) ^ ror32(x, 11) ^ ror32(x, 25)
}

#[inline(always)]
fn s0_32(x: u32) -> u32 {
    ror32(x, 7) ^ ror32(x, 18) ^ (x >> 3)
}

#[inline(always)]
fn s1_32(x: u32) -> u32 {
    ror32(x, 17) ^ ror32(x, 19) ^ (x >> 10)
}

pub fn sha256_block_generic(state: &mut Sha256BlockState, input: &[u8], w: &mut [u32; 64]) {
    /* LOAD_OP / BLEND_OP, same unroll-by-8 as the C. */
    let mut i = 0usize;
    while i < 16 {
        let mut j = 0usize;
        while j < 8 {
            w[i + j] = get_unaligned_be32(&input[(i + j) * 4..]);
            j += 1;
        }
        i += 8;
    }
    let mut i = 16usize;
    while i < 64 {
        let mut j = 0usize;
        while j < 8 {
            w[i + j] = s1_32(w[i + j - 2])
                .wrapping_add(w[i + j - 7])
                .wrapping_add(s0_32(w[i + j - 15]))
                .wrapping_add(w[i + j - 16]);
            j += 1;
        }
        i += 8;
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state.h;

    macro_rules! sha256_round {
        ($i:expr, $a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident,
         $h:ident) => {{
            let t1 = $h
                .wrapping_add(e1_32($e))
                .wrapping_add(ch32($e, $f, $g))
                .wrapping_add(SHA256_K[$i])
                .wrapping_add(w[$i]);
            let t2 = e0_32($a).wrapping_add(maj32($a, $b, $c));
            $d = $d.wrapping_add(t1);
            $h = t1.wrapping_add(t2);
        }};
    }

    let mut i = 0usize;
    #[allow(clippy::identity_op)] /* mirrors C's SHA256_ROUND(i + 0, ...) */
    while i < 64 {
        sha256_round!(i + 0, a, b, c, d, e, f, g, h);
        sha256_round!(i + 1, h, a, b, c, d, e, f, g);
        sha256_round!(i + 2, g, h, a, b, c, d, e, f);
        sha256_round!(i + 3, f, g, h, a, b, c, d, e);
        sha256_round!(i + 4, e, f, g, h, a, b, c, d);
        sha256_round!(i + 5, d, e, f, g, h, a, b, c);
        sha256_round!(i + 6, c, d, e, f, g, h, a, b);
        sha256_round!(i + 7, b, c, d, e, f, g, h, a);
        i += 8;
    }

    state.h[0] = state.h[0].wrapping_add(a);
    state.h[1] = state.h[1].wrapping_add(b);
    state.h[2] = state.h[2].wrapping_add(c);
    state.h[3] = state.h[3].wrapping_add(d);
    state.h[4] = state.h[4].wrapping_add(e);
    state.h[5] = state.h[5].wrapping_add(f);
    state.h[6] = state.h[6].wrapping_add(g);
    state.h[7] = state.h[7].wrapping_add(h);
}

/* sha256_blocks: CONFIG_CRYPTO_LIB_SHA256_ARCH unset -> generic */
pub fn sha256_blocks(state: &mut Sha256BlockState, mut data: &[u8], mut nblocks: usize) {
    let mut w = [0u32; 64];
    loop {
        sha256_block_generic(state, data, &mut w);
        data = &data[SHA256_BLOCK_SIZE..];
        nblocks -= 1;
        if nblocks == 0 {
            break;
        }
    }
    w.fill(0); /* memzero_explicit */
}

pub fn __sha256_init(ctx: &mut __Sha256Ctx, iv: &Sha256BlockState, initial_bytecount: u64) {
    ctx.state = *iv;
    ctx.bytecount = initial_bytecount;
}

pub fn sha224_init(ctx: &mut Sha224Ctx) {
    __sha256_init(&mut ctx.ctx, &SHA224_IV, 0);
}

pub fn sha256_init(ctx: &mut Sha256Ctx) {
    __sha256_init(&mut ctx.ctx, &SHA256_IV, 0);
}

pub fn __sha256_update(ctx: &mut __Sha256Ctx, data_in: &[u8], len_in: usize) {
    let mut data = &data_in[..len_in];
    let mut len = len_in;
    let mut partial = (ctx.bytecount % SHA256_BLOCK_SIZE as u64) as usize;

    ctx.bytecount = ctx.bytecount.wrapping_add(len as u64);

    if partial + len >= SHA256_BLOCK_SIZE {
        if partial != 0 {
            let l = SHA256_BLOCK_SIZE - partial;
            ctx.buf[partial..partial + l].copy_from_slice(&data[..l]);
            data = &data[l..];
            len -= l;
            sha256_blocks(&mut ctx.state, &ctx.buf, 1);
        }
        let nblocks = len / SHA256_BLOCK_SIZE;
        len %= SHA256_BLOCK_SIZE;
        if nblocks != 0 {
            sha256_blocks(
                &mut ctx.state,
                &data[..nblocks * SHA256_BLOCK_SIZE],
                nblocks,
            );
            data = &data[nblocks * SHA256_BLOCK_SIZE..];
        }
        partial = 0;
    }
    if len != 0 {
        ctx.buf[partial..partial + len].copy_from_slice(&data[..len]);
    }
}

pub fn __sha256_final(ctx: &mut __Sha256Ctx, out: &mut [u8], digest_size: usize) {
    let bitcount = ctx.bytecount.wrapping_shl(3);
    let mut partial = (ctx.bytecount % SHA256_BLOCK_SIZE as u64) as usize;

    ctx.buf[partial] = 0x80;
    partial += 1;
    if partial > SHA256_BLOCK_SIZE - 8 {
        ctx.buf[partial..SHA256_BLOCK_SIZE].fill(0);
        sha256_blocks(&mut ctx.state, &ctx.buf, 1);
        partial = 0;
    }
    ctx.buf[partial..SHA256_BLOCK_SIZE - 8].fill(0);
    ctx.buf[SHA256_BLOCK_SIZE - 8..SHA256_BLOCK_SIZE].copy_from_slice(&bitcount.to_be_bytes());
    sha256_blocks(&mut ctx.state, &ctx.buf, 1);

    let mut i = 0usize;
    while i < digest_size {
        put_unaligned_be32(ctx.state.h[i / 4], &mut out[i..]);
        i += 4;
    }
}

pub fn sha224_final(ctx: &mut Sha224Ctx, out: &mut [u8]) {
    __sha256_final(&mut ctx.ctx, out, SHA224_DIGEST_SIZE);
    *ctx = Sha224Ctx::default(); /* memzero_explicit */
}

pub fn sha256_final(ctx: &mut Sha256Ctx, out: &mut [u8]) {
    __sha256_final(&mut ctx.ctx, out, SHA256_DIGEST_SIZE);
    *ctx = Sha256Ctx::default(); /* memzero_explicit */
}

/* header inlines */
pub fn sha224_update(ctx: &mut Sha224Ctx, data: &[u8], len: usize) {
    __sha256_update(&mut ctx.ctx, data, len);
}

pub fn sha256_update(ctx: &mut Sha256Ctx, data: &[u8], len: usize) {
    __sha256_update(&mut ctx.ctx, data, len);
}

pub fn sha224(data: &[u8], len: usize, out: &mut [u8]) {
    let mut ctx = Sha224Ctx::default();
    sha224_init(&mut ctx);
    sha224_update(&mut ctx, data, len);
    sha224_final(&mut ctx, out);
}

pub fn sha256(data: &[u8], len: usize, out: &mut [u8]) {
    let mut ctx = Sha256Ctx::default();
    sha256_init(&mut ctx);
    sha256_update(&mut ctx, data, len);
    sha256_final(&mut ctx, out);
}

/* CONFIG_CRYPTO_LIB_SHA256_ARCH unset -> arch hooks are the false stubs */
pub fn sha256_finup_2x_arch(
    _ctx: &__Sha256Ctx,
    _data1: &[u8],
    _data2: &[u8],
    _len: usize,
    _out1: &mut [u8],
    _out2: &mut [u8],
) -> bool {
    false
}

pub fn sha256_finup_2x_is_optimized_arch() -> bool {
    false
}

/* Sequential fallback implementation of sha256_finup_2x() */
#[inline(never)] /* noinline_for_stack */
pub fn sha256_finup_2x_sequential(
    ctx: &__Sha256Ctx,
    data1: &[u8],
    data2: &[u8],
    len: usize,
    out1: &mut [u8],
    out2: &mut [u8],
) {
    let mut mut_ctx = *ctx;
    __sha256_update(&mut mut_ctx, data1, len);
    __sha256_final(&mut mut_ctx, out1, SHA256_DIGEST_SIZE);

    let mut mut_ctx = *ctx;
    __sha256_update(&mut mut_ctx, data2, len);
    __sha256_final(&mut mut_ctx, out2, SHA256_DIGEST_SIZE);
}

pub fn sha256_finup_2x(
    ctx: Option<&Sha256Ctx>,
    data1: &[u8],
    data2: &[u8],
    len: usize,
    out1: &mut [u8],
    out2: &mut [u8],
) {
    let ctx = match ctx {
        Some(c) => c,
        None => &INITIAL_SHA256_CTX,
    };
    if sha256_finup_2x_arch(&ctx.ctx, data1, data2, len, out1, out2) {
        return;
    }
    sha256_finup_2x_sequential(&ctx.ctx, data1, data2, len, out1, out2);
}

pub fn sha256_finup_2x_is_optimized() -> bool {
    sha256_finup_2x_is_optimized_arch()
}

pub fn __hmac_sha256_preparekey(
    istate: &mut Sha256BlockState,
    ostate: &mut Sha256BlockState,
    raw_key: &[u8],
    raw_key_len: usize,
    iv: &'static Sha256BlockState,
) {
    let mut derived_key = [0u8; SHA256_BLOCK_SIZE];

    if raw_key_len > SHA256_BLOCK_SIZE {
        if core::ptr::eq(iv, &SHA224_IV) {
            sha224(raw_key, raw_key_len, &mut derived_key[..SHA224_DIGEST_SIZE]);
        } else {
            sha256(raw_key, raw_key_len, &mut derived_key[..SHA256_DIGEST_SIZE]);
        }
    } else {
        derived_key[..raw_key_len].copy_from_slice(&raw_key[..raw_key_len]);
    }

    /* per-word ^= REPEAT_BYTE(ipad) == per-byte ^= ipad on LP64-LE */
    for b in derived_key.iter_mut() {
        *b ^= HMAC_IPAD_VALUE;
    }
    *istate = *iv;
    sha256_blocks(istate, &derived_key, 1);

    for b in derived_key.iter_mut() {
        *b ^= HMAC_OPAD_VALUE ^ HMAC_IPAD_VALUE;
    }
    *ostate = *iv;
    sha256_blocks(ostate, &derived_key, 1);

    memzero_explicit(&mut derived_key);
}

pub fn hmac_sha224_preparekey(key: &mut HmacSha224Key, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha256_preparekey(
        &mut key.key.istate,
        &mut key.key.ostate,
        raw_key,
        raw_key_len,
        &SHA224_IV,
    );
}

pub fn hmac_sha256_preparekey(key: &mut HmacSha256Key, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha256_preparekey(
        &mut key.key.istate,
        &mut key.key.ostate,
        raw_key,
        raw_key_len,
        &SHA256_IV,
    );
}

pub fn __hmac_sha256_init(ctx: &mut __HmacSha256Ctx, key: &__HmacSha256Key) {
    __sha256_init(&mut ctx.sha_ctx, &key.istate, SHA256_BLOCK_SIZE as u64);
    ctx.ostate = key.ostate;
}

pub fn hmac_sha224_init_usingrawkey(ctx: &mut HmacSha224Ctx, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha256_preparekey(
        &mut ctx.ctx.sha_ctx.state,
        &mut ctx.ctx.ostate,
        raw_key,
        raw_key_len,
        &SHA224_IV,
    );
    ctx.ctx.sha_ctx.bytecount = SHA256_BLOCK_SIZE as u64;
}

pub fn hmac_sha256_init_usingrawkey(ctx: &mut HmacSha256Ctx, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha256_preparekey(
        &mut ctx.ctx.sha_ctx.state,
        &mut ctx.ctx.ostate,
        raw_key,
        raw_key_len,
        &SHA256_IV,
    );
    ctx.ctx.sha_ctx.bytecount = SHA256_BLOCK_SIZE as u64;
}

fn __hmac_sha256_final(ctx: &mut __HmacSha256Ctx, out: &mut [u8], digest_size: usize) {
    /* Generate the padded input for the outer hash in ctx->sha_ctx.buf. */
    let mut tmp = ctx.sha_ctx.buf;
    __sha256_final(&mut ctx.sha_ctx, &mut tmp, digest_size);
    ctx.sha_ctx.buf = tmp;
    ctx.sha_ctx.buf[digest_size..SHA256_BLOCK_SIZE].fill(0);
    ctx.sha_ctx.buf[digest_size] = 0x80;
    ctx.sha_ctx.buf[SHA256_BLOCK_SIZE - 4..SHA256_BLOCK_SIZE]
        .copy_from_slice(&(8 * (SHA256_BLOCK_SIZE + digest_size) as u32).to_be_bytes());

    /* Compute the outer hash, which gives the HMAC value. */
    sha256_blocks(&mut ctx.ostate, &ctx.sha_ctx.buf, 1);
    let mut i = 0usize;
    while i < digest_size {
        put_unaligned_be32(ctx.ostate.h[i / 4], &mut out[i..]);
        i += 4;
    }

    *ctx = __HmacSha256Ctx::default(); /* memzero_explicit */
}

pub fn hmac_sha224_final(ctx: &mut HmacSha224Ctx, out: &mut [u8]) {
    __hmac_sha256_final(&mut ctx.ctx, out, SHA224_DIGEST_SIZE);
}

pub fn hmac_sha256_final(ctx: &mut HmacSha256Ctx, out: &mut [u8]) {
    __hmac_sha256_final(&mut ctx.ctx, out, SHA256_DIGEST_SIZE);
}

/* header inlines */
pub fn hmac_sha224_init(ctx: &mut HmacSha224Ctx, key: &HmacSha224Key) {
    __hmac_sha256_init(&mut ctx.ctx, &key.key);
}

pub fn hmac_sha256_init(ctx: &mut HmacSha256Ctx, key: &HmacSha256Key) {
    __hmac_sha256_init(&mut ctx.ctx, &key.key);
}

pub fn hmac_sha224_update(ctx: &mut HmacSha224Ctx, data: &[u8], data_len: usize) {
    __sha256_update(&mut ctx.ctx.sha_ctx, data, data_len);
}

pub fn hmac_sha256_update(ctx: &mut HmacSha256Ctx, data: &[u8], data_len: usize) {
    __sha256_update(&mut ctx.ctx.sha_ctx, data, data_len);
}

pub fn hmac_sha224(key: &HmacSha224Key, data: &[u8], data_len: usize, out: &mut [u8]) {
    let mut ctx = HmacSha224Ctx::default();
    hmac_sha224_init(&mut ctx, key);
    hmac_sha224_update(&mut ctx, data, data_len);
    hmac_sha224_final(&mut ctx, out);
}

pub fn hmac_sha256(key: &HmacSha256Key, data: &[u8], data_len: usize, out: &mut [u8]) {
    let mut ctx = HmacSha256Ctx::default();
    hmac_sha256_init(&mut ctx, key);
    hmac_sha256_update(&mut ctx, data, data_len);
    hmac_sha256_final(&mut ctx, out);
}

pub fn hmac_sha224_usingrawkey(
    raw_key: &[u8],
    raw_key_len: usize,
    data: &[u8],
    data_len: usize,
    out: &mut [u8],
) {
    let mut ctx = HmacSha224Ctx::default();
    hmac_sha224_init_usingrawkey(&mut ctx, raw_key, raw_key_len);
    hmac_sha224_update(&mut ctx, data, data_len);
    hmac_sha224_final(&mut ctx, out);
}

pub fn hmac_sha256_usingrawkey(
    raw_key: &[u8],
    raw_key_len: usize,
    data: &[u8],
    data_len: usize,
    out: &mut [u8],
) {
    let mut ctx = HmacSha256Ctx::default();
    hmac_sha256_init_usingrawkey(&mut ctx, raw_key, raw_key_len);
    hmac_sha256_update(&mut ctx, data, data_len);
    hmac_sha256_final(&mut ctx, out);
}

// ---- SHA-384/SHA-512 (lib/crypto/sha512.c) -----------------------------------

pub const SHA384_DIGEST_SIZE: usize = 48;
pub const SHA384_BLOCK_SIZE: usize = 128;
pub const SHA512_DIGEST_SIZE: usize = 64;
pub const SHA512_BLOCK_SIZE: usize = 128;

pub const SHA384_H0: u64 = 0xcbbb9d5dc1059ed8;
pub const SHA384_H1: u64 = 0x629a292a367cd507;
pub const SHA384_H2: u64 = 0x9159015a3070dd17;
pub const SHA384_H3: u64 = 0x152fecd8f70e5939;
pub const SHA384_H4: u64 = 0x67332667ffc00b31;
pub const SHA384_H5: u64 = 0x8eb44a8768581511;
pub const SHA384_H6: u64 = 0xdb0c2e0d64f98fa7;
pub const SHA384_H7: u64 = 0x47b5481dbefa4fa4;

pub const SHA512_H0: u64 = 0x6a09e667f3bcc908;
pub const SHA512_H1: u64 = 0xbb67ae8584caa73b;
pub const SHA512_H2: u64 = 0x3c6ef372fe94f82b;
pub const SHA512_H3: u64 = 0xa54ff53a5f1d36f1;
pub const SHA512_H4: u64 = 0x510e527fade682d1;
pub const SHA512_H5: u64 = 0x9b05688c2b3e6c1f;
pub const SHA512_H6: u64 = 0x1f83d9abfb41bd6b;
pub const SHA512_H7: u64 = 0x5be0cd19137e2179;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha512BlockState {
    pub h: [u64; 8],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __Sha512Ctx {
    pub state: Sha512BlockState,
    pub bytecount_lo: u64,
    pub bytecount_hi: u64,
    pub buf: [u8; SHA512_BLOCK_SIZE],
}

impl Default for __Sha512Ctx {
    fn default() -> Self {
        __Sha512Ctx {
            state: Sha512BlockState::default(),
            bytecount_lo: 0,
            bytecount_hi: 0,
            buf: [0; SHA512_BLOCK_SIZE],
        }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha384Ctx {
    pub ctx: __Sha512Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Sha512Ctx {
    pub ctx: __Sha512Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct __HmacSha512Key {
    pub istate: Sha512BlockState,
    pub ostate: Sha512BlockState,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct __HmacSha512Ctx {
    pub sha_ctx: __Sha512Ctx,
    pub ostate: Sha512BlockState,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha384Key {
    pub key: __HmacSha512Key,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha384Ctx {
    pub ctx: __HmacSha512Ctx,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha512Key {
    pub key: __HmacSha512Key,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HmacSha512Ctx {
    pub ctx: __HmacSha512Ctx,
}

/* static, not const: `iv == &sha384_iv` is an address comparison in C. */
static SHA384_IV: Sha512BlockState = Sha512BlockState {
    h: [
        SHA384_H0, SHA384_H1, SHA384_H2, SHA384_H3, SHA384_H4, SHA384_H5, SHA384_H6, SHA384_H7,
    ],
};

static SHA512_IV: Sha512BlockState = Sha512BlockState {
    h: [
        SHA512_H0, SHA512_H1, SHA512_H2, SHA512_H3, SHA512_H4, SHA512_H5, SHA512_H6, SHA512_H7,
    ],
};

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

#[inline(always)]
fn ch64(x: u64, y: u64, z: u64) -> u64 {
    z ^ (x & (y ^ z))
}

#[inline(always)]
fn maj64(x: u64, y: u64, z: u64) -> u64 {
    (x & y) | (z & (x | y))
}

#[inline(always)]
fn e0_64(x: u64) -> u64 {
    ror64(x, 28) ^ ror64(x, 34) ^ ror64(x, 39)
}

#[inline(always)]
fn e1_64(x: u64) -> u64 {
    ror64(x, 14) ^ ror64(x, 18) ^ ror64(x, 41)
}

#[inline(always)]
fn s0_64(x: u64) -> u64 {
    ror64(x, 1) ^ ror64(x, 8) ^ (x >> 7)
}

#[inline(always)]
fn s1_64(x: u64) -> u64 {
    ror64(x, 19) ^ ror64(x, 61) ^ (x >> 6)
}

pub fn sha512_block_generic(state: &mut Sha512BlockState, data: &[u8]) {
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state.h;
    let mut w = [0u64; 16];

    for j in 0..16usize {
        w[j] = get_unaligned_be64(&data[j * 8..]);
    }

    /* One round of the unroll-by-8; w[(i + j) & 15] == W[(i & 15) + j]
     * since i is a multiple of 8 here. */
    macro_rules! sha512_round {
        ($i:expr, $a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident,
         $h:ident) => {{
            let t1 = $h
                .wrapping_add(e1_64($e))
                .wrapping_add(ch64($e, $f, $g))
                .wrapping_add(SHA512_K[$i])
                .wrapping_add(w[($i) & 15]);
            let t2 = e0_64($a).wrapping_add(maj64($a, $b, $c));
            $d = $d.wrapping_add(t1);
            $h = t1.wrapping_add(t2);
        }};
    }

    let mut i = 0usize;
    #[allow(clippy::identity_op)] /* mirrors the C unroll's W[i&15] + j */
    while i < 80 {
        if (i & 15) == 0 && i != 0 {
            for j in 0..16usize {
                w[j & 15] = w[j & 15]
                    .wrapping_add(s1_64(w[j.wrapping_sub(2) & 15]))
                    .wrapping_add(w[j.wrapping_sub(7) & 15])
                    .wrapping_add(s0_64(w[j.wrapping_sub(15) & 15]));
            }
        }
        sha512_round!(i + 0, a, b, c, d, e, f, g, h);
        sha512_round!(i + 1, h, a, b, c, d, e, f, g);
        sha512_round!(i + 2, g, h, a, b, c, d, e, f);
        sha512_round!(i + 3, f, g, h, a, b, c, d, e);
        sha512_round!(i + 4, e, f, g, h, a, b, c, d);
        sha512_round!(i + 5, d, e, f, g, h, a, b, c);
        sha512_round!(i + 6, c, d, e, f, g, h, a, b);
        sha512_round!(i + 7, b, c, d, e, f, g, h, a);
        i += 8;
    }

    state.h[0] = state.h[0].wrapping_add(a);
    state.h[1] = state.h[1].wrapping_add(b);
    state.h[2] = state.h[2].wrapping_add(c);
    state.h[3] = state.h[3].wrapping_add(d);
    state.h[4] = state.h[4].wrapping_add(e);
    state.h[5] = state.h[5].wrapping_add(f);
    state.h[6] = state.h[6].wrapping_add(g);
    state.h[7] = state.h[7].wrapping_add(h);
}

/* sha512_blocks: CONFIG_CRYPTO_LIB_SHA512_ARCH unset -> generic */
pub fn sha512_blocks(state: &mut Sha512BlockState, mut data: &[u8], mut nblocks: usize) {
    loop {
        sha512_block_generic(state, data);
        data = &data[SHA512_BLOCK_SIZE..];
        nblocks -= 1;
        if nblocks == 0 {
            break;
        }
    }
}

pub fn __sha512_init(ctx: &mut __Sha512Ctx, iv: &Sha512BlockState, initial_bytecount: u64) {
    ctx.state = *iv;
    ctx.bytecount_lo = initial_bytecount;
    ctx.bytecount_hi = 0;
}

pub fn sha384_init(ctx: &mut Sha384Ctx) {
    __sha512_init(&mut ctx.ctx, &SHA384_IV, 0);
}

pub fn sha512_init(ctx: &mut Sha512Ctx) {
    __sha512_init(&mut ctx.ctx, &SHA512_IV, 0);
}

pub fn __sha512_update(ctx: &mut __Sha512Ctx, data_in: &[u8], len_in: usize) {
    let mut data = &data_in[..len_in];
    let mut len = len_in;
    let mut partial = (ctx.bytecount_lo % SHA512_BLOCK_SIZE as u64) as usize;

    /* check_add_overflow(bytecount_lo, len, &bytecount_lo) */
    let (sum, overflow) = ctx.bytecount_lo.overflowing_add(len as u64);
    ctx.bytecount_lo = sum;
    if overflow {
        ctx.bytecount_hi += 1;
    }

    if partial + len >= SHA512_BLOCK_SIZE {
        if partial != 0 {
            let l = SHA512_BLOCK_SIZE - partial;
            ctx.buf[partial..partial + l].copy_from_slice(&data[..l]);
            data = &data[l..];
            len -= l;
            sha512_blocks(&mut ctx.state, &ctx.buf, 1);
        }
        let nblocks = len / SHA512_BLOCK_SIZE;
        len %= SHA512_BLOCK_SIZE;
        if nblocks != 0 {
            sha512_blocks(
                &mut ctx.state,
                &data[..nblocks * SHA512_BLOCK_SIZE],
                nblocks,
            );
            data = &data[nblocks * SHA512_BLOCK_SIZE..];
        }
        partial = 0;
    }
    if len != 0 {
        ctx.buf[partial..partial + len].copy_from_slice(&data[..len]);
    }
}

pub fn __sha512_final(ctx: &mut __Sha512Ctx, out: &mut [u8], digest_size: usize) {
    let bitcount_hi = (ctx.bytecount_hi << 3) | (ctx.bytecount_lo >> 61);
    let bitcount_lo = ctx.bytecount_lo << 3;
    let mut partial = (ctx.bytecount_lo % SHA512_BLOCK_SIZE as u64) as usize;

    ctx.buf[partial] = 0x80;
    partial += 1;
    if partial > SHA512_BLOCK_SIZE - 16 {
        ctx.buf[partial..SHA512_BLOCK_SIZE].fill(0);
        sha512_blocks(&mut ctx.state, &ctx.buf, 1);
        partial = 0;
    }
    ctx.buf[partial..SHA512_BLOCK_SIZE - 16].fill(0);
    ctx.buf[SHA512_BLOCK_SIZE - 16..SHA512_BLOCK_SIZE - 8]
        .copy_from_slice(&bitcount_hi.to_be_bytes());
    ctx.buf[SHA512_BLOCK_SIZE - 8..SHA512_BLOCK_SIZE].copy_from_slice(&bitcount_lo.to_be_bytes());
    sha512_blocks(&mut ctx.state, &ctx.buf, 1);

    let mut i = 0usize;
    while i < digest_size {
        put_unaligned_be64(ctx.state.h[i / 8], &mut out[i..]);
        i += 8;
    }
}

pub fn sha384_final(ctx: &mut Sha384Ctx, out: &mut [u8]) {
    __sha512_final(&mut ctx.ctx, out, SHA384_DIGEST_SIZE);
    *ctx = Sha384Ctx::default(); /* memzero_explicit */
}

pub fn sha512_final(ctx: &mut Sha512Ctx, out: &mut [u8]) {
    __sha512_final(&mut ctx.ctx, out, SHA512_DIGEST_SIZE);
    *ctx = Sha512Ctx::default(); /* memzero_explicit */
}

/* header inlines */
pub fn sha384_update(ctx: &mut Sha384Ctx, data: &[u8], len: usize) {
    __sha512_update(&mut ctx.ctx, data, len);
}

pub fn sha512_update(ctx: &mut Sha512Ctx, data: &[u8], len: usize) {
    __sha512_update(&mut ctx.ctx, data, len);
}

pub fn sha384(data: &[u8], len: usize, out: &mut [u8]) {
    let mut ctx = Sha384Ctx::default();
    sha384_init(&mut ctx);
    sha384_update(&mut ctx, data, len);
    sha384_final(&mut ctx, out);
}

pub fn sha512(data: &[u8], len: usize, out: &mut [u8]) {
    let mut ctx = Sha512Ctx::default();
    sha512_init(&mut ctx);
    sha512_update(&mut ctx, data, len);
    sha512_final(&mut ctx, out);
}

pub fn __hmac_sha512_preparekey(
    istate: &mut Sha512BlockState,
    ostate: &mut Sha512BlockState,
    raw_key: &[u8],
    raw_key_len: usize,
    iv: &'static Sha512BlockState,
) {
    let mut derived_key = [0u8; SHA512_BLOCK_SIZE];

    if raw_key_len > SHA512_BLOCK_SIZE {
        if core::ptr::eq(iv, &SHA384_IV) {
            sha384(raw_key, raw_key_len, &mut derived_key[..SHA384_DIGEST_SIZE]);
        } else {
            sha512(raw_key, raw_key_len, &mut derived_key[..SHA512_DIGEST_SIZE]);
        }
    } else {
        derived_key[..raw_key_len].copy_from_slice(&raw_key[..raw_key_len]);
    }

    for b in derived_key.iter_mut() {
        *b ^= HMAC_IPAD_VALUE;
    }
    *istate = *iv;
    sha512_blocks(istate, &derived_key, 1);

    for b in derived_key.iter_mut() {
        *b ^= HMAC_OPAD_VALUE ^ HMAC_IPAD_VALUE;
    }
    *ostate = *iv;
    sha512_blocks(ostate, &derived_key, 1);

    memzero_explicit(&mut derived_key);
}

pub fn hmac_sha384_preparekey(key: &mut HmacSha384Key, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha512_preparekey(
        &mut key.key.istate,
        &mut key.key.ostate,
        raw_key,
        raw_key_len,
        &SHA384_IV,
    );
}

pub fn hmac_sha512_preparekey(key: &mut HmacSha512Key, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha512_preparekey(
        &mut key.key.istate,
        &mut key.key.ostate,
        raw_key,
        raw_key_len,
        &SHA512_IV,
    );
}

pub fn __hmac_sha512_init(ctx: &mut __HmacSha512Ctx, key: &__HmacSha512Key) {
    __sha512_init(&mut ctx.sha_ctx, &key.istate, SHA512_BLOCK_SIZE as u64);
    ctx.ostate = key.ostate;
}

pub fn hmac_sha384_init_usingrawkey(ctx: &mut HmacSha384Ctx, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha512_preparekey(
        &mut ctx.ctx.sha_ctx.state,
        &mut ctx.ctx.ostate,
        raw_key,
        raw_key_len,
        &SHA384_IV,
    );
    ctx.ctx.sha_ctx.bytecount_lo = SHA512_BLOCK_SIZE as u64;
    ctx.ctx.sha_ctx.bytecount_hi = 0;
}

pub fn hmac_sha512_init_usingrawkey(ctx: &mut HmacSha512Ctx, raw_key: &[u8], raw_key_len: usize) {
    __hmac_sha512_preparekey(
        &mut ctx.ctx.sha_ctx.state,
        &mut ctx.ctx.ostate,
        raw_key,
        raw_key_len,
        &SHA512_IV,
    );
    ctx.ctx.sha_ctx.bytecount_lo = SHA512_BLOCK_SIZE as u64;
    ctx.ctx.sha_ctx.bytecount_hi = 0;
}

fn __hmac_sha512_final(ctx: &mut __HmacSha512Ctx, out: &mut [u8], digest_size: usize) {
    /* Generate the padded input for the outer hash in ctx->sha_ctx.buf. */
    let mut tmp = ctx.sha_ctx.buf;
    __sha512_final(&mut ctx.sha_ctx, &mut tmp, digest_size);
    ctx.sha_ctx.buf = tmp;
    ctx.sha_ctx.buf[digest_size..SHA512_BLOCK_SIZE].fill(0);
    ctx.sha_ctx.buf[digest_size] = 0x80;
    ctx.sha_ctx.buf[SHA512_BLOCK_SIZE - 4..SHA512_BLOCK_SIZE]
        .copy_from_slice(&(8 * (SHA512_BLOCK_SIZE + digest_size) as u32).to_be_bytes());

    /* Compute the outer hash, which gives the HMAC value. */
    sha512_blocks(&mut ctx.ostate, &ctx.sha_ctx.buf, 1);
    let mut i = 0usize;
    while i < digest_size {
        put_unaligned_be64(ctx.ostate.h[i / 8], &mut out[i..]);
        i += 8;
    }

    *ctx = __HmacSha512Ctx::default(); /* memzero_explicit */
}

pub fn hmac_sha384_final(ctx: &mut HmacSha384Ctx, out: &mut [u8]) {
    __hmac_sha512_final(&mut ctx.ctx, out, SHA384_DIGEST_SIZE);
}

pub fn hmac_sha512_final(ctx: &mut HmacSha512Ctx, out: &mut [u8]) {
    __hmac_sha512_final(&mut ctx.ctx, out, SHA512_DIGEST_SIZE);
}

/* header inlines */
pub fn hmac_sha384_init(ctx: &mut HmacSha384Ctx, key: &HmacSha384Key) {
    __hmac_sha512_init(&mut ctx.ctx, &key.key);
}

pub fn hmac_sha512_init(ctx: &mut HmacSha512Ctx, key: &HmacSha512Key) {
    __hmac_sha512_init(&mut ctx.ctx, &key.key);
}

pub fn hmac_sha384_update(ctx: &mut HmacSha384Ctx, data: &[u8], data_len: usize) {
    __sha512_update(&mut ctx.ctx.sha_ctx, data, data_len);
}

pub fn hmac_sha512_update(ctx: &mut HmacSha512Ctx, data: &[u8], data_len: usize) {
    __sha512_update(&mut ctx.ctx.sha_ctx, data, data_len);
}

pub fn hmac_sha384(key: &HmacSha384Key, data: &[u8], data_len: usize, out: &mut [u8]) {
    let mut ctx = HmacSha384Ctx::default();
    hmac_sha384_init(&mut ctx, key);
    hmac_sha384_update(&mut ctx, data, data_len);
    hmac_sha384_final(&mut ctx, out);
}

pub fn hmac_sha512(key: &HmacSha512Key, data: &[u8], data_len: usize, out: &mut [u8]) {
    let mut ctx = HmacSha512Ctx::default();
    hmac_sha512_init(&mut ctx, key);
    hmac_sha512_update(&mut ctx, data, data_len);
    hmac_sha512_final(&mut ctx, out);
}

pub fn hmac_sha384_usingrawkey(
    raw_key: &[u8],
    raw_key_len: usize,
    data: &[u8],
    data_len: usize,
    out: &mut [u8],
) {
    let mut ctx = HmacSha384Ctx::default();
    hmac_sha384_init_usingrawkey(&mut ctx, raw_key, raw_key_len);
    hmac_sha384_update(&mut ctx, data, data_len);
    hmac_sha384_final(&mut ctx, out);
}

pub fn hmac_sha512_usingrawkey(
    raw_key: &[u8],
    raw_key_len: usize,
    data: &[u8],
    data_len: usize,
    out: &mut [u8],
) {
    let mut ctx = HmacSha512Ctx::default();
    hmac_sha512_init_usingrawkey(&mut ctx, raw_key, raw_key_len);
    hmac_sha512_update(&mut ctx, data, data_len);
    hmac_sha512_final(&mut ctx, out);
}

/// `struct crypto_sha256_state` — legacy marshal core (sha2.h); export_core
/// serializes exactly this prefix of `__sha256_ctx` (offsetof(buf) = 40 bytes).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct CryptoSha256State {
    pub state: [u32; SHA256_STATE_WORDS],
    pub count: u64,
}

/// `sha224_block_init()` — header inline.
pub fn sha224_block_init(sctx: &mut CryptoSha256State) {
    sctx.state = [
        SHA224_H0, SHA224_H1, SHA224_H2, SHA224_H3, SHA224_H4, SHA224_H5, SHA224_H6, SHA224_H7,
    ];
    sctx.count = 0;
}

/// `sha256_block_init()` — header inline.
pub fn sha256_block_init(sctx: &mut CryptoSha256State) {
    sctx.state = [
        SHA256_H0, SHA256_H1, SHA256_H2, SHA256_H3, SHA256_H4, SHA256_H5, SHA256_H6, SHA256_H7,
    ];
    sctx.count = 0;
}

/* C-ABI layout contracts from crypto/sha{1,256,512}.c's static_asserts —
 * `mem::offset_of!` pins marshal-compatible layouts for the shash
 * export/import paths (future crypto/ glue wave). */
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(Sha1Ctx, state) == 0);
    assert!(offset_of!(Sha1Ctx, bytecount) == 24);
    assert!(offset_of!(Sha1Ctx, buf) == 32);
    assert!(core::mem::size_of::<Sha1Ctx>() == 96);
    assert!(offset_of!(__Sha256Ctx, state) == 0);
    assert!(offset_of!(__Sha256Ctx, bytecount) == 32);
    assert!(offset_of!(__Sha256Ctx, buf) == 40);
    assert!(core::mem::size_of::<__Sha256Ctx>() == 104);
    assert!(offset_of!(__Sha512Ctx, state) == 0);
    assert!(offset_of!(__Sha512Ctx, bytecount_lo) == 64);
    assert!(offset_of!(__Sha512Ctx, bytecount_hi) == 72);
    assert!(offset_of!(__Sha512Ctx, buf) == 80);
    assert!(core::mem::size_of::<__Sha512Ctx>() == 208);
};

/// `SHA1_STATE_SIZE` — offsetof(struct sha1_state, buffer)
pub const SHA1_STATE_SIZE: usize = 32;
/// `SHA256_SHASH_STATE_SIZE` — shash export/import blob size (sizeof + 1)
pub const SHA256_SHASH_STATE_SIZE: usize = 105;
/// `SHA224_SHASH_STATE_SIZE` — same ctx family as SHA-256
pub const SHA224_SHASH_STATE_SIZE: usize = SHA256_SHASH_STATE_SIZE;
/// `SHA1_SHASH_STATE_SIZE` — sizeof(struct sha1_ctx) + 1
pub const SHA1_SHASH_STATE_SIZE: usize = 97;
/// `SHA512_STATE_SIZE` — offsetof-style core size
pub const SHA512_STATE_SIZE: usize = 80;
/// `SHA512_SHASH_STATE_SIZE` — shash export/import blob size (sizeof + 1)
pub const SHA512_SHASH_STATE_SIZE: usize = 209;
/// `SHA384_SHASH_STATE_SIZE` — same ctx family as SHA-512
pub const SHA384_SHASH_STATE_SIZE: usize = SHA512_SHASH_STATE_SIZE;

/// `struct sha1_state` — legacy serialization layout (byte-compat with `Sha1Ctx`).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sha1State {
    pub state: [u32; 5],
    pub count: u64,
    pub buffer: [u8; SHA1_BLOCK_SIZE],
}

/// `struct sha256_state` — legacy serialization layout (byte-compat with `__Sha256Ctx`).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sha256State {
    pub state: [u32; 8],
    pub count: u64,
    pub buf: [u8; SHA256_BLOCK_SIZE],
}

/// `struct sha512_state` — legacy serialization layout (byte-compat with `__Sha512Ctx`).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sha512State {
    pub state: [u64; 8],
    pub count: [u64; 2],
    pub buf: [u8; SHA512_BLOCK_SIZE],
}

impl Default for Sha1State {
    fn default() -> Self {
        Sha1State { state: [0; 5], count: 0, buffer: [0; SHA1_BLOCK_SIZE] }
    }
}
impl Default for Sha256State {
    fn default() -> Self {
        Sha256State { state: [0; 8], count: 0, buf: [0; SHA256_BLOCK_SIZE] }
    }
}
impl Default for Sha512State {
    fn default() -> Self {
        Sha512State { state: [0; 8], count: [0; 2], buf: [0; SHA512_BLOCK_SIZE] }
    }
}

const _: () = {
    use core::mem::offset_of;
    assert!(core::mem::size_of::<Sha1State>() == core::mem::size_of::<Sha1Ctx>());
    assert!(offset_of!(Sha1State, state) == offset_of!(Sha1Ctx, state));
    assert!(offset_of!(Sha1State, count) == offset_of!(Sha1Ctx, bytecount));
    assert!(offset_of!(Sha1State, buffer) == offset_of!(Sha1Ctx, buf));
    assert!(core::mem::size_of::<Sha256State>() == core::mem::size_of::<__Sha256Ctx>());
    assert!(offset_of!(Sha256State, state) == offset_of!(__Sha256Ctx, state));
    assert!(offset_of!(Sha256State, count) == offset_of!(__Sha256Ctx, bytecount));
    assert!(offset_of!(Sha256State, buf) == offset_of!(__Sha256Ctx, buf));
    assert!(core::mem::size_of::<Sha512State>() == core::mem::size_of::<__Sha512Ctx>());
    assert!(offset_of!(Sha512State, state) == offset_of!(__Sha512Ctx, state));
    assert!(offset_of!(Sha512State, count) == offset_of!(__Sha512Ctx, bytecount_lo));
    assert!(offset_of!(Sha512State, buf) == offset_of!(__Sha512Ctx, buf));
    /* export_core serializes offsetof(__sha256_ctx, buf) == sizeof(crypto_sha256_state) */
    assert!(core::mem::size_of::<CryptoSha256State>() == offset_of!(__Sha256Ctx, buf));
};

/// `hmac_sha224_zeroize_ctx()` — header inline: memzero_explicit(ctx).
pub fn hmac_sha224_zeroize_ctx(ctx: &mut HmacSha224Ctx) {
    *ctx = HmacSha224Ctx::default();
}
/// `hmac_sha256_zeroize_ctx()`
pub fn hmac_sha256_zeroize_ctx(ctx: &mut HmacSha256Ctx) {
    *ctx = HmacSha256Ctx::default();
}
/// `hmac_sha384_zeroize_ctx()`
pub fn hmac_sha384_zeroize_ctx(ctx: &mut HmacSha384Ctx) {
    *ctx = HmacSha384Ctx::default();
}
/// `hmac_sha512_zeroize_ctx()`
pub fn hmac_sha512_zeroize_ctx(ctx: &mut HmacSha512Ctx) {
    *ctx = HmacSha512Ctx::default();
}

/// `hmac_sha1_zeroize_key()` — header inline: memzero_explicit(key, sizeof(*key)).
pub fn hmac_sha1_zeroize_key(key: &mut HmacSha1Key) {
    *key = HmacSha1Key::default();
}
/// `hmac_sha224_zeroize_key()`
pub fn hmac_sha224_zeroize_key(key: &mut HmacSha224Key) {
    *key = HmacSha224Key::default();
}
/// `hmac_sha256_zeroize_key()`
pub fn hmac_sha256_zeroize_key(key: &mut HmacSha256Key) {
    *key = HmacSha256Key::default();
}
/// `hmac_sha384_zeroize_key()`
pub fn hmac_sha384_zeroize_key(key: &mut HmacSha384Key) {
    *key = HmacSha384Key::default();
}
/// `hmac_sha512_zeroize_key()`
pub fn hmac_sha512_zeroize_key(key: &mut HmacSha512Key) {
    *key = HmacSha512Key::default();
}

#[cfg(test)]
mod tests {
    include!("sha_tests.inc");
}

