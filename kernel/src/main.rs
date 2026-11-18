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

    {
        use klib::rbtree::*;
        const RT: usize = 8;
        let mut nodes = [RbNode::new(); RT];
        let rk = [5u64, 3, 8, 1, 4, 7, 9, 2];
        let mut root = RbRoot::EMPTY;
        for (i, &k) in rk.iter().enumerate() {
            let mut link = Link::Root;
            let mut parent = NIL;
            let mut cur = root.rb_node;
            while cur != NIL {
                parent = cur;
                if k < rk[cur] {
                    link = Link::Left(cur);
                    cur = nodes[cur].left;
                } else {
                    link = Link::Right(cur);
                    cur = nodes[cur].right;
                }
            }
            rb_link_node(&mut nodes, i, parent, link, &mut root);
            rb_insert_color(&mut nodes, i, &mut root);
        }
        // in-order traversal should visit keys 1..9 sorted
        let mut inorder = 0u64;
        let mut i = rb_first(&nodes, &root);
        while i != NIL {
            inorder = inorder * 10 + rk[i];
            i = rb_next(&nodes, i);
        }
        // erase the middle node and verify traversal still sorted
        rb_erase(&mut nodes, 4, &mut root); // node index 4 has key 4
        let mut inorder2 = 0u64;
        let mut i = rb_first(&nodes, &root);
        while i != NIL {
            inorder2 = inorder2 * 10 + rk[i];
            i = rb_next(&nodes, i);
        }
        // ascending inserts force __rb_insert's rotation cases
        let mut n2 = [RbNode::new(); RT];
        let mut root2 = RbRoot::EMPTY;
        for i in 0..RT {
            let mut link = Link::Root;
            let mut parent = NIL;
            let mut cur = root2.rb_node;
            while cur != NIL {
                parent = cur;
                link = Link::Right(cur);
                cur = n2[cur].right;
            }
            rb_link_node(&mut n2, i, parent, link, &mut root2);
            rb_insert_color(&mut n2, i, &mut root2);
        }
        let mut h = 0usize;
        let mut i = rb_first(&n2, &root2);
        while i != NIL {
            h += 1;
            i = rb_next(&n2, i);
        }
        let _ = writeln!(
            com1,
            "linux-rust: klib::rbtree: insert(5,3,8,1,4,7,9,2) inorder={} erase(4)={} asc(1..8)->{}nodes (verified port of lib/rbtree.c)",
            inorder, inorder2, h
        );
        assert_eq!(inorder, 12345789);
        assert_eq!(inorder2, 1235789);
        assert_eq!(h, 8);
    }

    {
        use klib::string::*;
        let mut dst = [0u8; 16];
        let copied = sized_strscpy(&mut dst, b"linux-rust\0", 16);
        let hay = b"the quick brown fox\0";
        let _ = writeln!(
            com1,
            "linux-rust: klib::string: strscpy={} strstr(fox)={:?} memchr_inv={:?} strcasecmp={} (verified port of lib/string.c)",
            copied,
            strstr(hay, b"fox\0"),
            memchr_inv(b"aaaab", b'a', 5),
            strcasecmp(b"ReSeT\0", b"reset\0")
        );
        assert_eq!(copied, 10);
        assert_eq!(strstr(hay, b"fox\0"), Some(16));
        assert_eq!(memchr_inv(b"aaaab", b'a', 5), Some(4));
        assert_eq!(strcasecmp(b"ReSeT\0", b"reset\0"), 0);
    }

    {
        use klib::sha::*;
        let mut d = [0u8; 32];
        sha256(b"abc", 3, &mut d);
        let mut ctx = Sha1Ctx::default();
        sha1_init(&mut ctx);
        sha1_update(&mut ctx, b"a", 1);
        sha1_update(&mut ctx, b"bc", 2);
        let mut d1 = [0u8; 20];
        sha1_final(&mut ctx, &mut d1);
        let mut hkey = HmacSha256Key::default();
        hmac_sha256_preparekey(&mut hkey, b"Jefe", 4);
        let mut hm = [0u8; 32];
        hmac_sha256(&hkey, b"what do ya want for nothing?", 28, &mut hm);
        let _ = writeln!(
            com1,
            "linux-rust: klib::sha: sha256(abc)={:02x}{:02x}..{:02x}{:02x} sha1(a+bc)={:02x}{:02x}.. hmac={:02x}{:02x}.. (verified port of lib/crypto/sha*.c)",
            d[0], d[1], d[30], d[31], d1[0], d1[1], hm[0], hm[1]
        );
        assert_eq!(
            d,
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad
            ]
        );
        assert_eq!(
            d1,
            [
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
                0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d
            ]
        );
        assert_eq!(hm[..4], [0x5b, 0xdc, 0xc1, 0x46]);
    }

    {
        use klib::kfifo::*;
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let n = kfifo_in(&mut f, b"abcdef", 6);
        let mut out = [0u8; 8];
        kfifo_out(&mut f, &mut out, 4);
        kfifo_in(&mut f, b"ghij", 4); // wraps the ring
        let mut all = [0u8; 8];
        let got = kfifo_out(&mut f, &mut all, 8);
        let _ = writeln!(
            com1,
            "linux-rust: klib::kfifo: in={} wrap-drain={} -> {:?} (verified port of lib/kfifo.c)",
            n,
            got,
            &all[..8]
        );
        assert_eq!(&all[..6], b"efghij");
    }

    {
        use klib::llist::*;
        let mut head = LlistHead::new();
        let mut next = [NIL; 8];
        for i in 0..3 {
            init_llist_node(&mut next, i);
            llist_add(&mut head, &mut next, i);
        }
        let a = llist_del_first(&mut head, &mut next); // 2 (LIFO)
        let chain = llist_del_all(&mut head); // 1 -> 0
        let rev = llist_reverse_order(&mut next, chain); // 0 -> 1
        let _ = writeln!(
            com1,
            "linux-rust: klib::llist: pop={} chain={} rev-head={} (verified port of lib/llist.c)",
            a,
            chain,
            rev
        );
        assert_eq!((a, chain, rev), (2, 1, 0));
        assert_eq!(next[0], 1);
        assert_eq!(next[1], NIL);
    }

    {
        use klib::fmt::Arg;
        use klib::seqbuf::*;
        let mut buf = [0u8; 64];
        let mut s = SeqBuf::new(&mut buf);
        s.puts(b"boot\0");
        s.printf(b"=%d", &[Arg::I(33)]);
        let _ = writeln!(
            com1,
            "linux-rust: klib::seqbuf: puts+printf -> {:?} (verified port of lib/seq_buf.c)",
            s.as_slice()
        );
        assert_eq!(s.as_slice(), b"boot=33");
        assert_eq!(s.pop(), b'3' as i64);
        let mut lb = [0u8; 131];
        let n = hex_dump_to_buffer(&[0xde, 0xad], 2, 16, 1, &mut lb, false);
        assert_eq!(&lb[..n as usize], b"de ad");
    }

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
