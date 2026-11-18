//! C ABI for `wrappers.c`: drop-in replacements with the original names and signatures, generated
//! by rust-anything from `rust-anything.json`. All of the crate's `unsafe` code lives in this file.

pub mod port;

/// `uint64_t sort_u32_sum(uint64_t seed)` (`wrappers.c:69`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_u32_sum(seed: u64) -> u64 {
    port::sort_u32_sum(seed)
}

/// `sort_u32_sum` as `rust_anything_sort_u32_sum`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_u32_sum(seed: u64) -> u64 {
    port::sort_u32_sum(seed)
}

/// `uint32_t sort_u32_elem(uint64_t seed, int64_t idx)` (`wrappers.c:77`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_u32_elem(seed: u64, idx: i64) -> u32 {
    port::sort_u32_elem(seed, idx)
}

/// `sort_u32_elem` as `rust_anything_sort_u32_elem`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_u32_elem(seed: u64, idx: i64) -> u32 {
    port::sort_u32_elem(seed, idx)
}

/// `uint64_t sort_r_u32_sum(uint64_t seed)` (`wrappers.c:85`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_r_u32_sum(seed: u64) -> u64 {
    port::sort_r_u32_sum(seed)
}

/// `sort_r_u32_sum` as `rust_anything_sort_r_u32_sum`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_r_u32_sum(seed: u64) -> u64 {
    port::sort_r_u32_sum(seed)
}

/// `uint64_t sort_u64_elems(uint64_t seed)` (`wrappers.c:101`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_u64_elems(seed: u64) -> u64 {
    port::sort_u64_elems(seed)
}

/// `sort_u64_elems` as `rust_anything_sort_u64_elems`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_u64_elems(seed: u64) -> u64 {
    port::sort_u64_elems(seed)
}

/// `uint64_t sort_u8_elems(uint64_t seed)` (`wrappers.c:123`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_u8_elems(seed: u64) -> u64 {
    port::sort_u8_elems(seed)
}

/// `sort_u8_elems` as `rust_anything_sort_u8_elems`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_u8_elems(seed: u64) -> u64 {
    port::sort_u8_elems(seed)
}

/// `uint64_t sort_u32_partial(uint64_t seed, int64_t n)` (`wrappers.c:140`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn sort_u32_partial(seed: u64, n: i64) -> u64 {
    port::sort_u32_partial(seed, n)
}

/// `sort_u32_partial` as `rust_anything_sort_u32_partial`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_sort_u32_partial(seed: u64, n: i64) -> u64 {
    port::sort_u32_partial(seed, n)
}

/// `uint64_t listsort_sum(uint64_t seed)` (`wrappers.c:196`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn listsort_sum(seed: u64) -> u64 {
    port::listsort_sum(seed)
}

/// `listsort_sum` as `rust_anything_listsort_sum`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_listsort_sum(seed: u64) -> u64 {
    port::listsort_sum(seed)
}

/// `uint64_t listsort_ties_sum(uint64_t seed)` (`wrappers.c:205`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn listsort_ties_sum(seed: u64) -> u64 {
    port::listsort_ties_sum(seed)
}

/// `listsort_ties_sum` as `rust_anything_listsort_ties_sum`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_listsort_ties_sum(seed: u64) -> u64 {
    port::listsort_ties_sum(seed)
}

/// `uint32_t listsort_elem(uint64_t seed, int64_t idx)` (`wrappers.c:234`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn listsort_elem(seed: u64, idx: i64) -> u32 {
    port::listsort_elem(seed, idx)
}

/// `listsort_elem` as `rust_anything_listsort_elem`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_listsort_elem(seed: u64, idx: i64) -> u32 {
    port::listsort_elem(seed, idx)
}

/// `uint64_t listsort_small(uint64_t seed, int64_t n)` (`wrappers.c:214`).
#[cfg(not(feature = "shadow"))]
// SAFETY: exported under the C name it replaces: link it instead of the original object.
#[unsafe(no_mangle)]
pub extern "C" fn listsort_small(seed: u64, n: i64) -> u64 {
    port::listsort_small(seed, n)
}

/// `listsort_small` as `rust_anything_listsort_small`, for shadow builds beside the original.
// SAFETY: a name no C code defines.
#[unsafe(no_mangle)]
pub extern "C" fn rust_anything_listsort_small(seed: u64, n: i64) -> u64 {
    port::listsort_small(seed, n)
}
