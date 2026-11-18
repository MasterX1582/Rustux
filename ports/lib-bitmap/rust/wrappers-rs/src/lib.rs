//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_raw_find_first_bit(uint64_t s, int64_t size)` (`wrappers.c:132`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_first_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_bit(s, size)
}

/// `wrap_raw_find_first_bit` as `rust_anything_wrap_raw_find_first_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_first_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_bit(s, size)
}

/// `uint64_t wrap_raw_find_first_and_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:137`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_and_bit(s1, s2, size)
}

/// `wrap_raw_find_first_and_bit` as `rust_anything_wrap_raw_find_first_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_and_bit(s1, s2, size)
}

/// `uint64_t wrap_raw_find_first_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:142`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_andnot_bit(s1, s2, size)
}

/// `wrap_raw_find_first_andnot_bit` as `rust_anything_wrap_raw_find_first_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_andnot_bit(s1, s2, size)
}

/// `uint64_t wrap_raw_find_first_and_and_bit(uint64_t s1, uint64_t s2, uint64_t s3, int64_t size)` (`wrappers.c:147`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_first_and_and_bit(s1: u64, s2: u64, s3: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_and_and_bit(s1, s2, s3, size)
}

/// `wrap_raw_find_first_and_and_bit` as `rust_anything_wrap_raw_find_first_and_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_first_and_and_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
) -> u64 {
    port::wrap_raw_find_first_and_and_bit(s1, s2, s3, size)
}

/// `uint64_t wrap_raw_find_first_zero_bit(uint64_t s, int64_t size)` (`wrappers.c:153`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_first_zero_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_zero_bit(s, size)
}

/// `wrap_raw_find_first_zero_bit` as `rust_anything_wrap_raw_find_first_zero_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_first_zero_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_first_zero_bit(s, size)
}

/// `uint64_t wrap_raw_find_next_bit(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:158`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_bit(s, size, start)
}

/// `wrap_raw_find_next_bit` as `rust_anything_wrap_raw_find_next_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_bit(s, size, start)
}

/// `uint64_t wrap_raw_find_nth_bit(uint64_t s, int64_t size, int64_t n)` (`wrappers.c:163`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    port::wrap_raw_find_nth_bit(s, size, n)
}

/// `wrap_raw_find_nth_bit` as `rust_anything_wrap_raw_find_nth_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    port::wrap_raw_find_nth_bit(s, size, n)
}

/// `uint64_t wrap_raw_find_nth_and_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t n)` (`wrappers.c:168`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_nth_and_bit(s1: u64, s2: u64, size: i64, n: i64) -> u64 {
    port::wrap_raw_find_nth_and_bit(s1, s2, size, n)
}

/// `wrap_raw_find_nth_and_bit` as `rust_anything_wrap_raw_find_nth_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_nth_and_bit(
    s1: u64,
    s2: u64,
    size: i64,
    n: i64,
) -> u64 {
    port::wrap_raw_find_nth_and_bit(s1, s2, size, n)
}

/// `uint64_t wrap_raw_find_nth_and_andnot_bit(uint64_t s1, uint64_t s2, uint64_t s3, int64_t size, int64_t n)` (`wrappers.c:174`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_nth_and_andnot_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
    n: i64,
) -> u64 {
    port::wrap_raw_find_nth_and_andnot_bit(s1, s2, s3, size, n)
}

/// `wrap_raw_find_nth_and_andnot_bit` as `rust_anything_wrap_raw_find_nth_and_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_nth_and_andnot_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
    n: i64,
) -> u64 {
    port::wrap_raw_find_nth_and_andnot_bit(s1, s2, s3, size, n)
}

/// `uint64_t wrap_raw_find_next_and_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:181`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_next_and_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_and_bit(s1, s2, size, start)
}

/// `wrap_raw_find_next_and_bit` as `rust_anything_wrap_raw_find_next_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_next_and_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_raw_find_next_and_bit(s1, s2, size, start)
}

/// `uint64_t wrap_raw_find_next_andnot_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:187`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_next_andnot_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_andnot_bit(s1, s2, size, start)
}

/// `wrap_raw_find_next_andnot_bit` as `rust_anything_wrap_raw_find_next_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_next_andnot_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_raw_find_next_andnot_bit(s1, s2, size, start)
}

/// `uint64_t wrap_raw_find_next_or_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:193`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_next_or_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_or_bit(s1, s2, size, start)
}

/// `wrap_raw_find_next_or_bit` as `rust_anything_wrap_raw_find_next_or_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_next_or_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_raw_find_next_or_bit(s1, s2, size, start)
}

/// `uint64_t wrap_raw_find_next_zero_bit(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:199`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_zero_bit(s, size, start)
}

/// `wrap_raw_find_next_zero_bit` as `rust_anything_wrap_raw_find_next_zero_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_raw_find_next_zero_bit(s, size, start)
}

/// `uint64_t wrap_raw_find_last_bit(uint64_t s, int64_t size)` (`wrappers.c:204`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_raw_find_last_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_last_bit(s, size)
}

/// `wrap_raw_find_last_bit` as `rust_anything_wrap_raw_find_last_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_raw_find_last_bit(s: u64, size: i64) -> u64 {
    port::wrap_raw_find_last_bit(s, size)
}

/// `uint64_t wrap_find_next_clump8(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:210`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_clump8(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_clump8(s, size, start)
}

/// `wrap_find_next_clump8` as `rust_anything_wrap_find_next_clump8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_clump8(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_clump8(s, size, start)
}

/// `uint64_t wrap_find_first_clump8(uint64_t s, int64_t size)` (`wrappers.c:217`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_clump8(s: u64, size: i64) -> u64 {
    port::wrap_find_first_clump8(s, size)
}

/// `wrap_find_first_clump8` as `rust_anything_wrap_find_first_clump8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_clump8(s: u64, size: i64) -> u64 {
    port::wrap_find_first_clump8(s, size)
}

/// `uint64_t wrap_find_random_bit(uint64_t s, int64_t size, uint64_t rseed, uint64_t mode)` (`wrappers.c:226`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_random_bit(s: u64, size: i64, rseed: u64, mode: u64) -> u64 {
    port::wrap_find_random_bit(s, size, rseed, mode)
}

/// `wrap_find_random_bit` as `rust_anything_wrap_find_random_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_random_bit(
    s: u64,
    size: i64,
    rseed: u64,
    mode: u64,
) -> u64 {
    port::wrap_find_random_bit(s, size, rseed, mode)
}

/// `uint64_t wrap_find_first_bit(uint64_t s, int64_t size)` (`wrappers.c:243`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_first_bit(s, size)
}

/// `wrap_find_first_bit` as `rust_anything_wrap_find_first_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_first_bit(s, size)
}

/// `uint64_t wrap_find_next_bit(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:248`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit(s, size, start)
}

/// `wrap_find_next_bit` as `rust_anything_wrap_find_next_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit(s, size, start)
}

/// `uint64_t wrap_find_nth_bit(uint64_t s, int64_t size, int64_t n)` (`wrappers.c:253`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    port::wrap_find_nth_bit(s, size, n)
}

/// `wrap_find_nth_bit` as `rust_anything_wrap_find_nth_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_bit(s: u64, size: i64, n: i64) -> u64 {
    port::wrap_find_nth_bit(s, size, n)
}

/// `uint64_t wrap_find_first_and_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:258`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_find_first_and_bit(s1, s2, size)
}

/// `wrap_find_first_and_bit` as `rust_anything_wrap_find_first_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_find_first_and_bit(s1, s2, size)
}

/// `uint64_t wrap_find_next_and_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:263`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_and_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_and_bit(s1, s2, size, start)
}

/// `wrap_find_next_and_bit` as `rust_anything_wrap_find_next_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_and_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_find_next_and_bit(s1, s2, size, start)
}

/// `uint64_t wrap_find_nth_and_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t n)` (`wrappers.c:269`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_and_bit(s1: u64, s2: u64, size: i64, n: i64) -> u64 {
    port::wrap_find_nth_and_bit(s1, s2, size, n)
}

/// `wrap_find_nth_and_bit` as `rust_anything_wrap_find_nth_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_and_bit(s1: u64, s2: u64, size: i64, n: i64) -> u64 {
    port::wrap_find_nth_and_bit(s1, s2, size, n)
}

/// `uint64_t wrap_find_first_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:275`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_find_first_andnot_bit(s1, s2, size)
}

