#pragma once
/* Minimal userspace compiler shim for the lib-seqbuf harness. */
#undef __always_inline
#define __always_inline inline __attribute__((__always_inline__))
#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define fallthrough __attribute__((__fallthrough__))
#define __aligned(x) __attribute__((__aligned__(x)))
#define __force
#define __user
#define __cold
#define __printf(a, b)
#define __must_check

#define WRITE_ONCE(x, val) ((x) = (val))
#define READ_ONCE(x) (x)
