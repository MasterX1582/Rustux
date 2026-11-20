# linux-rust

An incremental reimplementation of the Linux kernel in Rust, verified against
the original C kernel's recorded behavior.

The C kernel source is the reference and stays untouched in a separate tree
(`$LINUX_REF`, default `~/linux-ref`). Each unit is ported one at a time with
[rust-anything](https://github.com/MasterX1582/rust-anything): the original's
behavior is recorded to `cases.jsonl`, the Rust port must match every recorded
case (`diff`) and pass `gate` before it is adopted.

## Layout

- `kernel/` — the kernel itself; a `#![no_std]` UEFI application today
- `ports/` — per-unit work areas: extracted C harness + the verified Rust crate
- `docs/` — `kernel-map.md` (subsystem inventory) and `roadmap.md` (port order)
- `test/zorin/` — real-OS acceptance fixtures + ladder (see `test/zorin/README.md`)

## Build & test (Ubuntu 24.04 / Zorin OS 18.1)

Verified on Zorin OS 18.1 (Ubuntu 24.04 base) with gcc 13.3, QEMU 8.2.2 + OVMF,
cargo 1.100 via rustup:

```sh
rustup target add x86_64-unknown-uefi
sudo apt install qemu-system-x86 ovmf mtools gcc make

cargo test -p klib   # 327 unit tests for the ported klib modules
make run             # builds kernel.efi -> esp.img, boots under OVMF in QEMU;
                     # PASS when the kernel exits QEMU with status 33
```

The kernel prints its boot proofs to COM1 and to a GOP framebuffer text
console (VGA 8x16 font from `lib/fonts/font_8x16.c`), then exits UEFI boot
services and reports the firmware memory map — a `-display gtk` QEMU window
shows the kernel's own console output.
