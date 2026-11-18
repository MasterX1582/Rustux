#pragma once

/* Upstream check_add_overflow() compiles to the builtin on the types the
 * sha512 unit uses (u64 += size_t). */
#define check_add_overflow(a, b, d) __builtin_add_overflow(a, b, d)
