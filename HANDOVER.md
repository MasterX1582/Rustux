# Handover — Rustux port project (formerly linux-rust)

**Goal:** rebuild the Linux kernel in Rust as a drop-in replacement — not Rust-in-Linux support. Strategy: port `lib/` and subsystem units one at a time with differential verification (`rust-anything` record → diff → gate), adopt verified ports into `klib`, prove each under QEMU boot, audit, commit. Long tail leads to mm/sched/fs/net/drivers parity.

**Last commit:** `3b70842 kernel: ACPI + I/O APIC — real device IRQ routing` — MADT parsed (LAPIC/IOAPIC/ISO entries), I/O APIC redirection programmed, PIT ch0 IRQ0->GSI2 delivers 50 real device IRQs through our IDT on vec 0xf1. On top of Rustux rename (`d60118f`), kmalloc/slab heap (`8ab0e02`), LAPIC timer, GDT/IDT (`f26b2e4`), page tables + buddy (`e8946f8`/`4a48db4`). Suite: **327 klib tests green**, QEMU boot PASS (exit 33), clippy has 14 pre-existing bitmap warnings only. Repo: `https://github.com/MasterX1582/Rustux`.

**Naming:** the project/repo brand is Rustux; on-disk directory names may still be `linux-rust` (Windows checkout `C:\Users\Davy\Documents\GitHub\linux-rust`, audit artifact dir `security-audit-skill/linux-rust` — rename at leisure; git does not care). The kernel's eventual runtime identity (`uname` etc.) is a separate Linux-compatibility decision, independent of the project name.

## Environments

