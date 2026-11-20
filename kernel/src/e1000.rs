//! e1000 — Intel 82574L (PCIe GbE) driver skeleton.
//!
//! Two rungs: bind the function firmware discovered (enable memory
//! space + bus mastering, read the station MAC), then prove the DMA
//! datapath — reset, a legacy TX descriptor ring carved from one buddy
//! page, one broadcast frame, and the DD write-back bit the NIC sets
//! when it has DMA'd the buffer out of RAM. The kernel's identity map
//! makes virt == phys, which is exactly what the descriptor wants.

use crate::kalloc;
use crate::log::printk;
use crate::pci;

const VENDOR: u16 = 0x8086;
const DEVICE: u16 = 0x10d3; // 82574L, QEMU's e1000e model

// Register offsets within BAR0 (82574 spec).
const CTRL: usize = 0x0000;
const STATUS: usize = 0x0008;
const IMC: usize = 0x00d8; // write 1s mask all interrupt causes
const ICR: usize = 0x00c0; // read-to-clear cause register
const TCTL: usize = 0x0400;
const TIPG: usize = 0x0410;
const RAL0: usize = 0x5400;
const RAH0: usize = 0x5404;
const TDBAL: usize = 0x3800;
const TDBAH: usize = 0x3804;
const TDLEN: usize = 0x3808;
const TDH: usize = 0x3810;
const TDT: usize = 0x3818;

const CTRL_SLU: u32 = 1 << 6; // set link up
const CTRL_RST: u32 = 1 << 26; // device reset (self-clearing)
const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;
const TCTL_CT: u32 = 0x10 << 4; // collision threshold
const TCTL_COLD: u32 = 0x40 << 12; // full-duplex backoff distance
const TIPG_DEF: u32 = 10 | (8 << 10) | (6 << 20); // spec defaults

const N_TX: usize = 8; // descriptors in the ring (16 B each)
const TXD_EOP: u8 = 0x01;
const TXD_IFCS: u8 = 0x02;
const TXD_RS: u8 = 0x08;
const TXD_STA_DD: u8 = 0x01;

/// Legacy TX descriptor (16 bytes), 8254x/82574 layout.
#[repr(C)]
struct TxDesc {
    addr: u64,
    len: u16,
    cso: u8,
    cmd: u8,
    sta: u8,
    _rsv: u8,
    special: u16,
}

#[inline]
unsafe fn rd(base: usize, off: usize) -> u32 {
    unsafe { ((base + off) as *const u32).read_volatile() }
}

#[inline]
unsafe fn wr(base: usize, off: usize, v: u32) {
    unsafe { ((base + off) as *mut u32).write_volatile(v) }
}

