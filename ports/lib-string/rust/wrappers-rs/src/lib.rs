//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `int64_t wrap_strncasecmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cnt, int64_t mode)` (`wrappers.c:173`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strncasecmp(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strncasecmp(sa, sb, la, lb, cnt, mode)
}

/// `wrap_strncasecmp` as `rust_anything_wrap_strncasecmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strncasecmp(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cnt: i64,
    mode: i64,
) -> i64 {
    port::wrap_strncasecmp(sa, sb, la, lb, cnt, mode)
}

/// `int64_t wrap_strcasecmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:180`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strcasecmp(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strcasecmp(sa, sb, la, lb, mode)
}

/// `wrap_strcasecmp` as `rust_anything_wrap_strcasecmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strcasecmp(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    mode: i64,
) -> i64 {
    port::wrap_strcasecmp(sa, sb, la, lb, mode)
}

/// `uint64_t wrap_strcpy(uint64_t sa, int64_t la, int64_t mode)` (`wrappers.c:189`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strcpy(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_strcpy(sa, la, mode)
}

/// `wrap_strcpy` as `rust_anything_wrap_strcpy`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strcpy(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_strcpy(sa, la, mode)
}

/// `uint64_t wrap_sized_strscpy(uint64_t sa, int64_t la, int64_t cnt, int64_t mode)` (`wrappers.c:198`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_sized_strscpy(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_sized_strscpy(sa, la, cnt, mode)
}

/// `wrap_sized_strscpy` as `rust_anything_wrap_sized_strscpy`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_sized_strscpy(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_sized_strscpy(sa, la, cnt, mode)
}

/// `uint64_t wrap_stpcpy(uint64_t sa, int64_t la, int64_t mode)` (`wrappers.c:211`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_stpcpy(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_stpcpy(sa, la, mode)
}

/// `wrap_stpcpy` as `rust_anything_wrap_stpcpy`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_stpcpy(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_stpcpy(sa, la, mode)
}

/// `uint64_t wrap_strcat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:221`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strcat(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strcat(sa, sb, la, lb, mode)
}

/// `wrap_strcat` as `rust_anything_wrap_strcat`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strcat(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strcat(sa, sb, la, lb, mode)
}

/// `uint64_t wrap_strncat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cnt, int64_t mode)` (`wrappers.c:230`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strncat(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_strncat(sa, sb, la, lb, cnt, mode)
}

/// `wrap_strncat` as `rust_anything_wrap_strncat`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strncat(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cnt: i64,
    mode: i64,
) -> u64 {
    port::wrap_strncat(sa, sb, la, lb, cnt, mode)
}

/// `uint64_t wrap_strlcat(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cnt, int64_t mode)` (`wrappers.c:240`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strlcat(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_strlcat(sa, sb, la, lb, cnt, mode)
}

/// `wrap_strlcat` as `rust_anything_wrap_strlcat`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strlcat(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cnt: i64,
    mode: i64,
) -> u64 {
    port::wrap_strlcat(sa, sb, la, lb, cnt, mode)
}

/// `int64_t wrap_strcmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:256`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strcmp(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strcmp(sa, sb, la, lb, mode)
}

/// `wrap_strcmp` as `rust_anything_wrap_strcmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strcmp(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strcmp(sa, sb, la, lb, mode)
}

/// `int64_t wrap_strncmp(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cnt, int64_t mode)` (`wrappers.c:263`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strncmp(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strncmp(sa, sb, la, lb, cnt, mode)
}

/// `wrap_strncmp` as `rust_anything_wrap_strncmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strncmp(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cnt: i64,
    mode: i64,
) -> i64 {
    port::wrap_strncmp(sa, sb, la, lb, cnt, mode)
}

/// `int64_t wrap_strchr(uint64_t sa, int64_t la, int64_t c, int64_t mode)` (`wrappers.c:272`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strchr(sa, la, c, mode)
}

/// `wrap_strchr` as `rust_anything_wrap_strchr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strchr(sa, la, c, mode)
}

/// `int64_t wrap_strchrnul(uint64_t sa, int64_t la, int64_t c, int64_t mode)` (`wrappers.c:280`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strchrnul(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strchrnul(sa, la, c, mode)
}

/// `wrap_strchrnul` as `rust_anything_wrap_strchrnul`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strchrnul(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strchrnul(sa, la, c, mode)
}

/// `int64_t wrap_strnchrnul(uint64_t sa, int64_t la, int64_t c, int64_t cnt, int64_t mode)` (`wrappers.c:286`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strnchrnul(sa: u64, la: i64, c: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strnchrnul(sa, la, c, cnt, mode)
}

