//! Minimal 16550-compatible UART driver for the serial console.
//!
//! QEMU's `-serial stdio` and every PC's COM1 speak this register layout, so
//! this is the kernel's first real device driver: early output before any
//! console subsystem exists.

use core::arch::asm;
use core::fmt::{self, Write};

/// I/O base of the first serial port.
pub const COM1: u16 = 0x3f8;

// 16550 register offsets from the I/O base.
const RX_TX: u16 = 0; // data register (read: RX, write: TX)
const IER: u16 = 1; // interrupt enable
const LCR: u16 = 3; // line control
const MCR: u16 = 4; // modem control
const LSR: u16 = 5; // line status
const LSR_THR_EMPTY: u8 = 0x20;

/// SAFETY: `port`/`value` are raw x86 port I/O; callers must ensure the port
/// belongs to the device they mean to drive.
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

/// SAFETY: same contract as [`outb`].
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}

pub struct Uart16550 {
    base: u16,
}

impl Uart16550 {
    /// SAFETY: `base` must be the I/O base of a present UART (COM1..COM4).
    /// Constructing two drivers for the same port aliases it.
    pub const unsafe fn new(base: u16) -> Self {
        Self { base }
    }

    /// 115200 8N1, no interrupts: the firmware already enabled the port, we
    /// only pin the line parameters we rely on.
    pub fn init(&mut self) {
        unsafe {
            outb(self.base + IER, 0x00); // polling only, no IRQs
            outb(self.base + LCR, 0x80); // DLAB: divisor latch access
            outb(self.base + RX_TX, 0x01); // divisor low  = 1 -> 115200 baud
            outb(self.base + IER, 0x00); // divisor high = 0
            outb(self.base + LCR, 0x03); // 8 data bits, no parity, 1 stop bit
            outb(self.base + MCR, 0x0b); // DTR + RTS asserted, OUT2 (needed for IRQ path later)
        }
    }

    fn put_byte(&mut self, byte: u8) {
        unsafe {
            while inb(self.base + LSR) & LSR_THR_EMPTY == 0 {}
            outb(self.base + RX_TX, byte);
        }
    }
}

impl Write for Uart16550 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put_byte(b'\r');
            }
            self.put_byte(b);
        }
        Ok(())
    }
}
