# Zorin OS real-world acceptance ladder

The project's end goal is a drop-in replacement for the Linux kernel. The
`test/zorin/` tree holds the fixtures and milestones for proving that against
a real distribution instead of synthetic harnesses.

## Fixtures

| File | What it is | Source |
| --- | --- | --- |
| `fixtures/vmlinuz` | Zorin 18.1 stock kernel (`bzImage 6.17.0-20-generic`) | ISO `casper/vmlinuz` |
| `fixtures/initrd.zstd` | Ubuntu-style initramfs — early microcode cpio (`070701`) prefix + compressed initramfs (the real payload starts after the first archive) | ISO `casper/initrd.zstd` |
| `fixtures/grub.cfg` | The real boot entry — kernel cmdline (`maybe-ubiquity quiet splash`) and paths | ISO `boot/grub/grub.cfg` |

The ISO itself (`Zorin OS 18.1 Pro.iso`, 7.9 GB) stays on
the host; only the boot assets are checked out here (fixtures are gitignored
— they're ~99 MB of redistributable distro files).

`filesystem.squashfs` (7.1 GB root FS) remains inside the ISO; extract it on
demand at later milestones.

## Zorin dev VM

The dev VM is a live Zorin OS 18.1 install — kernel
`7.0.0-43-generic` after updates — and doubles as:

- a development/test host for `klib` + `kernel/` (cargo, qemu via
  mirrors.kernel.org apt; ubuntu archive is allowlisted, not the default
  be.archive mirror — sources were repointed),
- the **reference oracle**: any behavior our kernel implements can be
  diffed against the same workload running on real Zorin,
- the eventual acceptance environment for the ladder below.

Repo sync: `~/linux-rust` on the VM is populated via the PUT receiver
(`~/recv/recv.py`, port 9876) from the Windows host — see
`test/zorin/` scripts.

## The ladder (each rung is a real bootable test)

- [ ] **M0 — fixtures + baseline.** ISO assets extracted; QEMU boots the
  stock ISO (stock kernel) as the reference baseline for observable
  behavior. (VM runs the real thing natively — use it for syscall/ABI
  reference, e.g. `strace`/`ltrace` of the same workloads.)
- [ ] **M1 — initramfs decode.** Our kernel decompresses `initrd.zstd`
  (cpio + zstd in-kernel) and walks the archive. Exposes the decompressor
  and cpio-parser ports needed early.
- [ ] **M2 — single real ELF.** A static ELF extracted from the Zorin
  initrd executes under our kernel: minimal syscall surface
  (execve/exit/write/open/close/mmap/brk). First real userspace.
- [ ] **M3 — initrd /init.** The distro's early-userspace `/init` runs far
  enough to reach `switch_root` — fork/exec, mounts, devtmpfs, modules.
- [ ] **M4 — real root.** `filesystem.squashfs` mounts as root (squashfs
  + overlay semantics), init continues on the real filesystem.
- [ ] **M5 — systemd PID1.** systemd starts units; requires cgroup,
  dbus, and a large syscall surface.
- [ ] **M6 — desktop session.** GNOME session + login — the drop-in
  replacement milestone (drivers, DRM or fbdev, input, networking).

The ladder is intentionally ordered by increasing kernel surface: each
rung is a concrete, bootable test against a real-world artifact rather
than an abstract feature matrix.

## initrd layout (Zorin 18.1 live, casper)

The 82 MB initrd is four concatenated parts — the kernel consumes them in order:

| offset | format | contents |
|--------|--------|----------|
| 0 | cpio (plain) | kernel/x86/microcode/AuthenticAMD.bin (~148 KB) |
| 148480 | cpio (plain) | kernel/x86/microcode/GenuineIntel.bin (~14.8 MB) |
| 14949376 | cpio (plain) | firmware usr/lib/firmware/** (*.zst) + modules usr/lib/modules/6.17.0-20-generic/** (~47.9 MB, 1601 entries) |
| ~62.9 MB | gzip -> cpio | main userspace: init, scripts/, etc/casper.conf, bin/busybox (62 MB, 1151 entries) |

Consequence for the Rust kernel initrd reader: it must skip leading uncompressed
cpios (parse-and-discard), then gzip-decompress the tail. Module .ko.zst and
firmware .zst files additionally need zstd — so our decompressor pipeline needs
both gzip and zstd before a real Zorin init can run.

Extraction tooling lives at test/zorin/fixtures/initrd-extract/ on the VM
(microcode.cpio, main.cpio, layer3.bin, main-final.cpio, rootfs/).