/// `wrap_strnchrnul` as `rust_anything_wrap_strnchrnul`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strnchrnul(
    sa: u64,
    la: i64,
    c: i64,
    cnt: i64,
    mode: i64,
) -> i64 {
    port::wrap_strnchrnul(sa, la, c, cnt, mode)
}

/// `int64_t wrap_strrchr(uint64_t sa, int64_t la, int64_t c, int64_t mode)` (`wrappers.c:294`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strrchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strrchr(sa, la, c, mode)
}

/// `wrap_strrchr` as `rust_anything_wrap_strrchr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strrchr(sa: u64, la: i64, c: i64, mode: i64) -> i64 {
    port::wrap_strrchr(sa, la, c, mode)
}

/// `int64_t wrap_strnchr(uint64_t sa, int64_t la, int64_t c, int64_t cnt, int64_t mode)` (`wrappers.c:302`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strnchr(sa: u64, la: i64, c: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strnchr(sa, la, c, cnt, mode)
}

/// `wrap_strnchr` as `rust_anything_wrap_strnchr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strnchr(sa: u64, la: i64, c: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strnchr(sa, la, c, cnt, mode)
}

/// `uint64_t wrap_strlen(uint64_t sa, int64_t la, int64_t mode)` (`wrappers.c:313`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strlen(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_strlen(sa, la, mode)
}

/// `wrap_strlen` as `rust_anything_wrap_strlen`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strlen(sa: u64, la: i64, mode: i64) -> u64 {
    port::wrap_strlen(sa, la, mode)
}

/// `uint64_t wrap_strnlen(uint64_t sa, int64_t la, int64_t cnt, int64_t mode)` (`wrappers.c:319`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strnlen(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_strnlen(sa, la, cnt, mode)
}

/// `wrap_strnlen` as `rust_anything_wrap_strnlen`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strnlen(sa: u64, la: i64, cnt: i64, mode: i64) -> u64 {
    port::wrap_strnlen(sa, la, cnt, mode)
}

/// `uint64_t wrap_strspn(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:327`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strspn(sa, sb, la, lb, mode)
}

/// `wrap_strspn` as `rust_anything_wrap_strspn`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strspn(sa, sb, la, lb, mode)
}

/// `uint64_t wrap_strcspn(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:334`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strcspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strcspn(sa, sb, la, lb, mode)
}

/// `wrap_strcspn` as `rust_anything_wrap_strcspn`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strcspn(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> u64 {
    port::wrap_strcspn(sa, sb, la, lb, mode)
}

/// `int64_t wrap_strpbrk(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:341`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strpbrk(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strpbrk(sa, sb, la, lb, mode)
}

/// `wrap_strpbrk` as `rust_anything_wrap_strpbrk`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strpbrk(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strpbrk(sa, sb, la, lb, mode)
}

/// `uint64_t wrap_strsep(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cur, int64_t mode)` (`wrappers.c:354`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strsep(sa: u64, sb: u64, la: i64, lb: i64, cur: i64, mode: i64) -> u64 {
    port::wrap_strsep(sa, sb, la, lb, cur, mode)
}

/// `wrap_strsep` as `rust_anything_wrap_strsep`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strsep(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cur: i64,
    mode: i64,
) -> u64 {
    port::wrap_strsep(sa, sb, la, lb, cur, mode)
}

/// `uint64_t wrap_memset(uint64_t sb, int64_t c, int64_t len, int64_t off, int64_t mode)` (`wrappers.c:379`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memset(sb: u64, c: i64, len: i64, off: i64, mode: i64) -> u64 {
    port::wrap_memset(sb, c, len, off, mode)
}

/// `wrap_memset` as `rust_anything_wrap_memset`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memset(sb: u64, c: i64, len: i64, off: i64, mode: i64) -> u64 {
    port::wrap_memset(sb, c, len, off, mode)
}

/// `uint64_t wrap_memset16(uint64_t sb, int64_t v, int64_t len, int64_t off, int64_t mode)` (`wrappers.c:395`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memset16(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    port::wrap_memset16(sb, v, len, off, mode)
}

/// `wrap_memset16` as `rust_anything_wrap_memset16`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memset16(
    sb: u64,
    v: i64,
    len: i64,
    off: i64,
    mode: i64,
) -> u64 {
    port::wrap_memset16(sb, v, len, off, mode)
}

/// `uint64_t wrap_memset32(uint64_t sb, int64_t v, int64_t len, int64_t off, int64_t mode)` (`wrappers.c:411`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memset32(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    port::wrap_memset32(sb, v, len, off, mode)
}

/// `wrap_memset32` as `rust_anything_wrap_memset32`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memset32(
    sb: u64,
    v: i64,
    len: i64,
    off: i64,
    mode: i64,
) -> u64 {
    port::wrap_memset32(sb, v, len, off, mode)
}

