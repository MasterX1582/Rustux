#pragma once
/* Minimal userspace types shim for the lib-llist harness.
 * LP64 only: the recorded reference behavior is the x86-64 build. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-llist harness is LP64-only: pointer width differs on ILP32"
#endif
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <sys/types.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int8_t s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;

#define BITS_PER_LONG 64
