# Kernel map

Inventory of the Linux reference tree (`../linux`, 7.4-rc mainline), grouped by
porting layer. File counts are exact (`fd -e c/-e h`); lines are `wc -l` over
C + headers. The kernel already ships ~568 Rust files (Rust for Linux) — the
`rust/` crate and a few drivers; those are design reference, not our target.

## Layer A — freestanding library code (port first)

Self-contained algorithms; most compile in userspace or have kernel-maintained
userspace test harnesses in `tools/testing/` — i.e. `rust-anything record` can
capture their behavior directly.

| Dir | .c | .h | lines | Contents |
| --- | ---: | ---: | ---: | --- |
| `lib/` | 530 | 218 | 279,229 | rbtree, xarray/idr, bitmap, sort, list_sort, crc, decompress, kobject, string ops, siphash, kfifo, gcd, min_heap |
| `crypto/` | 148 | 18 | 105,041 | crypto API + implementations: sha*, aes, chacha20, poly1305, crc32c, xxhash, rsa, ecdsa |
| `tools/lib/` | (subset of tools/) | | | userspace copies of rbtree, string, bitmap, find_bit, symbol — pre-shimmed for record |
| `tools/testing/` | | | | kernel-maintained userspace harnesses: radix-tree, maple, xarray, idr, bitmap, scatterlist |

## Layer B — core kernel services

Logic is often algorithmic but entangled with kernel runtime (locking, percpu,
preempt). Needs custom userspace shims per unit before `record` works.

| Dir | .c | .h | lines | Contents |
| --- | ---: | ---: | ---: | --- |
| `kernel/` | 514 | 156 | 590,543 | sched (fair/EEVDF, rt, dl), fork/exit, signals, timers/hrtimer, workqueue, printk+ringbuffer, modules, sys.c syscall glue, locking (rwsem, mutex, rtmutex, futex) |
| `mm/` | 165 | 32 | 233,639 | page_alloc (buddy), slab/slub, vmalloc, mmap/madvise, page cache, swap, oom, memory cgroup, maple tree backend |
| `ipc/` | 11 | 1 | 9,880 | SysV IPC |
| `security/` | 172 | 101 | 130,380 | LSM framework + selinux/apparmor/smack/landlock |
| `virt/` | 11 | 5 | 11,580 | KVM core (arch parts live in arch/) |

## Layer C — architecture & core ABI

x86-64 only for now. asm glue, page tables, entry/exit, syscall dispatch —
the least userspace-testable code; verification is differential boot testing,
not recorded cases.

| Dir | .c | .h | lines | Contents |
| --- | ---: | ---: | ---: | --- |
| `arch/x86/` | 582 | 576 | 436,291 | entry/exit asm, mm/pgtable, apic/ioapic, cpu bring-up, kvm x86, fpu, alternatives patching |
| `init/` | 11 | 2 | 4,843 | boot/init sequence |

## Layer D — I/O stacks

| Dir | .c | .h | lines | Contents |
| --- | ---: | ---: | ---: | --- |
| `fs/` | 1,483 | 701 | 1,674,333 | VFS core + ~60 filesystems (ext4, btrfs, xfs, nfs, tmpfs, proc, sysfs) |
| `block/` | 76 | 22 | 74,029 | block layer, mq, io schedulers |
| `net/` | 1,459 | 270 | 1,305,555 | socket layer, TCP/IP, netfilter, wireless, ~50 protocol families |
| `sound/` | 1,918 | 834 | 1,711,738 | ALSA |

## Layer E — drivers (75% of the tree)

| Dir | .c | .h | .rs | lines | Contents |
| --- | ---: | ---: | ---: | ---: | --- |
| `drivers/` | 22,661 | 11,762 | 129 | 26,729,651 | everything: gpu (amd/intel/nvidia alone is several M), net, usb, storage, input, virtio |
| `tools/` | 3,998 | 1,334 | 1 | 1,525,503 | perf & userspace utilities — not kernel code, but hosts the test harnesses used in Layer A |

## Totals

- **37,768** `.c` files, **27,300** `.h`, **568** `.rs`
- Non-driver core ≈ **15k** C files / ~5.3M lines; drivers+sound ≈ 24.6k files / **~28.4M lines**
- Rust for Linux already present: `rust/` kernel crate, `pin-init`, `macros`, phy/net bindings

## Dependency order (what calls what)

`init` → `arch/x86` (early mm/console) → `kernel` (sched, printk) → `mm`
→ `fs`/`block`/`ipc` → `net` → `drivers`. `lib/` and `crypto/` sit at the
bottom — everything uses them, they use nothing. `security`/`virt` plug in via
hook points.
