//! kbd — i8042 PS/2 keyboard: IRQ1 -> scancode ring -> key events.
//!
//! The handler path stays tiny: check the output-buffer-full flag, read
//! the scancode byte, push it into a lock-free SPSC ring, EOI. All decode
//! (set-1 make/break codes, shift state, E0 prefix) happens consumer-side
//! in `poll`, so the IRQ handler is a few dozen instructions.
//!
//! Ring safety: single producer (the IRQ handler, which runs serialized
//! by the interrupt gate) and single consumer (`poll`). Byte writes are
//! Relaxed, head/tail indices Release/Acquire — the standard SPSC ring.

use crate::pio::{inb, outb};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

const CMD: u16 = 0x64; // i8042 status(read)/command(write)
const DATA: u16 = 0x60; // i8042 data port
const STS_OBF: u8 = 0x01; // output buffer full — a byte waits at DATA
const STS_IBF: u8 = 0x02; // input buffer full — controller busy

/// IRQs seen on the keyboard vector.
pub static IRQS: AtomicU64 = AtomicU64::new(0);
/// Keys successfully decoded (consumer side).
pub static DECODED: AtomicU64 = AtomicU64::new(0);

const RING_SIZE: usize = 64;
static RING: [core::sync::atomic::AtomicU8; RING_SIZE] =
    [const { core::sync::atomic::AtomicU8::new(0) }; RING_SIZE];
static HEAD: AtomicUsize = AtomicUsize::new(0); // producer index
static TAIL: AtomicUsize = AtomicUsize::new(0); // consumer index
static SHIFT: AtomicBool = AtomicBool::new(false);
static E0: AtomicBool = AtomicBool::new(false);

/// IRQ handler — called from apic::irq_dispatch on the keyboard vector.
/// Reads OBF-guarded data so a spurious vector can't fabricate a key.
pub fn on_irq() {
    IRQS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: i8042 ports are fixed hardware addresses.
    unsafe {
        if inb(CMD) & STS_OBF == 0 {
            return;
        }
        let code = inb(DATA);
        let head = HEAD.load(Ordering::Relaxed);
        let next = (head + 1) % RING_SIZE;
        if next != TAIL.load(Ordering::Acquire) {
            RING[head].store(code, Ordering::Relaxed);
            HEAD.store(next, Ordering::Release);
        }
        // Ring full: drop the scancode — better than spinning in an IRQ.
    }
}

/// Wait for the input buffer to drain before writing to DATA; bounded so
/// a misbehaving/absent controller can't hang the boot.
fn ibf_clear() -> bool {
    for _ in 0..100_000u32 {
        // SAFETY: fixed port.
        if unsafe { inb(CMD) } & STS_IBF == 0 {
            return true;
        }
    }
    false
}

/// Wait for output data; bounded for the same reason.
fn obf_set() -> bool {
    for _ in 0..100_000u32 {
        if unsafe { inb(CMD) } & STS_OBF != 0 {
            return true;
        }
    }
    false
}

/// Send a byte to the keyboard itself and consume its ACK.
fn kbd_write(v: u8) -> bool {
    if !ibf_clear() {
        return false;
    }
    unsafe {
        outb(DATA, v);
    }
    // Keyboard replies 0xfa (ACK) for commands; bounded wait.
    if !obf_set() {
        return false;
    }
    unsafe { inb(DATA) == 0xfa }
}

/// Flush the output buffer, enable the keyboard IRQ bit in the controller
/// command byte, and tell the keyboard to scan. Returns false when the
/// controller doesn't answer — the boot proceeds keyboardless.
pub fn init() -> bool {
    unsafe {
        // Drain anything the firmware left in the output buffer.
        for _ in 0..64 {
            if inb(CMD) & STS_OBF == 0 {
                break;
            }
            let _ = inb(DATA);
        }
        // Command byte: set bit 0 (KBD port interrupt enable) and CLEAR
        // bit 4 — that's the keyboard-interface clock-disable flag; if
        // firmware left the port disabled, enabling its IRQ bit alone
        // produces nothing. Preserve the rest (translation/aux bits).
        outb(CMD, 0x20); // read command byte
        if !obf_set() {
            return false;
        }
        let cmd = (inb(DATA) | 0x01) & !0x10;
        outb(CMD, 0x60); // write command byte
        if !ibf_clear() {
            return false;
        }
        outb(DATA, cmd);
    }
    kbd_write(0xf4) // enable scanning; keyboard ACKs
}

/// Pop one decoded key, or None. Consumes raw scancodes from the ring:
/// break codes (bit 7) update shift state, E0-prefixed bytes are skipped
/// (extended keys unmapped for now), plain make codes map through the
/// set-1 tables.
pub fn poll() -> Option<char> {
    loop {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            return None;
        }
        let code = RING[tail].load(Ordering::Relaxed);
        TAIL.store((tail + 1) % RING_SIZE, Ordering::Release);

        if code == 0xe0 {
            E0.store(true, Ordering::Relaxed);
            continue;
        }
        if E0.swap(false, Ordering::Relaxed) {
            continue; // extended scancode — unmapped
        }
        if code & 0x80 != 0 {
            // Break code — only shift release matters for echoing.
            if code == 0xaa || code == 0xb6 {
                SHIFT.store(false, Ordering::Relaxed);
            }
            continue;
        }
        if code == 0x2a || code == 0x36 {
            SHIFT.store(true, Ordering::Relaxed);
            continue;
        }
        let c = if SHIFT.load(Ordering::Relaxed) {
            SHIFT_MAP.get(code as usize).copied().unwrap_or(0)
        } else {
            NORMAL_MAP.get(code as usize).copied().unwrap_or(0)
        };
        if c == 0 {
            continue; // unmapped make code (F-keys, etc.)
        }
        DECODED.fetch_add(1, Ordering::Relaxed);
        return Some(c as char);
    }
}

// Set-1 make-code tables, indexed by scancode (unmapped slots are 0).
// Row order: numbers row, QWERTY row, home row, bottom row, space.
#[rustfmt::skip]
const NORMAL_MAP: [u8; 0x3a] = [
    0,    0,    b'1', b'2', b'3', b'4', b'5', b'6', // 0x00-0x07
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t', // 0x08-0x0f (0x0e = backspace)
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', // 0x10-0x17
    b'o', b'p', b'[', b']', b'\n',0,    b'a', b's', // 0x18-0x1f (0x1c enter, 0x1d lctrl)
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', // 0x20-0x27
    b'\'',b'`', 0,    b'\\',b'z', b'x', b'c', b'v', // 0x28-0x2f (0x2a lshift)
    b'b', b'n', b'm', b',', b'.', b'/', 0,    b'*', // 0x30-0x37 (0x36 rshift)
    0,    b' ',                                     // 0x38 lalt, 0x39 space
];

#[rustfmt::skip]
const SHIFT_MAP: [u8; 0x3a] = [
    0,    0,    b'!', b'@', b'#', b'$', b'%', b'^',
    b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I',
    b'O', b'P', b'{', b'}', b'\n',0,    b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',
    b'"', b'~', 0,    b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0,    b'*',
    0,    b' ',
];