/// `wrap_find_first_andnot_bit` as `rust_anything_wrap_find_first_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_find_first_andnot_bit(s1, s2, size)
}

/// `uint64_t wrap_find_next_andnot_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:280`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_andnot_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_andnot_bit(s1, s2, size, start)
}

/// `wrap_find_next_andnot_bit` as `rust_anything_wrap_find_next_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_andnot_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_find_next_andnot_bit(s1, s2, size, start)
}

/// `uint64_t wrap_find_nth_and_andnot_bit(uint64_t s1, uint64_t s2, uint64_t s3, int64_t size, int64_t n)` (`wrappers.c:286`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_and_andnot_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
    n: i64,
) -> u64 {
    port::wrap_find_nth_and_andnot_bit(s1, s2, s3, size, n)
}

/// `wrap_find_nth_and_andnot_bit` as `rust_anything_wrap_find_nth_and_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_and_andnot_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
    n: i64,
) -> u64 {
    port::wrap_find_nth_and_andnot_bit(s1, s2, s3, size, n)
}

/// `uint64_t wrap_find_first_and_and_bit(uint64_t s1, uint64_t s2, uint64_t s3, int64_t size)` (`wrappers.c:293`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_and_and_bit(s1: u64, s2: u64, s3: u64, size: i64) -> u64 {
    port::wrap_find_first_and_and_bit(s1, s2, s3, size)
}

/// `wrap_find_first_and_and_bit` as `rust_anything_wrap_find_first_and_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_and_and_bit(
    s1: u64,
    s2: u64,
    s3: u64,
    size: i64,
) -> u64 {
    port::wrap_find_first_and_and_bit(s1, s2, s3, size)
}

/// `uint64_t wrap_find_next_or_bit(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:299`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_or_bit(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_or_bit(s1, s2, size, start)
}

/// `wrap_find_next_or_bit` as `rust_anything_wrap_find_next_or_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_or_bit(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_find_next_or_bit(s1, s2, size, start)
}

/// `uint64_t wrap_find_first_zero_bit(uint64_t s, int64_t size)` (`wrappers.c:305`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_zero_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_first_zero_bit(s, size)
}

/// `wrap_find_first_zero_bit` as `rust_anything_wrap_find_first_zero_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_zero_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_first_zero_bit(s, size)
}

/// `uint64_t wrap_find_next_zero_bit(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:310`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit(s, size, start)
}

/// `wrap_find_next_zero_bit` as `rust_anything_wrap_find_next_zero_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_zero_bit(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit(s, size, start)
}

/// `uint64_t wrap_find_last_bit(uint64_t s, int64_t size)` (`wrappers.c:315`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_last_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_last_bit(s, size)
}

/// `wrap_find_last_bit` as `rust_anything_wrap_find_last_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_last_bit(s: u64, size: i64) -> u64 {
    port::wrap_find_last_bit(s, size)
}

/// `uint64_t wrap_find_next_bit_wrap(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:320`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit_wrap(s, size, start)
}

/// `wrap_find_next_bit_wrap` as `rust_anything_wrap_find_next_bit_wrap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit_wrap(s, size, start)
}

/// `uint64_t wrap_find_next_and_bit_wrap(uint64_t s1, uint64_t s2, int64_t size, int64_t start)` (`wrappers.c:325`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_and_bit_wrap(s1: u64, s2: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_and_bit_wrap(s1, s2, size, start)
}

/// `wrap_find_next_and_bit_wrap` as `rust_anything_wrap_find_next_and_bit_wrap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_and_bit_wrap(
    s1: u64,
    s2: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_find_next_and_bit_wrap(s1, s2, size, start)
}

/// `uint64_t wrap_find_next_bit_le(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:333`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit_le(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit_le(s, size, start)
}

/// `wrap_find_next_bit_le` as `rust_anything_wrap_find_next_bit_le`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit_le(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_bit_le(s, size, start)
}

/// `uint64_t wrap_find_next_zero_bit_le(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:338`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_zero_bit_le(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_le(s, size, start)
}

/// `wrap_find_next_zero_bit_le` as `rust_anything_wrap_find_next_zero_bit_le`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_zero_bit_le(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_le(s, size, start)
}

/// `uint64_t wrap_find_first_zero_bit_le(uint64_t s, int64_t size)` (`wrappers.c:343`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_zero_bit_le(s: u64, size: i64) -> u64 {
    port::wrap_find_first_zero_bit_le(s, size)
}

/// `wrap_find_first_zero_bit_le` as `rust_anything_wrap_find_first_zero_bit_le`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_zero_bit_le(s: u64, size: i64) -> u64 {
    port::wrap_find_first_zero_bit_le(s, size)
}

/// `uint64_t wrap___for_each_wrap(uint64_t s, int64_t size, int64_t start, int64_t n)` (`wrappers.c:348`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap___for_each_wrap(s: u64, size: i64, start: i64, n: i64) -> u64 {
    port::wrap___for_each_wrap(s, size, start, n)
}

/// `wrap___for_each_wrap` as `rust_anything_wrap___for_each_wrap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap___for_each_wrap(s: u64, size: i64, start: i64, n: i64) -> u64 {
    port::wrap___for_each_wrap(s, size, start, n)
}

/// `uint64_t wrap_find_first_bit_c(uint64_t s)` (`wrappers.c:361`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_bit_c(s: u64) -> u64 {
    port::wrap_find_first_bit_c(s)
}

/// `wrap_find_first_bit_c` as `rust_anything_wrap_find_first_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_bit_c(s: u64) -> u64 {
    port::wrap_find_first_bit_c(s)
}

/// `uint64_t wrap_find_next_bit_c(uint64_t s, int64_t start)` (`wrappers.c:366`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_c(s, start)
}

/// `wrap_find_next_bit_c` as `rust_anything_wrap_find_next_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_c(s, start)
}

/// `uint64_t wrap_find_nth_bit_c(uint64_t s, int64_t n)` (`wrappers.c:371`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_bit_c(s: u64, n: i64) -> u64 {
    port::wrap_find_nth_bit_c(s, n)
}

/// `wrap_find_nth_bit_c` as `rust_anything_wrap_find_nth_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_bit_c(s: u64, n: i64) -> u64 {
    port::wrap_find_nth_bit_c(s, n)
}

/// `uint64_t wrap_find_first_and_bit_c(uint64_t s1, uint64_t s2)` (`wrappers.c:376`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_and_bit_c(s1: u64, s2: u64) -> u64 {
    port::wrap_find_first_and_bit_c(s1, s2)
}

/// `wrap_find_first_and_bit_c` as `rust_anything_wrap_find_first_and_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_and_bit_c(s1: u64, s2: u64) -> u64 {
    port::wrap_find_first_and_bit_c(s1, s2)
}

/// `uint64_t wrap_find_next_and_bit_c(uint64_t s1, uint64_t s2, int64_t start)` (`wrappers.c:381`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_and_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_and_bit_c(s1, s2, start)
}

/// `wrap_find_next_and_bit_c` as `rust_anything_wrap_find_next_and_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_and_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_and_bit_c(s1, s2, start)
}

/// `uint64_t wrap_find_nth_and_bit_c(uint64_t s1, uint64_t s2, int64_t n)` (`wrappers.c:386`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_and_bit_c(s1: u64, s2: u64, n: i64) -> u64 {
    port::wrap_find_nth_and_bit_c(s1, s2, n)
}

/// `wrap_find_nth_and_bit_c` as `rust_anything_wrap_find_nth_and_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_and_bit_c(s1: u64, s2: u64, n: i64) -> u64 {
    port::wrap_find_nth_and_bit_c(s1, s2, n)
}

/// `uint64_t wrap_find_first_andnot_bit_c(uint64_t s1, uint64_t s2)` (`wrappers.c:391`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_andnot_bit_c(s1: u64, s2: u64) -> u64 {
    port::wrap_find_first_andnot_bit_c(s1, s2)
}

/// `wrap_find_first_andnot_bit_c` as `rust_anything_wrap_find_first_andnot_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_andnot_bit_c(s1: u64, s2: u64) -> u64 {
    port::wrap_find_first_andnot_bit_c(s1, s2)
}

/// `uint64_t wrap_find_next_andnot_bit_c(uint64_t s1, uint64_t s2, int64_t start)` (`wrappers.c:396`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_andnot_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_andnot_bit_c(s1, s2, start)
}

