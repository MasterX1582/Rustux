#pragma once
/* Minimal userspace types shim for the lib-seqbuf harness.
 * LP64 only: the recorded reference behavior is the x86-64 build. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-seqbuf harness is LP64-only: pointer width differs on ILP32"
#endif
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>

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

/* seq_buf.h prototypes reference this VFS type; the port harness never
 * instantiates it (seq_buf_path is out of scope, needs d_path). */
struct path;

#define BITS_PER_LONG 64