| host | path | role |
|------|------|------|
| Primary host | `$REPO` (this checkout, dir still named `linux-rust`) | primary repo, port jobs run here |
| Primary host | `$LINUX_REF` | Linux reference tree (torvalds clone or scoped subset); `check-sources.sh` reads `${LINUX_REF:-$HOME/linux-ref}` |
| Linux dev VM | `~/rustux` | git checkout — sync via `git pull` (origin = GitHub `MasterX1582/Rustux`, public); native build+test+boot host |
| Linux dev VM | `~/linux-ref` | scoped ref tree (lib/, include/, arch/x86/include/, Kbuild/Kconfig/Makefile) — the `$LINUX_REF` default |
| Linux dev VM | `~/recv/recv.py` | HTTP PUT receiver on `:9876` — `curl -T file http://<VM_IP>:9876/name` |
| Linux dev VM | `~/rustux/test/zorin/fixtures/` | Zorin 18.1 ISO fixtures: vmlinuz 6.17, initrd (4-layer: AMD ucode → Intel ucode → modules/firmware cpio → gzip'd casper userspace), grub.cfg; dissection docs in `test/zorin/README.md` |

VM toolchain: gcc 13.3, clang 18.1, QEMU 8.2.2+OVMF, mtools, cargo 1.100 (`~/.cargo/bin`), rust-anything 2.0.0, passwordless sudo. Verify on VM: `cargo test -p klib` and `make run` (boot → exit 33 = PASS). NOTE: cargo/rustup need `export PATH=$HOME/.cargo/bin:$PATH`.

## Port pipeline (per unit)

`ports/<pkg>/` holds: upstream C copy + `check-sources.sh` (byte-identical verify vs ref tree), `selftest.c` (userspace harness), `wrappers.c` (FFI scalars), `rust/wrappers-rs/` (port crate, `#![forbid(unsafe_code)]` preferred), `cases.jsonl` (recorded corpus), `verify-all.sh`. Reference exemplars: `ports/lib-siphash/` (newest, clean layout) and `ports/lib-idr-radix/`.

Gate bar: `rust-anything diff` 100% case match + `rust-anything gate` all checks. Then serial adoption: generator/extractor script → `klib/src/<mod>.rs` + `<mod>_tests.inc` → register in `klib/src/lib.rs` → boot proof in `kernel/src/main.rs` → `cargo test -p klib` + `make run` → scoped security-audit run → commit.

## Port status (as of 2159f67)

All four wave-2 parallel ports **landed**: siphash (`0149328`, klib::hash already identical), genradix+interval_tree (`eb55a9b`, 899/899), assoc_array (`6097eab`, 1383/1383), strtox-helpers (`a4d19df`, 11667/11667, merge-adopted). Port packages stay in `ports/` as regression fixtures.

Adopted so far (in klib): math/div64/gcd/int-pow/lcm/primes, bcd, bitops, bitmap, crc, hash(+siphash), rbtree, string, sha1/256/512+hmac, kfifo, llist, seq_buf, fmt, rhashtable, xarray, idr, radix_tree, maple_tree, percpu_counter, prb (printk ringbuf), genradix, interval_tree, assoc_array, kstrtox, string_helpers, font. **327 tests.**

## Kernel runtime status (as of 2159f67)

- Consoles: COM1 serial (`kernel/src/uart.rs`) + GOP framebuffer text console (`kernel/src/fbcon.rs`, `klib::font` = generated `lib/fonts/font_8x16.c` glyphs via `extract_font.py`). `Console` in `main.rs` fans every write to both. Committed `e7486d7`, audit run-21 clean.
- Boot logo (`2159f67`, audit run-25 clean; new Rustux shield + bg-agnostic fill pending run-29): `gen_logo.ps1` learns the background color range from edge pixels then 4-connected flood-fills it to alpha 0 → transparent `docs/logo.png` + 96×143 RGB const `kernel/src/logo_data.rs`; `fbcon::splash()` draws it centered at top and a `top` row bound makes `scroll()` move only the text region (fbcon-style pinned logo). Verified by QEMU monitor `screendump` on the VM.
- `ExitBootServices` claimed (`ea5cfcf`, audit run-22 clean): kernel leaves UEFI boot services, takes the final memory map, prints usable/reclaimable/largest-region summary. Post-exit output still reaches both consoles (COM1 = port I/O, fb = GOP BAR memory; the uefi crate's `MemoryMapOwned` drop is post-EBS-safe).
- Own page tables + buddy allocator (`e8946f8`, audit run-23 clean; buddy upgrade audit run-27 clean): `kernel/src/mm.rs` builds a 4-level identity map 0..max(4 GiB, map top) with 2 MiB pages (multi-PDPT layout — QEMU's ~1 TiB PCI-hole descriptor needs >512 entries), switches CR3, and pools every conventional frame minus the carve into a Linux-style buddy allocator (`free_area[order]` lists, split-on-alloc, merge-on-free via address-buddy, largest-aligned seeding). Boot proof: order-0 + order-2 contiguous alloc/write/free, count conserved.
- kmalloc/slab heap (audit run-28 clean): `kernel/src/kalloc.rs` — size-class caches 32..4096 carved from buddy blocks, direct blocks for larger requests, 16-byte per-allocation header so kfree needs no size hint (the struct-page role), irqsave/irqrestore on every op. Boot proof: freelist reuse, zalloc zero-fill, 12 KiB direct-block round-trip.
- Own GDT + IDT (`f26b2e4`, audit run-24 clean): `kernel/src/cpu.rs` flat 64-bit GDT (KCODE=0x08, KDATA=0x10, CS reload via `lretq`), 256-entry IDT, naked asm stubs → shared dispatcher → Rust reporter; installed AFTER ExitBootServices (firmware reloads GDTR/IDTR and raises timer IRQs internally — installing earlier triple-faults). UEFI target is Windows x64 ABI: stub args go in rcx/rdx, not rdi/rsi. #UD proof catches, advances rip, resumes. Vector 8 (#DF) runs on a dedicated IST1 stack via a TSS in GDT slots 3-4 (`ltr`) — a #DF on a corrupt stack can now report instead of triple-faulting (audit run-27).
- LAPIC periodic timer (audit run-26 clean): `kernel/src/apic.rs` masks the legacy PIC, enables the LAPIC (IA32_APIC_BASE; xAPIC MMIO + x2APIC MSR paths — x2APIC coded but unexercised on QEMU), calibrates the timer against PIT ch2 (~63 MHz counter), arms 100 Hz periodic on vec 0xf0 (spurious 0xff). IRQs route through dedicated IDT stubs; `dispatch` saves/restores every caller-saved reg (incl. the interrupted rcx — stubs are pure push-immediate now; an earlier `mov rcx, rsp` clobbered it and made the tick wait read a garbage bound). Boot proof: `sti` + hlt-wait delivers exactly 50 ticks, #UD resume intact.
- ACPI + I/O APIC device IRQs (audit run-30, standard profile — firmware-data trust boundary): `kernel/src/acpi.rs` grabs RSDP from the UEFI config table pre-EBS, checksum-verifies RSDP/XSDT/MADT, walks MADT records (min-length + end-bounds per record). `apic.rs` gained IOREGSEL/IOWIN MMIO + `ioapic_route` (page-aligned high-MMIO addr + GSI-within-redirs guards; mask->program->unmask; ISO flags -> polarity/trigger bits) + `arm_pit` (ch0 mode-2 ~100 Hz). Boot proof: QEMU MADT shows IRQ0->GSI2, route programmed on vec 0xf1, exactly 50 PIT IRQs delivered via I/O APIC alongside 50 LAPIC timer ticks. x2APIC >8-bit APIC-ids for redir dest noted needs_validation.
- Visible demo on VM: `~/boot-demo.sh` (GTK window + serial; launch with `DISPLAY=:0 XAUTHORITY=/run/user/1000/.mutter-Xwaylandauth.*` if no session env). Headless pass/fail: `make run` (exit 33). VM MCP (LinuxMCP-VM-3) provides shell/PTY/fs/desktop control when sshd is down.

## Conventions that matter

- **Generators are authoritative.** `extract_*.py` at repo root regenerate `klib/src/*.rs` — hardening fixes go in the extractor, never edit generated files directly. `.replace()` calls are assert-guarded (drift can't silently no-op).
- **Adversarial hardening patterns** (apply to new ports): `wrapping_*` where C wraps; arena-index routing to scratch nodes (`nat`/`nmat`, `nidx`/`spare`) for crafted pointers; iteration caps (`MAS_ITER_LIMIT`-style `__lim`) on pointer-chasing/retry loops; clamp table/slot/mark/type indexes; bounds on copy/zero helpers; widen-then-multiply for `batch*ncpu`-style arithmetic.
- **Security mandate (standing rule):** every executable-code change → scoped `security-audit` run (quick profile; standard for crypto/auth/etc.), artifacts to `<AUDIT_DIR>/linux-rust/run-N\` (outside the target repo) (next free N; run-28 closed — runs 18-28 covered the wave-2 adoptions, fbcon, EBS, mm, cpu, boot logo, LAPIC timer, IST + buddy, kmalloc). Fix confirmed findings or record as needs_validation before commit.
- **Commits:** signed `Generated with [Devin]` + co-author trailer. One commit per adoption wave.
- **Delegation model:** port jobs run as parallel background subagents, one `ports/` dir each, no shared-file edits. Adoption/wiring/boot/audit/commit stay serial. Rate limits killed 3 of 4 jobs this round — resume by re-dispatching with the resume-step above; their on-disk progress survives.

## Next steps

1. Kernel runtime: GDT/IDT (`f26b2e4`), LAPIC timer, IST #DF, buddy, kmalloc, ACPI+MADT parse, I/O APIC routing (50 PIT device IRQs) DONE. Next: early-drain printk into `klib::prb`, then real drivers on the IOAPIC path (PS/2 kbd IRQ1, AHCI/e1000), MSI via MADT-independent PCI cfg.
2. Interrupt/timer + early printk drain into `klib::prb`, panic path → both consoles.
3. Queue next lib units per `docs/roadmap.md` (Wave-1 remainder or Wave-2 core services: timer wheels, workqueues, sched math — needs userspace shims).
4. Zorin acceptance ladder (see `test/zorin/README.md`): M0 fixtures done → next rungs are boot our kernel under the real grub cmdline, initrd cpio/gzip readers, eventually casper init (M6 desktop = drop-in milestone).

## Warnings

- Native Windows build of klib has a pre-existing `c_ulong` u32/u64 platform mismatch (`slow_is_prime_number`); verify on WSL/Linux only.
- DrvFs is slow; large tars of the ref tree stall — use scoped tarballs or git bundles (worked well).
- VM apt mirror must be `mirrors.kernel.org` (default `be.archive.ubuntu.com` is blocked by egress allowlist; github/crates.io/kernel.org/debian/qemu.org all reachable).
- A stray untracked `nul` file sits in the repo root (Windows artifact) — ignore/delete.
