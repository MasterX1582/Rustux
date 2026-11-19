//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t wrap_kstrtoull(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:519`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoull(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoull(seed, base, mode)
}

/// `wrap_kstrtoull` as `rust_anything_wrap_kstrtoull`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoull(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoull(seed, base, mode)
}

/// `uint64_t wrap_kstrtoll(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:528`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoll(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoll(seed, base, mode)
}

/// `wrap_kstrtoll` as `rust_anything_wrap_kstrtoll`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoll(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoll(seed, base, mode)
}

/// `uint64_t wrap__kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:537`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap__kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap__kstrtoul(seed, base, mode)
}

/// `wrap__kstrtoul` as `rust_anything_wrap__kstrtoul`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap__kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap__kstrtoul(seed, base, mode)
}

/// `uint64_t wrap__kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:546`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap__kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap__kstrtol(seed, base, mode)
}

/// `wrap__kstrtol` as `rust_anything_wrap__kstrtol`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap__kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap__kstrtol(seed, base, mode)
}

/// `uint64_t wrap_kstrtouint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:555`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtouint(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtouint(seed, base, mode)
}

/// `wrap_kstrtouint` as `rust_anything_wrap_kstrtouint`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtouint(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtouint(seed, base, mode)
}

/// `uint64_t wrap_kstrtoint(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:564`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoint(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoint(seed, base, mode)
}

/// `wrap_kstrtoint` as `rust_anything_wrap_kstrtoint`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoint(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoint(seed, base, mode)
}

/// `uint64_t wrap_kstrtou16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:573`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou16(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou16(seed, base, mode)
}

/// `wrap_kstrtou16` as `rust_anything_wrap_kstrtou16`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou16(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou16(seed, base, mode)
}

/// `uint64_t wrap_kstrtos16(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:582`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos16(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos16(seed, base, mode)
}

/// `wrap_kstrtos16` as `rust_anything_wrap_kstrtos16`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos16(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos16(seed, base, mode)
}

/// `uint64_t wrap_kstrtou8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:591`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou8(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou8(seed, base, mode)
}

/// `wrap_kstrtou8` as `rust_anything_wrap_kstrtou8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou8(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou8(seed, base, mode)
}

/// `uint64_t wrap_kstrtos8(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:600`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos8(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos8(seed, base, mode)
}

/// `wrap_kstrtos8` as `rust_anything_wrap_kstrtos8`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos8(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos8(seed, base, mode)
}

/// `uint64_t wrap_kstrtou64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:609`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou64(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou64(seed, base, mode)
}

/// `wrap_kstrtou64` as `rust_anything_wrap_kstrtou64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou64(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou64(seed, base, mode)
}

/// `uint64_t wrap_kstrtos64(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:618`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos64(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos64(seed, base, mode)
}

/// `wrap_kstrtos64` as `rust_anything_wrap_kstrtos64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos64(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos64(seed, base, mode)
}

/// `uint64_t wrap_kstrtou32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:627`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou32(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou32(seed, base, mode)
}

/// `wrap_kstrtou32` as `rust_anything_wrap_kstrtou32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou32(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou32(seed, base, mode)
}

/// `uint64_t wrap_kstrtos32(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:636`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos32(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos32(seed, base, mode)
}

/// `wrap_kstrtos32` as `rust_anything_wrap_kstrtos32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos32(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos32(seed, base, mode)
}

/// `uint64_t wrap_kstrtoul(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:645`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoul(seed, base, mode)
}

/// `wrap_kstrtoul` as `rust_anything_wrap_kstrtoul`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoul(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoul(seed, base, mode)
}

/// `uint64_t wrap_kstrtol(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:654`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtol(seed, base, mode)
}

/// `wrap_kstrtol` as `rust_anything_wrap_kstrtol`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtol(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtol(seed, base, mode)
}

/// `uint64_t wrap_parse_integer_limit(uint64_t seed, int64_t base, int64_t maxc, int64_t init, int64_t mode)` (`wrappers.c:663`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_parse_integer_limit(
    seed: u64,
    base: i64,
    maxc: i64,
    init: i64,
    mode: i64,
) -> u64 {
    port::wrap_parse_integer_limit(seed, base, maxc, init, mode)
}

