//! e1000 — Intel 82574L (PCIe GbE) driver skeleton.
//!
//! First rung: bind on the function firmware discovered, enable memory
//! space + bus mastering in the PCI command register, and read the
//! station MAC address out of RAL0/RAH0 through the BAR0 MMIO window.
//! The kernel's identity map already covers the BAR region.

use crate::log::printk;
use crate::pci;

const VENDOR: u16 = 0x8086;
const DEVICE: u16 = 0x10d3; // 82574L, QEMU's e1000e model

// Register offsets within BAR0 (82574 spec).
const CTRL: usize = 0x0000;
const STATUS: usize = 0x0008;
const RAL0: usize = 0x5400;
const RAH0: usize = 0x5404;

#[inline]
unsafe fn rd(base: usize, off: usize) -> u32 {
    unsafe { ((base + off) as *const u32).read_volatile() }
}

/// Probe for the NIC. On success prints MAC + link status and returns
/// the MMIO base so callers can hold the binding.
pub fn probe() -> Option<usize> {
    let (bus, dev, f) = pci::find(VENDOR, DEVICE)?;
    printk(format_args!("rustux: e1000: found {VENDOR:04x}:{DEVICE:04x} at {bus:02x}:{dev:02x}.{f}"));

    // Decode BAR0: must be a memory BAR.
    let bar = pci::bar_raw(bus, dev, f, 0);
    if bar == 0 || bar == u32::MAX || bar & 1 != 0 {
        printk(format_args!("rustux: e1000: BAR0 {bar:#x} not a usable MMIO BAR"));
        return None;
    }
    let base = (bar & !0xf) as usize;

    // Memory space + bus mastering — required before touching MMIO
    // (DMA comes later, but the bit goes on now, like e1000e_probe).
    let cmd = pci::command_or(bus, dev, f, pci::CMD_MEM_SPACE | pci::CMD_BUS_MASTER);
    if cmd & pci::CMD_MEM_SPACE == 0 {
        printk(format_args!("rustux: e1000: memory space did not enable (cmd {cmd:#x})"));
        return None;
    }

    // SAFETY: `base` is the firmware-assigned BAR inside our identity
    // map; reads are pure device-register loads.
    let (ctrl, status, mac) = unsafe {
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
        (rd(base, CTRL), rd(base, STATUS), m)
    };
    printk(format_args!(
        "rustux: e1000: MMIO @{base:#x} ctrl={ctrl:#x} status={status:#x} \
         mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
    if status & (1 << 1) != 0 {
        printk(format_args!("rustux: e1000: link UP"));
    }
    Some(base)
}
