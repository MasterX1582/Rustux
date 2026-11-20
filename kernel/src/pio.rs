//! pio — x86 port I/O primitives shared by drivers.

#[inline]
pub unsafe fn outb(port: u16, v: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") v, options(nomem, nostack));
    }
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack));
    }
    v
}
