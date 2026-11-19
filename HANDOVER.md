# Handover — linux-rust port project

**Goal:** rebuild the Linux kernel in Rust as a drop-in replacement — not Rust-in-Linux support. Strategy: port `lib/` and subsystem units one at a time with differential verification (`rust-anything` record → diff → gate), adopt verified ports into `klib`, prove each under QEMU boot, audit, commit. Long tail leads to mm/sched/fs/net/drivers parity.

**Last commit:** `0149328 ports/lib-siphash: source-faithful port, gate green` on top of `bf56d6d` (wave-1 tail: idr/radix/maple/percpu/prb). Suite: **265 klib tests green**, QEMU boot PASS (exit 33), clippy has 14 pre-existing bitmap warnings only.

## Environments

| host | path | role |
|------|------|------|
| Windows/WSL | `C:\Users\dev\Documents\GitHub\linux-rust` (`/mnt/c/...` in WSL) | primary repo, port jobs run here via `wsl bash -lc` |
| Windows/WSL | `/mnt/c/Users/dev/Documents/GitHub/linux` | full Linux reference tree (torvalds clone) |
| Zorin VM (10.0.0.115, MCP `LinuxMCP-VM-3`) | `/home/dev/linux-rust` | git checkout — sync via `git bundle`/fetch; native build+test+boot host |
| Zorin VM | `/home/dev/linux-ref` | scoped ref tree (lib/, include/, arch/x86/include/, Kbuild/Kconfig/Makefile); symlinked at `/mnt/c/Users/dev/Documents/GitHub/linux` so `check-sources.sh` works unmodified |
| Zorin VM | `/home/dev/recv/recv.py` | HTTP PUT server on `:9876` — `curl -T file http://10.0.0.115:9876/name` |
| Zorin VM | `~/linux-rust/test/zorin/fixtures/` | Zorin 18.1 ISO fixtures: vmlinuz 6.17, initrd (4-layer: AMD ucode → Intel ucode → modules/firmware cpio → gzip'd casper userspace), grub.cfg; dissection docs in `test/zorin/README.md` |

VM toolchain: gcc 13.3, clang 18.1, QEMU 8.2.2+OVMF, mtools, cargo 1.100 (`~/.cargo/bin`), rust-anything 2.0.0, passwordless sudo. Verify on VM: `cargo test -p klib` and `make run` (boot → exit 33 = PASS). NOTE: cargo/rustup need `export PATH=$HOME/.cargo/bin:$PATH`.

## Port pipeline (per unit)

`ports/<pkg>/` holds: upstream C copy + `check-sources.sh` (byte-identical verify vs ref tree), `selftest.c` (userspace harness), `wrappers.c` (FFI scalars), `rust/wrappers-rs/` (port crate, `#![forbid(unsafe_code)]` preferred), `cases.jsonl` (recorded corpus), `verify-all.sh`. Reference exemplars: `ports/lib-siphash/` (newest, clean layout) and `ports/lib-idr-radix/`.

Gate bar: `rust-anything diff` 100% case match + `rust-anything gate` all checks. Then serial adoption: generator/extractor script → `klib/src/<mod>.rs` + `<mod>_tests.inc` → register in `klib/src/lib.rs` → boot proof in `kernel/src/main.rs` → `cargo test -p klib` + `make run` → scoped security-audit run → commit.

## In-flight ports (as of handover)

| package | status | resume step |
|---------|--------|-------------|
| `ports/lib-siphash` | **DONE, committed** (`0149328`). 4815/4815 gate green. `klib/src/hash.rs` already holds identical impl — **no adoption needed**. Note: source is `lib/crypto/siphash.c` (not `lib/siphash.c`); vectors from `lib/crypto/tests/siphash_kunit.c` | — |
| `ports/lib-genradix-interval` | **nearly done.** Sources+selftest+wrappers+verify-all built; 899 cases recorded; diff.json = 899/899 PASS | run `rust-anything gate`, then adopt genradix + interval_tree |
| `ports/lib-strtox-helpers` | substantial. kstrtox+hexdump+string_helpers+ctype dep copied; selftest+wrappers+check-sources+verify-all built; **11667 cases recorded**; diff not yet run | run `diff`, fix mismatches, `gate`, then adopt (merge into `klib/src/string.rs` where overlapping — check overlap first) |
| `ports/lib-assoc-array` | skeleton only. Sources+wrappers.c+selftest.c+crate scaffold+rust-anything.json; no cases recorded | record cases, `diff`, `gate`, then adopt |

Adopted so far (in klib): math/div64/gcd/int-pow/lcm/primes, bcd, bitops, bitmap, crc, hash(+siphash), rbtree, string, sha1/256/512+hmac, kfifo, llist, seq_buf, fmt, rhashtable, xarray, idr, radix_tree, maple_tree, percpu_counter, prb (printk ringbuf). 265 tests.

## Conventions that matter

- **Generators are authoritative.** `extract_*.py` at repo root regenerate `klib/src/*.rs` — hardening fixes go in the extractor, never edit generated files directly. `.replace()` calls are assert-guarded (drift can't silently no-op).
- **Adversarial hardening patterns** (apply to new ports): `wrapping_*` where C wraps; arena-index routing to scratch nodes (`nat`/`nmat`, `nidx`/`spare`) for crafted pointers; iteration caps (`MAS_ITER_LIMIT`-style `__lim`) on pointer-chasing/retry loops; clamp table/slot/mark/type indexes; bounds on copy/zero helpers; widen-then-multiply for `batch*ncpu`-style arithmetic.
- **Security mandate (standing rule):** every executable-code change → scoped `security-audit` run (quick profile; standard for crypto/auth/etc.), artifacts to `C:\Users\dev\security-audit-skill\linux-rust\run-N\` (next free N; run-16 closed for wave-1; the siphash agent self-audited as run-1 of its own namespace — renumber if continuing centrally). Fix confirmed findings or record as needs_validation before commit.
- **Commits:** signed `Generated with [Devin]` + co-author trailer. One commit per adoption wave.
- **Delegation model:** port jobs run as parallel background subagents, one `ports/` dir each, no shared-file edits. Adoption/wiring/boot/audit/commit stay serial. Rate limits killed 3 of 4 jobs this round — resume by re-dispatching with the resume-step above; their on-disk progress survives.

## Next steps

1. Finish genradix-interval (gate → adopt) — closest to green.
2. Finish strtox-helpers (diff → gate → adopt) — large corpus already recorded.
3. assoc_array: record → diff → gate → adopt.
4. Audit + commit the wave; update `docs/roadmap.md`.
5. Queue next lib units (see `docs/roadmap.md` for mapping).
6. Zorin acceptance ladder (see `test/zorin/README.md`): M0 fixtures done → next rungs are boot our kernel under the real grub cmdline, initrd cpio/gzip readers, eventually casper init (M6 desktop = drop-in milestone).

## Warnings

- Native Windows build of klib has a pre-existing `c_ulong` u32/u64 platform mismatch (`slow_is_prime_number`); verify on WSL/Linux only.
- DrvFs is slow; large tars of the ref tree stall — use scoped tarballs or git bundles (worked well).
- VM apt mirror must be `mirrors.kernel.org` (default `be.archive.ubuntu.com` is blocked by egress allowlist; github/crates.io/kernel.org/debian/qemu.org all reachable).
- A stray untracked `nul` file sits in the repo root (Windows artifact) — ignore/delete.
