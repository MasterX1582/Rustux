//! Verified Rust port of `lib/kfifo.c` — the kernel's generic FIFO.
//!
//! Covers the `__kfifo_*` surface: init, in/out/peek/linear, the
//! user-copy paths (plain-memory model — same semantics as the C shim's
//! always-successful `copy_*_user`), the DMA-prepare scatterlist paths,
//! the record FIFO variants (`*_r`, recsize 0/1/2), the `kfifo.h`
//! arithmetic helpers (len/avail/is_empty/is_full/reset/size), and
//! `sg_init_table`.
//!
//! Behavior-verified against the byte-identical C unit over 960
//! recorded op-sequence cases (ports/lib-kfifo) — wraparound, partial
//! ops, record framing, counter overflow, multi-entry scatterlists.
//!
//! API notes:
//! - `Kfifo<'a>` borrows its backing buffer (C: `void *data`). A NULL
//!   `data` is unrepresentable; the `-EINVAL` paths surface as `Err`
//!   from `Kfifo::init`/`Kfifo::alloc_node` instead.
//! - `Kfifo::init` is `__kfifo_init` (slice len = C's `size`, rounds
//!   DOWN to pow2); `Kfifo::alloc_node` is `__kfifo_alloc_node` (rounds
//!   UP) on a caller buffer — klib is no_std, no kmalloc.
//! - `u32` counters wrap exactly like C (`in - out` is the intended
//!   length idiom).
//! - `*_from_user`/`*_to_user` return `(ret, copied)` like the C
//!   `int ret; unsigned *copied` pair.
//! - Caller contract (upstream `kfifo.h` docs): single reader + single
//!   writer without locking; `in - out` never exceeds `mask + 1`; for
//!   record fifos the caller must ensure the next record's declared
//!   length + recsize header fits the occupied space before the
//!   `*_r` out-ops (the harness's `rec_fits` predicate).

#![allow(clippy::len_zero)]

/// `-EINVAL` (asm-generic errno).
pub const EINVAL: i32 = 22;
/// `-ENOMEM` (asm-generic errno).
pub const ENOMEM: i32 = 12;
/// `-EFAULT` (asm-generic errno).
pub const EFAULT: i32 = 14;
/// `DMA_MAPPING_ERROR` (all-ones dma_addr_t in the harness model).
pub const DMA_MAPPING_ERROR: u64 = u64::MAX;

/// `struct __kfifo` — borrows the caller's backing buffer (C: `void *data`).
pub struct Kfifo<'a> {
    pub in_: u32,
    pub out: u32,
    pub mask: u32,
    pub esize: u32,
    pub data: &'a mut [u8],
}

/// `struct scatterlist` — `off` is the byte offset into `Kfifo::data`
/// (C's `sg_set_buf` target); `offset`/`length`/`dma_*` carry the same
/// field values the C entries hold.
#[derive(Clone, Copy, Default)]
pub struct Sg {
    pub off: u32,
    pub offset: u32,
    pub length: u32,
    pub dma_address: u64,
    pub dma_length: u32,
}

