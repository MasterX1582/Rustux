//! The Rust port of `crc4.c`: safe Rust; `lib.rs` exports each function under its C name.

#![forbid(unsafe_code)]

const CRC4_TAB: [u8; 16] = [
    0x0, 0x7, 0xe, 0x9, 0xb, 0xc, 0x5, 0x2, 0x1, 0x6, 0xf, 0x8, 0xa, 0xd, 0x4, 0x3,
];

/// Port of `uint8_t crc4(uint8_t c, uint64_t x, int bits)` (`crc4.c:26`).
/// `bits` is a C `int`; `x` is treated as left-aligned and bits above
/// `bits` are ignored. For out-of-range `bits` (negative or >64) the C's
/// shifts are UB, but the recorded harness runs x86-64 `shl`/`shr`, which
/// mask the count to 6 bits — the port mirrors that hardware behavior so
/// the recorded cases stay behavior-identical.
pub fn crc4(c: u8, x: u64, bits: std::ffi::c_int) -> u8 {
    // x &= (1 << bits) - 1  —  x86-64: count masked to bits & 63.
    let x = x & (1u64 << (bits & 63)).wrapping_sub(1);
    // bits = (bits + 3) & ~0x3  —  i32 wraparound, like C.
    let bits = bits.wrapping_add(3) & !0x3;

    let mut c = c;
    let mut i = bits.wrapping_sub(4);
    while i >= 0 {
        c = CRC4_TAB[(c ^ ((x >> (i & 63)) & 0xf) as u8) as usize];
        i -= 4;
    }
    c
}
