#pragma once
/* Minimal userspace types shim for the lib-hash harness.
 * LP64 only: the recorded reference behavior is the x86-64 build. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-hash harness is LP64-only: unsigned long width and the \
CONFIG_64BIT code paths in siphash.h/jhash.h differ on ILP32"
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

#define BITS_PER_LONG 64

#define __ARG_PLACEHOLDER_1 0,
#define __take_second_arg(__ignored, val, ...) val
#define __is_defined(x) ___is_defined(x)
#define ___is_defined(val) ____is_defined(__ARG_PLACEHOLDER_##val)
#define ____is_defined(arg1_or_junk) __take_second_arg(arg1_or_junk 1, 0)
#define IS_ENABLED(option) __is_defined(option)

#define IS_ALIGNED(x, a) (((x) & ((typeof(x))(a) - 1)) == 0)
