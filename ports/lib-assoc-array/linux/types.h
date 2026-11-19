#pragma once
/* linux/types.h shim for the lib-assoc-array harness: fixed-width
 * kernel typedefs over the userspace headers.  64-bit unsigned long
 * (x86_64) is assumed throughout: ASSOC_ARRAY_KEY_CHUNK_SIZE ==
 * BITS_PER_LONG == 64. */
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <limits.h>

typedef uint8_t		u8;
typedef uint16_t	u16;
typedef uint32_t	u32;
typedef uint64_t	u64;
typedef int8_t		s8;
typedef int16_t		s16;
typedef int32_t		s32;
typedef int64_t		s64;

#define BITS_PER_LONG	64
#define __rcu

/* assoc_array.c / assoc_array{,_priv}.h guard all their contents on this
 * Kconfig symbol; the unit is compiled with it always on.  Defined here
 * too so the byte-identical headers see it regardless of -D flags. */
#ifndef CONFIG_ASSOCIATIVE_ARRAY
#define CONFIG_ASSOCIATIVE_ARRAY 1
#endif
