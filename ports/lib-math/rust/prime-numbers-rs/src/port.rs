//! The Rust port of `prime_numbers.c`: safe Rust; `lib.rs` exports each function under its C name.

#![forbid(unsafe_code)]

use std::ffi::c_ulong;
use std::sync::{Mutex, MutexGuard, OnceLock};

// LP64 harness (see ports/lib-math/linux/types.h): unsigned long is 64-bit.
const _: () = assert!(std::mem::size_of::<c_ulong>() == std::mem::size_of::<u64>());

const BITS_PER_LONG: u64 = 64;

/* `small_primes` from the C source: the LP64 static sieve covering <= 61. */
const SMALL_PRIMES_LAST: u64 = 61;
const SMALL_PRIMES_SZ: u64 = 64;
const SMALL_PRIMES_WORD: u64 = (1 << 2)
    | (1 << 3)
    | (1 << 5)
    | (1 << 7)
    | (1 << 11)
    | (1 << 13)
    | (1 << 17)
    | (1 << 19)
    | (1 << 23)
    | (1 << 29)
    | (1 << 31)
    | (1 << 37)
    | (1 << 41)
    | (1 << 43)
    | (1 << 47)
    | (1 << 53)
    | (1 << 59)
    | (1 << 61);

/// `struct primes`, minus the RCU bookkeeping the single-threaded harness
/// stubs out: the Mutex plays `mutex lock` + the RCU pointer together.
struct Sieve {
    last: u64,
    sz: u64,
    bits: Vec<u64>,
}

static SIEVE: OnceLock<Mutex<Sieve>> = OnceLock::new();

fn sieve() -> MutexGuard<'static, Sieve> {
    SIEVE
        .get_or_init(|| {
            Mutex::new(Sieve {
                last: SMALL_PRIMES_LAST,
                sz: SMALL_PRIMES_SZ,
                bits: vec![SMALL_PRIMES_WORD],
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Verbatim port of `lib/math/int_sqrt.c` (adopted earlier as
/// `klib::math::int_sqrt`; duplicated here so this crate stays standalone).
fn int_sqrt(x: u64) -> u64 {
    if x <= 1 {
        return x;
    }
    let mut m = 1u64 << ((63 - x.leading_zeros() as u64) & !1);
    let mut y = 0u64;
    let mut x = x;
    while m != 0 {
        let b = y.wrapping_add(m);
        y >>= 1;
        if x >= b {
            x = x.wrapping_sub(b);
            y = y.wrapping_add(m);
        }
        m >>= 2;
    }
    y
}

fn test_bit(n: u64, bits: &[u64]) -> bool {
    bits[(n / BITS_PER_LONG) as usize] & (1u64 << (n % BITS_PER_LONG)) != 0
}

fn clear_bit(n: u64, bits: &mut [u64]) {
    bits[(n / BITS_PER_LONG) as usize] &= !(1u64 << (n % BITS_PER_LONG));
}

fn find_next_bit(bits: &[u64], size: u64, offset: u64) -> u64 {
    let mut i = offset;
    while i < size {
        if test_bit(i, bits) {
            return i;
        }
        i += 1;
    }
    size
}

fn clear_multiples(x: u64, bits: &mut [u64], start: u64, end: u64) -> u64 {
    let mut m = x.wrapping_mul(2);
    if m < start {
        // roundup(start, x) — x >= 2 here, so no division by zero.
        m = start.wrapping_add(x - 1) / x * x;
    }
    while m < end {
        clear_bit(m, bits);
        m = m.wrapping_add(x);
    }
    x
}

/// Port of `expand_to_next_prime`: doubles the sieve to `round_up(2x, 64)`
/// bits. Allocation failure maps to `false` (the C `kmalloc` NULL path);
/// `Vec::try_reserve` keeps that a recoverable branch, not an abort.
fn expand_to_next_prime(x: u64) -> bool {
    let mut sz = x.wrapping_mul(2);
    if sz < x {
        return false;
    }
    sz = (sz.wrapping_sub(1) | (BITS_PER_LONG - 1)).wrapping_add(1);

    // sz == 0 means round_up wrapped (x in [2^63-31, 2^63-1]): the C misses
    // this case (`sz < x` sees only the pre-round_up value) and proceeds to
    // heap-corrupt — bitmap_copy writes 8 bytes past a 24-byte allocation.
    // Upstream-faithful crash surface: panic deterministically instead.
    if sz == 0 {
        panic!("expand_to_next_prime: sz wrapped to 0 (upstream heap OOB)");
    }

    let nwords = (sz / BITS_PER_LONG) as usize;
    let mut bits: Vec<u64> = Vec::new();
    if bits.try_reserve_exact(nwords).is_err() {
        return false;
    }
    bits.resize(nwords, !0u64); // bitmap_fill

    let mut s = sieve();
    if x < s.last {
        // Another expansion already covered x — the C frees `new` and
        // reports success without installing it.
        return true;
    }

    let old_words = s.sz.div_ceil(BITS_PER_LONG) as usize;
    bits[..old_words].copy_from_slice(&s.bits[..old_words]); // bitmap_copy

    let mut new_last = 0u64;
    let mut y = 2u64;
    while y < sz {
        new_last = clear_multiples(y, &mut bits, s.sz, sz);
        y = find_next_bit(&bits, sz, y + 1);
    }
    // BUG_ON(new->last <= x): Bertrand's postulate guarantees a prime in
    // (x, 2x); fire the same trap the (fixed) BUG_ON shim does. Note: when
    // sz wrapped to 0 upstream reads an indeterminate field; we panic
    // instead — a documented divergence on an out-of-contract input.
    if new_last <= x {
        panic!("BUG_ON: new->last <= x");
    }

    s.bits = bits;
    s.last = new_last;
    s.sz = sz;
    true
}

fn slow_next_prime_number(mut x: u64) -> u64 {
    while x < u64::MAX {
        x = x.wrapping_add(1);
        if slow_is_prime_number(x) {
            break;
        }
    }
    x
}

/// Port of `bool slow_is_prime_number(unsigned long x)` (`prime_numbers.c:88`).
pub fn slow_is_prime_number(x: c_ulong) -> bool {
    let mut y = int_sqrt(x);

    while y > 1 {
        if x.is_multiple_of(y) {
            break;
        }
        y -= 1;
    }
    y == 1
}

/// Port of `unsigned long next_prime_number(unsigned long x)` (`prime_numbers.c:207`).
pub fn next_prime_number(x: c_ulong) -> c_ulong {
    let x: u64 = x;

    loop {
        {
            let s = sieve();
            if x < s.last {
                // find_next_bit(p->primes, p->last, x + 1); x+1 wraps in C.
                return find_next_bit(&s.bits, s.last, x.wrapping_add(1));
            }
        }
        if !expand_to_next_prime(x) {
            return slow_next_prime_number(x);
        }
    }
}

/// Port of `bool is_prime_number(unsigned long x)` (`prime_numbers.c:240`).
pub fn is_prime_number(x: c_ulong) -> bool {
    let x: u64 = x;

    loop {
        {
            let s = sieve();
            if x < s.sz {
                return test_bit(x, &s.bits);
            }
        }
        if !expand_to_next_prime(x) {
            return slow_is_prime_number(x);
        }
    }
}