/// `uint64_t wrap_memset64(uint64_t sb, int64_t v, int64_t len, int64_t off, int64_t mode)` (`wrappers.c:427`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memset64(sb: u64, v: i64, len: i64, off: i64, mode: i64) -> u64 {
    port::wrap_memset64(sb, v, len, off, mode)
}

/// `wrap_memset64` as `rust_anything_wrap_memset64`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memset64(
    sb: u64,
    v: i64,
    len: i64,
    off: i64,
    mode: i64,
) -> u64 {
    port::wrap_memset64(sb, v, len, off, mode)
}

/// `uint64_t wrap_memcpy(uint64_t sa, uint64_t sb, int64_t doff, int64_t soff, int64_t len, int64_t mode)` (`wrappers.c:442`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memcpy(sa: u64, sb: u64, doff: i64, soff: i64, len: i64, mode: i64) -> u64 {
    port::wrap_memcpy(sa, sb, doff, soff, len, mode)
}

/// `wrap_memcpy` as `rust_anything_wrap_memcpy`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memcpy(
    sa: u64,
    sb: u64,
    doff: i64,
    soff: i64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_memcpy(sa, sb, doff, soff, len, mode)
}

/// `uint64_t wrap_memmove(uint64_t sb, int64_t doff, int64_t soff, int64_t len, int64_t mode)` (`wrappers.c:460`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memmove(sb: u64, doff: i64, soff: i64, len: i64, mode: i64) -> u64 {
    port::wrap_memmove(sb, doff, soff, len, mode)
}

/// `wrap_memmove` as `rust_anything_wrap_memmove`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memmove(
    sb: u64,
    doff: i64,
    soff: i64,
    len: i64,
    mode: i64,
) -> u64 {
    port::wrap_memmove(sb, doff, soff, len, mode)
}

/// `int64_t wrap_memcmp(uint64_t sa, uint64_t sb, int64_t len, int64_t mode)` (`wrappers.c:479`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    port::wrap_memcmp(sa, sb, len, mode)
}

/// `wrap_memcmp` as `rust_anything_wrap_memcmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    port::wrap_memcmp(sa, sb, len, mode)
}

/// `int64_t wrap_bcmp(uint64_t sa, uint64_t sb, int64_t len, int64_t mode)` (`wrappers.c:491`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_bcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    port::wrap_bcmp(sa, sb, len, mode)
}

/// `wrap_bcmp` as `rust_anything_wrap_bcmp`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_bcmp(sa: u64, sb: u64, len: i64, mode: i64) -> i64 {
    port::wrap_bcmp(sa, sb, len, mode)
}

/// `int64_t wrap_memscan(uint64_t sb, int64_t c, int64_t len, int64_t mode)` (`wrappers.c:503`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memscan(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memscan(sb, c, len, mode)
}

/// `wrap_memscan` as `rust_anything_wrap_memscan`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memscan(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memscan(sb, c, len, mode)
}

/// `int64_t wrap_memchr(uint64_t sb, int64_t c, int64_t len, int64_t mode)` (`wrappers.c:512`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memchr(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memchr(sb, c, len, mode)
}

/// `wrap_memchr` as `rust_anything_wrap_memchr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memchr(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memchr(sb, c, len, mode)
}

/// `int64_t wrap_memchr_inv(uint64_t sb, int64_t c, int64_t len, int64_t mode)` (`wrappers.c:524`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_memchr_inv(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memchr_inv(sb, c, len, mode)
}

/// `wrap_memchr_inv` as `rust_anything_wrap_memchr_inv`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_memchr_inv(sb: u64, c: i64, len: i64, mode: i64) -> i64 {
    port::wrap_memchr_inv(sb, c, len, mode)
}

/// `int64_t wrap_strstr(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t mode)` (`wrappers.c:552`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strstr(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strstr(sa, sb, la, lb, mode)
}

/// `wrap_strstr` as `rust_anything_wrap_strstr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strstr(sa: u64, sb: u64, la: i64, lb: i64, mode: i64) -> i64 {
    port::wrap_strstr(sa, sb, la, lb, mode)
}

/// `int64_t wrap_strnstr(uint64_t sa, uint64_t sb, int64_t la, int64_t lb, int64_t cnt, int64_t mode)` (`wrappers.c:567`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn wrap_strnstr(sa: u64, sb: u64, la: i64, lb: i64, cnt: i64, mode: i64) -> i64 {
    port::wrap_strnstr(sa, sb, la, lb, cnt, mode)
}

/// `wrap_strnstr` as `rust_anything_wrap_strnstr`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_wrap_strnstr(
    sa: u64,
    sb: u64,
    la: i64,
    lb: i64,
    cnt: i64,
    mode: i64,
) -> i64 {
    port::wrap_strnstr(sa, sb, la, lb, cnt, mode)
}