/// `wrap_find_next_andnot_bit_c` as `rust_anything_wrap_find_next_andnot_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_andnot_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_andnot_bit_c(s1, s2, start)
}

/// `uint64_t wrap_find_nth_and_andnot_bit_c(uint64_t s1, uint64_t s2, uint64_t s3, int64_t n)` (`wrappers.c:401`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_nth_and_andnot_bit_c(s1: u64, s2: u64, s3: u64, n: i64) -> u64 {
    port::wrap_find_nth_and_andnot_bit_c(s1, s2, s3, n)
}

/// `wrap_find_nth_and_andnot_bit_c` as `rust_anything_wrap_find_nth_and_andnot_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_nth_and_andnot_bit_c(
    s1: u64,
    s2: u64,
    s3: u64,
    n: i64,
) -> u64 {
    port::wrap_find_nth_and_andnot_bit_c(s1, s2, s3, n)
}

/// `uint64_t wrap_find_first_and_and_bit_c(uint64_t s1, uint64_t s2, uint64_t s3)` (`wrappers.c:407`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_and_and_bit_c(s1: u64, s2: u64, s3: u64) -> u64 {
    port::wrap_find_first_and_and_bit_c(s1, s2, s3)
}

/// `wrap_find_first_and_and_bit_c` as `rust_anything_wrap_find_first_and_and_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_and_and_bit_c(s1: u64, s2: u64, s3: u64) -> u64 {
    port::wrap_find_first_and_and_bit_c(s1, s2, s3)
}

/// `uint64_t wrap_find_next_or_bit_c(uint64_t s1, uint64_t s2, int64_t start)` (`wrappers.c:412`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_or_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_or_bit_c(s1, s2, start)
}

/// `wrap_find_next_or_bit_c` as `rust_anything_wrap_find_next_or_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_or_bit_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_or_bit_c(s1, s2, start)
}

/// `uint64_t wrap_find_first_zero_bit_c(uint64_t s)` (`wrappers.c:417`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_zero_bit_c(s: u64) -> u64 {
    port::wrap_find_first_zero_bit_c(s)
}

/// `wrap_find_first_zero_bit_c` as `rust_anything_wrap_find_first_zero_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_zero_bit_c(s: u64) -> u64 {
    port::wrap_find_first_zero_bit_c(s)
}

/// `uint64_t wrap_find_next_zero_bit_c(uint64_t s, int64_t start)` (`wrappers.c:422`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_zero_bit_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_c(s, start)
}

/// `wrap_find_next_zero_bit_c` as `rust_anything_wrap_find_next_zero_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_zero_bit_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_c(s, start)
}

/// `uint64_t wrap_find_last_bit_c(uint64_t s)` (`wrappers.c:427`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_last_bit_c(s: u64) -> u64 {
    port::wrap_find_last_bit_c(s)
}

/// `wrap_find_last_bit_c` as `rust_anything_wrap_find_last_bit_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_last_bit_c(s: u64) -> u64 {
    port::wrap_find_last_bit_c(s)
}

/// `uint64_t wrap_find_next_bit_wrap_c(uint64_t s, int64_t start)` (`wrappers.c:432`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit_wrap_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_wrap_c(s, start)
}

/// `wrap_find_next_bit_wrap_c` as `rust_anything_wrap_find_next_bit_wrap_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit_wrap_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_wrap_c(s, start)
}

/// `uint64_t wrap_find_next_and_bit_wrap_c(uint64_t s1, uint64_t s2, int64_t start)` (`wrappers.c:437`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_and_bit_wrap_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_and_bit_wrap_c(s1, s2, start)
}

/// `wrap_find_next_and_bit_wrap_c` as `rust_anything_wrap_find_next_and_bit_wrap_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_and_bit_wrap_c(s1: u64, s2: u64, start: i64) -> u64 {
    port::wrap_find_next_and_bit_wrap_c(s1, s2, start)
}

/// `uint64_t wrap_find_next_bit_le_c(uint64_t s, int64_t start)` (`wrappers.c:442`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_bit_le_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_le_c(s, start)
}

/// `wrap_find_next_bit_le_c` as `rust_anything_wrap_find_next_bit_le_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_bit_le_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_bit_le_c(s, start)
}

/// `uint64_t wrap_find_next_zero_bit_le_c(uint64_t s, int64_t start)` (`wrappers.c:447`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_next_zero_bit_le_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_le_c(s, start)
}

/// `wrap_find_next_zero_bit_le_c` as `rust_anything_wrap_find_next_zero_bit_le_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_next_zero_bit_le_c(s: u64, start: i64) -> u64 {
    port::wrap_find_next_zero_bit_le_c(s, start)
}

/// `uint64_t wrap_find_first_zero_bit_le_c(uint64_t s)` (`wrappers.c:452`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_find_first_zero_bit_le_c(s: u64) -> u64 {
    port::wrap_find_first_zero_bit_le_c(s)
}

/// `wrap_find_first_zero_bit_le_c` as `rust_anything_wrap_find_first_zero_bit_le_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_find_first_zero_bit_le_c(s: u64) -> u64 {
    port::wrap_find_first_zero_bit_le_c(s)
}

/// `uint64_t wrap_each_set_bit(uint64_t s, int64_t size)` (`wrappers.c:460`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_bit(s: u64, size: i64) -> u64 {
    port::wrap_each_set_bit(s, size)
}

/// `wrap_each_set_bit` as `rust_anything_wrap_each_set_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_bit(s: u64, size: i64) -> u64 {
    port::wrap_each_set_bit(s, size)
}

/// `uint64_t wrap_each_set_bit_from(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:468`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_bit_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bit_from(s, size, start)
}

/// `wrap_each_set_bit_from` as `rust_anything_wrap_each_set_bit_from`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_bit_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bit_from(s, size, start)
}

/// `uint64_t wrap_each_clear_bit(uint64_t s, int64_t size)` (`wrappers.c:477`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_clear_bit(s: u64, size: i64) -> u64 {
    port::wrap_each_clear_bit(s, size)
}

/// `wrap_each_clear_bit` as `rust_anything_wrap_each_clear_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_clear_bit(s: u64, size: i64) -> u64 {
    port::wrap_each_clear_bit(s, size)
}

/// `uint64_t wrap_each_clear_bit_from(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:485`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_clear_bit_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_clear_bit_from(s, size, start)
}

/// `wrap_each_clear_bit_from` as `rust_anything_wrap_each_clear_bit_from`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_clear_bit_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_clear_bit_from(s, size, start)
}

/// `uint64_t wrap_each_and_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:494`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_and_bit(s1, s2, size)
}

/// `wrap_each_and_bit` as `rust_anything_wrap_each_and_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_and_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_and_bit(s1, s2, size)
}

/// `uint64_t wrap_each_andnot_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:502`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_andnot_bit(s1, s2, size)
}

/// `wrap_each_andnot_bit` as `rust_anything_wrap_each_andnot_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_andnot_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_andnot_bit(s1, s2, size)
}

/// `uint64_t wrap_each_or_bit(uint64_t s1, uint64_t s2, int64_t size)` (`wrappers.c:510`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_or_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_or_bit(s1, s2, size)
}

/// `wrap_each_or_bit` as `rust_anything_wrap_each_or_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_or_bit(s1: u64, s2: u64, size: i64) -> u64 {
    port::wrap_each_or_bit(s1, s2, size)
}

/// `uint64_t wrap_each_set_bitrange(uint64_t s, int64_t size)` (`wrappers.c:518`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_bitrange(s: u64, size: i64) -> u64 {
    port::wrap_each_set_bitrange(s, size)
}

/// `wrap_each_set_bitrange` as `rust_anything_wrap_each_set_bitrange`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_bitrange(s: u64, size: i64) -> u64 {
    port::wrap_each_set_bitrange(s, size)
}

/// `uint64_t wrap_each_set_bitrange_from(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:526`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_bitrange_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bitrange_from(s, size, start)
}

/// `wrap_each_set_bitrange_from` as `rust_anything_wrap_each_set_bitrange_from`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_bitrange_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bitrange_from(s, size, start)
}

/// `uint64_t wrap_each_clear_bitrange(uint64_t s, int64_t size)` (`wrappers.c:535`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_clear_bitrange(s: u64, size: i64) -> u64 {
    port::wrap_each_clear_bitrange(s, size)
}

