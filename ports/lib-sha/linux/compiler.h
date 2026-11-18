#pragma once

#undef __always_inline
#define __always_inline inline __attribute__((__always_inline__))
#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define fallthrough __attribute__((__fallthrough__))
#define __aligned(x) __attribute__((__aligned__(x)))
#define __force
#define __user
#define __cold
#define __visible __attribute__((__visibility__("default")))
#define __maybe_unused __attribute__((__unused__))

/* sha256_finup_2x_sequential() is marked noinline_for_stack upstream:
 * an ordinary noinline in userspace. */
#define noinline_for_stack __attribute__((__noinline__))

/* Kernel meaning (with !__CHECKER__): `u8 out[at_least N]` is the C99
 * `u8 out[static N]` parameter bound. */
#define at_least static