impl<'a> Kfifo<'a> {
    /// `__kfifo_init(fifo, buffer, size, esize)` — `data` is the caller's
    /// buffer; its length is C's `size`. `Err(-EINVAL)` when the
    /// rounded-down usable size is < 2 (same as C).
    pub fn init(data: &'a mut [u8], esize: u32) -> Result<Kfifo<'a>, i32> {
        let mut size = data.len() as u32;
        // `size /= esize` — unsigned int divided by size_t.
        size = (size as u64 / esize as u64) as u32;
        if !is_power_of_2(size as u64) {
            size = rounddown_pow_of_two(size as u64) as u32;
        }
        if size < 2 {
            return Err(-EINVAL);
        }
        Ok(Kfifo {
            in_: 0,
            out: 0,
            mask: size - 1,
            esize,
            data,
        })
    }

    /// `__kfifo_alloc_node(fifo, size, esize, gfp, node)` — the kmalloc
    /// path modeled on a caller buffer: `size` rounds UP to a power of
    /// two (init rounds DOWN); `data` must already hold at least
    /// `roundup_pow_of_two(size) * esize` bytes.
    /// `Err(-EINVAL)` on size<2, `Err(-ENOMEM)` when the buffer is short
    /// (kmalloc failure analog).
    pub fn alloc_node(data: &'a mut [u8], size: u32, esize: u32) -> Result<Kfifo<'a>, i32> {
        let size = roundup_pow_of_two(size as u64) as u32;
        if size < 2 {
            return Err(-EINVAL);
        }
        let need = size as usize * esize as usize;
        if data.len() < need {
            return Err(-ENOMEM);
        }
        Ok(Kfifo {
            in_: 0,
            out: 0,
            mask: size - 1,
            esize,
            data: &mut data[..need],
        })
    }
}

/// `sg_init_table()` — resets every entry (the port's `Sg` model carries
/// no end marker; setup_sgl bounds iteration by the returned count).
pub fn sg_init_table(sgl: &mut [Sg]) {
    for e in sgl.iter_mut() {
        *e = Sg::default();
    }
}

// ---- kfifo.h arithmetic helpers (the user-facing macro layer) --------------

/// `kfifo_initialized(fifo)` — `mask != 0`.
pub fn kfifo_initialized(fifo: &Kfifo) -> bool {
    fifo.mask != 0
}

/// `kfifo_esize(fifo)`.
pub fn kfifo_esize(fifo: &Kfifo) -> u32 {
    fifo.esize
}

/// `kfifo_size(fifo)` — capacity in elements.
pub fn kfifo_size(fifo: &Kfifo) -> u32 {
    fifo.mask.wrapping_add(1)
}

/// `kfifo_reset(fifo)` — drop all content (single-reader single-writer only).
pub fn kfifo_reset(fifo: &mut Kfifo) {
    fifo.in_ = 0;
    fifo.out = 0;
}

/// `kfifo_reset_out(fifo)` — skip content (reader side).
pub fn kfifo_reset_out(fifo: &mut Kfifo) {
    fifo.out = fifo.in_;
}

/// `kfifo_len(fifo)` — used elements (`in - out`, wraps by design).
pub fn kfifo_len(fifo: &Kfifo) -> u32 {
    fifo.in_.wrapping_sub(fifo.out)
}

/// `kfifo_is_empty(fifo)`.
pub fn kfifo_is_empty(fifo: &Kfifo) -> bool {
    fifo.in_ == fifo.out
}

/// `kfifo_is_full(fifo)` — `len > mask`.
pub fn kfifo_is_full(fifo: &Kfifo) -> bool {
    kfifo_len(fifo) > fifo.mask
}

/// `kfifo_avail(fifo)` — free elements. Record fifos (`recsize != 0`)
/// report `max_r(avail - recsize)` like the C macro: the header bytes of
/// the next record are not usable payload space.
pub fn kfifo_avail(fifo: &Kfifo, recsize: u32) -> u32 {
    let avail = kfifo_size(fifo).wrapping_sub(kfifo_len(fifo));
    if recsize != 0 {
        if avail <= recsize {
            0
        } else {
            kfifo_max_r(avail - recsize, recsize)
        }
    } else {
        avail
    }
}

/// `kfifo_skip_count(fifo, count)` — record fifos (`recsize != 0`) skip the
/// whole next record regardless of `count`; plain fifos skip `count` elements.
pub fn kfifo_skip_count(fifo: &mut Kfifo, count: u32, recsize: u32) {
    if recsize != 0 {
        kfifo_skip_r(fifo, recsize);
    } else {
        fifo.out = fifo.out.wrapping_add(count);
    }
}

/// `kfifo_skip(fifo)` — skip one element / one record.
pub fn kfifo_skip(fifo: &mut Kfifo, recsize: u32) {
    kfifo_skip_count(fifo, 1, recsize);
}

/// `kfifo_dma_in_finish(fifo, len)` — `len` is bytes like the C macro.
/// Record fifos (`recsize != 0`) dispatch to `kfifo_dma_in_finish_r`;
/// plain fifos do `in += len/esize`.
pub fn kfifo_dma_in_finish(fifo: &mut Kfifo, len: u32, recsize: u32) {
    if recsize != 0 {
        kfifo_dma_in_finish_r(fifo, len, recsize);
    } else {
        fifo.in_ = fifo.in_.wrapping_add(len / fifo.esize);
    }
}

/// `kfifo_dma_out_finish(fifo, len)` — `out += len/esize` for plain
/// fifos, whole-record skip for record fifos (via `kfifo_skip_count`).
pub fn kfifo_dma_out_finish(fifo: &mut Kfifo, len: u32, recsize: u32) {
    kfifo_skip_count(fifo, len / fifo.esize, recsize);
}

/// `kfifo_peek_len(fifo)` — size of the next record, or `len * esize`
/// bytes for a plain fifo (kfifo.h:338).
pub fn kfifo_peek_len(fifo: &Kfifo, recsize: u32) -> u32 {
    if recsize == 0 {
        kfifo_len(fifo) * fifo.esize
    } else {
        kfifo_len_r(fifo, recsize)
    }
}

/// `kfifo_put(fifo, val)` — one `esize`-byte element. Record fifos go
/// through `kfifo_in_r`; plain fifos return 0 when full, else store the
/// element at `in & mask` and bump `in`.
pub fn kfifo_put(fifo: &mut Kfifo, val: &[u8], recsize: u32) -> u32 {
    if recsize != 0 {
        return kfifo_in_r(fifo, val, val.len() as u32, recsize);
    }
    if kfifo_is_full(fifo) {
        return 0;
    }
    let off = ((fifo.in_ & fifo.mask) * fifo.esize) as usize;
    let n = fifo.esize as usize;
    fifo.data[off..off + n].copy_from_slice(&val[..n]);
    fifo.in_ = fifo.in_.wrapping_add(1);
    1
}

/// `kfifo_get(fifo, val)` — one element out; 0 when empty.
pub fn kfifo_get(fifo: &mut Kfifo, val: &mut [u8], recsize: u32) -> u32 {
    if recsize != 0 {
        return kfifo_out_r(fifo, val, val.len() as u32, recsize);
    }
    if kfifo_is_empty(fifo) {
        return 0;
    }
    let off = ((fifo.out & fifo.mask) * fifo.esize) as usize;
    let n = fifo.esize as usize;
    val[..n].copy_from_slice(&fifo.data[off..off + n]);
    fifo.out = fifo.out.wrapping_add(1);
    1
}

/// `kfifo_peek(fifo, val)` — one element out without consuming.
pub fn kfifo_peek(fifo: &Kfifo, val: &mut [u8], recsize: u32) -> u32 {
    if recsize != 0 {
        return kfifo_out_peek_r(fifo, val, val.len() as u32, recsize);
    }
    if kfifo_is_empty(fifo) {
        return 0;
    }
    let off = ((fifo.out & fifo.mask) * fifo.esize) as usize;
    let n = fifo.esize as usize;
    val[..n].copy_from_slice(&fifo.data[off..off + n]);
    1
}

pub fn fls_long(x: u64) -> u32 {
    if x == 0 { 0 } else { 64 - x.leading_zeros() }
}

/// __roundup_pow_of_two: `1UL << fls_long(n - 1)` — the C shift is a machine
/// shl (count masked to 6 bits on x86), so wrapping_shl reproduces even the
/// n == 0 case the C standard calls UB.
pub fn roundup_pow_of_two(n: u64) -> u64 {
    1u64.wrapping_shl(fls_long(n.wrapping_sub(1)))
}

/// __rounddown_pow_of_two: `1UL << (fls_long(n) - 1)`; fls_long(0) - 1 == -1
/// becomes a huge count, same masked-shl semantics the C emits.
pub fn rounddown_pow_of_two(n: u64) -> u64 {
    1u64.wrapping_shl(fls_long(n).wrapping_sub(1))
}

/// is_power_of_2: `n - 1 < (n ^ (n - 1))` — zero is not a power of two.
pub fn is_power_of_2(n: u64) -> bool {
    n.wrapping_sub(1) < (n ^ n.wrapping_sub(1))
}

// ---- lib/kfifo.c -----------------------------------------------------------------

/// internal helper to calculate the unused elements in a fifo
pub fn kfifo_unused(fifo: &Kfifo) -> u32 {
    fifo.mask
        .wrapping_add(1)
        .wrapping_sub(fifo.in_.wrapping_sub(fifo.out))
}

pub fn kfifo_copy_in(fifo: &mut Kfifo, src: &[u8], mut len: u32, mut off: u32) {
    let mut size = fifo.mask.wrapping_add(1);
    let esize = fifo.esize;

    off &= fifo.mask;
    if esize != 1 {
        off = off.wrapping_mul(esize);
        size = size.wrapping_mul(esize);
        len = len.wrapping_mul(esize);
    }
    let l = len.min(size - off);

    fifo.data[off as usize..off as usize + l as usize].copy_from_slice(&src[..l as usize]);
    fifo.data[..(len - l) as usize].copy_from_slice(&src[l as usize..len as usize]);
    // smp_wmb()
}

pub fn kfifo_in(fifo: &mut Kfifo, buf: &[u8], mut len: u32) -> u32 {
    let l = kfifo_unused(fifo);
    if len > l {
        len = l;
    }
    kfifo_copy_in(fifo, buf, len, fifo.in_);
    fifo.in_ = fifo.in_.wrapping_add(len);
    len
}

pub fn kfifo_copy_out(fifo: &Kfifo, dst: &mut [u8], mut len: u32, mut off: u32) {
    let mut size = fifo.mask.wrapping_add(1);
    let esize = fifo.esize;

    off &= fifo.mask;
    if esize != 1 {
        off = off.wrapping_mul(esize);
        size = size.wrapping_mul(esize);
        len = len.wrapping_mul(esize);
    }
    let l = len.min(size - off);

    dst[..l as usize].copy_from_slice(&fifo.data[off as usize..off as usize + l as usize]);
    dst[l as usize..len as usize].copy_from_slice(&fifo.data[..(len - l) as usize]);
    // smp_wmb()
}

pub fn kfifo_out_peek(fifo: &Kfifo, buf: &mut [u8], mut len: u32) -> u32 {
    let l = fifo.in_.wrapping_sub(fifo.out);
    if len > l {
        len = l;
    }
    kfifo_copy_out(fifo, buf, len, fifo.out);
    len
}

/// __kfifo_out_linear returns (count, tail).
pub fn kfifo_out_linear(fifo: &Kfifo, n: u32) -> (u32, u32) {
    let size = fifo.mask.wrapping_add(1);
    let off = fifo.out & fifo.mask;
    // min3(n, in - out, size - off)
    (n.min(fifo.in_.wrapping_sub(fifo.out)).min(size - off), off)
}

pub fn kfifo_out(fifo: &mut Kfifo, buf: &mut [u8], len: u32) -> u32 {
    let len = kfifo_out_peek(fifo, buf, len);
    fifo.out = fifo.out.wrapping_add(len);
    len
}

/// kfifo_copy_from_user: copy_from_user is memcpy returning 0, so the
/// `unlikely(ret)` branches never run; `*copied = len` in element-bytes.
pub fn kfifo_copy_from_user(fifo: &mut Kfifo, from: &[u8], mut len: u32, mut off: u32) -> (u64, u32) {
    let mut size = fifo.mask.wrapping_add(1);
    let esize = fifo.esize;

    off &= fifo.mask;
    if esize != 1 {
        off = off.wrapping_mul(esize);
        size = size.wrapping_mul(esize);
        len = len.wrapping_mul(esize);
    }
    let l = len.min(size - off);

    let mut ret: u64 = 0; // copy_from_user() == 0 in the shim
    fifo.data[off as usize..off as usize + l as usize].copy_from_slice(&from[..l as usize]);
    if ret != 0 {
        // DIV_ROUND_UP(ret + len - l, esize)
        ret = (ret + (len - l) as u64).div_ceil(esize as u64);
    } else {
        fifo.data[..(len - l) as usize].copy_from_slice(&from[l as usize..len as usize]);
        if ret != 0 {
            ret = ret.div_ceil(esize as u64);
        }
    }
    // smp_wmb(); *copied = len - ret * esize
    let copied = len.wrapping_sub((ret as u32).wrapping_mul(esize));
    (ret, copied)
}

pub fn kfifo_from_user(fifo: &mut Kfifo, from: &[u8], mut len: u64) -> (i32, u32) {
    let esize = fifo.esize;

    if esize != 1 {
        len /= esize as u64;
    }
    let mut len = len as u32;
    let l = kfifo_unused(fifo);
    if len > l {
        len = l;
    }
    let (ret, copied) = kfifo_copy_from_user(fifo, from, len, fifo.in_);
    let err = if ret != 0 {
        len -= ret as u32;
        -EFAULT
    } else {
        0
    };
    fifo.in_ = fifo.in_.wrapping_add(len);
    (err, copied)
}

pub fn kfifo_copy_to_user(fifo: &Kfifo, to: &mut [u8], mut len: u32, mut off: u32) -> (u64, u32) {
    let mut size = fifo.mask.wrapping_add(1);
    let esize = fifo.esize;

    off &= fifo.mask;
    if esize != 1 {
        off = off.wrapping_mul(esize);
        size = size.wrapping_mul(esize);
        len = len.wrapping_mul(esize);
    }
    let l = len.min(size - off);

    let mut ret: u64 = 0; // copy_to_user() == 0 in the shim
    to[..l as usize].copy_from_slice(&fifo.data[off as usize..off as usize + l as usize]);
    if ret != 0 {
        ret = (ret + (len - l) as u64).div_ceil(esize as u64);
    } else {
        to[l as usize..len as usize].copy_from_slice(&fifo.data[..(len - l) as usize]);
        if ret != 0 {
            ret = ret.div_ceil(esize as u64);
        }
    }
    let copied = len.wrapping_sub((ret as u32).wrapping_mul(esize));
    (ret, copied)
}

pub fn kfifo_to_user(fifo: &mut Kfifo, to: &mut [u8], mut len: u64) -> (i32, u32) {
    let esize = fifo.esize;

    if esize != 1 {
        len /= esize as u64;
    }
    let mut len = len as u32;
    let l = fifo.in_.wrapping_sub(fifo.out);
    if len > l {
        len = l;
    }
    let (ret, copied) = kfifo_copy_to_user(fifo, to, len, fifo.out);
    let err = if ret != 0 {
        len -= ret as u32;
        -EFAULT
    } else {
        0
    };
    fifo.out = fifo.out.wrapping_add(len);
    (err, copied)
}

// ---- scatterlist setup ---------------------------------------------------------

/// setup_sgl_buf for entry `idx`; `data_offset` is a byte offset into data.
pub fn setup_sgl_buf(
    sgl: &mut [Sg],
    idx: usize,
    data_offset: u32,
    nents: i32,
    len: u32,
    dma: u64,
) -> u32 {
    if nents == 0 || len == 0 {
        return 0;
    }
    // sg_set_buf(sgl, buf, len): buf = data + data_offset
    sgl[idx].off = data_offset;
    sgl[idx].offset = 0;
    sgl[idx].length = len;

    if dma != DMA_MAPPING_ERROR {
        sgl[idx].dma_address = dma.wrapping_add(data_offset as u64);
        sgl[idx].dma_length = len;
    }
    1
}

pub fn setup_sgl(
    fifo: &Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    mut off: u32,
    dma: u64,
) -> u32 {
    let mut size = fifo.mask.wrapping_add(1);
    let esize = fifo.esize;

    off &= fifo.mask;
    if esize != 1 {
        off = off.wrapping_mul(esize);
        size = size.wrapping_mul(esize);
        len = len.wrapping_mul(esize);
    }
    let len_to_end = len.min(size - off);

    let n = setup_sgl_buf(sgl, 0, off, nents, len_to_end, dma);
    n + setup_sgl_buf(sgl, n as usize, 0, nents - n as i32, len - len_to_end, dma)
}

pub fn kfifo_dma_in_prepare(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    dma: u64,
) -> u32 {
    let l = kfifo_unused(fifo);
    if len > l {
        len = l;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.in_, dma)
}

pub fn kfifo_dma_out_prepare(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    dma: u64,
) -> u32 {
    let l = fifo.in_.wrapping_sub(fifo.out);
    if len > l {
        len = l;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.out, dma)
}

pub fn kfifo_max_r(len: u32, recsize: u32) -> u32 {
    let max = (1u32 << (recsize << 3)) - 1;
    if len > max { max } else { len }
}

// __KFIFO_PEEK(data, out, mask) == data[(out) & (mask)]
pub fn kfifo_peek_n(fifo: &Kfifo, recsize: u32) -> u32 {
    let mut l = fifo.data[(fifo.out & fifo.mask) as usize] as u32;
    if recsize.wrapping_sub(1) != 0 {
        l |= (fifo.data[(fifo.out.wrapping_add(1) & fifo.mask) as usize] as u32) << 8;
    }
    l
}

pub fn kfifo_poke_n(fifo: &mut Kfifo, n: u32, recsize: u32) {
    let mask = fifo.mask;
    let in_ = fifo.in_;
    fifo.data[(in_ & mask) as usize] = n as u8;
    if recsize > 1 {
        fifo.data[(in_.wrapping_add(1) & mask) as usize] = (n >> 8) as u8;
    }
}

pub fn kfifo_len_r(fifo: &Kfifo, recsize: u32) -> u32 {
    kfifo_peek_n(fifo, recsize)
}

pub fn kfifo_in_r(fifo: &mut Kfifo, buf: &[u8], len: u32, recsize: u32) -> u32 {
    if len as u64 + recsize as u64 > kfifo_unused(fifo) as u64 {
        return 0;
    }
    kfifo_poke_n(fifo, len, recsize);
    let off = fifo.in_.wrapping_add(recsize);
    kfifo_copy_in(fifo, buf, len, off);
    fifo.in_ = fifo.in_.wrapping_add(len.wrapping_add(recsize));
    len
}

pub fn kfifo_out_copy_r(fifo: &Kfifo, buf: &mut [u8], mut len: u32, recsize: u32) -> u32 {
    let n = kfifo_peek_n(fifo, recsize);
    if len > n {
        len = n;
    }
    kfifo_copy_out(fifo, buf, len, fifo.out.wrapping_add(recsize));
    len
}

pub fn kfifo_out_peek_r(fifo: &Kfifo, buf: &mut [u8], len: u32, recsize: u32) -> u32 {
    if fifo.in_ == fifo.out {
        return 0;
    }
    kfifo_out_copy_r(fifo, buf, len, recsize)
}

/// __kfifo_out_linear_r returns (count, tail); the C writes *tail only on
/// the non-empty path — the driver folds tail=0 when empty.
pub fn kfifo_out_linear_r(fifo: &Kfifo, n: u32, recsize: u32) -> (u32, u32) {
    if fifo.in_ == fifo.out {
        return (0, 0);
    }
    (
        n.min(kfifo_peek_n(fifo, recsize)),
        fifo.out.wrapping_add(recsize),
    )
}

pub fn kfifo_out_r(fifo: &mut Kfifo, buf: &mut [u8], mut len: u32, recsize: u32) -> u32 {
    if fifo.in_ == fifo.out {
        return 0;
    }
    let n = kfifo_peek_n(fifo, recsize);
    if len > n {
        len = n;
    }
    kfifo_copy_out(fifo, buf, len, fifo.out.wrapping_add(recsize));
    fifo.out = fifo.out.wrapping_add(n.wrapping_add(recsize));
    len
}

pub fn kfifo_skip_r(fifo: &mut Kfifo, recsize: u32) {
    let n = kfifo_peek_n(fifo, recsize);
    fifo.out = fifo.out.wrapping_add(n.wrapping_add(recsize));
}

pub fn kfifo_from_user_r(fifo: &mut Kfifo, from: &[u8], len: u64, recsize: u32) -> (i32, u32) {
    // `unsigned long` len truncated to unsigned int by the __kfifo_max_r call.
    let len = kfifo_max_r(len as u32, recsize);

    if len as u64 + recsize as u64 > kfifo_unused(fifo) as u64 {
        return (0, 0);
    }
    kfifo_poke_n(fifo, len, recsize);
    let off = fifo.in_.wrapping_add(recsize);
    let (ret, copied) = kfifo_copy_from_user(fifo, from, len, off);
    if ret != 0 {
        return (-EFAULT, 0);
    }
    fifo.in_ = fifo.in_.wrapping_add(len.wrapping_add(recsize));
    (0, copied)
}

pub fn kfifo_to_user_r(fifo: &mut Kfifo, to: &mut [u8], len: u64, recsize: u32) -> (i32, u32) {
    if fifo.in_ == fifo.out {
        return (0, 0);
    }
    let n = kfifo_peek_n(fifo, recsize);
    let mut len = len as u32;
    if len > n {
        len = n;
    }
    let off = fifo.out.wrapping_add(recsize);
    let (ret, copied) = kfifo_copy_to_user(fifo, to, len, off);
    if ret != 0 {
        return (-EFAULT, 0);
    }
    fifo.out = fifo.out.wrapping_add(n.wrapping_add(recsize));
    (0, copied)
}

pub fn kfifo_dma_in_prepare_r(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    recsize: u32,
    dma: u64,
) -> u32 {
    if nents == 0 {
        panic!("BUG: !nents");
    }
    len = kfifo_max_r(len, recsize);
    if len as u64 + recsize as u64 > kfifo_unused(fifo) as u64 {
        return 0;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.in_.wrapping_add(recsize), dma)
}

pub fn kfifo_dma_in_finish_r(fifo: &mut Kfifo, mut len: u32, recsize: u32) {
    len = kfifo_max_r(len, recsize);
    kfifo_poke_n(fifo, len, recsize);
    fifo.in_ = fifo.in_.wrapping_add(len.wrapping_add(recsize));
}

pub fn kfifo_dma_out_prepare_r(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    recsize: u32,
    dma: u64,
) -> u32 {
    if nents == 0 {
        panic!("BUG: !nents");
    }
    len = kfifo_max_r(len, recsize);
    if len as u64 + recsize as u64 > fifo.in_.wrapping_sub(fifo.out) as u64 {
        return 0;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.out.wrapping_add(recsize), dma)
}

#[cfg(test)]
mod tests {
    include!("kfifo_tests.inc");
}
