#pragma once
/* Userspace compiler-attr shim for the lib-strtox-helpers harness. */
#define noinline		__attribute__((noinline))
#define __must_check		__attribute__((warn_unused_result))
#define likely(x)		__builtin_expect(!!(x), 1)
#define unlikely(x)		__builtin_expect(!!(x), 0)
#define __user
#define __aligned(x)		__attribute__((aligned(x)))
#define __printf(a, b)		__attribute__((format(printf, a, b)))
#define __counted_by_ptr(x)
/* Token-identical to glibc's <sys/cdefs.h> definition so fortified libc
 * headers can re-#define it without a conflict. */
#define __always_inline		__inline __attribute__((__always_inline__))
