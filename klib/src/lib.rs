//! klib: kernel library — the linux-rust counterpart of the C kernel's `lib/`.
//!
//! Every function here was ported from a C unit under `ports/` and proven
//! behavior-identical against the original's recorded cases before adoption
//! (`rust-anything record`/`diff`/`gate`). Comments cite the C source each
//! port was verified against.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod bcd;
pub mod bitmap;
pub mod bitops;
pub mod crc;
pub mod fmt;
pub mod hash;
pub mod idr;
pub mod kfifo;
pub mod list_sort;
pub mod llist;
pub mod maple_tree;
pub mod math;
pub mod percpu_counter;
pub mod prb;
pub mod rbtree;
pub mod radix_tree;
pub mod rhashtable;
pub mod seqbuf;
pub mod sha;
pub mod sort;
pub mod string;
pub mod xarray;
