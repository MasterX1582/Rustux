/* Userspace shim for <linux/types.h>: fixed-width integer typedefs. */
#pragma once

/* The whole lib-math harness is LP64-only: the .c extracts had their
 * BITS_PER_LONG-dependent branches resolved for a 64-bit `unsigned long`
 * and the shims mirror LP64 kernel headers. Compiling narrower would
 * silently produce different (wrong) recordings. */
#if __LONG_WIDTH__ < 64 || !defined(__SIZEOF_INT128__)
#error "ports/lib-math harness is LP64-only: resolve the original units for narrower targets"
#endif

#include <stdint.h>
#include <stdbool.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int8_t s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;

#if defined(__SIZEOF_INT128__)
typedef __uint128_t u128;
#endif

/* Upstream types.h also carries this; prime_numbers_private.h needs it.
 * Two members as upstream ({next, func}) so sizeof/offsets match. */
struct rcu_head {
	void *next;
	void (*func)(struct rcu_head *head);
};

/* Kconfig options are all off in the harness; usable in `#if` too. */
#define IS_ENABLED(x) 0