/// `wrap_each_clear_bitrange` as `rust_anything_wrap_each_clear_bitrange`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_clear_bitrange(s: u64, size: i64) -> u64 {
    port::wrap_each_clear_bitrange(s, size)
}

/// `uint64_t wrap_each_clear_bitrange_from(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:543`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_clear_bitrange_from(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_clear_bitrange_from(s, size, start)
}

/// `wrap_each_clear_bitrange_from` as `rust_anything_wrap_each_clear_bitrange_from`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_clear_bitrange_from(
    s: u64,
    size: i64,
    start: i64,
) -> u64 {
    port::wrap_each_clear_bitrange_from(s, size, start)
}

/// `uint64_t wrap_each_set_bit_wrap(uint64_t s, int64_t size, int64_t start)` (`wrappers.c:552`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bit_wrap(s, size, start)
}

/// `wrap_each_set_bit_wrap` as `rust_anything_wrap_each_set_bit_wrap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_bit_wrap(s: u64, size: i64, start: i64) -> u64 {
    port::wrap_each_set_bit_wrap(s, size, start)
}

/// `uint64_t wrap_each_set_clump8(uint64_t s, int64_t size)` (`wrappers.c:560`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_each_set_clump8(s: u64, size: i64) -> u64 {
    port::wrap_each_set_clump8(s, size)
}

/// `wrap_each_set_clump8` as `rust_anything_wrap_each_set_clump8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_each_set_clump8(s: u64, size: i64) -> u64 {
    port::wrap_each_set_clump8(s, size)
}

/// `uint64_t wrap_bitmap_and(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:571`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_and(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_and(s1, s2, ds, nb)
}

/// `wrap_bitmap_and` as `rust_anything_wrap_bitmap_and`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_and(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_and(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_or(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:578`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_or(s1, s2, ds, nb)
}

/// `wrap_bitmap_or` as `rust_anything_wrap_bitmap_or`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_or(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_xor(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:584`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_xor(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_xor(s1, s2, ds, nb)
}

/// `wrap_bitmap_xor` as `rust_anything_wrap_bitmap_xor`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_xor(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_xor(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_andnot(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:590`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_andnot(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_andnot(s1, s2, ds, nb)
}

/// `wrap_bitmap_andnot` as `rust_anything_wrap_bitmap_andnot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_andnot(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_andnot(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_weighted_or(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:597`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weighted_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weighted_or(s1, s2, ds, nb)
}

/// `wrap_bitmap_weighted_or` as `rust_anything_wrap_bitmap_weighted_or`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weighted_or(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weighted_or(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_weighted_xor(uint64_t s1, uint64_t s2, uint64_t ds, int64_t nb)` (`wrappers.c:605`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weighted_xor(s1: u64, s2: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weighted_xor(s1, s2, ds, nb)
}

/// `wrap_bitmap_weighted_xor` as `rust_anything_wrap_bitmap_weighted_xor`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weighted_xor(
    s1: u64,
    s2: u64,
    ds: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_weighted_xor(s1, s2, ds, nb)
}

/// `uint64_t wrap_bitmap_complement(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:613`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_complement(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_complement(s, ds, nb)
}

/// `wrap_bitmap_complement` as `rust_anything_wrap_bitmap_complement`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_complement(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_complement(s, ds, nb)
}

/// `uint64_t wrap_bitmap_replace(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t ds, int64_t nb)` (`wrappers.c:619`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_replace(s1: u64, s2: u64, s3: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_replace(s1, s2, s3, ds, nb)
}

/// `wrap_bitmap_replace` as `rust_anything_wrap_bitmap_replace`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_replace(
    s1: u64,
    s2: u64,
    s3: u64,
    ds: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_replace(s1, s2, s3, ds, nb)
}

/// `uint64_t wrap_bitmap_equal(uint64_t s1, uint64_t s2, uint64_t sel, int64_t nb)` (`wrappers.c:626`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_equal(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_equal(s1, s2, sel, nb)
}

/// `wrap_bitmap_equal` as `rust_anything_wrap_bitmap_equal`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_equal(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_equal(s1, s2, sel, nb)
}

/// `uint64_t wrap_bitmap_or_equal(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t sel, int64_t nb)` (`wrappers.c:632`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_or_equal(s1: u64, s2: u64, s3: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_or_equal(s1, s2, s3, sel, nb)
}

/// `wrap_bitmap_or_equal` as `rust_anything_wrap_bitmap_or_equal`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_or_equal(
    s1: u64,
    s2: u64,
    s3: u64,
    sel: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_or_equal(s1, s2, s3, sel, nb)
}

/// `uint64_t wrap_bitmap_intersects(uint64_t s1, uint64_t s2, uint64_t sel, int64_t nb)` (`wrappers.c:645`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_intersects(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_intersects(s1, s2, sel, nb)
}

/// `wrap_bitmap_intersects` as `rust_anything_wrap_bitmap_intersects`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_intersects(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_intersects(s1, s2, sel, nb)
}

/// `uint64_t wrap_bitmap_intersects_and(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t sel, int64_t nb)` (`wrappers.c:652`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_intersects_and(s1: u64, s2: u64, s3: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_intersects_and(s1, s2, s3, sel, nb)
}

/// `wrap_bitmap_intersects_and` as `rust_anything_wrap_bitmap_intersects_and`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_intersects_and(
    s1: u64,
    s2: u64,
    s3: u64,
    sel: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_intersects_and(s1, s2, s3, sel, nb)
}

/// `uint64_t wrap_bitmap_subset(uint64_t s1, uint64_t s2, uint64_t sel, int64_t nb)` (`wrappers.c:659`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_subset(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_subset(s1, s2, sel, nb)
}

/// `wrap_bitmap_subset` as `rust_anything_wrap_bitmap_subset`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_subset(s1: u64, s2: u64, sel: u64, nb: i64) -> u64 {
    port::wrap_bitmap_subset(s1, s2, sel, nb)
}

/// `uint64_t wrap_bitmap_empty(uint64_t s, uint64_t mode, int64_t nb)` (`wrappers.c:665`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_empty(s: u64, mode: u64, nb: i64) -> u64 {
    port::wrap_bitmap_empty(s, mode, nb)
}

/// `wrap_bitmap_empty` as `rust_anything_wrap_bitmap_empty`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_empty(s: u64, mode: u64, nb: i64) -> u64 {
    port::wrap_bitmap_empty(s, mode, nb)
}

/// `uint64_t wrap_bitmap_full(uint64_t s, uint64_t mode, int64_t nb)` (`wrappers.c:674`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_full(s: u64, mode: u64, nb: i64) -> u64 {
    port::wrap_bitmap_full(s, mode, nb)
}

/// `wrap_bitmap_full` as `rust_anything_wrap_bitmap_full`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_full(s: u64, mode: u64, nb: i64) -> u64 {
    port::wrap_bitmap_full(s, mode, nb)
}

/// `uint64_t wrap_bitmap_weight(uint64_t s, int64_t nb)` (`wrappers.c:683`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight(s: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight(s, nb)
}

/// `wrap_bitmap_weight` as `rust_anything_wrap_bitmap_weight`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight(s: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight(s, nb)
}

/// `uint64_t wrap_bitmap_weight_and(uint64_t s1, uint64_t s2, int64_t nb)` (`wrappers.c:688`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_and(s1: u64, s2: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight_and(s1, s2, nb)
}

/// `wrap_bitmap_weight_and` as `rust_anything_wrap_bitmap_weight_and`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_and(s1: u64, s2: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight_and(s1, s2, nb)
}

/// `uint64_t wrap_bitmap_weight_andnot(uint64_t s1, uint64_t s2, int64_t nb)` (`wrappers.c:693`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_andnot(s1: u64, s2: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight_andnot(s1, s2, nb)
}

/// `wrap_bitmap_weight_andnot` as `rust_anything_wrap_bitmap_weight_andnot`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_andnot(s1: u64, s2: u64, nb: i64) -> u64 {
    port::wrap_bitmap_weight_andnot(s1, s2, nb)
}

/// `uint64_t wrap_bitmap_weight_from(uint64_t s, int64_t start, int64_t end)` (`wrappers.c:698`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_from(s: u64, start: i64, end: i64) -> u64 {
    port::wrap_bitmap_weight_from(s, start, end)
}