/// `wrap_parse_integer_limit` as `rust_anything_wrap_parse_integer_limit`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_parse_integer_limit(
    seed: u64,
    base: i64,
    maxc: i64,
    init: i64,
    mode: i64,
) -> u64 {
    port::wrap_parse_integer_limit(seed, base, maxc, init, mode)
}

/// `uint64_t wrap_parse_integer_fixup_radix(uint64_t seed, int64_t base, int64_t mode)` (`wrappers.c:675`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_parse_integer_fixup_radix(seed: u64, base: i64, mode: i64) -> u64 {
    port::wrap_parse_integer_fixup_radix(seed, base, mode)
}

/// `wrap_parse_integer_fixup_radix` as `rust_anything_wrap_parse_integer_fixup_radix`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_parse_integer_fixup_radix(
    seed: u64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_parse_integer_fixup_radix(seed, base, mode)
}

/// `uint64_t wrap_kstrtobool(uint64_t seed, int64_t mode)` (`wrappers.c:686`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtobool(seed: u64, mode: i64) -> u64 {
    port::wrap_kstrtobool(seed, mode)
}

/// `wrap_kstrtobool` as `rust_anything_wrap_kstrtobool`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtobool(seed: u64, mode: i64) -> u64 {
    port::wrap_kstrtobool(seed, mode)
}

/// `uint64_t wrap_kstrtoull_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:698`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoull_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoull_from_user(seed, count, base, mode)
}

/// `wrap_kstrtoull_from_user` as `rust_anything_wrap_kstrtoull_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoull_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtoull_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtoll_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:709`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoll_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoll_from_user(seed, count, base, mode)
}

/// `wrap_kstrtoll_from_user` as `rust_anything_wrap_kstrtoll_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoll_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtoll_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtoul_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:720`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoul_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoul_from_user(seed, count, base, mode)
}

/// `wrap_kstrtoul_from_user` as `rust_anything_wrap_kstrtoul_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoul_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtoul_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtol_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:731`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtol_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtol_from_user(seed, count, base, mode)
}

/// `wrap_kstrtol_from_user` as `rust_anything_wrap_kstrtol_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtol_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtol_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtouint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:742`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtouint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtouint_from_user(seed, count, base, mode)
}

/// `wrap_kstrtouint_from_user` as `rust_anything_wrap_kstrtouint_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtouint_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtouint_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtoint_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:753`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoint_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtoint_from_user(seed, count, base, mode)
}

/// `wrap_kstrtoint_from_user` as `rust_anything_wrap_kstrtoint_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoint_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtoint_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtou16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:764`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou16_from_user(seed, count, base, mode)
}

/// `wrap_kstrtou16_from_user` as `rust_anything_wrap_kstrtou16_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou16_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtou16_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtos16_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:775`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos16_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos16_from_user(seed, count, base, mode)
}

/// `wrap_kstrtos16_from_user` as `rust_anything_wrap_kstrtos16_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos16_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtos16_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtou8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:786`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtou8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtou8_from_user(seed, count, base, mode)
}

/// `wrap_kstrtou8_from_user` as `rust_anything_wrap_kstrtou8_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtou8_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtou8_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtos8_from_user(uint64_t seed, int64_t count, int64_t base, int64_t mode)` (`wrappers.c:797`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtos8_from_user(seed: u64, count: i64, base: i64, mode: i64) -> u64 {
    port::wrap_kstrtos8_from_user(seed, count, base, mode)
}

/// `wrap_kstrtos8_from_user` as `rust_anything_wrap_kstrtos8_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtos8_from_user(
    seed: u64,
    count: i64,
    base: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrtos8_from_user(seed, count, base, mode)
}

/// `uint64_t wrap_kstrtobool_from_user(uint64_t seed, int64_t count, int64_t mode)` (`wrappers.c:808`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtobool_from_user(seed: u64, count: i64, mode: i64) -> u64 {
    port::wrap_kstrtobool_from_user(seed, count, mode)
}

