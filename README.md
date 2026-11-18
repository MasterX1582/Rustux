# linux-rust

An incremental reimplementation of the Linux kernel in Rust, verified against
the original C kernel's recorded behavior.

The C kernel source is the reference and stays untouched in a sibling checkout
(`../linux`). Each unit is ported one at a time with
[rust-anything](https://github.com/MasterX1582/rust-anything): the original's
behavior is recorded to `cases.jsonl`, the Rust port must match every recorded
case (`diff`) and pass `gate` before it is adopted.

## Layout

- `kernel/` — the kernel itself; a `#![no_std]` UEFI application today
- `ports/` — per-unit work areas: extracted C harness + the verified Rust crate
- `docs/` — `kernel-map.md` (subsystem inventory) and `roadmap.md` (port order)

## Build & boot test (WSL)

```sh
rustup target add x86_64-unknown-uefi
sudo apt install qemu-system-x86 ovmf
make run        # boots under OVMF in QEMU, PASS when serial shows the banner
```