/// `wrap_bitmap_weight_from` as `rust_anything_wrap_bitmap_weight_from`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_from(s: u64, start: i64, end: i64) -> u64 {
    port::wrap_bitmap_weight_from(s, start, end)
}

/// `uint64_t wrap_bitmap_set(uint64_t s, int64_t start, int64_t len)` (`wrappers.c:703`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_set(s: u64, start: i64, len: i64) -> u64 {
    port::wrap_bitmap_set(s, start, len)
}

/// `wrap_bitmap_set` as `rust_anything_wrap_bitmap_set`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_set(s: u64, start: i64, len: i64) -> u64 {
    port::wrap_bitmap_set(s, start, len)
}

/// `uint64_t wrap_bitmap_clear(uint64_t s, int64_t start, int64_t len)` (`wrappers.c:709`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_clear(s: u64, start: i64, len: i64) -> u64 {
    port::wrap_bitmap_clear(s, start, len)
}

/// `wrap_bitmap_clear` as `rust_anything_wrap_bitmap_clear`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_clear(s: u64, start: i64, len: i64) -> u64 {
    port::wrap_bitmap_clear(s, start, len)
}

/// `uint64_t wrap_bitmap_shift_right(uint64_t s, uint64_t ds, int64_t shift, int64_t nb)` (`wrappers.c:715`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_shift_right(s: u64, ds: u64, shift: i64, nb: i64) -> u64 {
    port::wrap_bitmap_shift_right(s, ds, shift, nb)
}

/// `wrap_bitmap_shift_right` as `rust_anything_wrap_bitmap_shift_right`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_shift_right(
    s: u64,
    ds: u64,
    shift: i64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_shift_right(s, ds, shift, nb)
}

/// `uint64_t wrap_bitmap_shift_left(uint64_t s, uint64_t ds, int64_t shift, int64_t nb)` (`wrappers.c:724`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_shift_left(s: u64, ds: u64, shift: i64, nb: i64) -> u64 {
    port::wrap_bitmap_shift_left(s, ds, shift, nb)
}

/// `wrap_bitmap_shift_left` as `rust_anything_wrap_bitmap_shift_left`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_shift_left(
    s: u64,
    ds: u64,
    shift: i64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_shift_left(s, ds, shift, nb)
}

/// `uint64_t wrap_bitmap_cut(uint64_t s, uint64_t ds, int64_t first, int64_t cut, int64_t nb)` (`wrappers.c:733`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_cut(s: u64, ds: u64, first: i64, cut: i64, nb: i64) -> u64 {
    port::wrap_bitmap_cut(s, ds, first, cut, nb)
}

/// `wrap_bitmap_cut` as `rust_anything_wrap_bitmap_cut`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_cut(
    s: u64,
    ds: u64,
    first: i64,
    cut: i64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_cut(s, ds, first, cut, nb)
}

/// `uint64_t wrap_bitmap_scatter(uint64_t s, uint64_t sm, uint64_t ds, int64_t nb)` (`wrappers.c:741`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_scatter(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_scatter(s, sm, ds, nb)
}

/// `wrap_bitmap_scatter` as `rust_anything_wrap_bitmap_scatter`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_scatter(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_scatter(s, sm, ds, nb)
}

/// `uint64_t wrap_bitmap_gather(uint64_t s, uint64_t sm, uint64_t ds, int64_t nb)` (`wrappers.c:747`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_gather(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_gather(s, sm, ds, nb)
}

/// `wrap_bitmap_gather` as `rust_anything_wrap_bitmap_gather`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_gather(s: u64, sm: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_gather(s, sm, ds, nb)
}

/// `uint64_t wrap_bitmap_remap(uint64_t s, uint64_t so, uint64_t sn, uint64_t sel, uint64_t ds, int64_t nb)` (`wrappers.c:753`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_remap(s: u64, so: u64, sn: u64, sel: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_remap(s, so, sn, sel, ds, nb)
}

/// `wrap_bitmap_remap` as `rust_anything_wrap_bitmap_remap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_remap(
    s: u64,
    so: u64,
    sn: u64,
    sel: u64,
    ds: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_remap(s, so, sn, sel, ds, nb)
}

/// `uint64_t wrap_bitmap_bitremap(uint64_t so, uint64_t sn, uint64_t sel, int64_t oldbit, int64_t bits)` (`wrappers.c:762`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_bitremap(so: u64, sn: u64, sel: u64, oldbit: i64, bits: i64) -> u64 {
    port::wrap_bitmap_bitremap(so, sn, sel, oldbit, bits)
}

/// `wrap_bitmap_bitremap` as `rust_anything_wrap_bitmap_bitremap`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_bitremap(
    so: u64,
    sn: u64,
    sel: u64,
    oldbit: i64,
    bits: i64,
) -> u64 {
    port::wrap_bitmap_bitremap(so, sn, sel, oldbit, bits)
}

/// `uint64_t wrap_bitmap_onto(uint64_t s, uint64_t sr, uint64_t sel, uint64_t ds, int64_t nb)` (`wrappers.c:772`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_onto(s: u64, sr: u64, sel: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_onto(s, sr, sel, ds, nb)
}

/// `wrap_bitmap_onto` as `rust_anything_wrap_bitmap_onto`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_onto(
    s: u64,
    sr: u64,
    sel: u64,
    ds: u64,
    nb: i64,
) -> u64 {
    port::wrap_bitmap_onto(s, sr, sel, ds, nb)
}

/// `uint64_t wrap_bitmap_fold(uint64_t s, uint64_t ds, int64_t sz, int64_t nb)` (`wrappers.c:781`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_fold(s: u64, ds: u64, sz: i64, nb: i64) -> u64 {
    port::wrap_bitmap_fold(s, ds, sz, nb)
}

/// `wrap_bitmap_fold` as `rust_anything_wrap_bitmap_fold`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_fold(s: u64, ds: u64, sz: i64, nb: i64) -> u64 {
    port::wrap_bitmap_fold(s, ds, sz, nb)
}

/// `uint64_t wrap_bitmap_find_next_zero_area(uint64_t s, int64_t size, int64_t start, int64_t nr, int64_t am)` (`wrappers.c:787`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_find_next_zero_area(
    s: u64,
    size: i64,
    start: i64,
    nr: i64,
    am: i64,
) -> u64 {
    port::wrap_bitmap_find_next_zero_area(s, size, start, nr, am)
}

/// `wrap_bitmap_find_next_zero_area` as `rust_anything_wrap_bitmap_find_next_zero_area`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_find_next_zero_area(
    s: u64,
    size: i64,
    start: i64,
    nr: i64,
    am: i64,
) -> u64 {
    port::wrap_bitmap_find_next_zero_area(s, size, start, nr, am)
}

/// `uint64_t wrap_bitmap_find_next_zero_area_off(uint64_t s, int64_t size, int64_t start, int64_t nr, int64_t am, int64_t ao)` (`wrappers.c:796`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_find_next_zero_area_off(
    s: u64,
    size: i64,
    start: i64,
    nr: i64,
    am: i64,
    ao: i64,
) -> u64 {
    port::wrap_bitmap_find_next_zero_area_off(s, size, start, nr, am, ao)
}

/// `wrap_bitmap_find_next_zero_area_off` as `rust_anything_wrap_bitmap_find_next_zero_area_off`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_find_next_zero_area_off(
    s: u64,
    size: i64,
    start: i64,
    nr: i64,
    am: i64,
    ao: i64,
) -> u64 {
    port::wrap_bitmap_find_next_zero_area_off(s, size, start, nr, am, ao)
}

/// `uint64_t wrap_bitmap_zero(uint64_t ds, int64_t nb)` (`wrappers.c:807`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_zero(ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_zero(ds, nb)
}

/// `wrap_bitmap_zero` as `rust_anything_wrap_bitmap_zero`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_zero(ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_zero(ds, nb)
}

/// `uint64_t wrap_bitmap_fill(uint64_t ds, int64_t nb)` (`wrappers.c:813`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_fill(ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_fill(ds, nb)
}

/// `wrap_bitmap_fill` as `rust_anything_wrap_bitmap_fill`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_fill(ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_fill(ds, nb)
}

/// `uint64_t wrap_bitmap_copy(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:819`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_copy(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_copy(s, ds, nb)
}

/// `wrap_bitmap_copy` as `rust_anything_wrap_bitmap_copy`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_copy(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_copy(s, ds, nb)
}

/// `uint64_t wrap_bitmap_copy_clear_tail(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:825`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_copy_clear_tail(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_copy_clear_tail(s, ds, nb)
}

