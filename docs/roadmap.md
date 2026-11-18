# Roadmap: a Linux-compatible kernel in Rust

Goal: a drop-in replacement for the Linux kernel — same syscall ABI, same
behavior — built unit by unit, each verified against the original C code's
recorded behavior before adoption.

## Method (per unit)

1. Extract the C unit into `ports/<unit>/` with the smallest shim that makes it
   compile as a userspace shared library (`tools/lib` and `tools/testing`
   provide pre-shimmed copies of many kernel data structures).
2. `rust-anything bridge <harness-dir> <unit>` — generates a Rust crate with
   the same `extern "C"` API.
3. `rust-anything record <crate>` — runs the original C on edge cases, saves
   `cases.jsonl`. Add real-world inputs with `--inputs` when needed.
4. Write `src/port.rs`. Iterate `build`/`diff` until every recorded case
   matches. `gate` must pass: no `unwrap`/`panic` where the original returns
   errors, no `unsafe` without `// SAFETY:`, fmt+clippy clean.
5. Adopt the verified logic into `kernel/src/` adapted to `no_std`. The
   verification crate keeps the original API forever as the regression suite.
6. Stop, report, get approval for the next unit (rust-anything's rule).

## Waves

### Wave 0 — freestanding algorithms (now)

`lib/` + `crypto/` + `tools/lib`. Everything uses these; they use nothing.
rust-anything's record/diff works almost directly — the strongest possible
verification. ~680 files, ~380k lines.

First units (userspace-compilable today): `tools/lib/rbtree.c`,
`tools/lib/bitmap.c`, `tools/lib/string.c`, `tools/lib/find_bit.c`,
`lib/sort.c`, `lib/list_sort.c`, `lib/crc*.c`, `crypto/sha256_generic.c`,
`crypto/xxhash_generic.c`, `lib/siphash.c`, `lib/kfifo.c`.

### Wave 1 — kernel data structures (test harnesses exist)

xarray/idr/radix-tree/maple-tree, rhashtable, percpu counter logic, llist,
kfifo ring, printk ringbuffer, seq_buf. `tools/testing/{radix-tree,maple,vma,bitmap}`
already builds them in userspace — record against those harnesses.
~100 units, most of `lib/` + key `mm/` internals.

### Wave 2 — core services (custom shims needed)

Timer wheels, workqueues, sched policy math (deadline calc, fair scheduling
decisions), slab allocator internals, buddy allocator, waitqueues, futex hash
logic. Each needs a hand-written userspace shim exposing the unit's contract;
record/diff still applies to the algorithmic core. ~2k files (`kernel/`+`mm/`).

### Wave 3 — kernel core & ABI (verification shifts to differential testing)

Entry/exit, interrupts, context switch, page tables, syscall dispatch, VFS,
ELF loader, signals. Not unit-recordable — verify by booting the same
initramfs workload under both kernels and diffing observable behavior
(serial log, syscall traces via a test suite like LTP). ~4k files.

### Wave 4 — drivers & stacks

Per-device/conformance. Start with virtio (mmio/blk/net/console — QEMU gives
us free test hardware), then storage/display/input. GPU drivers are ~5M LOC of
the tree and last. `net/`+`fs/` per-protocol, per-filesystem. ~25k files.

### Wave 5 — drop-in parity

Full syscall surface vs `man-pages`/kernel selftests + LTP, `/proc`+`/sys`
compat, modules vs fixed ABI decision, userspace distro boot.

## Notes

- Reference designs worth studying: **Asterinas** (Rust framekernel, already
  Linux-ABI compatible), **Maestro**, and upstream **Rust for Linux**
  (`rust/` in the C tree — abstractions we can mirror).
- The kernel skeleton boots UEFI today; when it grows past a single binary,
  switch to a Limine-booted ELF kernel + initramfs (needed for wave-3
  differential testing anyway).
- `unsafe` policy: allowed only at hardware boundaries (port I/O, MMIO, paging,
  context switch), always with `// SAFETY:` comments — same bar `gate` enforces.