/// `wrap_kstrtobool_from_user` as `rust_anything_wrap_kstrtobool_from_user`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtobool_from_user(seed: u64, count: i64, mode: i64) -> u64 {
    port::wrap_kstrtobool_from_user(seed, count, mode)
}

/// `uint64_t wrap_kstrtoudec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:820`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtoudec64(seed: u64, scale: i64, mode: i64) -> u64 {
    port::wrap_kstrtoudec64(seed, scale, mode)
}

/// `wrap_kstrtoudec64` as `rust_anything_wrap_kstrtoudec64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtoudec64(seed: u64, scale: i64, mode: i64) -> u64 {
    port::wrap_kstrtoudec64(seed, scale, mode)
}

/// `uint64_t wrap_kstrtodec64(uint64_t seed, int64_t scale, int64_t mode)` (`wrappers.c:830`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrtodec64(seed: u64, scale: i64, mode: i64) -> u64 {
    port::wrap_kstrtodec64(seed, scale, mode)
}

/// `wrap_kstrtodec64` as `rust_anything_wrap_kstrtodec64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrtodec64(seed: u64, scale: i64, mode: i64) -> u64 {
    port::wrap_kstrtodec64(seed, scale, mode)
}

/// `int64_t wrap_hex_to_bin(int64_t c)` (`wrappers.c:843`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hex_to_bin(c: i64) -> i64 {
    port::wrap_hex_to_bin(c)
}

/// `wrap_hex_to_bin` as `rust_anything_wrap_hex_to_bin`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hex_to_bin(c: i64) -> i64 {
    port::wrap_hex_to_bin(c)
}

/// `uint64_t wrap_hex2bin(uint64_t seed, int64_t slen, int64_t cnt, int64_t mode)` (`wrappers.c:848`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hex2bin(seed: u64, slen: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_hex2bin(seed, slen, cnt, mode)
}

/// `wrap_hex2bin` as `rust_anything_wrap_hex2bin`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hex2bin(seed: u64, slen: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_hex2bin(seed, slen, cnt, mode)
}

/// `uint64_t wrap_bin2hex(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:866`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bin2hex(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_bin2hex(seed, slen, mode)
}

/// `wrap_bin2hex` as `rust_anything_wrap_bin2hex`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bin2hex(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_bin2hex(seed, slen, mode)
}

/// `uint64_t wrap_hex_dump_to_buffer(uint64_t seed, int64_t len, int64_t rowsize, int64_t groupsize, int64_t linebuflen, int64_t ascii)` (`wrappers.c:878`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_hex_dump_to_buffer(
    seed: u64,
    len: i64,
    rowsize: i64,
    groupsize: i64,
    linebuflen: i64,
    ascii: i64,
) -> u64 {
    port::wrap_hex_dump_to_buffer(seed, len, rowsize, groupsize, linebuflen, ascii)
}

/// `wrap_hex_dump_to_buffer` as `rust_anything_wrap_hex_dump_to_buffer`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_hex_dump_to_buffer(
    seed: u64,
    len: i64,
    rowsize: i64,
    groupsize: i64,
    linebuflen: i64,
    ascii: i64,
) -> u64 {
    port::wrap_hex_dump_to_buffer(seed, len, rowsize, groupsize, linebuflen, ascii)
}

/// `uint64_t wrap_print_hex_dump(uint64_t seed, int64_t ptype, int64_t rowsize, int64_t groupsize, int64_t len, int64_t ascii, int64_t mode)` (`wrappers.c:894`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_print_hex_dump(
    seed: u64,
    ptype: i64,
    rowsize: i64,
    groupsize: i64,
    len: i64,
    ascii: i64,
    mode: i64,
) -> u64 {
    port::wrap_print_hex_dump(seed, ptype, rowsize, groupsize, len, ascii, mode)
}

/// `wrap_print_hex_dump` as `rust_anything_wrap_print_hex_dump`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_print_hex_dump(
    seed: u64,
    ptype: i64,
    rowsize: i64,
    groupsize: i64,
    len: i64,
    ascii: i64,
    mode: i64,
) -> u64 {
    port::wrap_print_hex_dump(seed, ptype, rowsize, groupsize, len, ascii, mode)
}