/// `wrap_bitmap_copy_clear_tail` as `rust_anything_wrap_bitmap_copy_clear_tail`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_copy_clear_tail(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_copy_clear_tail(s, ds, nb)
}

/// `uint64_t wrap_bitmap_copy_and_extend(uint64_t s, uint64_t ds, int64_t count, int64_t size)` (`wrappers.c:831`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_copy_and_extend(s: u64, ds: u64, count: i64, size: i64) -> u64 {
    port::wrap_bitmap_copy_and_extend(s, ds, count, size)
}

/// `wrap_bitmap_copy_and_extend` as `rust_anything_wrap_bitmap_copy_and_extend`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_copy_and_extend(
    s: u64,
    ds: u64,
    count: i64,
    size: i64,
) -> u64 {
    port::wrap_bitmap_copy_and_extend(s, ds, count, size)
}

/// `uint64_t wrap_bitmap_from_arr32(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:840`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_from_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_from_arr32(s, ds, nb)
}

/// `wrap_bitmap_from_arr32` as `rust_anything_wrap_bitmap_from_arr32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_from_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_from_arr32(s, ds, nb)
}

/// `uint64_t wrap_bitmap_to_arr32(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:846`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_to_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_to_arr32(s, ds, nb)
}

/// `wrap_bitmap_to_arr32` as `rust_anything_wrap_bitmap_to_arr32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_to_arr32(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_to_arr32(s, ds, nb)
}

/// `uint64_t wrap_bitmap_from_arr64(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:852`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_from_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_from_arr64(s, ds, nb)
}

/// `wrap_bitmap_from_arr64` as `rust_anything_wrap_bitmap_from_arr64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_from_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_from_arr64(s, ds, nb)
}

/// `uint64_t wrap_bitmap_to_arr64(uint64_t s, uint64_t ds, int64_t nb)` (`wrappers.c:858`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_to_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_to_arr64(s, ds, nb)
}

/// `wrap_bitmap_to_arr64` as `rust_anything_wrap_bitmap_to_arr64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_to_arr64(s: u64, ds: u64, nb: i64) -> u64 {
    port::wrap_bitmap_to_arr64(s, ds, nb)
}

/// `uint64_t wrap_bitmap_from_u64(uint64_t v, uint64_t ds)` (`wrappers.c:864`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_from_u64(v: u64, ds: u64) -> u64 {
    port::wrap_bitmap_from_u64(v, ds)
}

/// `wrap_bitmap_from_u64` as `rust_anything_wrap_bitmap_from_u64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_from_u64(v: u64, ds: u64) -> u64 {
    port::wrap_bitmap_from_u64(v, ds)
}

/// `uint64_t wrap_bitmap_read(uint64_t s, int64_t start, int64_t nb)` (`wrappers.c:870`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_read(s: u64, start: i64, nb: i64) -> u64 {
    port::wrap_bitmap_read(s, start, nb)
}

/// `wrap_bitmap_read` as `rust_anything_wrap_bitmap_read`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_read(s: u64, start: i64, nb: i64) -> u64 {
    port::wrap_bitmap_read(s, start, nb)
}

/// `uint64_t wrap_bitmap_write(uint64_t s, int64_t start, int64_t nb, uint64_t v)` (`wrappers.c:875`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_write(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    port::wrap_bitmap_write(s, start, nb, v)
}

/// `wrap_bitmap_write` as `rust_anything_wrap_bitmap_write`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_write(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    port::wrap_bitmap_write(s, start, nb, v)
}

/// `uint64_t wrap_bitmap_read_hi(uint64_t s, int64_t start, int64_t nb)` — nbits 64..67 edge arm (wrappers.c).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_read_hi(s: u64, start: i64, nb: i64) -> u64 {
    port::wrap_bitmap_read_hi(s, start, nb)
}

/// `wrap_bitmap_read_hi` as `rust_anything_wrap_bitmap_read_hi`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_read_hi(s: u64, start: i64, nb: i64) -> u64 {
    port::wrap_bitmap_read_hi(s, start, nb)
}

/// `uint64_t wrap_bitmap_write_hi(uint64_t s, int64_t start, int64_t nb, uint64_t v)` — nbits 64..67 edge arm (wrappers.c).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_write_hi(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    port::wrap_bitmap_write_hi(s, start, nb, v)
}

/// `wrap_bitmap_write_hi` as `rust_anything_wrap_bitmap_write_hi`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_write_hi(s: u64, start: i64, nb: i64, v: u64) -> u64 {
    port::wrap_bitmap_write_hi(s, start, nb, v)
}

/// `uint64_t wrap_bitmap_get_value8(uint64_t s, int64_t start)` (`wrappers.c:881`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_get_value8(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_get_value8(s, start)
}

/// `wrap_bitmap_get_value8` as `rust_anything_wrap_bitmap_get_value8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_get_value8(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_get_value8(s, start)
}

/// `uint64_t wrap_bitmap_set_value8(uint64_t s, int64_t start, int64_t v)` (`wrappers.c:886`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_set_value8(s: u64, start: i64, v: i64) -> u64 {
    port::wrap_bitmap_set_value8(s, start, v)
}

/// `wrap_bitmap_set_value8` as `rust_anything_wrap_bitmap_set_value8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_set_value8(s: u64, start: i64, v: i64) -> u64 {
    port::wrap_bitmap_set_value8(s, start, v)
}

/// `uint64_t wrap_bitmap_release_region(uint64_t s, int64_t pos, int64_t order)` (`wrappers.c:892`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_release_region(s: u64, pos: i64, order: i64) -> u64 {
    port::wrap_bitmap_release_region(s, pos, order)
}

/// `wrap_bitmap_release_region` as `rust_anything_wrap_bitmap_release_region`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_release_region(s: u64, pos: i64, order: i64) -> u64 {
    port::wrap_bitmap_release_region(s, pos, order)
}

/// `uint64_t wrap_bitmap_allocate_region(uint64_t s, int64_t pos, int64_t order)` (`wrappers.c:899`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_allocate_region(s: u64, pos: i64, order: i64) -> u64 {
    port::wrap_bitmap_allocate_region(s, pos, order)
}

/// `wrap_bitmap_allocate_region` as `rust_anything_wrap_bitmap_allocate_region`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_allocate_region(s: u64, pos: i64, order: i64) -> u64 {
    port::wrap_bitmap_allocate_region(s, pos, order)
}

/// `uint64_t wrap_bitmap_find_free_region(uint64_t s, uint64_t mode, int64_t bits, int64_t order)` (`wrappers.c:907`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_find_free_region(s: u64, mode: u64, bits: i64, order: i64) -> u64 {
    port::wrap_bitmap_find_free_region(s, mode, bits, order)
}

/// `wrap_bitmap_find_free_region` as `rust_anything_wrap_bitmap_find_free_region`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_find_free_region(
    s: u64,
    mode: u64,
    bits: i64,
    order: i64,
) -> u64 {
    port::wrap_bitmap_find_free_region(s, mode, bits, order)
}

/// `uint64_t wrap_bitmap_and_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:926`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_and_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_and_c(s1, s2, ds)
}

/// `wrap_bitmap_and_c` as `rust_anything_wrap_bitmap_and_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_and_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_and_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_or_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:933`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_or_c(s1, s2, ds)
}

/// `wrap_bitmap_or_c` as `rust_anything_wrap_bitmap_or_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_or_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_xor_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:939`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_xor_c(s1, s2, ds)
}

/// `wrap_bitmap_xor_c` as `rust_anything_wrap_bitmap_xor_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_xor_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_andnot_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:945`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_andnot_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_andnot_c(s1, s2, ds)
}

/// `wrap_bitmap_andnot_c` as `rust_anything_wrap_bitmap_andnot_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_andnot_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_andnot_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_weighted_or_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:952`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weighted_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_weighted_or_c(s1, s2, ds)
}

/// `wrap_bitmap_weighted_or_c` as `rust_anything_wrap_bitmap_weighted_or_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weighted_or_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_weighted_or_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_weighted_xor_c(uint64_t s1, uint64_t s2, uint64_t ds)` (`wrappers.c:959`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weighted_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_weighted_xor_c(s1, s2, ds)
}

/// `wrap_bitmap_weighted_xor_c` as `rust_anything_wrap_bitmap_weighted_xor_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weighted_xor_c(s1: u64, s2: u64, ds: u64) -> u64 {
    port::wrap_bitmap_weighted_xor_c(s1, s2, ds)
}

