#pragma once
/* Kernel <linux/limits.h> values (LP64), defined literally so the harness
 * does not depend on how the record build wires system include paths. */
#define INT_MAX		2147483647
#define INT_MIN		(-2147483647 - 1)
#define UINT_MAX	4294967295U
#define LONG_MAX	9223372036854775807L
#define LONG_MIN	(-9223372036854775807L - 1)
#define ULONG_MAX	18446744073709551615UL
#define LLONG_MAX	9223372036854775807LL
#define LLONG_MIN	(-9223372036854775807LL - 1)
#define ULLONG_MAX	18446744073709551615ULL
#ifndef PATH_MAX
#define PATH_MAX	4096
#endif
