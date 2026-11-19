//! Safe Rust models of upstream `lib/generic-radix-tree.c` and
//! `lib/interval_tree.c` for the lib-genradix-interval differential
//! harness (`wrappers-rs` consumes this crate).
//!
//! * [`genradix`] — the page-granular sparse array of
//!   `include/linux/generic-radix-tree.h` + `lib/generic-radix-tree.c`.
//!   C nodes live in a caller-owned arena and C pointers become `usize`
//!   indices; a leaf slot is `(node, byte offset)` instead of a typed
//!   pointer, so the model is object-size-generic like the C macro layer.
//!
//! * [`interval_tree`] — the `INTERVAL_TREE_DEFINE` instantiation from
//!   `lib/interval_tree.c`, built on the already-verified
//!   [`klib::rbtree`] index-arena port. The macro template's generic
//!   parameters (node struct, rb field, endpoint accessors, augmented
//!   field) map onto parallel `Vec<u64>` arrays in [`ItData`].
#![forbid(unsafe_code)]

pub mod genradix;
pub mod interval_tree;