/// Probe for the NIC. `is_mmio` must confirm the claimed BAR does not
/// overlap RAM before we touch it. On success returns (MMIO base, MAC).
pub fn probe(is_mmio: impl Fn(u64, u64) -> bool) -> Option<(usize, [u8; 6])> {
    let (bus, dev, f) = pci::find(VENDOR, DEVICE)?;
    printk(format_args!(
        "rustux: e1000: found {VENDOR:04x}:{DEVICE:04x} at {bus:02x}:{dev:02x}.{f}"
    ));

    // Decode BAR0: must be a memory BAR outside conventional RAM —
    // this driver is about to *write* through it.
    let bar = pci::bar_raw(bus, dev, f, 0);
    if bar == 0 || bar == u32::MAX || bar & 1 != 0 {
        printk(format_args!("rustux: e1000: BAR0 {bar:#x} not a usable MMIO BAR"));
        return None;
    }
    let base = (bar & !0xf) as usize;
    if !is_mmio(base as u64, 0x2_0000) {
        // 82574 BAR0 is 128 KiB.
        printk(format_args!(
            "rustux: e1000: BAR0 {base:#x} overlaps RAM -- refusing to bind"
        ));
        return None;
    }

    let cmd = pci::command_or(bus, dev, f, pci::CMD_MEM_SPACE | pci::CMD_BUS_MASTER);
    if cmd & pci::CMD_MEM_SPACE == 0 {
        printk(format_args!(
            "rustux: e1000: memory space did not enable (cmd {cmd:#x})"
        ));
        return None;
    }

    // SAFETY: `base` is the firmware-assigned BAR, validated non-RAM
    // above and inside our identity map; reads are device registers.
    let (status, mac) = unsafe {
        let ral = rd(base, RAL0);
        let rah = rd(base, RAH0);
        let m = [
            ral as u8,
            (ral >> 8) as u8,
            (ral >> 16) as u8,
            (ral >> 24) as u8,
            rah as u8,
            (rah >> 8) as u8,
        ];
        (rd(base, STATUS), m)
    };
    printk(format_args!(
        "rustux: e1000: MMIO @{base:#x} status={status:#x} \
         mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
    if status & (1 << 1) != 0 {
        printk(format_args!("rustux: e1000: link UP"));
    }
    Some((base, mac))
}

/// Reset the NIC: CTRL.RST self-clears when the core comes back.
fn reset(base: usize) -> bool {
    // SAFETY: base is our bound MMIO window.
    unsafe {
        wr(base, CTRL, rd(base, CTRL) | CTRL_RST);
    }
    let mut i = 0u32;
    // SAFETY: bounded poll on the same window.
    while unsafe { rd(base, CTRL) } & CTRL_RST != 0 && i < 1_000_000 {
        i += 1;
    }
    i < 1_000_000
}

/// Transmit one frame through the legacy TX ring. `frame` is the raw
/// Ethernet frame. Returns true when the NIC reports DD (done).
pub fn tx_once(base: usize, frame: &[u8]) -> bool {
    // One page: [TxDesc x N_TX][packet buffer]. Buddy pages are
    // physically contiguous and identity-mapped, so addr == phys.
    let page = kalloc::kmalloc(4096);
    if page.is_null() {
        printk(format_args!("rustux: e1000: tx page alloc failed"));
        return false;
    }
    let ring = page as *mut TxDesc;
    // SAFETY: page names 4096 owned bytes; N_TX*16 fits the front half,
    // the packet lands at +2048 with headroom to 1518+ bytes.
    unsafe {
        core::ptr::write_bytes(ring, 0, N_TX);
    }
    let buf = unsafe { page.add(2048) };
    if frame.len() > 2048 {
        return false;
    }
    // SAFETY: buf names 2048 owned bytes in the same page.
    unsafe {
        core::ptr::copy_nonoverlapping(frame.as_ptr(), buf, frame.len());
    }

    // SAFETY: `base` is the bound MMIO window; offsets are the 82574
    // TX-block registers. Identity map => &*ring is also the DMA addr.
    unsafe {
        wr(base, IMC, u32::MAX); // polling mode: mask every cause
        let _ = rd(base, ICR); // drain pending causes
        wr(base, TDBAL, ring as u32);
        wr(base, TDBAH, ((ring as usize) >> 32) as u32);
        wr(base, TDLEN, (N_TX * 16) as u32);
        wr(base, TDH, 0);
        wr(base, TDT, 0);
        wr(base, TIPG, TIPG_DEF);
        wr(base, TCTL, TCTL_EN | TCTL_PSP | TCTL_CT | TCTL_COLD);
    }

    // Program descriptor 0; TDT = 1 rings the doorbell.
    // SAFETY: ring names N_TX owned descriptors in the page.
    unsafe {
        let d = &mut *ring;
        d.addr = buf as u64;
        d.len = frame.len() as u16;
        d.cso = 0;
        d.cmd = TXD_EOP | TXD_IFCS | TXD_RS;
        d.sta = 0;
        d.special = 0;
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        wr(base, TDT, 1);
    }

    // Poll DD: the NIC DMAs the frame, transmits it, writes sta back.
    let mut i = 0u32;
    // SAFETY: bounded volatile read of our own descriptor.
    let done = loop {
        if unsafe { (*ring).sta } & TXD_STA_DD != 0 {
            break true;
        }
        i += 1;
        if i == 10_000_000 {
            break false;
        }
    };
    // SAFETY: reads on our ring + registers.
    let (tdh, tdt) = unsafe { (rd(base, TDH), rd(base, TDT)) };
    printk(format_args!(
        "rustux: e1000: tx {}-byte frame {} (TDH {tdh}/TDT {tdt})",
        frame.len(),
        if done { "DONE" } else { "TIMEOUT" }
    ));
    done
}

/// Full datapath self-test: reset, ring bring-up, one broadcast frame.
pub fn tx_selftest(base: usize, mac: [u8; 6]) -> bool {
    if !reset(base) {
        printk(format_args!("rustux: e1000: reset timed out"));
        return false;
    }
    // SAFETY: post-reset register write on the bound window — keep the
    // link forced up and mask interrupts for the polling proof.
    unsafe {
        wr(base, CTRL, rd(base, CTRL) | CTRL_SLU);
        wr(base, IMC, u32::MAX);
    }
    printk(format_args!("rustux: e1000: reset ok, IRQs masked (polling TX)"));

    // Broadcast frame, experimental ethertype 0x88b5, "rustux" payload.
    let mut frame = [0u8; 64];
    frame[..6].copy_from_slice(&[0xff; 6]);
    frame[6..12].copy_from_slice(&mac);
    frame[12] = 0x88;
    frame[13] = 0xb5;
    frame[14..20].copy_from_slice(b"rustux");
    tx_once(base, &frame[..60])
}
