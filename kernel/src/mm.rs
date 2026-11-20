//! mm — early memory management: own page tables + a physical frame allocator.
//!
//! After ExitBootServices we are still running on OVMF's page tables. This
//! module builds a fresh 4-level identity map covering 0..max(4 GiB, top of
//! physical memory) with 2 MiB large pages — the 4 GiB floor guarantees the
//! framebuffer BAR and other 32-bit MMIO stay reachable — then switches CR3.
//! From that point every ConventionalMemory frame (minus the table carve-out)
//! feeds an intrusive freelist allocator: the first u64 of each free frame
//! stores the next-free link, so the allocator needs zero backing memory.

use uefi::mem::memory_map::MemoryMap;

pub const PAGE: u64 = 4096;
const PAGE_2M: u64 = 2 * 1024 * 1024;
const GIB: u64 = 1 << 30;

const PRESENT: u64 = 1;
const RW: u64 = 1 << 1;
const PS: u64 = 1 << 7; // large page at this level

#[inline]
unsafe fn write64(addr: u64, v: u64) {
    // SAFETY: callers only touch addresses inside the active identity map
    // (table memory pre-switch, free frames post-switch).
    unsafe { (addr as *mut u64).write_volatile(v) }
}

#[inline]
unsafe fn read64(addr: u64) -> u64 {
    // SAFETY: same contract as write64.
    unsafe { (addr as *const u64).read_volatile() }
}

/// GiB of address space the identity map must cover: end of the highest
/// memory-map descriptor, rounded up, but never less than 4 GiB so 32-bit
/// BARs (framebuffer included) remain mapped.
pub fn map_gib<M: MemoryMap>(mmap: &M) -> u64 {
    let mut end = 0u64;
    for d in mmap.entries() {
        let e = d
            .phys_start
            .saturating_add(d.page_count.saturating_mul(PAGE));
        if e > end {
            end = e;
        }
    }
    core::cmp::max(4, end.div_ceil(GIB))
}

/// Number of 4 KiB table frames the identity map needs for `map_gib` GiB:
/// one PML4, one PDPT per 512 GiB, one PD per GiB.
pub fn table_frames(ngib: u64) -> u64 {
    1 + ngib.div_ceil(512) + ngib
}

/// Build the identity map in `table_frames(ngib)` frames at `table_base`
/// (physical). Returns the PML4 physical address.
///
/// Layout: [PML4][PDPT x npdpt][PD x ngib]. PML4[p] -> PDPT p; PDPT[p][i] ->
/// the PD covering GiB 512*p+i; each PD holds 512 x 2 MiB entries.
/// Everything is P|RW — this is a boot map; W^X and userspace splits belong
/// to the real page-table layer.
pub fn build_identity_map<M: MemoryMap>(mmap: &M, table_base: u64) -> u64 {
    let ngib = map_gib(mmap);
    let npdpt = ngib.div_ceil(512);
    let bytes = table_frames(ngib) * PAGE;
    let mut a = table_base;
    while a < table_base + bytes {
        unsafe { write64(a, 0) };
        a += 8;
    }

    let pml4 = table_base;
    // SAFETY: all table frames are inside the carve we own, mapped pre-switch.
    for p in 0..npdpt {
        let pdpt = table_base + (1 + p) * PAGE;
        unsafe { write64(pml4 + p * 8, pdpt | PRESENT | RW) };
    }
    for g in 0..ngib {
        let pd = table_base + (1 + npdpt + g) * PAGE;
        let pdpt = table_base + (1 + g / 512) * PAGE;
        unsafe { write64(pdpt + (g % 512) * 8, pd | PRESENT | RW) };
        for i in 0..512u64 {
            let phys = g * GIB + i * PAGE_2M;
            unsafe { write64(pd + i * 8, phys | PRESENT | RW | PS) };
        }
    }
    pml4
}

/// Switch CR3 to a freshly built PML4.
pub fn load_cr3(pml4_phys: u64) {
    // SAFETY: pml4_phys is a valid 4-level root identity-mapping the address
    // space we execute in; the write implicitly flushes the TLB.
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack));
    }
}

/// Intrusive freelist physical frame allocator. Free frames link through
/// their first u64 (value 0 = list end; frame 0 is never pooled).
pub struct FrameAlloc {
    head: u64,
    free: u64,
}

impl FrameAlloc {
    pub const fn new() -> Self {
        Self { head: 0, free: 0 }
    }

    /// Push every 4 KiB-aligned frame in `[start, end)` that does not
    /// intersect any `[rs, re)` in `reserved`.
    pub fn add_range(&mut self, start: u64, end: u64, reserved: &[(u64, u64)]) {
        let mut a = (start + PAGE - 1) & !(PAGE - 1);
        while a + PAGE <= end {
            let covered = reserved
                .iter()
                .any(|&(rs, re)| a + PAGE > rs && a < re);
            if !covered {
                // SAFETY: `a` is a mapped frame we own; the link lives in it.
                unsafe { write64(a, self.head) };
                self.head = a;
                self.free += 1;
            }
            a += PAGE;
        }
    }

    /// Pop one frame (physical address), or `None` when empty.
    pub fn alloc(&mut self) -> Option<u64> {
        if self.head == 0 {
            return None;
        }
        let a = self.head;
        // SAFETY: `a` came from add_range — mapped, owned, link in first u64.
        self.head = unsafe { read64(a) };
        self.free -= 1;
        Some(a)
    }

    /// Return a frame to the list.
    pub fn free_frame(&mut self, a: u64) {
        // SAFETY: `a` is a mapped frame previously returned by alloc().
        unsafe { write64(a, self.head) };
        self.head = a;
        self.free += 1;
    }

    pub fn free_count(&self) -> u64 {
        self.free
    }
}
