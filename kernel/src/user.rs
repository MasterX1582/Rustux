//! Ring-3 process support: per-process page tables (kernel entries
//! copied supervisor-only), PT_LOAD mapping from the ELF image, a user
//! stack, and the iretq/sysretq privilege-transition pair.
//!
//! The user page table shares the kernel's low PML4 slots (copied
//! verbatim — they were created supervisor-only, so user code cannot
//! touch kernel memory through them), while user pages land in PML4
//! slots 128..256 (elf::USER_VA_LO..HI). There is no scheduler yet:
//! run() is a one-shot drop — the image's exit syscall unwinds the
//! kernel frame saved by user_enter and control returns here.

use crate::{cpu, elf, kalloc, syscall};

const PAGE: u64 = 0x1000;
const PRESENT: u64 = 1;
const WRITE: u64 = 2;
const USER: u64 = 4;
const NX: u64 = 1 << 63;

/// User stack: 16 pages high in the user window.
const USTACK_VA: u64 = 0x5556_0000_0000;
const USTACK_PAGES: u64 = 16;

/// Maximum distinct user VA ranges recorded for syscall validation.
const MAX_RANGES: usize = 8;

struct Proc {
    /// Physical base of the process PML4.
    pml4: u64,
    entry: u64,
    stack_top: u64,
    /// [lo, hi) address ranges user pointers may legally reference.
    /// The syscall layer's user-pointer gate walks exactly this table.
    ranges: [(u64, u64); MAX_RANGES],
    nranges: usize,
}

/// The single resident process — none until run() loads an image.
static mut CUR: Option<Proc> = None;

fn rd64(p: u64) -> u64 {
    // SAFETY: callers only name page-table frames inside the identity
    // map; the kernel runs under an identity mapping.
    unsafe { (p as *const u64).read_volatile() }
}

fn wr64(p: u64, v: u64) {
    // SAFETY: see rd64.
    unsafe { (p as *mut u64).write_volatile(v) }
}

/// Map `va` -> a fresh zeroed frame under `pml4`, creating PDPT/PD/PT
/// tables on demand. Intermediate entries get PRESENT|RW|USER (CPL3
/// traversal needs U/S at every level); the leaf carries `flags`.
/// Returns the backing frame's physical address. A page already
/// present keeps its frame and ORs in the new permissions — the union
/// semantics Linux applies to overlapping segments.
fn map_page(pml4: u64, va: u64, flags: u64) -> Option<u64> {
    let mut tab = pml4;
    for shift in [39u64, 30, 21] {
        let idx = ((va >> shift) & 0x1ff) as usize;
        let e = rd64(tab + 8 * idx as u64);
        if e & PRESENT != 0 {
            if e & (1 << 7) != 0 {
                return None; // collides with a huge page
            }
            tab = e & !0xfff;
        } else {
            let f = kalloc::alloc_frame()?;
            wr64(tab + 8 * idx as u64, f | PRESENT | WRITE | USER);
            tab = f;
        }
    }
    let idx = ((va >> 12) & 0x1ff) as usize;
    let ent = tab + 8 * idx as u64;
    let e = rd64(ent);
    if e & PRESENT != 0 {
        wr64(ent, (e & !0xfff) | flags | PRESENT);
        return Some(e & !0xfff);
    }
    let f = kalloc::alloc_frame()?;
    wr64(ent, f | flags | PRESENT);
    Some(f)
}

