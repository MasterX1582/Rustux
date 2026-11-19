#pragma once
/* Minimal userspace types shim for the lib-xarray harness.
 * LP64 only: unsigned long is 64 bits (BITS_PER_LONG == 64 ==
 * XA_CHUNK_SIZE), matching the x86-64 reference build. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-xarray harness is LP64-only"
#endif
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int8_t s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;

typedef unsigned int gfp_t;

#define BITS_PER_LONG 64
#define __BITS_PER_LONG BITS_PER_LONG
#define CONFIG_64BIT 1
#define BITS_PER_BYTE 8
#define BITS_PER_TYPE(t) (sizeof(t) * BITS_PER_BYTE)
#define BITS_TO_LONGS(n) \
	(((n) + BITS_PER_LONG - 1) / BITS_PER_LONG)

/* Harness config: multi-index entries enabled (x86-64 defconfig has
 * CONFIG_XARRAY_MULTI; the non-multi build is a different unit), and
 * !CONFIG_BASE_SMALL so XA_CHUNK_SHIFT == 6 (64 slots per node). */
#define CONFIG_XARRAY_MULTI 1

#define __ARG_PLACEHOLDER_1 0,
#define __take_second_arg(__ignored, val, ...) val
#define __is_defined(x) ___is_defined(x)
#define ___is_defined(val) ____is_defined(__ARG_PLACEHOLDER_##val)
#define ____is_defined(arg1_or_junk) __take_second_arg(arg1_or_junk 1, 0)
#define IS_ENABLED(option) __is_defined(option)

#define IS_ALIGNED(x, a) (((x) & ((typeof(x))(a) - 1)) == 0)
