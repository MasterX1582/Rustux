# Handover — Rustux port project (formerly linux-rust)

**Goal:** rebuild the Linux kernel in Rust as a drop-in replacement — not Rust-in-Linux support. Strategy: port `lib/` and subsystem units one at a time with differential verification (`rust-anything` record → diff → gate), adopt verified ports into `klib`, prove each under QEMU boot, audit, commit. Long tail leads to mm/sched/fs/net/drivers parity.

**Last commit:** `6b5e9f6` sched: preemptive round-robin — UCtx context switch via timer IRQ. Prior: `8541d13` execve, `d55104a` its docs.

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
- IPv4/ICMP ping (audit run-39 clean, standard — wire parser): `kernel/src/net.rs` — csum16, `parse_ipv4` (version/IHL/total_len bounds, checksum fold, fragments dropped — no reassembly queue by design), `parse_icmp` (checksum), `build_echo_request`. ARP reply seeds GW_MAC/GW_IP atomics (ARP cache entry #1). `ping_selftest` TXes a checksummed echo request and validates the reply end-to-end (EOP/len gate, proto/src/dst/ty/code/id/seq). Boot: `ping reply 10.0.2.2 seq=1 id=0x7278 64B icmp_rest=11B`.
- IPv8 two-guest exchange (audit run-40 clean, standard — wire parser; `f5f0c71`+`b669b15`): `net.rs` gains `parse_ipv8` (draft-thain-ipv8 header — v8, IHL>=7, Src/Dst RouteNumber + local addrs, checksum fold, frag reject) + `build_ipv8_echo` on EtherType 0x8080. `e1000::ipv8_demo` picks role by MAC last byte (:01 initiator broadcasts — ARP8 has no spec — and retries; :02 responder learns sender MAC from the frame and unicasts reply; else skip). PROOF on QEMU socket netdev pair: B `echo req from rn0x1 10.0.0.1 seq=1 -> replied`, A `echo reply rn0x2 10.0.0.2 seq=1 (71B native 0x8080)`. Side effect: exposed+fixed a REAL latent bug — RX repost wrote RDT=d+1 collapsing [RDH,RDT) to empty after 1 frame (ring starvation; invisible to all earlier <=2-frame tests); repost is now RDT=d (consumed slot = new single-desc gap). ARP8/ICMPv8/DHCP8 companion drafts were never submitted — don't invent their semantics. GPL-2.0-only LICENSE added.
- IPv6 link-local + NDP + ICMPv6 echo (audit run-41 clean, standard; `03f3db5`+`c4bfb5f`): `net.rs` gains parse_ipv6 (fixed 40B header, payload_len bounds, NO extension headers — consumers require NH_ICMPV6 directly), ll_addr EUI-64, solicited-node mcast, csum6 pseudo-header (sum-of-complements == complement-of-total; residual 0|0xffff both valid via csum6_ok), build_ns/na/echo6. `e1000::ipv6_selftest` same MAC role gate: initiator must resolve the peer via a real NS->NA exchange (hop==255, solicited dst, target==us) before echoing. PROOF on the socket pair: B `NS for us -> sent NA`, A `NA received`, B `echo req seq=7 -> replied`, A `echo reply seq=7 (79B native 0x86dd)`. Two real bugs found by the proof: build_ns/na returned 82 not 86 (option tail truncated on wire) and csum6 `== 0` would reject negative-zero residuals.
- UDP + IPv4 DHCP (audit run-42 clean, standard): `net.rs` gains parse_udp (ulen-bounded), csum4 pseudo-header (neg-zero via csum6_ok, field==0 legal), build_udp4 (csum field zeroed pre-checksum — CVE-2025-38432 shape). `e1000::build_dhcp`/`parse_dhcp` (len>=241, op==2, XID+cookie match, fully bounded option walk) + `dhcp_selftest`: DISCOVER->OFFER->REQUEST->ACK against slirp, UDP csum verified on RX, retransmit-bounded. PROOF: `dhcp: ACK 10.0.2.15 mask 255.255.255.0 gw 10.0.2.2 lease 86400s` — a real lease, not the hardcoded constant. Socket-demo MACs skip (no DHCP server on the link). NV-42-1: fixed XID + first-OFFER trust is self-test-scoped.
- TCP/IPv4 mini-stack (audit run-43 clean, standard; `11231e1`): `net.rs` gains parse_tcp (data-offset bounded, options region can't escape the segment) + build_tcp4 (zeroed csum field). `e1000::tcp_selftest` two-guest state machine: :01 active-opens :02 port 7 — SYN -> SYN|ACK (MSS option walked bounded, wscale ignored) -> ACK -> 15B PSH data -> echoed -> FIN -> FIN|ACK -> ACK. Every segment gated on TCP checksum + exact seq+ack + peer MAC/IP/ports; RST aborts; bounded per-phase retransmit. PROOF on socket pair: B `SYN dport7 mss=1460 -> SYN|ACK / established / 15B data -> echoed / FIN -> FIN|ACK / closed clean`, A `SYN|ACK -> established / echo verified -> FIN / closed clean`. ISN = rdtsc^const (NV-43-1: production needs hash ISN — klib::siphash already gated).
- demo_net build gate + SipHash ISN (audit run-44 clean; `4f3597f`): the socket-link demos (ipv8/ipv6/tcp selftests) now exist only under `RUSTFLAGS=--cfg demo_net` — `make run`/`make demo` set it, `make build` ships a kernel with zero demo paths (verified by .efi string scan). Closes NV-40-1/41-1/43-2. TCP ISN = `klib::hash::siphash_3u32(4-tuple, rdtsc, boot-key)` — the secure_tcp_seq pattern; closes NV-43-1. Residual NV-44-1: key derives from per-call rdtsc, wants a seeded RNG when one exists.
- CSPRNG (audit run-45 clean, standard): `kernel/src/rand.rs` — RDRAND-seeded ChaCha20 (CPUID-gated, bounded retry, honest weak-seed log on fallback; Makefile adds `qemu64,+rdrand` so QEMU exposes it). Boot line: `rand: CSPRNG seeded (rdrand)`. Consumers: TCP ISN now `siphash(4-tuple, t, per-boot-key)` drawn once at init (NV-44-1 closed); DHCP XID per-run random replacing the compile-time constant, plus ACK cross-checked against OFFER's server-id+yiaddr (NV-42-1 closed to the protocol-inherent race). NV-45-1: no reseed/IRQ-safety until non-boot consumers exist.
- Persistent TCP listener (audit run-46 clean, standard; `e96407b`): listener returns to LISTEN after clean close and issues a fresh SipHash ISN per accepted connection; initiator runs two sequential connections. PROOF: A `conn 1 closed -> reconnect (isn 0xc58e8872)` then `conn 2 closed clean (persistent server verified)` — two independent validated lifecycles on one boot.
- e1000 MSI-X (audit run-37 clean, standard profile — IRQ-on-packet): pci.rs MSI-X helpers (msix_cap/msix_set/msix_enable + write16 dword-RMW; classic MSI code removed — QEMU's e1000e notifies MSI-X only). IVAR 0x8 maps RXQ0 -> table[0] -> vector 0xf3; IMS unmasks RXQ0|OTHER; INTx disabled. Two real bugs found + fixed by diagnostic readback: (1) CF8/CFC moves aligned dwords only — write32 at unaligned offsets silently hits the wrong dword, so sub-dword access now uses read8/16 extraction + write16 RMW; (2) tx_once's IMC=u32::MAX re-masked the causes irq_selftest had just unmasked — masking is now caller-owned. Proof: `1 MSI IRQs on vec 0xf3 (1 rx-caused), reply received` at t=0.500.
- e1000 RX datapath (audit run-36 clean, standard profile — inbound DMA): legacy RX ring (8 descs x 2048B buddy-page buffers, RDBAL/RDLEN/RDH/RDT, RCTL EN|UPE|MPE|BAM promiscuous for the proof, RDT=d repost (gap ratchets with consumption; the d+1 form starved the ring — fixed b669b15)). arp_selftest broadcasts ARP who-has 10.0.2.2 and polls: `rx ARP op2 from 10.0.2.2 mac 52:55:0a:00:02:02 (68B desc0)` — QEMU slirp's reply DMA'd into our buffer. Non-ARP frames are logged + reposted.
- e1000 TX datapath (audit run-35 clean, standard profile — first DMA): reset via CTRL.RST (self-clearing poll), legacy 16-B TX descriptor ring carved from a buddy page (identity map => addr==phys), TDBAL/TDLEN/TDH/TDT programming, one 60-byte broadcast frame -> `tx DONE (TDH 1/TDT 1)` — the NIC DMA'd the packet out of RAM and wrote DD back. probe() now takes is_mmio(base,size): main validates the BAR against UEFI CONVENTIONAL ranges (closes run-33/34 carry). Skeleton notes: ring page intentionally kept; RX ring + real NIC IRQ next.
- e1000e probe (audit run-34 clean): `kernel/src/e1000.rs` — finds 8086:10d3, sets CMD_MEM_SPACE|CMD_BUS_MASTER via pci::command_or, reads MAC from RAL0/RAH0 through BAR0 MMIO (volatile). Boot prints `mac 52:54:00:12:34:56` (QEMU default) + link UP. pci.rs gained write32/find/command_or/bar_raw. Carried needs_validation: check BAR vs UEFI memory map before driver *writes* MMIO.
- PCI enumeration (audit run-33 clean): `kernel/src/pci.rs` — CF8/CFC config reads (inl/outl added to pio.rs), depth-first bus walk following bridge secondary-bus numbers (depth-capped), BAR decode incl. 64-bit BARs, class-name table. QEMU Q35 enumerates correctly: host bridge 8086:29c0, VGA 1234:1111, **e1000e NIC 8086:10d3** (MMIO @0x810a0000 — real driver target), ICH9 LPC 8086:2918. Follow-up before driver MMIO: validate BAR against the UEFI memory map.
- printk ring + panic path (audit run-32 clean): `kernel/src/log.rs` — all log output goes through `klib::prb` (reserve/commit append, `read_valid` FIFO drain); every ring op runs inside `irq_save/irq_restore` (the port is a sequential model of the C cmpxchg ring — kalloc-style serialization makes it IRQ-safe). `[secs.frac]` timestamps stamp records at append time from LAPIC ticks. `kprintln!(con, ...)` replaced all 57 boot `writeln!`s. Panic handler bypasses the ring (fresh UART or stored console, cli+hlt). Boot: `52 records, 0 dropped`, exception-path lines correctly carry no timestamp.
- PS/2 keyboard via IOAPIC (audit run-31 clean): `kernel/src/kbd.rs` — i8042 init (flush OBF, cmd byte: IRQ-enable on + kbd-interface-disable cleared, 0xf4 enable-scanning), IRQ1 -> GSI via ISO-aware `gsi_for_irq`, vec 0xf2. Handler: OBF-checked port read -> lock-free SPSC scancode ring -> EOI; all decode consumer-side (set-1 tables + shift + E0 skip). Boot: bounded ~3s echo window — proven on VM via QEMU `sendkey`: 48 IRQs -> 24 decoded chars echoed live. `kernel/src/pio.rs` shares inb/outb now used by apic + kbd.
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

## 2026-07-16 — Ring-3 userspace milestone (run-47)

- : hostile-input-bounded ELF64/EXEC parser (phdr extent
  checked arithmetic, phnum<=64, entry confined to user VA window).
- : per-process PML4 (kernel slots copied
  supervisor-only), PT_LOAD page-permission mapping (W^X via NX),
  64KiB user stack,  — the deny-by-default user-pointer gate.
- :  (user-rsp stash + dedicated
  8KiB kernel stack, Windows-x64 marshal),  (frame save +
  cr3 switch + iretq),  (sys_exit unwind path).
  Handlers: write(1/2, range-validated, 4KiB cap) + exit.
- : user code/data GDT slots (0x33/0x2b), TSS.rsp0 = INTR_STACK
  for CPL3 interrupts, wrmsr/rdmsr + 
  (STAR=0x20:0x08 sysret/syscall bases, LSTAR, SFMASK=0x700, EFER SCE|NXE).
-  — zeroed-page allocator for tables/user pages.
-  + Makefile: static ELF64 @0x555500000000,
  embedded via include_bytes; write+exit syscalls proved end-to-end.
- Debug lesson of record: x86_64-unknown-uefi = **Windows x64 ABI**
  (args rcx/rdx/r8/r9, NOT SysV) — asm taking args must marshal
  accordingly; caught because user_enter loaded CR3 with arg2
  (Windows rdx) instead of SysV rdx. Verified via QEMU -d int.
- cve.now pre-port check (binfmt/copy_to_user classes); audit run-47:
  0 confirmed, NV-47-1/2/3/4 (SMAP/SMEP, per-task stacks, FS/GS, frame
  reclaim — all gated on the future task model).


## 2026-07-17 — initrd/cpio loader (run-48)
- `kernel/src/cpio.rs`: hostile-input newc (070701/070702) parser, fail-closed.
  Verified-NUL filename terminator (CVE-2024-53142 class), bounded fields,
  checked extents, 256-entry cap, TRAILER!!! stop. 10-case hostile test suite
  (rustc --test, standalone) all pass — incl. a nuance case proving trailing-pad
  truncation is legal while data truncation is rejected.
- `load_initrd()`: reads `initrd.cpio` from the ESP via SimpleFS before
  ExitBootServices into LOADER_DATA pages (never pooled -> survives EBS
  identity-mapped). Cap 1 MiB.
- `tools/mkinitrd.py` packs `kernel/user/hello.elf` as arcname `init`; Makefile
  mcopy's it onto the ESP root. user::run now sources `init` from the archive
  with embedded-ELF fallback. PROOF: `initrd: 5000B`, no fallback line,
  ring-3 hello+exit PASS.
- Audit run-48 (standard, deserialization boundary): 0 confirmed; NV-48-1
  (re-audit if buddy ever pools LOADER_DATA), NV-48-2 (unsigned archive =
  boot-config trust), NV-48-3 (exact-1MiB cap edge).

## 2026-07-17 — syscall-boundary selftest + Linux-ABI register fix (run-49)
- `kernel/user/selftest.S`: second initrd entry exercising the syscall gates
  from CPL3 — EBADF/EFAULT/EINVAL/ENOSYS exact-errno checks plus an r10
  sentinel proving register preservation; exits with failure count.
- Real bug found + fixed: the entry stub preserved only rcx/r11/rdi, but the
  Rust handler runs under the Windows x64 ABI and was free to clobber
  rdx/rsi/r8/r9/r10 — deviating from Linux (syscall may clobber only
  rax/rcx/r11). Stub now saves/restores the full volatile set with explicit
  16-byte call alignment. Selftest `exit(0)` = all 5 checks pass.
- Makefile: `kernel/user/%.elf` pattern rule; initrd now packs `init` +
  `selftest` (10056B). PROOF: both entries run, `selftest run done`, PASS.
- cve.now: CVE-2006-0744 (sysret uncanonical rcx) is harmless by construction
  (rcx is the CPU-saved next-instruction rip) — recorded as NV-49-1 against
  any future rcx-touching syscall. Audit run-49 (standard): 0 confirmed.

## 2026-11-20 — task/process abstraction + frame reclaim (run-50)
- `kernel/src/user.rs`: `Proc` -> owned `Task` (pml4, entry, stacks, ranges).
  `Task::reap()` walks only user PML4 slots 128..255 and frees every leaf and
  table frame plus the per-task kstack — the page table IS the ledger
  (CVE-2022-50661: teardown freeing designed in). Shared kernel slots 0..127
  are never touched. `PHYS_MASK` (0x000f_ffff_ffff_f000) replaces `& !0xfff`
  when extracting frame addresses — the old mask kept the NX bit and
  fabricated non-canonical "addresses" out of read-only PTEs.
- `kernel/src/syscall.rs`: shared `SYS_STACK` replaced by per-task
  kmalloc'd `CUR_KSTACK` set before each entry. **`user_enter`/`user_terminate`
  now also save/restore `rdi`+`rsi`** — see below.
- `kernel/src/cpio.rs`: `entry_at`/`entries()` iterator — the initrd is now
  the process table; main.rs runs every archive entry as a sequential task
  with spawn/done lines and before/after frame accounting.
- `kernel/src/cpu.rs`: `init_syscall` zeroes FS/GS/KGS bases (no firmware
  leftovers into user state); exception reporter now prints cr2.
- `kernel/src/kalloc.rs`: `free_frame` (buddy order-0 wrapper) for reap.
- REAL BUG of the rung — F-50-1, second instance of the Windows-ABI trap:
  rsi/rdi are callee-saved under Windows x64 but argument registers under
  SysV. `user_enter` didn't preserve them; the compiler legally kept `&CUR`
  in rsi across `syscall::enter()`, and the syscall path left the saved
  user rip (0x555500000023) in rsi on return — take()'s memcpy read a user
  VA under the kernel table (#PF) or silently mis-sourced (reap skipped,
  fake 60-frame "leak"). Caught via COM1 markers + cr2 + caller-stack dump.
  Fix: push/pop rdi,rsi. Boot proof x3:
  `tasks: 2 ran, frames 18345 -> 18345 after reap (zero leak)`.
- Audit run-50 (standard, privilege boundary + teardown): 1 confirmed
  (F-50-1, fixed in-diff); NV-50-1..5 carried/reframed; closes NV-47-2/3/4
  and NV-49-2.

## fd table + initrd-as-rootfs (read/open/close) — run-51
- `kernel/src/user.rs`: Task carries a bounded 16-slot fd table —
  `Fd::{Stdin(EOF), Console(serial), File{initrd slice, pos}}`; slots
  0/1/2 pre-seeded. Range table gains a writable bit: PT_LOAD PF_W and
  the stack are writable, so read() targets are gated by range_ok_w —
  the Linux rule that you can't read() into a read-only mapping.
- `kernel/src/syscall.rs`: read(0)/open(2)/close(3) added; write now
  gates on the fd OBJECT (fd_is_console) not the number 1/2 — so a
  file landed in slot 1 still correctly gets EBADF on write (Linux
  O_RDONLY semantics). sys_open does getname-style copy: byte-wise
  under the pointer gate, 256B cap -> ENAMETOOLONG, O_RDONLY only,
  leading '/' tolerated over the flat archive namespace.
- `kernel/src/main.rs`: the cpio archive doubles as the read-only
  rootfs — INITRD static set once (LOADER_DATA lives forever, so
  Fd::File is a borrow, no refcount); non-ELF entries are data files
  and skipped as tasks.
- `kernel/user/fstest.S` + motd.txt prove the path end-to-end from
  CPL3: open /motd.txt -> fd3, read>0 and echoed, read(1) EBADF,
  ENOENT on missing, write-to-file EBADF, close 0 then EBADF —
  exit(0). Third archive entry ran as a task; motd.txt skipped.
- Frame accounting still exact: frames 18360 -> 18360 after reap.
- Audit run-51 (standard): 0 confirmed; RC-51-1..4 rejected with
  reasons (path wrap, TOCTOU, slot reuse, ../ traversal); NV-51-1..3
  (file refcount when heap-backed, SMAP reliance for copy_to_user,
  fd-sharing rules for fork/clone). cve.now: CVE-2025-68814 drove the
  getname/cleanup discipline.

## open-file descriptions (lseek/dup/dup2) — run-52
- `kernel/src/user.rs`: the fd slot is now an index into a per-task
  open-file table — the Linux files_struct/`struct file` split.
  `OpenFile {kind, pos, refs}` holds the seek offset, so dup'd slots
  genuinely share position. FdKind::{Stdin, Console, File} unchanged;
  std streams still pre-seeded 0/1/2.
- `kernel/src/syscall.rs`: lseek(8), dup(32), dup2(33). lseek takes
  a signed off: SEEK_SET/CUR/END via checked_add, EINVAL on a
  negative result, seek-past-EOF is legal (reads then EOF). ESPIPE
  for Stdin/Console like Linux devices. dup picks the lowest free
  slot; dup2 validates old first, releases the target's ref, and is
  a no-op when old==new — all inside a FIXED 16-slot table, which is
  the CVE-2025-39756 lesson applied: a user-chosen fd number can
  never size an allocation here.
- `kernel/user/fstest.S` grew to 19 checks with the real fidelity
  proof: read 'M' via fd3, 'O' via dup'd fd4, 'T' via dup2'd fd9
  after close(fd4) — the OFD survives with remaining refs and the
  offset is genuinely shared. dup(99)->EBADF, dup2(3,9999)->EBADF.
- Debug path worth noting: the first run showed exit(9) and a DBG
  printk trace proved every new syscall was returning correct EBADF —
  the NEW checks were running on fd3 AFTER the close test had closed
  it. Test bug, not kernel bug; reopening motd.txt fixed it. The
  errno discipline the rung enforces is what made the diagnosis
  one serial trace instead of a guessing game.
- Audit run-52 (standard): 0 confirmed; RC-52-1..4 rejected with
  reasons; NV-52-1..3 (OFD share policy for fork — replaces NV-51-3,
  OFD flags for O_APPEND/cloexec, atomic refs under scheduler).
  cve.now: CVE-2025-39756 was the design input for the fixed bound.

## getdents64 + Dir OFD — run-53
- `kernel/src/user.rs`: `FdKind::Dir` — `open("/")` (or ".") yields a
  directory OFD whose pos is the index of the next archive entry
  (Linux's opaque-cookie semantics; ours is just an index). `read`
  on a Dir -> EISDIR; `lseek` allows SEEK_SET 0 only (rewind).
- `fd_getdents`: emits linux_dirent64 records (ino/off/reclen/type/
  name). The whole user buffer is validated writable first, then each
  record's reclen is checked against the remaining space BEFORE any
  byte lands — no partial records ever. Pad bytes between the name
  NUL and reclen end are zeroed — the uninitialized-padding infoleak
  class closed by construction. EINVAL when count can't hold one
  record, count capped at 4096, 0 at EOF.
- `kernel/user/lstest.S`: 11 checks — open("/") gives a Dir, EISDIR
  on read, a full reclen-chain walk counting exactly 5 archive
  entries (init/selftest/fstest/lstest/motd.txt), DT_REG + 'init'
  first-name verification, EOF, rewind-and-reread, ENOTDIR on a
  console fd, EBADF on a bad fd. exit(0) first boot.
- cve.now: getdents/filldir queries returned no entries — the bounds
  and padding disciplines were applied anyway (no match is not
  evidence of absence).
- Audit run-53 (standard): 0 confirmed; RC-53-1..4 rejected;
  NV-53-1..2 (O(n) re-parse per call — fine at initrd scale;
  synthetic ino/off until a real inode table exists).
- 4 tasks, zero-leak streak unbroken: frames 18346 -> 18346.

## execve — image replacement + sysret redirect — run-54
- `kernel/src/syscall.rs`: the user rip now round-trips through a
  `SAVED_UIP` static — stored at entry, reloaded into rcx before
  sysretq. A syscall can therefore REDIRECT where the user resumes,
  which is exactly what exec needs. Canonicality holds because the
  only writers are the stub (CPU-saved rip) and `exec_return`
  (elf::parse-bounded entry) — the CVE-2006-0744 class stays dead.
- `ZERO_CTX`: exec'd images get a fully zeroed GPR file + r11=0x202
  — Linux ABI, and it keeps the caller's registers out of a fresh
  image's hands.
- `sys_execve`: shared getname path-copy, fs_lookup, then
  elf::parse BEFORE the reset — ENOEXEC on a bad image leaves the
  task alive (Linux semantics). Post-reset failure can only be
  allocator exhaustion -> sys_exit(127) kills the task, run() reaps.
- `kernel/src/user.rs`: `free_user_mappings` split out of `reap`
  (also clears the PML4 user slots so the space can be reused);
  `exec()` = free + load_inner + cr3 reload (flush stale user
  translations — without it the TLB could name freed frames) +
  exec_return. fds/OFDs survive exec, the Linux way.
- `kernel/user/exectest.S`: ENOENT returns to the same image (-2),
  ENOEXEC on motd.txt (-8), then execve("/init") — the boot log
  shows `exectest: execing /init` immediately followed by the
  exec'd image's own `hello from ring 3` and its exit(0).
- cve.now: CVE-2022-49264 (argv==NULL) recorded for the future
  stack-args rung — argv/envp are accepted but ignored today.
- Audit run-54 (standard, privilege path): 0 confirmed; RC-54-1..5
  rejected with reasons; NV-54-1..4 (SAVED_UIP canonicality for
  future writers — supersedes NV-49-1's redirect surface, argv/
  envp, cloexec sweep, per-image kernel-state invariant).
- 5 tasks, zero-leak streak unbroken: frames 18359 -> 18359.

## Milestone: preemptive round-robin scheduler (run-55)

The biggest single architectural step since ring-3: tasks no longer
run to completion — the LAPIC timer preempts them.

- `UCtx` (15 GPRs + iret frame) is the unit of scheduling. The
  `sched_dispatch` IRQ path for vector 0xf0 pushes the entire
  register file, `user::tick` snapshots it into the current task's
  ctx and round-robin picks the next Runnable task; `sched_iret`
  (cr3 + rsp=ctx + pops + iretq) is the one resume for launches,
  preemptions, and exits. Fresh tasks get a fabricated ctx
  (`rip=entry`, cs=0x33, ss=0x2b, rflags=0x202).
- `TASKS[16]` + `CUR_I` replace singleton `CUR`; `TaskState` is just
  Runnable|Exited. `pick_next` reaps zombies (never the running
  task — its kstack is underfoot) before scanning round-robin.
- `sys_exit` now switches task-to-task directly (dead kstack
  abandoned, reaped later); the last exit unwinds to the kernel
  frame `sched_launch` saved — same contract as `user_terminate`.
- Preemption discipline: `cs&3==0` frames only tick — syscall
  context is non-preemptible (IF masked by SFMASK end-to-end) and
  kernel context likewise — the CVE-2024-58090 rule by design.
- `log::drain_task()` — main is parked inside `sched_launch` for
  the entire task phase, so user output now flushes from syscall
  context instead of being lost to ring wrap.
- Boot proof: 7 concurrent tasks; log shows tick switches mid-run
  (exectest preempted inside 'execing /init', resumed to print the
  exec'd image), a `tick 6->0` round-robin wrap, ~160 spinA<->spinB
  transitions interleaving 'a'/'b' output; all exit(0);
  frames 18354 -> 18354 (zero leak).
- Audit run-55 (standard): 0 confirmed, 6 rejected candidates,
  NV-55-1..4 (FPU/SSE not in UCtx — required before real binaries;
  per-syscall statics are transaction-local only while syscall ctx
  is non-preemptible; CUR_KSTACK same; UART flood bound).

This closes NV-50-2 (the singleton-statics scheduling blocker) —
the task model now supports fork/clone/wait as the next rung.

## fork + wait4 + getpid (commit pending, audit run-56)

- The syscall entry stub now builds a COMPLETE `UCtx` on the kernel
  stack (15 GPRs + iret frame) and calls `handle_syscall(&mut UCtx)`;
  the return value goes back through `ctx.rax`. This retires
  `SAVED_UIP`/`ZERO_CTX` and the partial-save scheme — every syscall
  that redirects a task now does it by rewriting one struct:
  exec (`*ctx = UCtx::launch`), fork (copy + `rax=0`), wait4 (block,
  `rax=pid` on wake).
- `fork()`: new task slot, fresh page-table tree with a verbatim
  deep copy of the parent's user half (kernel slots shared by
  reference), `fds` copied but pointing at the SAME global OFDs —
  `OFDS[64]` replaces the per-task OFD table so offsets/refs are
  shared exactly like Linux `struct file`. Failure paths free the
  partial tree without touching OFD refs.
- `wait4(pid, status, flags)`: collects a zombie (status =
  `exit_code<<8` via the pre-validated user pointer), ECHILD when no
  matching children exist, otherwise `Blocked` — the exiting child's
  `task_exit` writes the status through `translate()` into the
  parent's (inactive) tables, sets the parent's `ctx.rax = pid`,
  and marks it Runnable. Orphans get ppid=0 and reap immediately.
- `TaskState` gains `Blocked`; `pick_next` reaps only zombies whose
  parent can't collect them; `block_switch` idles on `sti;hlt;cli`
  when nothing is runnable.
- Boot proof: forktest child prints, exits 42; parent collects
  `status=0x2a00`, shared-offset read returns 'T' (child's read
  advanced the joint OFD), second wait4 -> ECHILD, exit(0).
  8 tasks, frames before == after (zero leak).
- Audit run-56 (standard): 0 confirmed, 7 rejected, NV-56-1..6
  (eager copy has no COW; clone flags + WNOHANG + O_CLOEXEC
  unimplemented; hlt-idle instead of wait queues; unbounded pid
  counter; FPU ctx still absent — carried; all-blocked deadlock
  possible without signals).

## pipes (commit pending, audit run-57)

- `FdKind::Pipe{id,write}` + `PIPES[8]`: each pipe is one kernel
  frame as a monotonic head/tail ring. `pipe(22)`/`pipe2(293)`
  mint two OFDs — the ends are dup'able/fork-sharable like files,
  with endpoint counts tracked per-OFD (dup shares, fork bumps).
- **Blocking io generalizes the wait4 mechanism**: `wait_on`
  (WaitOn enum — Child/PipeRead/PipeWrite) + `wait_ptr/len` hold
  the sleeping syscall's user buffer; the WAKER does per-byte
  `translate()` through the sleeper's page tables — data crosses
  between address spaces while only the waker's is mapped — and
  deposits the count in `ctx.rax`. `pipe_wake` settles both
  directions monotonically.
- Endpoint semantics: writer count 0 -> blocked readers wake EOF(0);
  reader count 0 -> blocked writers and new writes get EPIPE(-32);
  <=PIPE_BUF writes are atomic (block until the whole len fits).
- `fd_read`/`fd_write` take the syscall ctx now; `sys_write`
  delegates non-console fds. `ofd_release` owns endpoint teardown
  for all ref drops (close/reap/dup2).
- Boot proof: `pipetest` — pipe2 EINVAL flags; parent BLOCKS on
  read, child writes "hello" through its own address space into the
  parent's (child exit(7) visible first in the log), wait4 0x700,
  EOF after writer close, EPIPE with no readers; exit(0), zero leak.
- Audit run-57 (standard): 0 confirmed, 9 rejected, NV-57-1..5
  (O_NONBLOCK/O_CLOEXEC unimplemented, >4KiB writes, SIGPIPE,
  per-byte translate perf, no readiness API).

## signals (commit pending, audit run-58)

- `rt_sigaction(13)` / `rt_sigreturn(15)` / `kill(62)`. Task grows
  `pending` (bit sig-1, sigs 1..=31) and `handlers[31]` (0=SIG_DFL,
  1=SIG_IGN, else a user handler VA).
- **Delivery at every resume point**: `deliver_current` runs at the
  end of each syscall; `deliver_for` runs inside `pick_ready` and
  the timer `tick`, so a signal pending on a *non-running* task
  lands before it resumes. A default-fatal signal marks the task
  Exited right there; a handler pushes `[signo][interrupted UCtx]`
  (168B) onto the task's own user stack through its own tables
  (inactive spaces included), then redirects rip=handler,
  rsp=frame, rdi=signo. One signal per resume, Linux-style.
- **rt_sigreturn treats the frame as hostile user memory** — the
  whole point of the CVE-2014-9322 / SROP family: it restores the
  pushed UCtx but pins cs=0x33/ss=0x2b, clears TF/NT/IOPL, bounds
  rip and rsp to canonical user ranges (rsp may sit at stack_top,
  so the byte BELOW it must be writable). A bad frame kills the
  task with SIGSEGV.
- **Real wstatus**: `exit_status` holds `code<<8` for exit() and
  the bare signo for signal death — wait4 reports 15 for the
  SIGTERM'd child. `task_exit_inner` pends SIGCHLD on the parent
  (default-ignore, or delivered to a caught handler).
- **EINTR model**: `kill` wakes a Blocked task with rax=-EINTR only
  when the signal's disposition isn't ignored (SIG_IGN or
  default-ignore signals never interrupt a sleep); all three block
  paths (wait4, pipe read, pipe write) also check `pending` BEFORE
  sleeping. The woken ctx delivers the handler at next pick;
  sigreturn resumes the interrupted call's -EINTR return.
- Disposition lifecycle: fork inherits handlers but not pending;
  exec resets caught handlers to default and preserves SIG_IGN.
  SIGKILL/SIGSTOP refuse every disposition change.
- Boot proof: `sigtest` — sigaction install/EINVAL/uncatchable,
  ESRCH; parent blocked on empty pipe -> child's kill -> handler
  writes marker -> sigreturn -> read returns -EINTR; child's
  self-SIGTERM dies silently, wait4 collects status 15;
  exit(0), zero leak.
- Audit run-58 (standard): 0 confirmed after one self-review fix
  (SIGKILL was SIG_IGN-able -> unkillable-by-signal task; now
  EINVAL like Linux), 6 rejected candidates, NV-58-1..5 (no
  credential check on kill — no UID model yet; no
  sigprocmask/siginfo/SA_* flags or SA_RESTART; user faults still
  halt the kernel — a forged sigframe rip at an unmapped/NX page is
  a new trigger of that pre-existing class; sigreturn accepts rsp
  in any writable range like Linux; signals coalesce by mask —
  queued counts absent).

## per-task fault isolation (commit pending, audit run-59)

- A user-mode exception used to hit `report_frame`'s halt path and
  take down the whole kernel — any null deref, ud2, or int3 was a
  trivial machine-kill (and after signals, a forged sigframe rip
  was a new trigger of the same class).
- `report_frame` now checks `cs & 3`: a CPL3 fault calls
  `user::fault_die(signo)` — same teardown as a signal death
  (bare-signo wstatus, SIGCHLD to parent, wait4 collects, kstack
  reaped). The abandoned exception frame lives on the task's own
  kstack and dies with it. Kernel faults still halt.
- Vector -> signal: #DB/#BP->TRAP(5), #UD->ILL(4),
  #DE/#OF/#NM/#MF/#XM->FPE(8), #BR/#AC->BUS(7),
  #TS/#NP/#SS/#GP/#PF/#VE->SEGV(11). Kernel-integrity exceptions
  (#DF, #MC, NMI, #CP, #VC) halt even from CPL3 — task teardown is
  not a trustworthy answer to a double fault.
- **Self-review found a real gap**: only the error-code exceptions
  had stubs — #DE/#DB/#NM/#MF/#XM hit `stub_default`'s unknown
  sentinel and halted anyway. All plausible user vectors are now
  stubbed and gated.
- The #UD-skip and #BP-resume proofs are gated to kernel cs — a
  user int3/ud2 now dies (SIGTRAP/SIGILL) instead of resuming or
  looping.
- CVE-2018-8897 (deferred #DB after MOV/POP SS): the user path can
  no longer reach kernel-halt — #DB->SIGTRAP, `mov ss`->#GP->SIGSEGV.
- Boot proof: `faulttest` — children die to null-write(11),
  ud2(4), div0(8); parent verifies all three statuses, prints,
  exit(0); 11 tasks, zero leak, boot PASS.
- Audit run-59 (standard): 0 confirmed, 6 rejected, NV-59-1..4
  (fault death doesn't deliver a catchable SIGSEGV to a handler
  yet — needs fault-driven signal frames; reserved vectors still
  sentinel-halt, can't fire in practice; si_addr/cr2 not reported
  to userspace — UART line only; kernel faults halt unconditionally).


## per-task FPU/SSE state (commit pending, audit run-60)

- The scheduler's UCtx covered GPRs only: enabling SSE without
  per-task fx state would leak xmm registers across tasks every
  timeslice, and not enabling it made any SSE userspace die #NM.
  Task now carries a 512-byte `FxState`; INIT_FX is captured once
  after `fninit` at boot and copied into every spawn.
- cpu::init sets CR0.MP+EM.clear/TS.clear and CR4.OSFXSR+OSXMMEXCPT
  -- explicit enable rather than inheriting firmware defaults;
  OSXSAVE/AVX left off (no XSAVE, plain FXSAVE only).
- Eager switching everywhere: tick fxsave's the preempted task,
  switch_to fxsave's the outgoing + fxrstor's the incoming,
  launch/resume_ctx fxrstor the target. fork inherits the parent's
  live image; exec resets to INIT_FX (matches exec-zeroes-FPU).
- fputest proves isolation: parent/child hold different 16-byte
  patterns in xmm0 across ~60k verify iterations each under
  10ms preemption -- a missing save/restore would fail immediately.
  parent exit(0), child exit(0), 12 tasks, zero leak.
- cve.now check: CVE-2018-3665 LazyFP n/a by design (eager is the
  mitigation); CVE-2006-1056 AMD FOP/FIP/FDP unrestored-after-
  fxrstor residual -> NV (QEMU/Intel don't exhibit; real AMD would
  need field scrubbing like Linux's eager-FPU hardening).
- Audit run-60 (standard): 0 confirmed, 5 rejected, NV-60-1..3
  (AVX/XSAVE unsupported so a CPUID-advertising guest could #UD on
  xsave -- matches "no AVX" intent; FxState bytes are kernel-static
  so hostile user data can never shape an fxrstor pointer;
  CR4.OSXMMEXCPT enables #XM which fault_die already maps to
  SIGFPE).


## fault-driven signal delivery (commit pending, audit run-61)

- User exceptions used to die unconditionally via fault_die -- a
  task with a SIGSEGV handler never got control, which any serious
  userspace needs (stack-growth handlers, GCs, JITs).
- The generic exception `dispatch` tail now saves ALL 15 GPRs (was
  9 caller-saved) and passes a `SavedRegs*` to report_frame; the
  interrupted rcx/rbp stay in their push slots above the frame.
  report_frame rebuilds the complete interrupted UCtx and hands it
  to `user::fault_signal`.
- fault_signal pends the signo and runs the SAME maybe_deliver
  frame-push/redirect a kill() gets -- [signo][UCtx] on the task's
  own stack, all existing sigreturn hardening applies verbatim --
  then cpu.rs splices (handler rip, frame rsp, signo rdi) into the
  trap frame so iretq lands in the handler. sigreturn restores the
  fault-time ctx and re-faults if unfixed, exactly like Linux.
  DFL/IGN-on-synchronous dies via task_exit_inner (force_sig).
- sigfault.S proves the whole loop: caught handler runs with
  rdi=SIGSEGV, patches saved rip past the faulting movb in its own
  sigframe, sigreturn RESUMES the task; a SIG_DFL-reset fork child
  still dies bare-11. exit(0), zero leak.
- Fixed along the way: MAX_TASKS 16->32 (13 queued + accumulated
  test children intermittently ENOMEM'd sigfault's fork) and a
  hardcoded live[16] in pick_next that panicked at index 16 --
  caught by the expanded table, not by tests.
- Exits now print the pid (`user: pid N exit(c)`) -- anonymous
  exits made this whole debug cycle blind.
- Audit run-61 (standard): 0 confirmed, 6 rejected, NV-61-1..3
  (no si_addr/si_code/siginfo to the handler; a pending lower
  signal delivers ahead of the fault signal, fault stays pending --
  acceptable ordering; nested fault frames bounded only by the
  user stack, as Linux).


## newfstatat (commit pending, audit run-62)

- Syscall 262: fills a 144-byte Linux x86_64 struct stat at a user
  VA. dirfd/flags are ABI-shape only -- the flat cpio namespace has
  no symlinks or per-dirfd views to honor.
- ino = the entry's archive index; mode synthesized
  (S_IFREG|0644, S_IFDIR|0755 for "."); st_size/st_blocks from the
  member header. Path goes through copy_user_path's getname
  discipline like open/execve.
- The stat struct is [0u64; 18] fully zeroed before fields are
  set -- the partially-initialized-kernel-struct copy_to_user
  info-leak class (CVE-2022-50531 family) is closed by
  construction. Write happens only after range_ok_w on all 144
  bytes.
- fstest covers: real file (mode/ino/size), ENOENT, "." -> S_IFDIR,
  EFAULT on an unmapped statbuf. exit(0), zero leak.
- Audit run-62 (quick): 0 confirmed, 3 rejected, NV-62-1
  (dirfd is ignored rather than ENOTDIR'd -- no real fd-relative
  resolution exists yet; timestamps are all-zero, no fs clock).

## poll(2) (commit pending, audit run-63)

- Syscall 7: Linux pollfd {i32 fd, u16 events, u16 revents} = 8B,
  nfds <= 32, timeout_ms (<0 infinite, 0 scan-only, >0 slept on a
  100Hz tick deadline). nfds=0+timeout is a plain interruptible
  sleep like Linux.
- Readiness is honest kernel state: pipe read ends report POLLIN
  on data plus POLLHUP when writers==0 (both may co-report);
  write ends report POLLOUT on space plus POLLERR when readers==0;
  file/dir/stdin reads never block -> POLLIN, console -> POLLOUT;
  unmapped/bad fd -> POLLNVAL. ERR/HUP/NVAL report regardless of
  the events mask, as Linux.
- The sleeper stores the user pfds VA + count in wait_ptr/wait_len
  and a tick deadline in wait_deadline; every pipe-state change
  (data, space, EOF/EPIPE teardown) re-scans all Poll waiters via
  per-byte translate through THEIR page tables -- same cross-space
  mechanism as pipe io wakes -- and a ready count>0 lands in
  ctx.rax. poll_expire in tick() zeroes revents and returns 0 on
  timeout.
- CVE-2023-54019 (poller/waitqueue lifetime UAF) is structurally
  absent: we hold no pointer from poller to polled object -- the
  fd->OFD->pipe chain is re-resolved each scan and the sleeper's
  own fds pin the objects.
- wait4 collects the zombie NOW (was: mark ppid=0, reap at next
  pick): Linux releases the task at wait; the deferral kept a
  dead child counted as a pipe writer, so post-wait4 POLLHUP
  arrived late -- polltest exposed it (exit 24 -> the deferred-
  reap window).
- polltest: empty-pipe poll timeout -> 0, POLLNVAL, poll on a
  plain file -> POLLIN, cross-fork write -> infinite poll wakes
  POLLIN, byte read, wait4, parent close -> POLLHUP, 5ms sleep.
  exit(0), zero leak.
- Audit run-63 (quick): 0 confirmed, 6 rejected, NV-63-1..2
  (reentrant &mut TASKS traversal pattern gains one more instance
  -- pre-existing class, single-CPU safe, unprovable without Miri;
  no select/epoll and nfds cap 32 vs Linux's RLIMIT-sized sets).
- Flake resolved: the run-63 "rare fputest exit(1)" recurred on the
  VM as child exit(9) -- root-caused to resume_ctx (the tick/IRQ
  resume path) never fxrstor'ing the incoming task's fx image.
  switch_to/launch had it; resume_ctx only set CUR_I/cr3/ctx, so a
  timer switch left the previous task's live xmm registers in the
  FPU. Invisible almost always (kernel emits zero xmm instructions,
  so stale live regs usually still hold the resuming task's own
  values); it needs a tick inside fputest's ~0.3ms verify loop AND
  round-robin to the other pattern-bearing task. A deterministic
  eager variant of the CVE-2018-3665 (LazyFP) cross-process FPU
  leak class. Fixed: resume_ctx now fxrstor(&t.fx); five clean WSL
  boots + native VM verify.

## UDP sockets (commit pending, audit run-64)

- Syscalls: socket(41) AF_INET/SOCK_DGRAM, bind(49) sockaddr_in,
  sendto(44), recvfrom(45). Args 4-6 now ride r10/r8/r9 out of the
  saved UCtx (arg4 is r10, never rcx -- the CPU-clobbered ret addr).
- A socket is an OFD kind (FdKind::Udp -> UDPSOCKS[8]): it dups and
  fork-shares like every fd; the endpoint dies with its last fd.
  bind claims a local port (0 -> ephemeral from 49152), re-bind is
  EINVAL, conflicts EADDRINUSE. First sendto auto-binds like Linux.
- The NIC got a live path: net_live() programs a persistent RX ring
  after the selftests (last rx_init wins RDBAL); rx_drain() runs in
  tick() plus once inside recvfrom before it sleeps. drain_desc
  answers ARP who-has for our IP, hands ARP replies to arp_resolve,
  and demuxes verified IPv4/UDP (for_us dst, checksum honored) into
  user::udp_rx -> socket queue or straight into a blocked recvfrom
  through the sleeper's tables -- the pipe-wake trick again.
- sendto resolves the link target: broadcast ff:ff, on-subnet ARPs
  the host (arp_resolve spins the live ring, other traffic keeps
  demuxing), off-subnet or unresolved -> the cached gateway MAC.
  Payload stages through a bounded kernel buffer; len>1400 EMSGSIZE.
- recvfrom sockaddr_out is a fully-zeroed 16-byte sockaddr_in
  (CVE-2021-47597 class closed by construction). Socket lifetime is
  synchronous -- no deferred-cleanup UAF class (CVE-2024-56642).
- Bug found + fixed by this rung: tx_once kmalloc'd a fresh 4KiB TX
  ring per frame and never freed it -- invisible while only selftests
  transmitted (leak inside the boot baseline), exposed the moment
  socket traffic ran inside the measured window: 8 frames = 8 tx
  calls. TX_PAGE is now persistent; zero-leak restored.
- translate split: translate_r drops the WRITE leaf gate for read
  paths (read_task_u/parse_sockaddr) -- reads of a read-only page
  must succeed; write paths keep the WRITE requirement.
- udptest: bind/EADDRINUSE/EAFNOSUPPORT, two real DNS round-trips to
  QEMU slirp's resolver (10.0.2.3:53) -- one via poll(8s)->POLLIN
  ->recvfrom, one via blocked recvfrom woken by the demux; txid echo,
  QR bit, sockaddr family/port all verified. exit(0), zero leak.
- Audit run-64 (quick): 0 confirmed after self-review fixes
  (bind re-bind, translate_r, tx page), 6 rejected, NV-64-1..4
  (one-dgram queue depth, no connect/send/recv aliases or
  getsockname, no O_NONBLOCK, sequential ephemeral ports).

## TCP sockets (commit pending, audit run-65)

- Syscalls: connect(42), accept(43), listen(50), shutdown(48) join
  socket(41)/bind(49); read/write and sendto/recvfrom route streams.
- A TCP endpoint is FdKind::Tcp -> TCPSOCKS[8] (bounded), an OFD kind
  that dups/fork-shares/closes like every fd. States: Init, Listen,
  SynSent, SynRecv, Estab, FinWait, CloseWait, LastAck, Closed.
- listen() turns a bound Init socket passive (backlog capped at 4;
  unbound gets an ephemeral port, separate namespace from UDP).
- connect() ISN = siphash(4-tuple, rdtsc, per-boot isn_key), SYN out,
  then blocks on WaitOn::TcpConn with a ~10s TICKS deadline
  (ETIMEDOUT). Loopback peers (127/8) canonicalize to our IP so the
  4-tuple stays exact on the injected path.
- Loopback is the hermetic test path AND a real OS feature: tcp_tx
  detects dst==our_ip/127.x and pushes the built frame onto LO_Q[4]
  instead of tx_once, then lo_drain() walks the SAME ip_demux as the
  NIC. The drain is iterative (LO_DRAIN guard) precisely so
  tx->demux->rx->tx can't nest and overflow the task kstack.
- accept() pops the completed backlog child into a fresh fd (sa_out
  is a zeroed sockaddr_in -- 47597 discipline); empty backlog blocks
  on WaitOn::TcpAccept and wake_accept mints the fd inside the
  sleeper's table through the cross-space wake machinery.
- RX demux: conn 4-tuple match first, then listener SYN, else RST.
  SynRecv children are bounded per-listener; the final ACK moves
  them to the backlog. RST -> dead + ECONNREFUSED/ECONNRESET wakes;
  ESTab consumes in-order seq only, ACKs, fin_rx -> CloseWait ->
  LastAck -> free. Orphaned (last-fd-gone) conns finish async.
- send copies the user range directly into the segment (no staging
  buffer -- range_ok'd, single-CPU, uninterruptible); recv copies
  out of a bounded 1400B conn ring or blocks on WaitOn::TcpRecv and
  wake_tcp_recv fills the sleeper's buffer through its tables.
- BUG FOUND + FIXED by this rung (the 4-frame leak): the loopback
  chain tcp_send pay[1400] + tcp_tx f[1600] + lo_drain fr[1600] +
  inner tcp_rx->tcp_seg->tcp_tx f[1600] + syscall frames summed to
  ~8.2KiB -- over the 8KiB kstack. The overflow wrote zeros over the
  kmalloc block header below kstack_base, so kfree read kind=0 and
  took the slab-link/corrupt arm, leaking the 4-frame block. Root-
  caused via af/km allocator counters + a corrupt-header probe.
  Fixes: (a) pay now reads the user slice directly, (b) tcp_tx's
  build buffer and lo_drain's pop buffer are non-reentrant statics
  (TX_F/DRAIN_F), (c) KSTACK_SIZE 8KiB -> 16KiB (x86_64 Linux's
  THREAD_SIZE), (d) kfree screams on a corrupt header instead of
  leaking silently (CVE-2022-4378 class: kernel-stack overflow
  corrupting allocator metadata).
- tcptest: socket/bind/listen/connect/accept over loopback, echo
  both directions, dead-port connect -> ECONNREFUSED, accept
  sockaddr_out family+port, teardown. exit(0), zero leak, 4x WSL.
- Audit run-65 (quick): the overflow was the confirmed finding and
  is fixed; NV-65-1 = pre-existing reentrant &mut statics pattern
  extended to TCPSOCKS (NV-63-1 family); known bounded limitations:
  no retransmit/timeout for orphans (slot pin), accept backlog
  tail-insert under EMFILE, no OOB/out-of-order queues (drop).

## Memory-management ABI: brk + anon mmap + munmap + mprotect (audit run-66)
- VMAs ARE the ranges[] table -- every mapped interval has an entry
  (R/W flag bits), so the syscall pointer gate, first-fit allocator,
  and teardown all walk the same truth. MAX_RANGES 8 -> 32 (the
  vm.max_map_count analog); load_inner now FAILS on overflow instead
  of silently dropping an entry (a dropped VMA would have been an
  ungated mapped page).
- brk(addr): page-granular break at the image-end-aligned brk_base,
  256MiB cap, returns current break on any failure (never errno --
  the Linux contract). Grow is collision-checked against other VMAs
  and rollback-unmaps on alloc failure; shrink unmaps + flushes.
- mmap: MAP_PRIVATE|MAP_ANONYMOUS only (fd/shared -> ENODEV until
  file VMAs exist). First-fit in a dedicated 4TiB arena
  (0x6000_0000_0000..0x7000_0000_0000); nonzero addr is a hint --
  collision falls back to first-fit like Linux; MAP_FIXED unmaps the
  overlap AND clips VMA entries then flushes the TLB before user
  code can run (freed frames must not stay cached -- that was the
  real stale-translation aliasing fix this rung caught in review).
- munmap: alignment-only EINVALs; wild VA silently no-ops; len
  rounds UP to whole pages; pages freed + VMAs split atomically --
  clip_entry_count proves the result fits the table BEFORE a single
  PTE is touched, so no half-applied unmap exists.
- mprotect: ENOMEM over unmapped VA, VMA splits (3 pieces max),
  leaf flags rewritten to match the gate flags; PROT_NONE maps as
  present-supervisor leaves -- VMA-reserved, faulting, recoverable.
- mmtest: 37 distinct failure exits covering grow/shrink/cap, both
  gates via pipe read/write, first-fit hole reuse after munmap,
  MAP_FIXED replace, kernel-window EINVAL, 3-page VMA split with
  surviving neighbor data, bad-arg table, 8-page single-call grow,
  and fork heap inheritance. exit(0), zero leak, 4x WSL.
- Audit run-66 (scoped, standard-grade on the diff): 0 confirmed
  after fixes; rejected: 29369/99463 race classes (n/a -- no
  concurrency), MAP_FIXED kernel-slot write, addr+len wraps, gate/
  PTE desync on sub-page unmap; NV-66-1 = NV-63-1 reentrant-statics
  pattern extended to the VM syscalls (functionally safe
  single-CPU, stacked-borrows UB in principle -- the same debt
  already tracked).

## Real-binary ABI: FS base + exec stack + the glibc startup cluster (audit run-67)
- Task.fs_base + arch_prctl(158): SET_FS validates 0-or-user-window
  (kernel-space TLS is impossible), GET_FS writes through a gated
  pointer. fs_base is restored on EVERY resume path -- launch,
  switch_to, and resume_ctx (the tick path, same slot the fx image
  needed in run-64a). Fork inherits it (Linux semantics), exec
  resets it to 0 -- TLS dies with the image. It lives in Task, not
  UCtx, so the 160B signal-frame layout is untouched (Linux doesn't
  save fs in sigframes either).
- exec/spawn build the REAL initial stack: strings top-down, 16B
  AT_RANDOM (ChaCha20), then [argc][argv*][NULL][envp*][NULL][auxv
  pairs: AT_PHDR/PHENT/PHNUM/PAGESZ/BASE/FLAGS/ENTRY/UIDs/HWCAP/
  CLKTCK/SECURE/RANDOM][AT_NULL] -- rsp 16-aligned at entry.
  sys_execve collects argv/envp into bounded kernel buffers (16/8/
  256B caps, E2BIG) while the OLD address space is still mapped;
  argv==NULL gets the binary name as argv0 (the CVE-2022-49264
  discipline -- never argc<1).
- ELF gains ET_DYN (static-PIE): a fixed LOAD_BIAS applied at parse
  (entry + phdr va biased, saturating_add against hostile wrap);
  glibc self-relocates from its own dynamic section, the kernel just
  maps + jumps. LOAD_BIAS reuses the 0x5555... test base.
- VMA truth fix the binary forced: ranges[] stored raw segment
  bounds (p.va..mend) but pages span the page-aligned va0..va1 --
  glibc's RELRO mprotect ENOMEM'd on the sub-page slack and aborted
  ("cannot apply additional memory protection"). VMAs now record
  the actually-mapped page range, matching Linux's page-aligned VMAs.
  First real-binary bug caught -- exactly why this rung exists.
- Syscall cluster: writev/readv (each iovec re-validated inside
  sys_write/fd_read, cnt<=32), exit_group (= exit, no threads),
  uname (zeroed 390B utsname), gettid/getpid + uid/gid family (0),
  set_tid_address, set_robust_list (len==24 check), getrandom
  (ChaCha20, <=4096B), ioctl -> ENOTTY, prlimit64 (RLIM_INFINITY),
  clock_gettime/gettimeofday (apic::TICKS 100Hz), nanosleep +
  clock_nanosleep via WaitOn::Timer + wait_deadline -- EINTR wakes
  fill rem; clock_nanosleep's positive-errno ABI rides wait_ptr2,
  TIMER_ABSTIME supported. Dispatch logs ENOSYS numbers (bring-up
  aid: it surfaced glibc's rseq 334 + readlinkat 267 probes, both
  tolerated).
- realbin.c (gcc -static-pie hello): glibc _start consumed our auxv
  stack, installed TLS via SET_FS, grew the heap via brk+malloc,
  uname'd us, slept 20ms through nr 230, survived the RELRO
  mprotect, and exited 42 via exit_group. 18 tasks, zero leak,
  WSL x3 green.
- Audit run-67 (scoped on the diff): 0 confirmed after the VMA fix;
  rejected: 49264 argc0 (handled), 1070-class unchecked reads
  (bounded collect + extent checks), SET_FS outside user window,
  ET_DYN va wrap (saturating), UCtx/sigframe growth (not needed),
  iovec under-validation (per-buffer gates), uname infoleak (zeroed);
  NV-63-1 reentrant-statics debt extends to the new paths.
## Process-lifecycle ABI: clone fork-subset, sigmask, identity/metadata (audit run-68)
- clone(56): the fork-shaped subset -- exit-signal 0 or SIGCHLD plus
  the child-TID writes glibc asks for (0x1200011 = SIGCHLD |
  CHILD_SETTID | CHILD_CLEARTID). PARENT_SETTID writes the child pid
  to *ptid in our space (range-gated); CHILD_SETTID writes it to
  *ctid in the CHILD's space via its own pml4; CHILD_CLEARTID is a
  futex-registration no-op -- no clear_child_tid state is ever
  stored, so the CVE-2009-2848 execve-forgot-to-clear class is
  structurally absent. stack!=0 or any other flag -> ENOSYS.
  clone3 stays ENOSYS; glibc falls back.
- rt_sigprocmask(14): a real per-task u32 sigmask. SIG_BLOCK/
  UNBLOCK/SETMASK, how validated only when set!=NULL, glibc's 8-byte
  set truncated to our 31 signals. deliverable() = pending & !mask
  gates BOTH delivery and every EINTR wake check, so a masked signal
  stays pending without interrupting sleeps; SIGKILL's bit is
  ignored on delivery (Linux: sig_kernel_only signals can't be
  blocked -- the self-review caught a masked-SIGKILL-survives bug
  here before commit).
- REVIEW-CAUGHT SECURITY FIX: translate()/translate_r() walked ANY
  va through the task's pml4, which shares kernel upper-half slots.
  A user-controlled VA therefore reached kernel memory: clone's ctid
  (4-byte kernel write of a pid), the sigframe rsp (user rsp=kernel
  va -> signal delivery writes [signo][UCtx] into kernel space --
  pre-existing!), and the argv/envp walkers (kernel infoleak into
  user stack). Both walkers now bound va to [USER_VA_LO,USER_VA_HI),
  covering every task-memory path at once.
- Task.name: cpio name stored at spawn, inherited at fork, replaced
  at exec. readlinkat(/proc/self/exe) returns / + name -- the
  absolute path glibc's _dl_get_origin asserts (dl-origin.c:41
  linkval[0]=='/'). The relative name made BOTH C binaries abort
  silently at glibc init via tgkill(SIGABRT) -- the bug this rung's
  real binaries caught.
- Metadata cluster: getcwd (/), stat/lstat/faccessat over the flat
  cpio fs, fstat by OFD kind (chr console, fifo pipe, sock socket),
  clock_getres (100Hz = 10ms), getrlimit aliased onto prlimit64.
  tgkill(234) -> kill (tgid==tid, no threads) -- glibc abort() lands
  there. sched_yield(24): ctx-save + pick_ready handoff.
- parent.c (gcc -static-pie): metadata checks all ok, then
  clone fork -> execve(realbin, argv, envp) -> waitpid: the child
  prints argc=2 argv1=from-parent env=child and exits 42; parent
  verifies WIFEXITED + status==42 -> "0 failures, done" exit(0).
  realbin still exits 42 standalone. MAX_INITRD bumped 1->8MiB (two
  ~830KB static-PIE binaries). 19 tasks, zero leak, WSL x4.
- Audit run-68 (scoped on the diff): 1 confirmed (unbounded task-
  space translate = kernel write/infoleak primitive) -- fixed
  pre-commit; copy_process race family n/a (single CPU); NV-63-1
  reentrant-statics debt extends.

## File-I/O ABI: openat, pread, fcntl FD flags, dup3, accept4, O_CLOEXEC (audit run-69)

- What landed: openat(257) with real dirfd semantics, pread64(17)
  that never moves the shared OFD position, fcntl(72) F_GETFD/
  F_SETFD/F_GETFL/F_SETFL/F_DUPFD/F_DUPFD_CLOEXEC, dup(32)/
  dup2(33)/dup3(292), accept4(288) with SOCK_CLOEXEC on both the
  immediate and blocked-wake paths, and a per-fd O_CLOEXEC bitmask
  (Task.fd_cloexec) that fork inherits and exec sweeps before the
  image swap. socket(41)/pipe2(293) flags mask now rejects
  NONBLOCK honestly (EINVAL) instead of silently ignoring.
- The bug only a compiled binary could catch: glibc passes dirfd
  as a 32-bit int -- mov edi,-100 zero-extends to 0xffffff9c, so a
  u64 compare against sign-extended AT_FDCWD never matched and
  EVERY openat returned EBADF. dirfd is now read as i32, matching
  Linux's 'int dfd' parameter. Hand-written asm tests always
  sign-extended; this is the second ABI-skew class (after
  _dl_get_origin's absolute-path assert) only real code surfaces.
- Honest-error discipline: pread answers EINVAL(neg pos) ->
  EBADF(bad fd) -> ESPIPE(non-file) -> EFAULT(bad buf) in Linux's
  order; openat EINVALs unknown flags, EROFSs write opens (the
  initrd is read-only, that is the truth), ENOTDIRs a non-Dir
  dirfd; dup3 EINVALs oldfd==newfd and non-CLOEXEC flags; fcntl
  F_SETFL accepts only 0 (no toggles exist to fake).
- O_CLOEXEC lifetime: the bit lives on the fd, not the OFD -- dup
  family clears it on the copy (Linux semantics), F_DUPFD_CLOEXEC
  sets it, the exec sweep drops every marked slot via ofd_release
  so a pipe/socket cannot leak into a binary that never opened it.
  MAX_FDS<=16 is a compile-time assert so a future fd-table bump
  cannot silently truncate the u16 mask.
- Proof: kernel/user/iotest.c (static-pie) -- fopen/fgets/fclose
  through glibc's openat, pread offset + no-pos-move, lseek all
  three whences + read-back, fcntl get/set/clear + GETFL, dup3
  cloexec vs plain-dup clearing, openat through a real O_DIRECTORY
  dirfd, and the ENOENT/EROFS/EINVAL honesty table: 0 failures,
  exit(0). 20 tasks, zero leak, WSL x3.
- Audit run-69 (scoped on the diff): 0 confirmed -- the dirfd-int
  and pread-ordering bugs were ABI-correctness, not boundary;
  cloexec-sweep leak class checked against cve.now
  (CVE-2025-68814 family, structurally absent: validation precedes
  minting, sweep has no early return). NV-63-1 reentrant-statics
  debt extends.

## Shell-pipeline proof + exit_files ordering fix (audit run-70)

- kernel/user/shelltest.c (static-pie): pipe() -> fork() -> child
  dup2(w,1) + close + execve(realbin) -> parent drains the pipe to
  EOF and finds realbins argv/env in the stream -> waitpid status 42;
  plus dup2(fd,0) stdin-redirect read-back. 0 failures, exit(0).
- The bug the shape caught: task fds released only in reap(), never
  at exit. A dead pipe-writer OFD kept writers>0 forever and the
  parents blocked reader starved for EOF -- observed live as pid 21
  still blocked at suite end while pipe_end_closed only fired during
  the final reap. Linux runs exit_files(2) inside do_exit; release_fds
  now runs in task_exit_inner (die_signal covered too), idempotent
  via take() so reap stays a no-op.
- Audit run-70 (scoped): 0 new confirmed -- release ordering was a
  liveness bug, not a boundary; cve.now fd-lifetime classes (39966
  async fput, 38722 install race) structurally absent. NV-63-1
  reentrant-statics extends (release under TASKS borrow, same shape
  as reap).
- 21 tasks, zero leak, WSL x2.

## Writable file layer + wiotest proof (audit run-71)

- A scratch namespace now shadows the flat archive: VFile slots
  (16 files, 1MiB each, kmalloc'd buffers) behind FdKind::Ram OFDs.
  fd_open resolves VFILES first then cpio -- mount-shadow order;
  archive writes answer EROFS at open, EBADF on an O_RDONLY fd at
  write (the fmode check, which the suite enforced via fstest).
- OpenFile carries accmode (f_mode analogue) and append; dup/fork
  share it through the OFD like Linux. New syscalls: unlink(87)/
  unlinkat(263), rename(82)/renameat(264)/renameat2(276),
  truncate(76)/ftruncate(77), creat(85), fsync(74)/fdatasync(75).
  getdents enumerates archive+VFILES as one virtual directory;
  newfstatat and access see both namespaces.
- Open-unlink semantics are real: unlink drops the name while an
  open OFD keeps the storage readable; last close frees it. Rename
  over an open target keeps the replaced file alive the same way.
- Self-review catches fixed pre-commit: zero-length write at pos>len
  allocated+extended (now early-0); empty-VFile pread UB (n==0 early
  return); access("") -> ENOENT. Known gaps recorded: renameat
  skips dirfd validation; no rmdir (AT_REMOVEDIR -> EISDIR);
  open("") -> EISDIR vs ENOENT.
- Proof: kernel/user/wiotest.c (static-pie) -- 33 checks: creat,
  write/read-back, EBADF on both wrong accmodes, fsync, fstat,
  O_TRUNC, O_APPEND-at-EOF, O_EXCL->EEXIST, ftruncate shrink +
  grow zero-fill, path truncate, rename + stat-both-sides,
  open-unlink-still-reads, ENOENT after unlink, EROFS table on
  archive unlink/rename, access + getdents listing a fresh file:
  0 failures, exit(0). 22 tasks, zero leak, WSL x3.
- Audit run-71 (scoped on the diff): the three catches above were
  correctness/UB-adjacent, not boundary violations; rename/unlink
  UAF classes (CVE-2022-49349, CVE-2025-21887) structurally absent
  under the OFD-pins-storage invariant. NV-63-1 reentrant-statics
  extends (VFILES under OFDS borrow in vfile_remove).

## Terminal ABI — real ioctl dispatch (audit run-72)

- ioctl(16) finally dispatches instead of blanket-ENOTTY: the
  Stdin|Console pair is our one tty and answers the termios family —
  TCGETS/TCSETS/TCSETSW/TCSETSF on a shared 36-byte kernel-ABI
  termios (cooked defaults: ICRNL|IXON, OPOST|ONLCR, B38400|CS8,
  ISIG|ICANON|ECHO). glibc's isatty() is just "does TCGETS succeed",
  so the kind gate is the entire isatty answer — files ENOTTY.
- TIOCGWINSZ reports the REAL grid: fbcon records cols/rows (GOP
  width/8 x height/16, currently 160x50) into TTY_DIMS at init,
  80x25 fallback when no framebuffer exists. TIOCGPGRP returns the
  task's own pid; TIOCSPGRP/TIOCSWINSZ validate the arg then accept
  (the geometry and the single pgrp can't change).
- FIONREAD is per-kind honest: pipe ring fill, File/Ram bytes
  remaining, TCP rxl, UDP next-datagram size, write-end pipe and
  dir EINVAL, stdin/console 0 (no input queue exists). FIONBIO
  EINVALs — no nonblocking mode exists to fake.
- Proof: kernel/user/termtest.c (static-pie) -- 18 checks: isatty
  0/1 + file ENOTTY, tcgetattr flag values, tcsetattr round-trip,
  winsize nonzero, pgrp==pid, FIONREAD empty/queued/file, EBADF +
  EINVAL refusals: 0 failures, exit(0). 23 tasks, zero leak, WSL x3.
- Audit run-72 (scoped): 0 confirmed -- every arg extent is bounded
  with the exact struct size before copy; TIOCSTI absent by
  omission (no input queue to inject); ldisc CVE family n/a.
  NV-63-1 reentrant-statics extends (TERMIOS cell).

## TTY stdin — IRQ1 to read(0) via a real line discipline (audit run-73)

- The keyboard path now ends at a real input queue, not the
  framebuffer: kbd IRQ -> 64B scancode ring -> kbd::poll decoder ->
  stdin_key line discipline (256B canonical edit buffer -> 4KiB
  ready queue) -> WaitOn::Stdin block/wake. The pump runs in the
  timer tick (live echo + bounded) and lazily at fd_read entry.
- Canonical mode edits the line in-place (VERASE erases + echoes
  backspace-space-backspace under ECHOE, VKILL drops the line) and
  submits on '\n' or VEOF; VEOF on an empty line is Linux's one-shot
  EOF (read returns 0 once, the next read blocks again). ICANON off
  pushes bytes straight to the ready queue. ISIG turns VINTR/VQUIT
  into SIGINT/SIGQUIT for the blocked reader through kill() — which
  already owns the EINTR-wake/ignored-check machinery. ECHO goes
  through printk like every other user-visible byte.
- poll(0) is honest now — POLLIN only when bytes or the EOF flag
  wait (the old arm hardcoded POLLIN because stdin meant EOF).
  FIONREAD reports the ready-queue depth; read(0) on an empty queue
  blocks with the pipe discipline (wait_ptr/len, waker copies
  through the sleeper's pml4, EFAULT on a dead mapping, EINTR on a
  pending signal).
- The boot-time ~3s echo window in main.rs is gone — it consumed
  decoded keys that now belong to the queue.
- Proof: kernel/user/kbdtest.c (static-pie) — isatty, empty-queue
  FIONREAD/poll-not-ready, then a bounded input window: headless
  runs hit the poll deadline and exit 0; a LIVE QEMU run injected
  "hi<ret>" via QMP human-monitor sendkey inside the window and
  read(0) returned the canonical 3-byte line. 0 failures, 24 tasks,
  zero leak, boot PASS x2.
- Fixed mid-rung: a termios tcsetattr roundtrip in kbdtest raced
  termtest on the shared tty cell (correct Linux semantics — the
  test was wrong to mutate it during a concurrent suite); removed.
- Audit run-73 (scoped): 0 confirmed — bounded queues, drop-on-full,
  no allocation in the feed path, IRQ-safe calls only, translate-
  gated wake copies, take/consume split means EFAULT loses no input.
  cve.now: n_tty buffer-swap/ldisc-attach families n/a (no replaceable
  buffers, no attach path, single CPU). NV-63-1 extends.
