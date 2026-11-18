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
    let _ = writeln!(
        com1,
        "linux-rust: klib::bcd::bcd2bin(0x45) = {}, bin2bcd(45) = {:#x} (verified port of lib/bcd.c)",
        klib::bcd::bcd2bin(0x45),
        klib::bcd::bin2bcd(45)
    );
    let mut arr = [9u32, 4, 7, 1, 8, 2, 6, 3, 5, 0];
    klib::sort::sort(&mut arr, |x, y| i32::from(x > y) - i32::from(x < y));
    let _ = writeln!(
        com1,
        "linux-rust: klib::sort::sort([9,4,7,1,8,2,6,3,5,0]) = {:?} (verified port of lib/sort.c)",
        arr
    );
    const L: usize = 8;
    let keys = [9u32, 4, 7, 1, 8, 2, 6, 3];
    let mut links = [klib::list_sort::Links::new(0, 0); L + 1];
    for (i, l) in links.iter_mut().take(L).enumerate() {
        l.next = i + 1;
        l.prev = if i == 0 { L } else { i - 1 };
    }
    links[L - 1].next = L;
    links[L] = klib::list_sort::Links::new(0, L - 1);
    {
        let mut cmp = |a: usize, b: usize| {
            i32::from(keys[a] > keys[b]) - i32::from(keys[a] < keys[b])
        };
        klib::list_sort::list_sort(&mut links, L, &mut cmp);
    }
    let first = keys[links[L].next];
    let last = keys[links[L].prev];
    let _ = writeln!(
        com1,
        "linux-rust: klib::list_sort::list_sort({:?}) first={} last={} (verified port of lib/list_sort.c)",
        keys, first, last
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::crc::crc32_le(0,\"123456789\") = {:#x}, crc16(0,...) = {:#x}, crc4(0,0x12345678,32) = {:#x} (verified ports of lib/crc/)",
        klib::crc::crc32_le(0, b"123456789"),
        klib::crc::crc16(0, b"123456789"),
        klib::crc::crc4(0, 0x12345678, 32)
    );
    let _ = writeln!(
        com1,
        "linux-rust: klib::hash::siphash_1u64(0xdeadbeef,k) = {:#x}, xxh64(\"123456789\",0) = {:#x}, jhash_1word(1,0) = {:#x} (verified ports of siphash.c/xxhash.c/jhash.h)",
        klib::hash::siphash_1u64(0xdeadbeef, (0x0706050403020100, 0x0f0e0d0c0b0a0908)),
        klib::hash::xxh64(b"123456789", 0),
        klib::hash::jhash_1word(1, 0)
    );

    let mut bm = [0u64; 2];
    klib::bitmap::bitmap_set(&mut bm, 62, 10);
    let free = klib::bitmap::bitmap_find_free_region(&mut bm, 128, 2);
    let _ = writeln!(
        com1,
        "linux-rust: klib::bitmap: set(62..72) -> first={} free_region(4)={} weight_after_alloc={} (verified ports of find_bit.c/bitmap.c)",
        klib::bitmap::find_first_bit(&bm, 128),
        free,
        klib::bitmap::bitmap_weight(&bm, 128)
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
