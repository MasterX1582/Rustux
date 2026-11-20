# Handover — Rustux port project (formerly linux-rust)

**Goal:** rebuild the Linux kernel in Rust as a drop-in replacement — not Rust-in-Linux support. Strategy: port `lib/` and subsystem units one at a time with differential verification (`rust-anything` record → diff → gate), adopt verified ports into `klib`, prove each under QEMU boot, audit, commit. Long tail leads to mm/sched/fs/net/drivers parity.

**Last commit:** `c98eb55 net: IPv4/ICMP wire layer + e1000 ping round-trip` — `kernel/src/net.rs` (csum16, validated IPv4/ICMP parsers, echo-request builder; fragments dropped by design) + ARP reply now seeds GW_MAC/GW_IP cache + `ping_selftest` (real `ping reply 10.0.2.2 seq=1` through our rings). Prior: `f6787a2` RX field validation + `docs/cve-matrix.md` retroactive cve.now sweep (all ported units), `cf49beb` MSI-X vec 0xf3. On top of Rustux rename (`d60118f`), kmalloc/slab heap (`8ab0e02`), LAPIC timer, GDT/IDT (`f26b2e4`), page tables + buddy (`e8946f8`/`4a48db4`). Suite: **327 klib tests green**, QEMU boot PASS (exit 33), clippy has 14 pre-existing bitmap warnings only. Repo: `https://github.com/MasterX1582/Rustux`.

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
