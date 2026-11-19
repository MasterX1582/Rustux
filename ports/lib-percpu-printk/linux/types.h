#pragma once
/* Minimal userspace types shim for the lib-percpu-printk harness.
 * LP64 only: unsigned long is 64 bits (BITS_PER_LONG == 64), matching
 * the x86-64 reference build — the printk ringbuffer packs a 2-bit
 * descriptor state into the top of state_var/lpos values. */
#if defined(__LP64__) || defined(_LP64)
/* ok */
#else
#error "lib-percpu-printk harness is LP64-only"
#endif
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <linux/limits.h>

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
#define BITS_PER_BYTE 8
#define BITS_PER_TYPE(t) (sizeof(t) * BITS_PER_BYTE)
#define BITS_TO_LONGS(n) \
	(((n) + BITS_PER_LONG - 1) / BITS_PER_LONG)

/* Harness config (x86-64 defconfig-shaped):
 *  - CONFIG_SMP: the SMP percpu_counter implementation (per-cpu buckets
 *    modelled as a flat array in linux/percpu.h).
 *  - CONFIG_64BIT: printk_ringbuffer.h's identity seq mapping.
 *  - !CONFIG_HOTPLUG_CPU: percpu_counters list compiles out; cpu
 *    hotplug stubs live in linux/cpu.h.
 *  - !CONFIG_DEBUG_OBJECTS_PERCPU_COUNTER: debugobjects stubbed.
 *  - !CONFIG_HAVE_CMPXCHG_LOCAL: the irq-save add_batch variant.
 *  - !CONFIG_PRINTK_EXECUTION_CTX, !CONFIG_KUNIT: shrink printk_info /
 *    no-export visibility, matching a typical !KUNIT build. */
#define CONFIG_SMP 1
#define CONFIG_64BIT 1

#define __ARG_PLACEHOLDER_1 0,
#define __take_second_arg(__ignored, val, ...) val
#define __is_defined(x) ___is_defined(x)
#define ___is_defined(val) ____is_defined(__ARG_PLACEHOLDER_##val)
#define ____is_defined(arg1_or_junk) __take_second_arg(arg1_or_junk 1, 0)
#define IS_ENABLED(option) __is_defined(option)

#define IS_ALIGNED(x, a) (((x) & ((typeof(x))(a) - 1)) == 0)
