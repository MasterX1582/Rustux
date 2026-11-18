#pragma once
/* Minimal userspace types shim for the lib-bitmap harness.
 * LP64 only: the recorded reference behavior is the x86-64 build. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-bitmap harness is LP64-only: unsigned long width and the \
BITS_PER_LONG == 64 code paths in bitmap.h/find.h differ on ILP32"
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

typedef u16 __le16;
typedef u32 __le32;
typedef u64 __le64;
typedef u16 __be16;
typedef u32 __be32;
typedef u64 __be64;

/* gfp_t and loff_t live in linux/types.h upstream; bitmap.h's alloc
 * prototypes and bitmap-str.h's print prototypes need them. */
typedef unsigned int gfp_t;
typedef s64 loff_t;

/* Harness config decisions, matching the x86-64 reference build:
 * little-endian (find.h's _le finders delegate to the plain ones and
 * find_bit.c's __BIG_ENDIAN section drops out) and CONFIG_NUMA=y
 * (x86_64 defconfig; compiles in bitmap_onto/bitmap_fold). */
#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define __LITTLE_ENDIAN 1234
#else
#error "lib-bitmap harness is little-endian only"
#endif
#define CONFIG_NUMA 1

#define BITS_PER_LONG 64
#define __BITS_PER_LONG BITS_PER_LONG
#define BITS_PER_BYTE 8

#define __ARG_PLACEHOLDER_1 0,
#define __take_second_arg(__ignored, val, ...) val
#define __is_defined(x) ___is_defined(x)
#define ___is_defined(val) ____is_defined(__ARG_PLACEHOLDER_##val)
#define ____is_defined(arg1_or_junk) __take_second_arg(arg1_or_junk 1, 0)
#define IS_ENABLED(option) __is_defined(option)

#define IS_ALIGNED(x, a) (((x) & ((typeof(x))(a) - 1)) == 0)