/// `uint64_t wrap_bitmap_complement_c(uint64_t s, uint64_t ds)` (`wrappers.c:966`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_complement_c(s: u64, ds: u64) -> u64 {
    port::wrap_bitmap_complement_c(s, ds)
}

/// `wrap_bitmap_complement_c` as `rust_anything_wrap_bitmap_complement_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_complement_c(s: u64, ds: u64) -> u64 {
    port::wrap_bitmap_complement_c(s, ds)
}

/// `uint64_t wrap_bitmap_replace_c(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t ds)` (`wrappers.c:972`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_replace_c(s1: u64, s2: u64, s3: u64, ds: u64) -> u64 {
    port::wrap_bitmap_replace_c(s1, s2, s3, ds)
}

/// `wrap_bitmap_replace_c` as `rust_anything_wrap_bitmap_replace_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_replace_c(s1: u64, s2: u64, s3: u64, ds: u64) -> u64 {
    port::wrap_bitmap_replace_c(s1, s2, s3, ds)
}

/// `uint64_t wrap_bitmap_equal_c(uint64_t s1, uint64_t s2, uint64_t sel)` (`wrappers.c:979`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_equal_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_equal_c(s1, s2, sel)
}

/// `wrap_bitmap_equal_c` as `rust_anything_wrap_bitmap_equal_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_equal_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_equal_c(s1, s2, sel)
}

/// `uint64_t wrap_bitmap_equal_c128(uint64_t s1, uint64_t s2, uint64_t sel)` (`wrappers.c:986`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_equal_c128(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_equal_c128(s1, s2, sel)
}

/// `wrap_bitmap_equal_c128` as `rust_anything_wrap_bitmap_equal_c128`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_equal_c128(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_equal_c128(s1, s2, sel)
}

/// `uint64_t wrap_bitmap_or_equal_c(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t sel)` (`wrappers.c:992`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_or_equal_c(s1: u64, s2: u64, s3: u64, sel: u64) -> u64 {
    port::wrap_bitmap_or_equal_c(s1, s2, s3, sel)
}

/// `wrap_bitmap_or_equal_c` as `rust_anything_wrap_bitmap_or_equal_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_or_equal_c(s1: u64, s2: u64, s3: u64, sel: u64) -> u64 {
    port::wrap_bitmap_or_equal_c(s1, s2, s3, sel)
}

/// `uint64_t wrap_bitmap_intersects_c(uint64_t s1, uint64_t s2, uint64_t sel)` (`wrappers.c:1005`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_intersects_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_intersects_c(s1, s2, sel)
}

/// `wrap_bitmap_intersects_c` as `rust_anything_wrap_bitmap_intersects_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_intersects_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_intersects_c(s1, s2, sel)
}

/// `uint64_t wrap_bitmap_intersects_and_c(uint64_t s1, uint64_t s2, uint64_t s3, uint64_t sel)` (`wrappers.c:1011`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_intersects_and_c(s1: u64, s2: u64, s3: u64, sel: u64) -> u64 {
    port::wrap_bitmap_intersects_and_c(s1, s2, s3, sel)
}

/// `wrap_bitmap_intersects_and_c` as `rust_anything_wrap_bitmap_intersects_and_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_intersects_and_c(
    s1: u64,
    s2: u64,
    s3: u64,
    sel: u64,
) -> u64 {
    port::wrap_bitmap_intersects_and_c(s1, s2, s3, sel)
}

/// `uint64_t wrap_bitmap_subset_c(uint64_t s1, uint64_t s2, uint64_t sel)` (`wrappers.c:1018`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_subset_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_subset_c(s1, s2, sel)
}

/// `wrap_bitmap_subset_c` as `rust_anything_wrap_bitmap_subset_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_subset_c(s1: u64, s2: u64, sel: u64) -> u64 {
    port::wrap_bitmap_subset_c(s1, s2, sel)
}

/// `uint64_t wrap_bitmap_empty_c(uint64_t s, uint64_t mode)` (`wrappers.c:1024`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_empty_c(s: u64, mode: u64) -> u64 {
    port::wrap_bitmap_empty_c(s, mode)
}

/// `wrap_bitmap_empty_c` as `rust_anything_wrap_bitmap_empty_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_empty_c(s: u64, mode: u64) -> u64 {
    port::wrap_bitmap_empty_c(s, mode)
}

/// `uint64_t wrap_bitmap_full_c(uint64_t s, uint64_t mode)` (`wrappers.c:1033`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_full_c(s: u64, mode: u64) -> u64 {
    port::wrap_bitmap_full_c(s, mode)
}

/// `wrap_bitmap_full_c` as `rust_anything_wrap_bitmap_full_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_full_c(s: u64, mode: u64) -> u64 {
    port::wrap_bitmap_full_c(s, mode)
}

/// `uint64_t wrap_bitmap_weight_c(uint64_t s)` (`wrappers.c:1042`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_c(s: u64) -> u64 {
    port::wrap_bitmap_weight_c(s)
}

/// `wrap_bitmap_weight_c` as `rust_anything_wrap_bitmap_weight_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_c(s: u64) -> u64 {
    port::wrap_bitmap_weight_c(s)
}

/// `uint64_t wrap_bitmap_weight_and_c(uint64_t s1, uint64_t s2)` (`wrappers.c:1047`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_and_c(s1: u64, s2: u64) -> u64 {
    port::wrap_bitmap_weight_and_c(s1, s2)
}

/// `wrap_bitmap_weight_and_c` as `rust_anything_wrap_bitmap_weight_and_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_and_c(s1: u64, s2: u64) -> u64 {
    port::wrap_bitmap_weight_and_c(s1, s2)
}

/// `uint64_t wrap_bitmap_weight_andnot_c(uint64_t s1, uint64_t s2)` (`wrappers.c:1052`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_andnot_c(s1: u64, s2: u64) -> u64 {
    port::wrap_bitmap_weight_andnot_c(s1, s2)
}

/// `wrap_bitmap_weight_andnot_c` as `rust_anything_wrap_bitmap_weight_andnot_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_andnot_c(s1: u64, s2: u64) -> u64 {
    port::wrap_bitmap_weight_andnot_c(s1, s2)
}

/// `uint64_t wrap_bitmap_weight_from_c(uint64_t s, int64_t start)` (`wrappers.c:1057`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_weight_from_c(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_weight_from_c(s, start)
}

/// `wrap_bitmap_weight_from_c` as `rust_anything_wrap_bitmap_weight_from_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_weight_from_c(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_weight_from_c(s, start)
}

/// `uint64_t wrap_bitmap_zero_c(uint64_t ds)` (`wrappers.c:1062`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_zero_c(ds: u64) -> u64 {
    port::wrap_bitmap_zero_c(ds)
}

/// `wrap_bitmap_zero_c` as `rust_anything_wrap_bitmap_zero_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_zero_c(ds: u64) -> u64 {
    port::wrap_bitmap_zero_c(ds)
}

/// `uint64_t wrap_bitmap_fill_c(uint64_t ds)` (`wrappers.c:1068`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_fill_c(ds: u64) -> u64 {
    port::wrap_bitmap_fill_c(ds)
}

/// `wrap_bitmap_fill_c` as `rust_anything_wrap_bitmap_fill_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_fill_c(ds: u64) -> u64 {
    port::wrap_bitmap_fill_c(ds)
}

/// `uint64_t wrap_bitmap_copy_c(uint64_t s, uint64_t ds)` (`wrappers.c:1074`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_copy_c(s: u64, ds: u64) -> u64 {
    port::wrap_bitmap_copy_c(s, ds)
}

/// `wrap_bitmap_copy_c` as `rust_anything_wrap_bitmap_copy_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_copy_c(s: u64, ds: u64) -> u64 {
    port::wrap_bitmap_copy_c(s, ds)
}

/// `uint64_t wrap_bitmap_set_c1(uint64_t s, int64_t start)` (`wrappers.c:1082`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_set_c1(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_set_c1(s, start)
}

/// `wrap_bitmap_set_c1` as `rust_anything_wrap_bitmap_set_c1`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_set_c1(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_set_c1(s, start)
}

/// `uint64_t wrap_bitmap_clear_c1(uint64_t s, int64_t start)` (`wrappers.c:1088`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_clear_c1(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_clear_c1(s, start)
}

