//! klib: kernel library — the linux-rust counterpart of the C kernel's `lib/`.
//!
//! Every function here was ported from a C unit under `ports/` and proven
//! behavior-identical against the original's recorded cases before adoption
//! (`rust-anything record`/`diff`/`gate`). Comments cite the C source each
//! port was verified against.

#![no_std]
#![forbid(unsafe_code)]

pub mod bcd;
pub mod crc;
pub mod hash;
pub mod list_sort;
pub mod math;
pub mod sort;