/// Load `img` into a fresh address space and remember it as CUR.
/// Returns false on any malformed input or allocation failure — the
/// partially built table is abandoned (frames leak; acceptable in a
/// one-shot demo, documented for the real task model).
fn load(img: &[u8]) -> bool {
    let Some(e) = elf::parse(img) else {
        return false;
    };
    let Some(pml4) = kalloc::alloc_frame() else {
        return false;
    };
    // Copy the kernel PML4 — its entries are supervisor-only, so user
    // code can't reach kernel memory through them even though the
    // mappings are shared.
    let kpml4 = cpu::read_cr3() & !0xfff;
    for i in 0..512u64 {
        let ent = rd64(kpml4 + 8 * i);
        if ent & PRESENT != 0 {
            wr64(pml4 + 8 * i, ent);
        }
    }
    let mut proc = Proc {
        pml4,
        entry: e.entry,
        stack_top: 0,
        ranges: [(0, 0); MAX_RANGES],
        nranges: 0,
    };
    for p in e.phdrs(img) {
        if p.ty != elf::PT_LOAD {
            continue;
        }
        // Header fields are hostile: filesz<=memsz, file range inside
        // the image, virtual range inside user space — all checked.
        if p.filesz > p.memsz {
            return false;
        }
        let Some(fend) = p.off.checked_add(p.filesz) else {
            return false;
        };
        if fend > img.len() as u64 {
            return false;
        }
        let Some(mend) = p.va.checked_add(p.memsz) else {
            return false;
        };
        if p.memsz == 0 {
            continue;
        }
        if p.va < elf::USER_VA_LO || mend > elf::USER_VA_HI {
            return false;
        }
        let mut fl = USER;
        if p.flags & elf::PF_W != 0 {
            fl |= WRITE;
        }
        if p.flags & elf::PF_X == 0 {
            fl |= NX;
        }
        // mend < 2^47, so +0xfff is safe after the range check above.
        let va0 = p.va & !0xfff;
        let va1 = (mend + 0xfff) & !0xfff;
        let mut v = va0;
        while v < va1 {
            let Some(frame) = map_page(pml4, v, fl) else {
                return false;
            };
            // Copy the overlap of this page with [va, va+filesz).
            let lo = core::cmp::max(v, p.va);
            let hi = core::cmp::min(v + PAGE, p.va + p.filesz);
            if lo < hi {
                let src = p.off + (lo - p.va);
                // SAFETY: frame is an owned page in the identity map;
                // n <= PAGE and src..src+n <= img.len() (fend check).
                unsafe {
                    ((frame + (lo - v)) as *mut u8)
                        .copy_from(img.as_ptr().add(src as usize), (hi - lo) as usize);
                }
            }
            v += PAGE;
        }
        if proc.nranges < MAX_RANGES {
            proc.ranges[proc.nranges] = (p.va, mend);
            proc.nranges += 1;
        }
    }
    for i in 0..USTACK_PAGES {
        if map_page(pml4, USTACK_VA + i * PAGE, USER | WRITE | NX).is_none() {
            return false;
        }
    }
    proc.stack_top = USTACK_VA + USTACK_PAGES * PAGE;
    if proc.nranges < MAX_RANGES {
        proc.ranges[proc.nranges] = (USTACK_VA, proc.stack_top);
        proc.nranges += 1;
    }
    // SAFETY: sole writer — loading is a main-context operation.
    unsafe { *core::ptr::addr_of_mut!(CUR) = Some(proc) };
    true
}

/// Is `[va, va+len)` wholly inside a mapped user range? This is THE
/// user-pointer gate for the syscall layer — checked arithmetic only.
pub fn range_ok(va: u64, len: u64) -> bool {
    if len == 0 {
        return false;
    }
    let Some(end) = va.checked_add(len) else {
        return false;
    };
    // SAFETY: read-only walk of the resident-process table.
    let cur = unsafe { &*core::ptr::addr_of!(CUR) };
    cur.as_ref().is_some_and(|p| {
        p.ranges[..p.nranges]
            .iter()
            .any(|&(lo, hi)| va >= lo && end <= hi)
    })
}

/// Load and enter `img` on ring 3. Returns false if the image is
/// malformed or memory runs out; returns true after the image's exit
/// syscall unwinds back here — a one-shot process for now.
pub fn run(img: &[u8]) -> bool {
    if !load(img) {
        return false;
    }
    crate::log::printk(format_args!("rustux: user: image loaded, entering ring 3"));
    cpu::init_syscall(syscall::entry_addr());
    // SAFETY: CUR was just populated by load().
    let p = unsafe { &*core::ptr::addr_of!(CUR) }.as_ref();
    let Some(p) = p else { return false };
    syscall::enter(p.entry, p.stack_top, p.pml4);
    true
}
