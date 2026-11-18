//! The Rust port of `wrappers.c`: safe Rust; `lib.rs` exports each function under its C name.
//!
//! Faithful port of lib/kfifo.c plus the include/linux/kfifo.h typed layer,
//! driven through the same seeded op-sequence harness as the C wrappers.
//!
//! Modelling decisions (same conventions as ports/lib-rbtree):
//! - `struct __kfifo`'s `void *data` is an owned byte buffer: for the
//!   fbuf-backed sequences it is a copy of the seeded backing array (every
//!   write the C makes through fifo->data lands in that array, so the copy
//!   is observably identical); for __kfifo_alloc_node it is the malloc'd
//!   block; `has_data` records `data != NULL` for the -EINVAL alloc path.
//! - `struct scatterlist`'s `buf` pointer becomes a byte offset into the
//!   fifo's data (setup_sgl only ever points entries inside the ring), so
//!   sg reads/writes index the same bytes the C touches through the pointer.
//! - copy_from_user/copy_to_user are memcpy + 0 (the shim's success path).
//! - Every `unsigned int` wraps: `in - out`, `mask + 1` and all counter
//!   updates use wrapping_* arithmetic exactly like the C.
//! - BUG_ON(!nents) -> process abort, unreachable: the driver never
//!   generates nents=0 for the _r entry points (C crash = UB divergence).

#![forbid(unsafe_code)]
// pub fn names mirror the wrappers.c export names verbatim.
#![allow(non_snake_case)]

use std::process;

const BMAX: usize = 4096; /* fifo backing: 512 elements * max esize 8 */
const IOMAX: usize = 8192; /* io buffer: also covers 1024*8 alloc'd fifo */
const MAXSG: usize = 8;
const NOSG: u32 = 0xffffffff; /* folded when a guarded op doesn't run */
const DMA_MAPPING_ERROR: u64 = !0u64; /* ~(dma_addr_t)0 */
const EINVAL: i32 = 22;
const EFAULT: i32 = 14;
const GFP_KERNEL: u32 = 0;
const NUMA_NO_NODE: i32 = -1;

// ---- struct __kfifo ----------------------------------------------------------

/// `struct __kfifo`: `has_data` models `data != NULL` (alloc EINVAL path);
/// `data` is the backing buffer the C code sees through `void *data`.
#[derive(Clone, Default)]
struct Kfifo {
    in_: u32,
    out: u32,
    mask: u32,
    esize: u32,
    data: Vec<u8>,
    has_data: bool,
}

/// `struct scatterlist`: `off` is the byte offset into `Kfifo::data`
/// (the C `buf` pointer minus the ring base); `offset`/`length`/`dma_*`
/// carry the same field values the C entries hold.
#[derive(Clone, Copy, Default)]
struct Sg {
    off: u32,
    offset: u32,
    length: u32,
    dma_address: u64,
    dma_length: u32,
}

// ---- harness state ------------------------------------------------------------

/// `Rig`: the seeded RNG stream and the running FNV-1a fold.  The buffers
/// live next to it so borrows never collide (iobuf/fbuf/sgl are locals).
struct Rig {
    rng: u64,
    h: u64,
}

impl Rig {
    fn new(seed: u64) -> Self {
        Rig {
            rng: seed | 1,
            h: 0x9e3779b97f4a7c15,
        }
    }
    fn xs64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(2685821657736338717)
    }
    fn fold(&mut self, v: u64) {
        self.h = (self.h ^ v).wrapping_mul(0x100000001b3);
    }
}

fn iabs64(v: i64) -> u64 {
    if v < 0 {
        (v as u64).wrapping_neg()
    } else {
        v as u64
    }
}