/// `wrap_bitmap_clear_c1` as `rust_anything_wrap_bitmap_clear_c1`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_clear_c1(s: u64, start: i64) -> u64 {
    port::wrap_bitmap_clear_c1(s, start)
}

/// `uint64_t wrap_bitmap_set_cs(uint64_t s)` (`wrappers.c:1094`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_set_cs(s: u64) -> u64 {
    port::wrap_bitmap_set_cs(s)
}

/// `wrap_bitmap_set_cs` as `rust_anything_wrap_bitmap_set_cs`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_set_cs(s: u64) -> u64 {
    port::wrap_bitmap_set_cs(s)
}

/// `uint64_t wrap_bitmap_clear_cs(uint64_t s)` (`wrappers.c:1100`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_clear_cs(s: u64) -> u64 {
    port::wrap_bitmap_clear_cs(s)
}

/// `wrap_bitmap_clear_cs` as `rust_anything_wrap_bitmap_clear_cs`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_clear_cs(s: u64) -> u64 {
    port::wrap_bitmap_clear_cs(s)
}

/// `uint64_t wrap_bitmap_set_cm(uint64_t s)` (`wrappers.c:1106`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_set_cm(s: u64) -> u64 {
    port::wrap_bitmap_set_cm(s)
}

/// `wrap_bitmap_set_cm` as `rust_anything_wrap_bitmap_set_cm`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_set_cm(s: u64) -> u64 {
    port::wrap_bitmap_set_cm(s)
}

/// `uint64_t wrap_bitmap_clear_cm(uint64_t s)` (`wrappers.c:1112`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_clear_cm(s: u64) -> u64 {
    port::wrap_bitmap_clear_cm(s)
}

/// `wrap_bitmap_clear_cm` as `rust_anything_wrap_bitmap_clear_cm`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_clear_cm(s: u64) -> u64 {
    port::wrap_bitmap_clear_cm(s)
}

/// `uint64_t wrap_bitmap_shift_right_c(uint64_t s, uint64_t ds, int64_t shift)` (`wrappers.c:1118`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_shift_right_c(s: u64, ds: u64, shift: i64) -> u64 {
    port::wrap_bitmap_shift_right_c(s, ds, shift)
}

/// `wrap_bitmap_shift_right_c` as `rust_anything_wrap_bitmap_shift_right_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_shift_right_c(s: u64, ds: u64, shift: i64) -> u64 {
    port::wrap_bitmap_shift_right_c(s, ds, shift)
}

/// `uint64_t wrap_bitmap_shift_left_c(uint64_t s, uint64_t ds, int64_t shift)` (`wrappers.c:1124`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bitmap_shift_left_c(s: u64, ds: u64, shift: i64) -> u64 {
    port::wrap_bitmap_shift_left_c(s, ds, shift)
}

/// `wrap_bitmap_shift_left_c` as `rust_anything_wrap_bitmap_shift_left_c`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bitmap_shift_left_c(s: u64, ds: u64, shift: i64) -> u64 {
    port::wrap_bitmap_shift_left_c(s, ds, shift)
}

/// `uint64_t wrap_test_bit(uint64_t s, int64_t pos)` (`wrappers.c:1133`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_test_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_bit(s, pos)
}

/// `wrap_test_bit` as `rust_anything_wrap_test_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_test_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_bit(s, pos)
}

/// `uint64_t wrap_set_bit(uint64_t s, int64_t pos)` (`wrappers.c:1138`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_set_bit(s: u64, pos: i64) -> u64 {
    port::wrap_set_bit(s, pos)
}

/// `wrap_set_bit` as `rust_anything_wrap_set_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_set_bit(s: u64, pos: i64) -> u64 {
    port::wrap_set_bit(s, pos)
}

/// `uint64_t wrap_clear_bit(uint64_t s, int64_t pos)` (`wrappers.c:1144`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap_clear_bit(s, pos)
}

/// `wrap_clear_bit` as `rust_anything_wrap_clear_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap_clear_bit(s, pos)
}

/// `uint64_t wrap_change_bit(uint64_t s, int64_t pos)` (`wrappers.c:1150`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_change_bit(s: u64, pos: i64) -> u64 {
    port::wrap_change_bit(s, pos)
}

/// `wrap_change_bit` as `rust_anything_wrap_change_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_change_bit(s: u64, pos: i64) -> u64 {
    port::wrap_change_bit(s, pos)
}

/// `uint64_t wrap___set_bit(uint64_t s, int64_t pos)` (`wrappers.c:1156`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap___set_bit(s: u64, pos: i64) -> u64 {
    port::wrap___set_bit(s, pos)
}

/// `wrap___set_bit` as `rust_anything_wrap___set_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap___set_bit(s: u64, pos: i64) -> u64 {
    port::wrap___set_bit(s, pos)
}

/// `uint64_t wrap___clear_bit(uint64_t s, int64_t pos)` (`wrappers.c:1162`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap___clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap___clear_bit(s, pos)
}

/// `wrap___clear_bit` as `rust_anything_wrap___clear_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap___clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap___clear_bit(s, pos)
}

/// `uint64_t wrap___change_bit(uint64_t s, int64_t pos)` (`wrappers.c:1168`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap___change_bit(s: u64, pos: i64) -> u64 {
    port::wrap___change_bit(s, pos)
}

/// `wrap___change_bit` as `rust_anything_wrap___change_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap___change_bit(s: u64, pos: i64) -> u64 {
    port::wrap___change_bit(s, pos)
}

/// `uint64_t wrap___assign_bit(uint64_t s, int64_t pos, int64_t v)` (`wrappers.c:1174`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap___assign_bit(s: u64, pos: i64, v: i64) -> u64 {
    port::wrap___assign_bit(s, pos, v)
}

/// `wrap___assign_bit` as `rust_anything_wrap___assign_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap___assign_bit(s: u64, pos: i64, v: i64) -> u64 {
    port::wrap___assign_bit(s, pos, v)
}

/// `uint64_t wrap_test_and_set_bit(uint64_t s, int64_t pos)` (`wrappers.c:1180`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_test_and_set_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_set_bit(s, pos)
}

/// `wrap_test_and_set_bit` as `rust_anything_wrap_test_and_set_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_test_and_set_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_set_bit(s, pos)
}

/// `uint64_t wrap_test_and_clear_bit(uint64_t s, int64_t pos)` (`wrappers.c:1187`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_test_and_clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_clear_bit(s, pos)
}

/// `wrap_test_and_clear_bit` as `rust_anything_wrap_test_and_clear_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_test_and_clear_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_clear_bit(s, pos)
}

/// `uint64_t wrap_test_and_change_bit(uint64_t s, int64_t pos)` (`wrappers.c:1194`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_test_and_change_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_change_bit(s, pos)
}

/// `wrap_test_and_change_bit` as `rust_anything_wrap_test_and_change_bit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_test_and_change_bit(s: u64, pos: i64) -> u64 {
    port::wrap_test_and_change_bit(s, pos)
}

/// `uint64_t wrap_hweight_long(uint64_t w)` (`wrappers.c:1201`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hweight_long(w: u64) -> u64 {
    port::wrap_hweight_long(w)
}

/// `wrap_hweight_long` as `rust_anything_wrap_hweight_long`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hweight_long(w: u64) -> u64 {
    port::wrap_hweight_long(w)
}

/// `uint64_t wrap_fns(uint64_t w, int64_t n)` (`wrappers.c:1205`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_fns(w: u64, n: i64) -> u64 {
    port::wrap_fns(w, n)
}

/// `wrap_fns` as `rust_anything_wrap_fns`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_fns(w: u64, n: i64) -> u64 {
    port::wrap_fns(w, n)
}

/// `uint64_t wrap_ffs(int64_t v)` (`wrappers.c:1209`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_ffs(v: i64) -> u64 {
    port::wrap_ffs(v)
}

/// `wrap_ffs` as `rust_anything_wrap_ffs`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_ffs(v: i64) -> u64 {
    port::wrap_ffs(v)
}

/// `uint64_t wrap_fls(uint64_t v)` (`wrappers.c:1213`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_fls(v: u64) -> u64 {
    port::wrap_fls(v)
}

/// `wrap_fls` as `rust_anything_wrap_fls`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_fls(v: u64) -> u64 {
    port::wrap_fls(v)
}
