#pragma once
/* include/vdso/limits.h values, defined directly: the record build adds
 * -I linux, so a <limits.h> passthrough would resolve right back to
 * this file (same trap as linux/errno.h in the other ports). */
#define USHRT_MAX	((unsigned short)~0U)
#define SHRT_MAX	((short)(USHRT_MAX >> 1))
#define SHRT_MIN	((short)(-SHRT_MAX - 1))
#define INT_MAX		((int)(~0U >> 1))
#define INT_MIN		(-INT_MAX - 1)
#define UINT_MAX	(~0U)
#define LONG_MAX	((long)(~0UL >> 1))
#define LONG_MIN	(-LONG_MAX - 1)
#define ULONG_MAX	(~0UL)
#define LLONG_MAX	((long long)(~0ULL >> 1))
#define LLONG_MIN	(-LLONG_MAX - 1)
#define ULLONG_MAX	(~0ULL)
/* UINTPTR_MAX intentionally omitted: glibc's <stdint.h> (pulled in via
 * types.h) already defines it, and nothing here needs it. */
