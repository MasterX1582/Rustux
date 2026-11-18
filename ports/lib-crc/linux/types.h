/* Userspace shim for <linux/types.h>: fixed-width integers + the function
 * pointer typedefs sort.h/list_sort.h consume. LP64-only like lib-math. */
#pragma once

#if __LONG_WIDTH__ < 64 || !defined(__SIZEOF_INT128__)
#error "ports/lib-core harness is LP64-only: resolve the original units for narrower targets"
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

/* verbatim from upstream <linux/types.h> */
typedef void (*swap_r_func_t)(void *a, void *b, int size, const void *priv);
typedef void (*swap_func_t)(void *a, void *b, int size);

typedef int (*cmp_r_func_t)(const void *a, const void *b, const void *priv);
typedef int (*cmp_func_t)(const void *a, const void *b);
