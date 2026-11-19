#pragma once
/* Minimal userspace compiler shim for the lib-genradix-interval
 * harness. WRITE_ONCE/READ_ONCE degrade to plain volatile-less
 * accesses: the recorded reference is a single-threaded -O2 build where
 * they compile to exactly that. TYPEOF_UNQUAL matches upstream's
 * non-C23 fallback.
 *
 * Harness config decision: CONFIG_INTERVAL_TREE_SPAN_ITER=y compiles in
 * lib/interval_tree.c's span iterator, which the wrappers exercise. */

#undef __always_inline
#define __always_inline inline __attribute__((__always_inline__))
#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define fallthrough __attribute__((__fallthrough__))
#define __aligned(x) __attribute__((__aligned__(x)))
#define __force
#define __user
#define __cold
#define __pure __attribute__((__pure__))
#define __bitwise
#define __ro_after_init

#define WRITE_ONCE(x, val) ((x) = (val))
#define READ_ONCE(x) (x)

#define TYPEOF_UNQUAL(exp) __typeof__(exp)

#define CONFIG_INTERVAL_TREE_SPAN_ITER 1
