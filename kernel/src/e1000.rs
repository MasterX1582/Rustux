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
use core::sync::atomic::Ordering;

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
const IMS: usize = 0x00d0; // write 1s unmask causes
const IVAR: usize = 0x00e4; // cause -> MSI-X vector map
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

// ICR/IMS interrupt-cause bits we use (82574).
const ICR_RXDMT0: u32 = 1 << 4; // rx desc minimum threshold hit
const ICR_RXT0: u32 = 1 << 7; // rx timer expired
const ICR_RXQ0: u32 = 1 << 20; // rx queue 0 (the MSI-X-era cause)
const ICR_OTHER: u32 = 1 << 24; // link/status group

/// Device coordinates, saved at probe so late init can program MSI.
static BDF: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(u32::MAX);
/// MSI interrupts delivered — read for the boot proof.
pub static IRQS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
static MMIO: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// Gateway MAC captured from the ARP reply (packed big-endian in the
/// low 48 bits) — the first entry of our ARP cache.
static GW_MAC: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
/// Gateway IPv4 captured with it (network byte order as u32).
static GW_IP: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

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
    BDF.store((bus as u32) << 8 | (dev as u32) << 3 | f as u32, Ordering::Relaxed);
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
    MMIO.store(base, Ordering::Relaxed);
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
    // Interrupt masking is the CALLER's choice — masking here would
    // clobber an IMS unmask done before the transmit (MSI-X path).
    unsafe {
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

/// MSI interrupt handler — the device wrote to the LAPIC's message
/// window; reading ICR acknowledges the causes device-side.
pub fn on_irq() {
    let base = MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    // SAFETY: base is the bound MMIO window; ICR is read-to-clear.
    let icr = unsafe { rd(base, ICR) };
    IRQS.fetch_add(1, Ordering::Relaxed);
    if icr & (ICR_RXDMT0 | ICR_RXT0 | ICR_RXQ0) != 0 {
        RX_IRQS.fetch_add(1, Ordering::Relaxed);
    }
}

/// RX-caused MSI count — the IRQ-side proof a packet triggered it.
pub static RX_IRQS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Interrupt-driven receive: enable MSI-X (per-vector message writes
/// to the LAPIC instead of an IOAPIC line), IVAR-map the RXQ0 cause to
/// table vector 0, unmask it in IMS, transmit the ARP request, then
/// wait for vector IRQ_NET to fire.
/// Requires interrupts already enabled (`sti`).
pub fn irq_selftest(base: usize, mac: [u8; 6]) -> bool {
    let bdf = BDF.load(Ordering::Relaxed);
    if bdf == u32::MAX {
        return false;
    }
    let (bus, dev, f) = (
        (bdf >> 8) as u8,
        ((bdf >> 3) & 0x1f) as u8,
        (bdf & 7) as u8,
    );
    let dest = crate::apic::lapic_id() as u8;
    let Some(cap) = pci::msix_cap(bus, dev, f) else {
        printk(format_args!("rustux: e1000: no MSI-X capability"));
        return false;
    };
    // Vector 0 carries the RX interrupt; the other four stay masked.
    // SAFETY: `cap.table` is the device MSI-X BAR window the
    // capability decoded — inside our identity map.
    unsafe {
        pci::msix_set(&cap, 0, crate::apic::IRQ_NET as u8, dest, false);
        for v in 1..cap.nvec {
            pci::msix_set(&cap, v, 0, 0, true);
        }
    }
    pci::msix_enable(bus, dev, f, &cap);
    printk(format_args!(
        "rustux: e1000: MSI-X on ({} vectors, table @{:#x}) -> vec {:#x} dest-lapic {dest}",
        cap.nvec,
        cap.table,
        crate::apic::IRQ_NET
    ));

    let Some((ring, bufs)) = rx_init(base) else {
        return false;
    };
    // IVAR maps causes to vectors: RXQ0 -> entry 0 = MSI-X vector 0
    // (valid bit set). Then unmask RXQ0 + OTHER in IMS.
    // SAFETY: bound window, architected registers.
    unsafe {
        wr(base, IVAR, 0x8);
        wr(base, IMS, ICR_RXQ0 | ICR_OTHER);
        let _ = rd(base, ICR); // clear stale causes
    }

    // Same ARP probe as the polling test — this time the reply raises
    // vector 0xf3 instead of being discovered by a DD poll.
    let mut fr = [0u8; 60];
    fr[..6].copy_from_slice(&[0xff; 6]);
    fr[6..12].copy_from_slice(&mac);
    fr[12] = 0x08;
    fr[13] = 0x06;
    fr[14..22].copy_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]);
    fr[22..28].copy_from_slice(&mac);
    fr[28..32].copy_from_slice(&[10, 0, 2, 15]);
    fr[38..42].copy_from_slice(&[10, 0, 2, 2]);
    let before = IRQS.load(Ordering::Relaxed);
    if !tx_once(base, &fr) {
        return false;
    }

    // Up to ~1.5s: hlt until the MSI (or any IRQ) wakes us.
    let t0 = crate::apic::TICKS.load(Ordering::Relaxed);
    let mut got_reply = false;
    while IRQS.load(Ordering::Relaxed) == before
        && crate::apic::TICKS.load(Ordering::Relaxed) - t0 < 150
    {
        // SAFETY: hlt wakes on any delivered IRQ; IF is set.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
    }
    let delivered = IRQS.load(Ordering::Relaxed) - before;
    // The IRQ already fired — now just harvest the reply descriptor.
    for d in 0..N_RX {
        // SAFETY: NIC wrote sta/len; buffer is ours.
        let (sta, len) = unsafe { ((*ring.add(d)).sta, (*ring.add(d)).len) };
        if sta & RXD_STA_DD != 0 && len >= 42 {
            let p = unsafe { bufs.add(d * RX_BUF) };
            if unsafe { *p.add(12) } == 0x08 && unsafe { *p.add(13) } == 0x06 {
                got_reply = true;
            }
        }
    }
    printk(format_args!(
        "rustux: e1000: {} MSI IRQs on vec {:#x} ({} rx-caused), reply {}",
        delivered,
        crate::apic::IRQ_NET,
        RX_IRQS.load(Ordering::Relaxed),
        if got_reply { "received" } else { "missing" }
    ));
    delivered > 0 && got_reply
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
            // Validate the device-written fields before touching the
            // buffer (CVE-2009-1385 class): len is untrusted — it must
            // describe a complete (EOP), minimally-sized frame that fits
            // the posted buffer. Anything else is reposted, not parsed.
            if sta & RXD_STA_EOP == 0
                || len < 14
                || len as usize > RX_BUF
            {
                printk(format_args!(
                    "rustux: e1000: rx {len}B sta{sta:#04x} invalid, dropping"
                ));
                unsafe {
                    (*ring.add(d)).sta = 0;
                    wr(base, RDT, ((d + 1) % N_RX) as u32);
                }
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
                // Cache the responder — ARP cache entry #1.
                GW_MAC.store(
                    u64::from_be_bytes([
                        0, 0, sha[0], sha[1], sha[2], sha[3], sha[4], sha[5],
                    ]),
                    Ordering::Relaxed,
                );
                GW_IP.store(
                    u32::from_be_bytes([spa[0], spa[1], spa[2], spa[3]]),
                    Ordering::Relaxed,
                );
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

/// IPv4/ICMP round-trip: ping the slirp gateway we learned via ARP.
/// First real protocol proof — the request is checksummed by us, and
/// every field of the reply is validated by net:: (length, version,
/// checksum, non-fragment, echo-reply type, id/seq match, source IP).
pub fn ping_selftest(base: usize, mac: [u8; 6]) -> bool {
    let gw = GW_MAC.load(Ordering::Relaxed);
    let gw_ip_raw = GW_IP.load(Ordering::Relaxed);
    if gw == 0 || gw_ip_raw == 0 {
        printk(format_args!("rustux: e1000: ping: no ARP cache entry"));
        return false;
    }
    let gw_mac: [u8; 6] = gw.to_be_bytes()[2..8].try_into().unwrap();
    let gw_ip: [u8; 4] = gw_ip_raw.to_be_bytes();
    let our_ip = [10, 0, 2, 15u8];
    let peer = crate::net::NetAddr { mac: gw_mac, ip: gw_ip };
    let us = crate::net::NetAddr { mac, ip: our_ip };

    let Some((ring, bufs)) = rx_init(base) else {
        return false;
    };

    const ID: u16 = 0x7278; // "rx"
    const SEQ: u16 = 1;
    let mut f = [0u8; 128];
    let flen = crate::net::build_echo_request(
        &mut f, &peer, &us, ID, SEQ, b"rustux ping",
    );
    if flen == 0 {
        return false;
    }
    // Pad to the 60-byte TX minimum; the pad isn't in IP total_len.
    if !tx_once(base, &f[..flen.max(60)]) {
        return false;
    }

    let mut i = 0u32;
    loop {
        for d in 0..N_RX {
            // SAFETY: as in arp_selftest — NIC writes sta/len, buffer
            // is ours.
            let (sta, len) = unsafe { ((*ring.add(d)).sta, (*ring.add(d)).len) };
            if sta & RXD_STA_DD == 0 {
                continue;
            }
            if sta & RXD_STA_EOP == 0 || len < 14 || len as usize > RX_BUF {
                unsafe {
                    (*ring.add(d)).sta = 0;
                    wr(base, RDT, ((d + 1) % N_RX) as u32);
                }
                continue;
            }
            let buf = unsafe {
                core::slice::from_raw_parts(bufs.add(d * RX_BUF), len as usize)
            };
            let et = u16::from_be_bytes([buf[12], buf[13]]);
            let mut done = false;
            if et == 0x0800 {
                if let Some(ip) = crate::net::parse_ipv4(&buf[14..]) {
                    if ip.proto == 1 && ip.src == gw_ip && ip.dst == our_ip {
                        if let Some(m) = crate::net::parse_icmp(ip.payload) {
                            if m.ty == 0 && m.code == 0
                                && m.id == ID && m.seq == SEQ
                            {
                                printk(format_args!(
                                    "rustux: e1000: ping reply \
                                     {}.{}.{}.{} seq={} id={:#06x} \
                                     {}B icmp_rest={}B",
                                    ip.src[0], ip.src[1], ip.src[2],
                                    ip.src[3], m.seq, m.id, len,
                                    m.rest.len()
                                ));
                                return true;
                            }
                            printk(format_args!(
                                "rustux: e1000: rx icmp ty{} id{:#06x} \
                                 seq{} (not ours)",
                                m.ty, m.id, m.seq
                            ));
                        }
                    }
                }
                done = true;
            }
            if done || et != 0x0806 {
                printk(format_args!(
                    "rustux: e1000: rx {len}B frame et{et:#06x} desc{d} \
                     (ping wait)"
                ));
            }
            // Not our reply — repost and keep waiting.
            // SAFETY: reposting a consumed descriptor.
            unsafe {
                (*ring.add(d)).sta = 0;
                wr(base, RDT, ((d + 1) % N_RX) as u32);
            }
        }
        i += 1;
        if i == 20_000_000 {
            printk(format_args!("rustux: e1000: ping timed out"));
            return false;
        }
    }
}

/// IPv8 two-node demo (draft-thain-ipv8, experimental).
/// Role is selected by our MAC's last byte: 0x01 = initiator,
/// 0x02 = responder; anything else skips (normal slirp boots).
/// The initiator broadcasts an IPv8 echo request (ARP8 does not
/// exist — its spec was never submitted); the responder learns the
/// sender's MAC from the received frame and unicasts the reply.
/// Addresses: A = RN 0.0.0.1 / 10.0.0.1, B = RN 0.0.0.2 / 10.0.0.2.
pub fn ipv8_demo(base: usize, mac: [u8; 6]) {
    let role = mac[5];
    if role != 0x01 && role != 0x02 {
        return; // single-node boot: no peer configured
    }
    let us8 = crate::net::Addr8 {
        rn: u32::from_be_bytes([0, 0, 0, role]),
        local: [10, 0, 0, role],
    };
    let Some((ring, bufs)) = rx_init(base) else {
        return;
    };
    const ID: u16 = 0x8088;
    const SEQ: u16 = 1;

    if role == 0x01 {
        printk(format_args!("rustux: ipv8: initiator up, broadcast echo req"));
        let peer8 = crate::net::Addr8 {
            rn: 0x00000002,
            local: [10, 0, 0, 2],
        };
        let mut f = [0u8; 128];
        let flen = crate::net::build_ipv8_echo(
            &mut f, 8, [0xff; 6], mac, &peer8, &us8, ID, SEQ, b"rustux ipv8",
        );
        if flen == 0 {
            return;
        }
        let mut sent = false;
        let mut rounds = 0u32;
        loop {
            for d in 0..N_RX {
                // SAFETY: same contract as arp_selftest.
                let (sta, len) =
                    unsafe { ((*ring.add(d)).sta, (*ring.add(d)).len) };
                if sta & RXD_STA_DD == 0 {
                    continue;
                }
                if sta & RXD_STA_EOP == 0 || len < 14 || len as usize > RX_BUF {
                    unsafe {
                        (*ring.add(d)).sta = 0;
                        wr(base, RDT, ((d + 1) % N_RX) as u32);
                    }
                    continue;
                }
                let buf = unsafe {
                    core::slice::from_raw_parts(
                        bufs.add(d * RX_BUF),
                        len as usize,
                    )
                };
                let et = u16::from_be_bytes([buf[12], buf[13]]);
                if et == crate::net::ET_IPV8 {
                    if let Some(ip) = crate::net::parse_ipv8(&buf[14..]) {
                        if ip.proto == 1
                            && ip.src_rn == peer8.rn
                            && ip.src == peer8.local
                            && ip.dst == us8.local
                            && ip.dst_rn == us8.rn
                        {
                            if let Some(m) =
                                crate::net::parse_icmp(ip.payload)
                            {
                                if m.ty == 0 && m.code == 0
                                    && m.id == ID && m.seq == SEQ
                                {
                                    printk(format_args!(
                                        "rustux: ipv8: echo reply \
                                         rn{:#x} {}.{}.{}.{} seq={} \
                                         ({len}B native 0x8080)",
                                        ip.src_rn, ip.src[0], ip.src[1],
                                        ip.src[2], ip.src[3], m.seq
                                    ));
                                    return;
                                }
                            }
                        }
                    }
                }
                // SAFETY: reposting a consumed descriptor.
                unsafe {
                    (*ring.add(d)).sta = 0;
                    wr(base, RDT, ((d + 1) % N_RX) as u32);
                }
            }
            rounds += 1;
            if rounds == 20_000_000 {
                printk(format_args!("rustux: ipv8: no reply (peer down?)"));
                return;
            }
            if !sent || rounds % 4_000_000 == 0 {
                let _ = tx_once(base, &f[..flen.max(60)]);
                sent = true;
            }
        }
    }

    // Responder: bounded listen for an IPv8 echo request, then reply.
    printk(format_args!("rustux: ipv8: responder listening ~50s"));
    let mut i = 0u32;
    loop {
        for d in 0..N_RX {
            // SAFETY: same contract as arp_selftest.
            let (sta, len) =
                unsafe { ((*ring.add(d)).sta, (*ring.add(d)).len) };
            if sta & RXD_STA_DD == 0 {
                continue;
            }
            if sta & RXD_STA_EOP == 0 || len < 14 || len as usize > RX_BUF {
                unsafe {
                    (*ring.add(d)).sta = 0;
                    wr(base, RDT, ((d + 1) % N_RX) as u32);
                }
                continue;
            }
            let buf = unsafe {
                core::slice::from_raw_parts(bufs.add(d * RX_BUF), len as usize)
            };
            let et = u16::from_be_bytes([buf[12], buf[13]]);
            if et == crate::net::ET_IPV8 && buf.len() >= 14 + 28 {
                let dst_mac: [u8; 6] =
                    buf[6..12].try_into().unwrap(); // learned sender MAC
                if let Some(ip) = crate::net::parse_ipv8(&buf[14..]) {
                    if ip.proto == 1
                        && ip.dst == us8.local
                        && ip.dst_rn == us8.rn
                    {
                        if let Some(m) = crate::net::parse_icmp(ip.payload)
                        {
                            if m.ty == 8 {
                                let them = crate::net::Addr8 {
                                    rn: ip.src_rn,
                                    local: ip.src,
                                };
                                let mut r = [0u8; 128];
                                let rlen = crate::net::build_ipv8_echo(
                                    &mut r, 0, dst_mac, mac, &them, &us8,
                                    m.id, m.seq, b"rustux ipv8 reply",
                                );
                                if rlen != 0
                                    && tx_once(base, &r[..rlen.max(60)])
                                {
                                    printk(format_args!(
                                        "rustux: ipv8: echo req from \
                                         rn{:#x} {}.{}.{}.{} seq={} -> \
                                         replied",
                                        ip.src_rn, ip.src[0], ip.src[1],
                                        ip.src[2], ip.src[3], m.seq
                                    ));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            // SAFETY: reposting a consumed descriptor.
            unsafe {
                (*ring.add(d)).sta = 0;
                wr(base, RDT, ((d + 1) % N_RX) as u32);
            }
        }
        i += 1;
        if i == 2_000_000_000 {
            printk(format_args!("rustux: ipv8: listen timed out"));
            return;
        }
    }
}