/// `uint64_t wrap_string_get_size(int64_t size, int64_t blk, int64_t units, int64_t len)` (`wrappers.c:914`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_get_size(size: i64, blk: i64, units: i64, len: i64) -> u64 {
    port::wrap_string_get_size(size, blk, units, len)
}

/// `wrap_string_get_size` as `rust_anything_wrap_string_get_size`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_get_size(
    size: i64,
    blk: i64,
    units: i64,
    len: i64,
) -> u64 {
    port::wrap_string_get_size(size, blk, units, len)
}

/// `uint64_t wrap_string_escape_mem(uint64_t seed, int64_t isz, int64_t osz, int64_t flags, int64_t only)` (`wrappers.c:928`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_escape_mem(
    seed: u64,
    isz: i64,
    osz: i64,
    flags: i64,
    only: i64,
) -> u64 {
    port::wrap_string_escape_mem(seed, isz, osz, flags, only)
}

/// `wrap_string_escape_mem` as `rust_anything_wrap_string_escape_mem`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_escape_mem(
    seed: u64,
    isz: i64,
    osz: i64,
    flags: i64,
    only: i64,
) -> u64 {
    port::wrap_string_escape_mem(seed, isz, osz, flags, only)
}

/// `uint64_t wrap_string_escape_str(uint64_t seed, int64_t slen, int64_t osz, int64_t flags, int64_t only, int64_t mode)` (`wrappers.c:943`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_escape_str(
    seed: u64,
    slen: i64,
    osz: i64,
    flags: i64,
    only: i64,
    mode: i64,
) -> u64 {
    port::wrap_string_escape_str(seed, slen, osz, flags, only, mode)
}

/// `wrap_string_escape_str` as `rust_anything_wrap_string_escape_str`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_escape_str(
    seed: u64,
    slen: i64,
    osz: i64,
    flags: i64,
    only: i64,
    mode: i64,
) -> u64 {
    port::wrap_string_escape_str(seed, slen, osz, flags, only, mode)
}

/// `uint64_t wrap_string_unescape(uint64_t seed, int64_t slen, int64_t size, int64_t flags, int64_t mode)` (`wrappers.c:958`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_unescape(
    seed: u64,
    slen: i64,
    size: i64,
    flags: i64,
    mode: i64,
) -> u64 {
    port::wrap_string_unescape(seed, slen, size, flags, mode)
}

/// `wrap_string_unescape` as `rust_anything_wrap_string_unescape`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_unescape(
    seed: u64,
    slen: i64,
    size: i64,
    flags: i64,
    mode: i64,
) -> u64 {
    port::wrap_string_unescape(seed, slen, size, flags, mode)
}

/// `uint64_t wrap_skip_spaces(uint64_t seed, int64_t mode)` (`wrappers.c:979`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_skip_spaces(seed: u64, mode: i64) -> u64 {
    port::wrap_skip_spaces(seed, mode)
}

/// `wrap_skip_spaces` as `rust_anything_wrap_skip_spaces`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_skip_spaces(seed: u64, mode: i64) -> u64 {
    port::wrap_skip_spaces(seed, mode)
}

/// `uint64_t wrap_strim(uint64_t seed, int64_t mode)` (`wrappers.c:993`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strim(seed: u64, mode: i64) -> u64 {
    port::wrap_strim(seed, mode)
}

/// `wrap_strim` as `rust_anything_wrap_strim`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strim(seed: u64, mode: i64) -> u64 {
    port::wrap_strim(seed, mode)
}

/// `uint64_t wrap_sysfs_streq(uint64_t seed, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:1010`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sysfs_streq(seed: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_sysfs_streq(seed, la, lb, mode)
}

/// `wrap_sysfs_streq` as `rust_anything_wrap_sysfs_streq`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sysfs_streq(seed: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_sysfs_streq(seed, la, lb, mode)
}

/// `uint64_t wrap_strreplace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1037`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strreplace(seed: u64, slen: i64, old: i64, new: i64, mode: i64) -> u64 {
    port::wrap_strreplace(seed, slen, old, new, mode)
}

