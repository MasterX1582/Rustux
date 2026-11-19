#pragma once
/* Minimal userspace compiler shim for the lib-xarray harness.
 * Annotations degrade to nothing; READ_ONCE/WRITE_ONCE are the plain
 * accesses a single-threaded -O1 build compiles them to. */

#undef __always_inline
#define __always_inline inline __attribute__((__always_inline__))
#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define fallthrough __attribute__((__fallthrough__))
#define __aligned(x) __attribute__((__aligned__(x)))
#define __force
#define __user
#define __rcu
#define __bitwise
#define __cold
#define __pure __attribute__((__pure__))
#define __ro_after_init
#define __must_check __attribute__((warn_unused_result))
#define __must_hold(lock)
#define __acquires(lock)
#define __releases(lock)
#define noinline
#define __init
#undef __nonnull
#define __nonnull(n)
#define __maybe_unused __attribute__((unused))
#define __read_mostly
#define __printf(f, a)
#define __aligned_u64 __attribute__((aligned(8)))

#define WRITE_ONCE(x, val) ((x) = (val))
#define READ_ONCE(x) (x)
#define data_race(x) (x)

#define TYPEOF_UNQUAL(exp) __typeof__(exp)
#define __same_type(a, b) __builtin_types_compatible_p(__typeof__(a), __typeof__(b))
