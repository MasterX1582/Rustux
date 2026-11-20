//! pci — PCI config-space enumeration via the legacy CF8/CFC I/O ports.
//!
//! Firmware (OVMF) has already assigned bus numbers and BARs, so a
//! read-only recursive walk discovers the whole topology: bus 0
//! depth-first, following PCI-to-PCI bridges' secondary bus numbers —
//! the same shape as Linux's early pci_direct_probe. Lines go into the
//! printk ring; the caller's next drain flushes them.

use crate::log::printk;
use crate::pio;

const CFG_ADDR: u16 = 0xcf8;
const CFG_DATA: u16 = 0xcfc;

fn cfg_addr(bus: u8, dev: u8, f: u8, off: u8) -> u32 {
    0x8000_0000 | (bus as u32) << 16 | (dev as u32) << 11 | (f as u32) << 8 | (off as u32 & 0xfc)
}

pub fn read32(bus: u8, dev: u8, f: u8, off: u8) -> u32 {
    // SAFETY: CF8/CFC is the architected config mechanism; a read of a
    // nonexistent function returns 0xffffffff, never faults.
    unsafe {
        pio::outl(CFG_ADDR, cfg_addr(bus, dev, f, off));
        pio::inl(CFG_DATA)
    }
}

fn read16(bus: u8, dev: u8, f: u8, off: u8) -> u16 {
    (read32(bus, dev, f, off) >> ((off as u32 & 2) * 8)) as u16
}

fn read8(bus: u8, dev: u8, f: u8, off: u8) -> u8 {
    (read32(bus, dev, f, off) >> ((off as u32 & 3) * 8)) as u8
}

/// Decode one BAR slot: (is_io, base, consumes_next_slot).
fn decode_bar(bus: u8, dev: u8, f: u8, off: u8) -> Option<(bool, u64, bool)> {
    let lo = read32(bus, dev, f, off);
    if lo == 0 || lo == u32::MAX {
        return None;
    }
    if lo & 1 != 0 {
        return Some((true, (lo & !3) as u64, false));
    }
    let mem64 = (lo >> 1) & 3 == 2;
    let base = if mem64 {
        (lo & !0xf) as u64 | (read32(bus, dev, f, off + 4) as u64) << 32
    } else {
        (lo & !0xf) as u64
    };
    Some((false, base, mem64))
}

fn class_name(class: u8, sub: u8) -> &'static str {
    match (class, sub) {
        (0x00, 0x00) => "vga-compat",
        (0x01, 0x00) => "scsi",
        (0x01, 0x01) => "ide",
        (0x01, 0x06) => "sata/ahci",
        (0x02, _) => "network",
        (0x03, _) => "display",
        (0x04, _) => "multimedia",
        (0x06, 0x00) => "host bridge",
        (0x06, 0x01) => "isa bridge",
        (0x06, 0x04) => "pci-pci bridge",
        (0x06, 0x80) => "bridge",
        (0x0c, 0x03) => "usb",
        (0x0c, 0x05) => "smbus",
        _ => "device",
    }
}

/// Depth-first walk of one bus; follows bridges recursively.
fn scan_bus(bus: u8, depth: u8) -> u32 {
    if depth > 4 {
        return 0;
    }
    let mut n = 0;
    for dev in 0..32u8 {
        let mut f = 0u8;
        loop {
            let id = read32(bus, dev, f, 0);
            let vendor = id as u16;
            if vendor == 0xffff || vendor == 0 {
                break;
            }
            n += 1;
            let device = (id >> 16) as u16;
            let class = read8(bus, dev, f, 0x0b);
            let sub = read8(bus, dev, f, 0x0a);
            let progif = read8(bus, dev, f, 0x09);
            let hdr = read8(bus, dev, f, 0x0e);
            printk(format_args!(
                "rustux: pci: {bus:02x}:{dev:02x}.{f} [{vendor:04x}:{device:04x}] \
                 class {class:02x}{sub:02x}.{progif:02x} {}",
                class_name(class, sub)
            ));
            // First two implemented BARs — what a driver binds on.
            let mut off = 0x10u8;
            let mut shown = 0;
            while off <= 0x24 && shown < 2 {
                if let Some((is_io, base, mem64)) = decode_bar(bus, dev, f, off) {
                    printk(format_args!(
                        "rustux: pci:   BAR{} {} @{base:#x}",
                        (off - 0x10) / 4,
                        if is_io { "io  " } else { "mmio" }
                    ));
                    shown += 1;
                    if mem64 {
                        off += 4;
                    }
                }
                off += 4;
            }
            // Header type 1: secondary bus number at 0x19.
            if hdr & 0x7f == 1 {
                let sec = read8(bus, dev, f, 0x19);
                if sec != 0 && sec != bus {
                    printk(format_args!("rustux: pci:   -> secondary bus {sec:02x}"));
                    n += scan_bus(sec, depth + 1);
                }
            }
            // The multi-function bit lives in function 0's header type.
            if (f == 0 && hdr & 0x80 == 0) || f == 7 {
                break;
            }
            f += 1;
        }
    }
    n
}

/// Enumerate every PCI function. Returns the count found.
pub fn scan() -> u32 {
    scan_bus(0, 0)
}
