//! linux-rust kernel entry point.
//!
//! Boots under UEFI (OVMF in QEMU). Today: bring up serial, prove we run, exit
//! QEMU cleanly through the isa-debug-exit device. Memory map, page tables,
//! ExitBootServices and the first ported kernel units come next.

#![no_main]
#![no_std]

use core::fmt::Write;

use uefi::prelude::*;

mod uart;

/// Exit QEMU through the isa-debug-exit device (`-device isa-debug-exit,
/// iobase=0xf4`): QEMU exits with status `(code << 1) | 1`. Halts forever when
/// the device is absent (real hardware, other firmware).
fn qemu_exit(code: u32) -> ! {
    unsafe {
        core::arch::asm!("out dx, eax", in("dx") 0xf4u16, in("eax") code, options(nomem, nostack));
    }
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}

#[entry]
fn main() -> Status {
    let mut com1 = unsafe { uart::Uart16550::new(uart::COM1) };
    com1.init();
    let _ = writeln!(com1, "linux-rust: Rust kernel alive under UEFI");
    let _ = writeln!(com1, "linux-rust: serial console up on COM1");
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::int_sqrt(1<<40) = {} (verified port of lib/math/int_sqrt.c)",
        klib::math::int_sqrt(1 << 40)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::gcd(1071,462) = {} (verified port of lib/math/gcd.c)",
        klib::math::gcd(1071, 462)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::lcm(21,6) = {} (verified port of lib/math/lcm.c)",
        klib::math::lcm(21, 6)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::int_pow(7,13) = {} (verified port of lib/math/int_pow.c)",
        klib::math::int_pow(7, 13)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::intlog10(255) = {} (verified port of lib/math/int_log.c)",
        klib::math::intlog10(255)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::mul_u64_add_u64_div_u64(2^63,2^63,0,2^63) = {} (verified port of lib/math/div64.c)",
        klib::math::mul_u64_add_u64_div_u64(1 << 63, 1 << 63, 0, 1 << 63)
    );
    let (rn, rd) = klib::math::rational_best_approximation(31415, 10000, 255, 31);
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::rational_best_approximation(3.1415) = {rn}/{rd} (verified port of lib/math/rational.c)"
    );
    let rv = klib::math::reciprocal_value(7);
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::reciprocal_value(7) = {:#x},{},{} (verified port of lib/math/reciprocal_div.c)",
        rv.m, rv.sh1, rv.sh2
    );
    let iq = klib::math::cordic_calc_iq(45);
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::cordic_calc_iq(45) = {},{} (verified port of lib/math/cordic.c)",
        iq.i, iq.q
    );
    let terms = [
        klib::math::PolynomialTerm {
            deg: 2,
            coef: 1,
            divider: 1,
            divider_leftover: 1,
        },
        klib::math::PolynomialTerm {
            deg: 1,
            coef: 2,
            divider: 1,
            divider_leftover: 1,
        },
        klib::math::PolynomialTerm {
            deg: 0,
            coef: 3,
            divider: 1,
            divider_leftover: 1,
        },
    ];
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::polynomial_calc(x^2+2x+3, x=4) = {} (verified port of lib/math/polynomial.c)",
        klib::math::polynomial_calc(&terms, 1, 4)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::math::is_prime_number(7919) = {}, next_prime_number(65535) = {} (verified port of lib/math/prime_numbers.c)",
        klib::math::is_prime_number(7919),
        klib::math::next_prime_number(65535)
    );

    // Exit code 0x10 -> QEMU exits 0x21. Anything else means we never got here.
    qemu_exit(0x10);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Best effort: reopen COM1 and report before halting.
    let mut com1 = unsafe { uart::Uart16550::new(uart::COM1) };
    com1.init();
    let _ = writeln!(com1, "KERNEL PANIC: {info}");
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}