/// `wrap_strreplace` as `rust_anything_wrap_strreplace`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strreplace(
    seed: u64,
    slen: i64,
    old: i64,
    new: i64,
    mode: i64,
) -> u64 {
    port::wrap_strreplace(seed, slen, old, new, mode)
}

/// `uint64_t wrap_memcpy_and_pad(uint64_t seed, int64_t dest_len, int64_t count, int64_t pad)` (`wrappers.c:1054`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memcpy_and_pad(seed: u64, dest_len: i64, count: i64, pad: i64) -> u64 {
    port::wrap_memcpy_and_pad(seed, dest_len, count, pad)
}

/// `wrap_memcpy_and_pad` as `rust_anything_wrap_memcpy_and_pad`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memcpy_and_pad(
    seed: u64,
    dest_len: i64,
    count: i64,
    pad: i64,
) -> u64 {
    port::wrap_memcpy_and_pad(seed, dest_len, count, pad)
}

/// `uint64_t wrap_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1090`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    port::wrap_match_string(seed, n, strsel, mode)
}

/// `wrap_match_string` as `rust_anything_wrap_match_string`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_match_string(
    seed: u64,
    n: i64,
    strsel: i64,
    mode: i64,
) -> u64 {
    port::wrap_match_string(seed, n, strsel, mode)
}

/// `uint64_t wrap_sysfs_match_string(uint64_t seed, int64_t n, int64_t strsel, int64_t mode)` (`wrappers.c:1111`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sysfs_match_string(seed: u64, n: i64, strsel: i64, mode: i64) -> u64 {
    port::wrap_sysfs_match_string(seed, n, strsel, mode)
}

/// `wrap_sysfs_match_string` as `rust_anything_wrap_sysfs_match_string`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sysfs_match_string(
    seed: u64,
    n: i64,
    strsel: i64,
    mode: i64,
) -> u64 {
    port::wrap_sysfs_match_string(seed, n, strsel, mode)
}

/// `uint64_t wrap_string_is_terminated(uint64_t seed, int64_t slen, int64_t len, int64_t mode)` (`wrappers.c:1145`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_is_terminated(seed: u64, slen: i64, len: i64, mode: i64) -> u64 {
    port::wrap_string_is_terminated(seed, slen, len, mode)
}

/// `wrap_string_is_terminated` as `rust_anything_wrap_string_is_terminated`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_is_terminated(
    seed: u64,
    slen: i64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_string_is_terminated(seed, slen, len, mode)
}

/// `uint64_t wrap_string_upper(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1156`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_upper(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_string_upper(seed, slen, mode)
}

/// `wrap_string_upper` as `rust_anything_wrap_string_upper`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_upper(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_string_upper(seed, slen, mode)
}

/// `uint64_t wrap_string_lower(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1166`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_string_lower(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_string_lower(seed, slen, mode)
}

/// `wrap_string_lower` as `rust_anything_wrap_string_lower`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_string_lower(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_string_lower(seed, slen, mode)
}

/// `uint64_t wrap_kstrdup_quotable(uint64_t seed, int64_t slen, int64_t mode)` (`wrappers.c:1176`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrdup_quotable(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_kstrdup_quotable(seed, slen, mode)
}

/// `wrap_kstrdup_quotable` as `rust_anything_wrap_kstrdup_quotable`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrdup_quotable(seed: u64, slen: i64, mode: i64) -> u64 {
    port::wrap_kstrdup_quotable(seed, slen, mode)
}

/// `uint64_t wrap_kstrdup_and_replace(uint64_t seed, int64_t slen, int64_t old, int64_t new, int64_t mode)` (`wrappers.c:1189`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_kstrdup_and_replace(
    seed: u64,
    slen: i64,
    old: i64,
    new: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrdup_and_replace(seed, slen, old, new, mode)
}

/// `wrap_kstrdup_and_replace` as `rust_anything_wrap_kstrdup_and_replace`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_kstrdup_and_replace(
    seed: u64,
    slen: i64,
    old: i64,
    new: i64,
    mode: i64,
) -> u64 {
    port::wrap_kstrdup_and_replace(seed, slen, old, new, mode)
}
