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
const RCTL: usize = 0x0100;
const RDBAL: usize = 0x2800;
const RDBAH: usize = 0x2804;
const RDLEN: usize = 0x2808;
const RDH: usize = 0x2810;
const RDT: usize = 0x2818;

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

const N_RX: usize = 8;
const RX_BUF: usize = 2048; // per-descriptor packet buffer
const RCTL_EN: u32 = 1 << 1;
const RCTL_UPE: u32 = 1 << 3; // unicast promiscuous
const RCTL_MPE: u32 = 1 << 4; // multicast promiscuous
const RCTL_BAM: u32 = 1 << 15; // broadcast accept
const RXD_STA_DD: u8 = 0x01;
const RXD_STA_EOP: u8 = 0x02;

/// Legacy RX descriptor (16 bytes). The NIC writes len/sta back when a
/// frame lands in the buffer we posted.
#[repr(C)]
struct RxDesc {
    addr: u64,
    len: u16,
    cksum: u16,
    sta: u8,
    err: u8,
    special: u16,
}

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

/// Bring up the RX ring: 8 descriptors, each pointing at its own 2 KiB
/// buffer. Returns (ring, buffers) — both buddy pages, kept for the
/// device lifetime. Hardware owns descriptors 0..N_RX-2 (RDT points one
/// past the last usable, the full/empty ambiguity keeps one free).
fn rx_init(base: usize) -> Option<(*mut RxDesc, *mut u8)> {
    let ring = kalloc::kmalloc(4096) as *mut RxDesc;
    let bufs = kalloc::kmalloc(N_RX * RX_BUF);
    if ring.is_null() || bufs.is_null() {
        printk(format_args!("rustux: e1000: rx ring/buf alloc failed"));
        return None;
    }
    // SAFETY: ring names a page (8*16 desc bytes fit); bufs names
    // N_RX*RX_BUF contiguous bytes — each descriptor gets its slice.
    unsafe {
        core::ptr::write_bytes(ring, 0, N_RX);
        for i in 0..N_RX {
            (*ring.add(i)).addr = bufs.add(i * RX_BUF) as u64;
        }
        wr(base, RDBAL, ring as u32);
        wr(base, RDBAH, ((ring as usize) >> 32) as u32);
        wr(base, RDLEN, (N_RX * 16) as u32);
        wr(base, RDH, 0);
        wr(base, RDT, 0);
        // Accept everything for the proof: promiscuous + broadcast.
        wr(base, RCTL, RCTL_EN | RCTL_UPE | RCTL_MPE | RCTL_BAM);
        wr(base, RDT, (N_RX - 1) as u32);
    }
    Some((ring, bufs))
}

/// Round-trip proof: broadcast an ARP who-has for QEMU's slirp gateway
/// (10.0.2.2), then poll the RX ring for the unicast reply. This is a
/// real frame leaving and a real frame arriving through our own rings.
pub fn arp_selftest(base: usize, mac: [u8; 6]) -> bool {
    let Some((ring, bufs)) = rx_init(base) else {
        return false;
    };
    printk(format_args!("rustux: e1000: rx ring up ({N_RX} descs x {RX_BUF}B)"));

    // Ethernet + ARP request, padded to the 60-byte TX minimum.
    let mut f = [0u8; 60];
    f[..6].copy_from_slice(&[0xff; 6]); // dst: broadcast
    f[6..12].copy_from_slice(&mac); // src: us
    f[12] = 0x08;
    f[13] = 0x06; // ethertype ARP
    f[14..22].copy_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]); // ht/pt/hl/pl/req
    f[22..28].copy_from_slice(&mac); // sender hw
    f[28..32].copy_from_slice(&[10, 0, 2, 15]); // sender proto (slirp guest)
    // target hw = zeros (that's the question), target proto:
    f[38..42].copy_from_slice(&[10, 0, 2, 2]); // slirp gateway
    if !tx_once(base, &f) {
        return false;
    }

    // Poll every posted descriptor for DD; report whatever arrives.
    let mut i = 0u32;
    loop {
        for d in 0..N_RX {
            // SAFETY: the NIC writes sta/len into our ring; the packet
            // lands in the buffer we posted for this descriptor.
            let (sta, len) = unsafe { ((*ring.add(d)).sta, (*ring.add(d)).len) };
            if sta & RXD_STA_DD == 0 {
                continue;
            }
            let p = unsafe { bufs.add(d * RX_BUF) };
            let et = unsafe { u16::from_be_bytes([*p.add(12), *p.add(13)]) };
            if et == 0x0806 && len >= 42 {
                // ARP reply: oper at eth+20, sender hw/proto at +22/+28.
                let (op, sha, spa) = unsafe {
                    (
                        u16::from_be_bytes([*p.add(20), *p.add(21)]),
                        core::slice::from_raw_parts(p.add(22), 6),
                        core::slice::from_raw_parts(p.add(28), 4),
                    )
                };
                printk(format_args!(
                    "rustux: e1000: rx ARP op{op} from {}.{}.{}.{} \
                     mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ({len}B desc{d})",
                    spa[0], spa[1], spa[2], spa[3],
                    sha[0], sha[1], sha[2], sha[3], sha[4], sha[5]
                ));
                return op == 2;
            }
            printk(format_args!(
                "rustux: e1000: rx {len}B frame et{et:#06x} desc{d} \
                 sta{sta:#04x}{}",
                if sta & RXD_STA_EOP != 0 { " eop" } else { "" }
            ));
            // Not our ARP — recycle the descriptor so more can arrive.
            // RDT is one-past-last-posted (next_to_use): reposting d
            // makes it the tail again at d+1.
            // SAFETY: reposting a consumed descriptor.
            unsafe {
                (*ring.add(d)).sta = 0;
                wr(base, RDT, ((d + 1) % N_RX) as u32);
            }
        }
        i += 1;
        if i == 20_000_000 {
            printk(format_args!("rustux: e1000: rx poll timed out"));
            return false;
        }
    }
}