fn fnv1a(p: &[u8]) -> u64 {
    let mut h = 14695981039346656037u64;
    for &b in p {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn fill_rng(rig: &mut Rig, b: &mut [u8]) {
    for byte in b.iter_mut() {
        *byte = rig.xs64() as u8;
    }
}

// ---- linux/log2.h --------------------------------------------------------------

/// fls_long: index of the highest set bit + 1 (0 for 0).
fn fls_long(x: u64) -> u32 {
    if x == 0 { 0 } else { 64 - x.leading_zeros() }
}

/// __roundup_pow_of_two: `1UL << fls_long(n - 1)` — the C shift is a machine
/// shl (count masked to 6 bits on x86), so wrapping_shl reproduces even the
/// n == 0 case the C standard calls UB.
fn roundup_pow_of_two(n: u64) -> u64 {
    1u64.wrapping_shl(fls_long(n.wrapping_sub(1)))
}

/// __rounddown_pow_of_two: `1UL << (fls_long(n) - 1)`; fls_long(0) - 1 == -1
/// becomes a huge count, same masked-shl semantics the C emits.
fn rounddown_pow_of_two(n: u64) -> u64 {
    1u64.wrapping_shl(fls_long(n).wrapping_sub(1))
}

/// is_power_of_2: `n - 1 < (n ^ (n - 1))` — zero is not a power of two.
fn is_power_of_2(n: u64) -> bool {
    n.wrapping_sub(1) < (n ^ n.wrapping_sub(1))
}

// ---- lib/kfifo.c -----------------------------------------------------------------

/// internal helper to calculate the unused elements in a fifo
fn kfifo_unused(fifo: &Kfifo) -> u32 {
    fifo.mask
        .wrapping_add(1)
        .wrapping_sub(fifo.in_.wrapping_sub(fifo.out))
}

fn kfifo_alloc_node(fifo: &mut Kfifo, size: u32, esize: u32, _gfp: u32, _node: i32) -> i32 {
    // round up to the next power of 2: `size` is unsigned int, the result of
    // the unsigned-long roundup is truncated back to unsigned int.
    let size = roundup_pow_of_two(size as u64) as u32;

    fifo.in_ = 0;
    fifo.out = 0;
    fifo.esize = esize;

    if size < 2 {
        fifo.data = Vec::new();
        fifo.has_data = false;
        fifo.mask = 0;
        return -EINVAL;
    }

    // kmalloc_array_node never fails in the shim (malloc).
    fifo.data = vec![0u8; size as usize * esize as usize];
    fifo.has_data = true;
    fifo.mask = size - 1;
    0
}

fn kfifo_free(fifo: &mut Kfifo) {
    fifo.data = Vec::new();
    fifo.has_data = false;
    fifo.in_ = 0;
    fifo.out = 0;
    fifo.esize = 0;
    fifo.mask = 0;
}

/// __kfifo_init: `backing` carries the full contents of the caller's buffer
/// (the harness fbuf); it becomes the owned `fifo->data`.
fn kfifo_init(fifo: &mut Kfifo, backing: &[u8], mut size: u32, esize: u32) -> i32 {
    // `size /= esize` — unsigned int divided by size_t.
    size = (size as u64 / esize as u64) as u32;

    if !is_power_of_2(size as u64) {
        size = rounddown_pow_of_two(size as u64) as u32;
    }

    fifo.in_ = 0;
    fifo.out = 0;
    fifo.esize = esize;
    fifo.data = backing.to_vec();
    fifo.has_data = true;

    if size < 2 {
        fifo.mask = 0;
        return -EINVAL;
    }
    fifo.mask = size - 1;
    0
}

fn kfifo_copy_in(fifo: &mut Kfifo, src: &[u8], mut len: u32, mut off: u32) {
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

fn kfifo_in(fifo: &mut Kfifo, buf: &[u8], mut len: u32) -> u32 {
    let l = kfifo_unused(fifo);
    if len > l {
        len = l;
    }
    kfifo_copy_in(fifo, buf, len, fifo.in_);
    fifo.in_ = fifo.in_.wrapping_add(len);
    len
}

fn kfifo_copy_out(fifo: &Kfifo, dst: &mut [u8], mut len: u32, mut off: u32) {
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

fn kfifo_out_peek(fifo: &Kfifo, buf: &mut [u8], mut len: u32) -> u32 {
    let l = fifo.in_.wrapping_sub(fifo.out);
    if len > l {
        len = l;
    }
    kfifo_copy_out(fifo, buf, len, fifo.out);
    len
}

/// __kfifo_out_linear returns (count, tail).
fn kfifo_out_linear(fifo: &Kfifo, n: u32) -> (u32, u32) {
    let size = fifo.mask.wrapping_add(1);
    let off = fifo.out & fifo.mask;
    // min3(n, in - out, size - off)
    (n.min(fifo.in_.wrapping_sub(fifo.out)).min(size - off), off)
}

fn kfifo_out(fifo: &mut Kfifo, buf: &mut [u8], len: u32) -> u32 {
    let len = kfifo_out_peek(fifo, buf, len);
    fifo.out = fifo.out.wrapping_add(len);
    len
}

/// kfifo_copy_from_user: copy_from_user is memcpy returning 0, so the
/// `unlikely(ret)` branches never run; `*copied = len` in element-bytes.
fn kfifo_copy_from_user(fifo: &mut Kfifo, from: &[u8], mut len: u32, mut off: u32) -> (u64, u32) {
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

fn kfifo_from_user(fifo: &mut Kfifo, from: &[u8], mut len: u64) -> (i32, u32) {
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

fn kfifo_copy_to_user(fifo: &Kfifo, to: &mut [u8], mut len: u32, mut off: u32) -> (u64, u32) {
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

fn kfifo_to_user(fifo: &mut Kfifo, to: &mut [u8], mut len: u64) -> (i32, u32) {
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
fn setup_sgl_buf(
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

fn setup_sgl(
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

fn kfifo_dma_in_prepare(
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

fn kfifo_dma_out_prepare(
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

fn kfifo_max_r(len: u32, recsize: u32) -> u32 {
    let max = (1u32 << (recsize << 3)) - 1;
    if len > max { max } else { len }
}

// __KFIFO_PEEK(data, out, mask) == data[(out) & (mask)]
fn kfifo_peek_n(fifo: &Kfifo, recsize: u32) -> u32 {
    let mut l = fifo.data[(fifo.out & fifo.mask) as usize] as u32;
    if recsize.wrapping_sub(1) != 0 {
        l |= (fifo.data[(fifo.out.wrapping_add(1) & fifo.mask) as usize] as u32) << 8;
    }
    l
}

fn kfifo_poke_n(fifo: &mut Kfifo, n: u32, recsize: u32) {
    let mask = fifo.mask;
    let in_ = fifo.in_;
    fifo.data[(in_ & mask) as usize] = n as u8;
    if recsize > 1 {
        fifo.data[(in_.wrapping_add(1) & mask) as usize] = (n >> 8) as u8;
    }
}

fn kfifo_len_r(fifo: &Kfifo, recsize: u32) -> u32 {
    kfifo_peek_n(fifo, recsize)
}

fn kfifo_in_r(fifo: &mut Kfifo, buf: &[u8], len: u32, recsize: u32) -> u32 {
    if len as u64 + recsize as u64 > kfifo_unused(fifo) as u64 {
        return 0;
    }
    kfifo_poke_n(fifo, len, recsize);
    let off = fifo.in_.wrapping_add(recsize);
    kfifo_copy_in(fifo, buf, len, off);
    fifo.in_ = fifo.in_.wrapping_add(len.wrapping_add(recsize));
    len
}

fn kfifo_out_copy_r(fifo: &Kfifo, buf: &mut [u8], mut len: u32, recsize: u32) -> u32 {
    let n = kfifo_peek_n(fifo, recsize);
    if len > n {
        len = n;
    }
    kfifo_copy_out(fifo, buf, len, fifo.out.wrapping_add(recsize));
    len
}

fn kfifo_out_peek_r(fifo: &Kfifo, buf: &mut [u8], len: u32, recsize: u32) -> u32 {
    if fifo.in_ == fifo.out {
        return 0;
    }
    kfifo_out_copy_r(fifo, buf, len, recsize)
}

/// __kfifo_out_linear_r returns (count, tail); the C writes *tail only on
/// the non-empty path — the driver folds tail=0 when empty.
fn kfifo_out_linear_r(fifo: &Kfifo, n: u32, recsize: u32) -> (u32, u32) {
    if fifo.in_ == fifo.out {
        return (0, 0);
    }
    (
        n.min(kfifo_peek_n(fifo, recsize)),
        fifo.out.wrapping_add(recsize),
    )
}

fn kfifo_out_r(fifo: &mut Kfifo, buf: &mut [u8], mut len: u32, recsize: u32) -> u32 {
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

fn kfifo_skip_r(fifo: &mut Kfifo, recsize: u32) {
    let n = kfifo_peek_n(fifo, recsize);
    fifo.out = fifo.out.wrapping_add(n.wrapping_add(recsize));
}

fn kfifo_from_user_r(fifo: &mut Kfifo, from: &[u8], len: u64, recsize: u32) -> (i32, u32) {
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

fn kfifo_to_user_r(fifo: &mut Kfifo, to: &mut [u8], len: u64, recsize: u32) -> (i32, u32) {
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

fn kfifo_dma_in_prepare_r(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    recsize: u32,
    dma: u64,
) -> u32 {
    if nents == 0 {
        process::abort(); // BUG_ON(!nents)
    }
    len = kfifo_max_r(len, recsize);
    if len as u64 + recsize as u64 > kfifo_unused(fifo) as u64 {
        return 0;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.in_.wrapping_add(recsize), dma)
}

fn kfifo_dma_in_finish_r(fifo: &mut Kfifo, mut len: u32, recsize: u32) {
    len = kfifo_max_r(len, recsize);
    kfifo_poke_n(fifo, len, recsize);
    fifo.in_ = fifo.in_.wrapping_add(len.wrapping_add(recsize));
}

fn kfifo_dma_out_prepare_r(
    fifo: &mut Kfifo,
    sgl: &mut [Sg],
    nents: i32,
    mut len: u32,
    recsize: u32,
    dma: u64,
) -> u32 {
    if nents == 0 {
        process::abort(); // BUG_ON(!nents)
    }
    len = kfifo_max_r(len, recsize);
    if len as u64 + recsize as u64 > fifo.in_.wrapping_sub(fifo.out) as u64 {
        return 0;
    }
    setup_sgl(fifo, sgl, nents, len, fifo.out.wrapping_add(recsize), dma)
}

// ---- driver predicates -----------------------------------------------------------

/// rec_fits(): the next record's declared length plus its header fits the
/// occupied space — keeps out <= in (upstream caller contract).
fn rec_fits(f: &Kfifo, recsize: u32) -> bool {
    f.in_ != f.out && kfifo_len_r(f, recsize) + recsize <= f.in_.wrapping_sub(f.out)
}

// =================== __kfifo_* op-sequence driver ===================

fn drive_seq(rig: &mut Rig, f: &mut Kfifo, iobuf: &mut [u8; IOMAX], recsize: u32, nops: u32) {
    let cap = f.mask.wrapping_add(1);

    for _ in 0..nops {
        let r = rig.xs64();
        let t = (r % 100) as u32;
        let big = rig.xs64();
        let mut len = (rig.xs64() % (cap as u64 + 64)) as u32;
        let rlen = len % (cap + 1);
        let n: u32;
        let copied: u32;
        let tail: u32;
        let err: i32;

        if big.is_multiple_of(7) {
            len = len.wrapping_add((big % 66000) as u32);
        }

        if t < 30 {
            // in
            fill_rng(rig, iobuf);
            n = if recsize != 0 {
                kfifo_in_r(f, iobuf, len, recsize)
            } else {
                kfifo_in(f, iobuf, len)
            };
            rig.fold(n as u64);
        } else if t < 52 {
            // out
            if recsize != 0 {
                if rec_fits(f, recsize) {
                    n = kfifo_out_r(f, iobuf, rlen, recsize);
                } else {
                    n = NOSG;
                }
            } else {
                n = kfifo_out(f, iobuf, len);
            }
            rig.fold(n as u64);
            if n != NOSG {
                let h = fnv1a(&iobuf[..n as usize * f.esize as usize]);
                rig.fold(h);
            }
        } else if t < 64 {
            // out_peek
            if recsize != 0 {
                n = kfifo_out_peek_r(f, iobuf, rlen, recsize);
            } else {
                n = kfifo_out_peek(f, iobuf, len);
            }
            rig.fold(n as u64);
            let h = fnv1a(&iobuf[..n as usize * f.esize as usize]);
            rig.fold(h);
        } else if t < 72 {
            // out_linear
            if recsize != 0 {
                let (nn, tt) = kfifo_out_linear_r(f, rlen, recsize);
                n = nn;
                tail = tt;
            } else {
                let (nn, tt) = kfifo_out_linear(f, len);
                n = nn;
                tail = tt;
            }
            rig.fold(n as u64);
            rig.fold(tail as u64);
            if n != 0 {
                let off = tail & f.mask;
                let mut run = n;
                if off + run > cap {
                    run = cap - off;
                }
                let lo = off as usize * f.esize as usize;
                let hi = lo + run as usize * f.esize as usize;
                let h = fnv1a(&f.data[lo..hi]);
                rig.fold(h);
            }
        } else if t < 80 {
            // skip
            if recsize != 0 {
                if rec_fits(f, recsize) {
                    kfifo_skip_r(f, recsize);
                    n = 1;
                } else {
                    n = NOSG;
                }
            } else if rlen <= f.in_.wrapping_sub(f.out) {
                f.out = f.out.wrapping_add(rlen); // kfifo_skip_count
                n = rlen;
            } else {
                n = NOSG;
            }
            rig.fold(n as u64);
        } else if t < 85 {
            // from_user
            fill_rng(rig, iobuf);
            if recsize != 0 {
                let (e, c) = kfifo_from_user_r(f, iobuf, len as u64, recsize);
                err = e;
                copied = c;
            } else {
                let (e, c) = kfifo_from_user(f, iobuf, len as u64);
                err = e;
                copied = c;
            }
            rig.fold(err as u32 as u64);
            rig.fold(copied as u64);
        } else if t < 90 {
            // to_user
            if recsize != 0 {
                if rec_fits(f, recsize) {
                    fill_rng(rig, iobuf);
                    let (e, c) = kfifo_to_user_r(f, iobuf, rlen as u64, recsize);
                    err = e;
                    copied = c;
                } else {
                    err = -1;
                    copied = NOSG;
                }
            } else {
                fill_rng(rig, iobuf);
                let (e, c) = kfifo_to_user(f, iobuf, len as u64);
                err = e;
                copied = c;
            }
            rig.fold(err as u32 as u64);
            rig.fold(copied as u64);
            if copied != NOSG {
                let m = (copied as usize).min(IOMAX);
                let h = fnv1a(&iobuf[..m]);
                rig.fold(h);
            }
        } else if t < 95 {
            // queries
            if recsize != 0 {
                let v = kfifo_len_r(f, recsize);
                rig.fold(v as u64);
                let v = kfifo_max_r(len, recsize);
                rig.fold(v as u64);
            } else {
                rig.fold(f.in_.wrapping_sub(f.out) as u64); // kfifo_len
                rig.fold(cap.wrapping_sub(f.in_.wrapping_sub(f.out)) as u64); // unused
            }
        } else {
            // reset
            if t & 1 != 0 {
                f.out = f.in_; // kfifo_reset_out
                rig.fold(0xdead);
            } else {
                f.in_ = 0; // kfifo_reset
                f.out = 0;
                rig.fold(0xbeef);
            }
        }
        rig.fold(f.in_ as u64);
        rig.fold(f.out as u64);
    }
}

pub fn wrap_kfifo_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    let m = iabs64(mode_i);
    let sp = 3 + (iabs64(sp_i) % 7) as u32;
    let elems = 1u32 << sp;
    let mut esize = 1u32 << (iabs64(esize_i) % 4);
    let recsize = (iabs64(recsize_i) % 3) as u32;
    let nops = (iabs64(nops_i) % 300) as u32;

    let mut rig = Rig::new(seed);
    let mut fbuf = [0u8; BMAX];
    let mut iobuf = [0u8; IOMAX];
    if recsize != 0 {
        esize = 1;
    }
    fill_rng(&mut rig, &mut fbuf);

    let mut initsz = elems * esize;
    if m & 1 != 0 {
        initsz = (elems + elems / 2) * esize + 1; // -> rounddown
    }
    if m & 2 != 0 {
        initsz = esize; // -> -EINVAL
    }
    let mut f = Kfifo::default();
    let ret = kfifo_init(&mut f, &fbuf, initsz, esize);
    rig.fold(ret as u32 as u64); // C folds `(unsigned int)ret`
    rig.fold(f.mask as u64);
    if m & 4 != 0 {
        // counter wraparound: in/out near 2^32
        f.in_ = 0xffffff00u32.wrapping_add((seed & 0xff) as u32);
        f.out = f.in_;
    }
    if m & 8 != 0 {
        // alternate buffer content — fbuf IS fifo->data, so resync the copy
        fill_rng(&mut rig, &mut fbuf);
        f.data = fbuf.to_vec();
    }

    drive_seq(&mut rig, &mut f, &mut iobuf, recsize, nops);

    rig.fold(f.in_ as u64);
    rig.fold(f.out as u64);
    rig.fold(f.mask as u64);
    rig.fold(f.esize as u64);
    let h = fnv1a(&f.data[..BMAX]);
    rig.fold(h);
    rig.h
}

// ===================== DMA scatterlist sequences =====================

fn fold_sg(rig: &mut Rig, sg: &Sg) {
    rig.fold(sg.dma_address);
    rig.fold(sg.dma_length as u64);
    rig.fold(sg.length as u64);
    rig.fold(sg.offset as u64);
}

fn sg_init_table(sgl: &mut [Sg]) {
    for s in sgl.iter_mut() {
        *s = Sg::default();
    }
}

fn drive_dma(
    rig: &mut Rig,
    f: &mut Kfifo,
    iobuf: &mut [u8; IOMAX],
    sgl: &mut [Sg; MAXSG],
    recsize: u32,
    nops: u32,
    mapped: bool,
) {
    let cap = f.mask.wrapping_add(1);

    for _ in 0..nops {
        let r = rig.xs64();
        let t = (r % 100) as u32;
        let big = rig.xs64();
        let mut len = (rig.xs64() % (cap as u64 + 64)) as u32;
        let mut nents = 1 + (rig.xs64() % MAXSG as u64) as i32;
        let n: u32;

        if big.is_multiple_of(7) {
            len = len.wrapping_add((big % 66000) as u32);
        }
        if recsize == 0 {
            nents = (rig.xs64() % (MAXSG as u64 + 1)) as i32;
        }
        let dma = if mapped {
            0x40000000u64 + (big & 0xffffff)
        } else {
            DMA_MAPPING_ERROR
        };

        if t < 40 {
            // dma in: prepare -> fake DMA write -> finish
            let unused = cap.wrapping_sub(f.in_.wrapping_sub(f.out));
            n = if recsize != 0 {
                kfifo_dma_in_prepare_r(f, sgl, nents, len, recsize, dma)
            } else {
                kfifo_dma_in_prepare(f, sgl, nents, len, dma)
            };
            rig.fold(n as u64);
            for sg in sgl.iter().take(n as usize) {
                fold_sg(rig, sg);
                fill_rng(
                    rig,
                    &mut f.data[sg.off as usize..sg.off as usize + sg.length as usize],
                );
            }
            if n != 0 {
                if recsize != 0 {
                    kfifo_dma_in_finish_r(f, len, recsize);
                } else {
                    f.in_ = f.in_.wrapping_add(len.min(unused)); // kfifo_dma_in_finish
                }
            }
            rig.fold(f.in_ as u64);
        } else if t < 75 {
            // dma out
            let avail = f.in_.wrapping_sub(f.out);
            n = if recsize != 0 {
                kfifo_dma_out_prepare_r(f, sgl, nents, len, recsize, dma)
            } else {
                kfifo_dma_out_prepare(f, sgl, nents, len, dma)
            };
            rig.fold(n as u64);
            for sg in sgl.iter().take(n as usize) {
                fold_sg(rig, sg);
                let h = fnv1a(&f.data[sg.off as usize..sg.off as usize + sg.length as usize]);
                rig.fold(h);
            }
            if n != 0 {
                if recsize != 0 {
                    if rec_fits(f, recsize) {
                        kfifo_skip_r(f, recsize);
                        rig.fold(1);
                    } else {
                        rig.fold(NOSG as u64);
                    }
                } else {
                    let l = len.min(avail);
                    f.out = f.out.wrapping_add(l);
                    rig.fold(l as u64);
                }
            }
        } else {
            // plain in/out keep data moving
            let n2 = (rig.xs64() % (cap as u64 + 1)) as u32;
            if t & 1 != 0 {
                fill_rng(rig, &mut iobuf[..n2 as usize]);
                if recsize != 0 {
                    let v = kfifo_in_r(f, iobuf, n2, recsize);
                    rig.fold(v as u64);
                } else {
                    let v = kfifo_in(f, iobuf, n2);
                    rig.fold(v as u64);
                }
            } else if recsize != 0 {
                if rec_fits(f, recsize) {
                    let v = kfifo_out_r(f, iobuf, n2, recsize);
                    rig.fold(v as u64);
                } else {
                    rig.fold(NOSG as u64);
                }
            } else {
                let v = kfifo_out(f, iobuf, n2);
                rig.fold(v as u64);
            }
        }
        rig.fold(f.in_ as u64);
        rig.fold(f.out as u64);
    }
}

pub fn wrap_kfifo_dma_seq(
    seed: u64,
    sp_i: i64,
    esize_i: i64,
    recsize_i: i64,
    nops_i: i64,
    mode_i: i64,
) -> u64 {
    let m = iabs64(mode_i);
    let sp = 3 + (iabs64(sp_i) % 7) as u32;
    let elems = 1u32 << sp;
    let mut esize = 1u32 << (iabs64(esize_i) % 4);
    let recsize = (iabs64(recsize_i) % 3) as u32;
    let nops = (iabs64(nops_i) % 300) as u32;

    let mut rig = Rig::new(seed);
    let mut fbuf = [0u8; BMAX];
    let mut iobuf = [0u8; IOMAX];
    let mut sgl = [Sg::default(); MAXSG];
    if recsize != 0 {
        esize = 1;
    }
    fill_rng(&mut rig, &mut fbuf);
    sg_init_table(&mut sgl);

    let mut f = Kfifo::default();
    let ret = kfifo_init(&mut f, &fbuf, elems * esize, esize);
    rig.fold(ret as u32 as u64);
    rig.fold(f.mask as u64);
    if m & 4 != 0 {
        f.in_ = 0xffffff00u32.wrapping_add((seed & 0xff) as u32);
        f.out = f.in_;
    }

    drive_dma(
        &mut rig,
        &mut f,
        &mut iobuf,
        &mut sgl,
        recsize,
        nops,
        m & 1 == 0,
    );

    rig.fold(f.in_ as u64);
    rig.fold(f.out as u64);
    rig.fold(f.mask as u64);
    rig.fold(f.esize as u64);
    let h = fnv1a(&f.data[..BMAX]);
    rig.fold(h);
    rig.h
}

// ================= alloc/free lifecycle + mini sequence =============

pub fn wrap_kfifo_alloc(seed: u64, size_i: i64, esize_i: i64, nops_i: i64, mode_i: i64) -> u64 {
    let m = iabs64(mode_i);
    let size = (iabs64(size_i) % 600) as u32;
    let mut esize = 1u32 << (iabs64(esize_i) % 4);
    let recsize = (m % 3) as u32;

    let mut rig = Rig::new(seed);
    let mut iobuf = [0u8; IOMAX];
    if recsize != 0 {
        esize = 1;
    }
    rig.fold(size as u64);
    rig.fold(esize as u64);
    let mut f = Kfifo::default();
    let ret = kfifo_alloc_node(&mut f, size, esize, GFP_KERNEL, NUMA_NO_NODE);
    rig.fold(ret as u32 as u64);
    rig.fold(f.mask as u64);
    rig.fold(f.esize as u64);
    rig.fold(f.has_data as u64);
    if ret == 0 {
        // malloc'd content is indeterminate — seed it
        let len = f.data.len();
        fill_rng(&mut rig, &mut f.data[..len]);
        drive_seq(
            &mut rig,
            &mut f,
            &mut iobuf,
            recsize,
            (iabs64(nops_i) % 60) as u32,
        );
        let h = fnv1a(&f.data[..]);
        rig.fold(h);
        kfifo_free(&mut f);
        rig.fold(f.in_ as u64);
        rig.fold(f.out as u64);
        rig.fold(f.mask as u64);
        rig.fold(f.esize as u64);
        rig.fold(f.has_data as u64);
    }
    rig.h
}

// ============== typed kfifo.h macro-layer sequence ===================

/// A typed fifo: the `Kfifo` state plus the compile-time recsize the C
/// macros read through `sizeof(*fifo->rectype)` (0/1/2).
struct Typed {
    k: Kfifo,
    recsize: u32,
}

impl Typed {
    fn len(&self) -> u32 {
        self.k.in_.wrapping_sub(self.k.out)
    }
    fn size(&self) -> u32 {
        self.k.mask.wrapping_add(1)
    }
    fn is_empty(&self) -> bool {
        self.k.in_ == self.k.out
    }
    fn is_full(&self) -> bool {
        self.len() > self.k.mask
    }
    /// kfifo_avail: size - len, max_r-clamped minus the record header.
    fn avail(&self) -> u32 {
        let avail = self.size().wrapping_sub(self.len());
        if self.recsize != 0 {
            if avail <= self.recsize {
                0
            } else {
                kfifo_max_r(avail - self.recsize, self.recsize)
            }
        } else {
            avail
        }
    }
    /// kfifo_peek_len: recsize -> record length; else len * sizeof(*type).
    fn peek_len(&self) -> u32 {
        if self.recsize == 0 {
            self.len() // * 1 (element is unsigned char)
        } else {
            kfifo_len_r(&self.k, self.recsize)
        }
    }

    /// kfifo_put: single element in. Returns the element count (0/1).
    fn put(&mut self, v: u8) -> u32 {
        if self.recsize != 0 {
            kfifo_in_r(&mut self.k, &[v], 1, self.recsize)
        } else {
            if self.is_full() {
                return 0;
            }
            // (__is_kfifo_ptr ? (type*)data : buf)[in & mask] = v — same byte
            let idx = (self.k.in_ & self.k.mask) as usize;
            self.k.data[idx] = v;
            // smp_wmb()
            self.k.in_ = self.k.in_.wrapping_add(1);
            1
        }
    }

    /// kfifo_get: single element out. Returns (count, value) with v=0 like
    /// the driver's initialized `unsigned char v` when nothing is read.
    fn get(&mut self) -> (u32, u8) {
        if self.recsize != 0 {
            let mut b = [0u8; 1];
            let r = kfifo_out_r(&mut self.k, &mut b, 1, self.recsize);
            (r, b[0])
        } else {
            if self.is_empty() {
                return (0, 0);
            }
            let v = self.k.data[(self.k.out & self.k.mask) as usize];
            // smp_wmb()
            self.k.out = self.k.out.wrapping_add(1);
            (1, v)
        }
    }

    /// kfifo_peek: single element peek; v=0 when nothing is read.
    fn peek(&self) -> (u32, u8) {
        if self.recsize != 0 {
            let mut b = [0u8; 1];
            let r = kfifo_out_peek_r(&self.k, &mut b, 1, self.recsize);
            (r, b[0])
        } else {
            if self.is_empty() {
                return (0, 0);
            }
            (1, self.k.data[(self.k.out & self.k.mask) as usize])
        }
    }

    /// kfifo_in dispatch on recsize.
    fn r#in(&mut self, buf: &[u8], n: u32) -> u32 {
        if self.recsize != 0 {
            kfifo_in_r(&mut self.k, buf, n, self.recsize)
        } else {
            kfifo_in(&mut self.k, buf, n)
        }
    }

    /// kfifo_out dispatch on recsize.
    fn out(&mut self, buf: &mut [u8], n: u32) -> u32 {
        if self.recsize != 0 {
            kfifo_out_r(&mut self.k, buf, n, self.recsize)
        } else {
            kfifo_out(&mut self.k, buf, n)
        }
    }

    /// kfifo_out_peek dispatch on recsize.
    fn out_peek(&self, buf: &mut [u8], n: u32) -> u32 {
        if self.recsize != 0 {
            kfifo_out_peek_r(&self.k, buf, n, self.recsize)
        } else {
            kfifo_out_peek(&self.k, buf, n)
        }
    }

    /// kfifo_skip_count: recsize -> skip one record; else out += count.
    fn skip_count(&mut self, count: u32) {
        if self.recsize != 0 {
            kfifo_skip_r(&mut self.k, self.recsize);
        } else {
            self.k.out = self.k.out.wrapping_add(count);
        }
    }

    /// kfifo_dma_in_finish(len): recsize -> finish_r; else in += len/esize.
    fn dma_in_finish(&mut self, len: u32) {
        if self.recsize != 0 {
            kfifo_dma_in_finish_r(&mut self.k, len, self.recsize);
        } else {
            self.k.in_ = self.k.in_.wrapping_add(len); // / sizeof(*type) == 1
        }
    }
}

/// REC_FITS_T(): kfifo_peek_len + kfifo_recsize <= kfifo_len on non-empty.
fn rec_fits_t(tp: &Typed) -> bool {
    !tp.is_empty() && tp.peek_len() + tp.recsize <= tp.len()
}

/// The C TYPED_RUN macro instantiated once — the C expands it per fifo
/// object, the port shares one body since the dispatch is on `tp.recsize`.
fn typed_run(rig: &mut Rig, tp: &mut Typed, iobuf: &mut [u8; IOMAX], tnops: u32) {
    let cap = tp.k.mask.wrapping_add(1);

    for _ in 0..tnops {
        let r = rig.xs64();
        let t = (r % 100) as u32;
        let n = (rig.xs64() % (cap as u64 + 64)) as u32;
        let rn = n % (cap + 1);
        let want = if tp.recsize != 0 { rn } else { n };

        if t < 28 {
            fill_rng(rig, &mut iobuf[..want as usize]);
            let v = tp.r#in(iobuf, want);
            rig.fold(v as u64);
        } else if t < 46 {
            if tp.recsize == 0 || rec_fits_t(tp) {
                let n2 = tp.out(iobuf, want);
                rig.fold(n2 as u64);
                let h = fnv1a(&iobuf[..n2 as usize]);
                rig.fold(h);
            } else {
                rig.fold(NOSG as u64);
            }
        } else if t < 54 {
            let v = rig.xs64() as u8;
            let r2 = tp.put(v);
            rig.fold(r2 as u64);
        } else if t < 62 {
            if tp.recsize == 0 || rec_fits_t(tp) {
                let (r2, v) = tp.get();
                rig.fold(r2 as u64);
                rig.fold(v as u64);
            } else {
                rig.fold(NOSG as u64);
            }
        } else if t < 70 {
            let (r2, v) = tp.peek();
            rig.fold(r2 as u64);
            rig.fold(v as u64);
        } else if t < 78 {
            if tp.recsize == 0 || rec_fits_t(tp) {
                let n2 = tp.out_peek(iobuf, want);
                rig.fold(n2 as u64);
                let h = fnv1a(&iobuf[..n2 as usize]);
                rig.fold(h);
            } else {
                let v = tp.peek_len();
                rig.fold(v as u64);
            }
        } else if t < 86 {
            let c = rn % 4;
            if if tp.recsize != 0 {
                rec_fits_t(tp)
            } else {
                c <= tp.len()
            } {
                tp.skip_count(c);
                rig.fold(c as u64);
            } else {
                rig.fold(NOSG as u64);
            }
        } else if t < 91 {
            let dn = rn % 8;
            if dn as u64 + tp.recsize as u64 <= tp.size().wrapping_sub(tp.len()) as u64 {
                tp.dma_in_finish(dn);
                rig.fold(dn as u64);
            } else {
                rig.fold(NOSG as u64);
            }
        } else if t < 95 {
            if if tp.recsize != 0 {
                rec_fits_t(tp)
            } else {
                rn <= tp.len()
            } {
                // kfifo_dma_out_finish -> kfifo_skip_count(rn / 1)
                tp.skip_count(rn);
                rig.fold(rn as u64);
            } else {
                rig.fold(NOSG as u64);
            }
        } else if t < 98 {
            fill_rng(rig, &mut iobuf[..4]);
            // kfifo_in_spinlocked_noirqsave: spinlock is a stub
            let v = tp.r#in(iobuf, rn % 4);
            rig.fold(v as u64);
            // kfifo_is_empty_spinlocked
            rig.fold(tp.is_empty() as u64);
        } else {
            tp.k.in_ = 0; // kfifo_reset
            tp.k.out = 0;
            rig.fold(0x7e57);
        }
        rig.fold(tp.len() as u64);
        rig.fold(tp.avail() as u64);
        rig.fold(tp.is_empty() as u64);
        rig.fold(tp.is_full() as u64);
        rig.fold(tp.peek_len() as u64);
    }
    rig.fold(tp.size() as u64);
    rig.fold(tp.k.esize as u64);
    rig.fold(tp.recsize as u64);
    rig.fold((tp.k.mask != 0) as u64); // kfifo_initialized
    let len = tp.k.mask.wrapping_add(1) as usize * tp.k.esize as usize;
    let h = fnv1a(&tp.k.data[..len]);
    rig.fold(h);
}

pub fn wrap_kfifo_typed(seed: u64, sp_i: i64, nops_i: i64, mode_i: i64) -> u64 {
    let m = iabs64(mode_i);
    let sp = 3 + (iabs64(sp_i) % 7) as u32;
    let elems = 1u32 << sp;
    let tnops = (iabs64(nops_i) % 300) as u32;
    let kind = (m % 4) as u32;
    let mut initsz = elems;

    let mut rig = Rig::new(seed);
    let mut fbuf = [0u8; BMAX];
    let mut iobuf = [0u8; IOMAX];
    fill_rng(&mut rig, &mut fbuf);
    if m & 8 != 0 {
        initsz = elems + elems / 2 + 1; // non-pow2: rounds down
    }

    let recsize = match kind {
        1 => 1,
        2 => 2,
        _ => 0,
    };
    // kind 3 is the in-place fifo: kfifo_init fails -EINVAL on it, then
    // INIT_KFIFO sets it up; kinds 0-2 are the ptr fifos via kfifo_init.
    let mut tp = if kind == 3 {
        rig.fold((-EINVAL) as i64 as u64); // C folds the int return
        Typed {
            k: Kfifo {
                in_: 0,
                out: 0,
                mask: 63,
                esize: 1,
                data: vec![0u8; 64], // itf.buf, zero-init static
                has_data: true,
            },
            recsize: 0,
        }
    } else {
        let mut k = Kfifo::default();
        let ret = kfifo_init(&mut k, &fbuf, initsz, 1);
        rig.fold(ret as i64 as u64); // C folds the int return
        Typed { k, recsize }
    };
    if m & 4 != 0 {
        tp.k.in_ = 0xffffff00u32.wrapping_add((seed & 0xff) as u32);
        tp.k.out = tp.k.in_;
    }
    typed_run(&mut rig, &mut tp, &mut iobuf, tnops);
    rig.h
}
