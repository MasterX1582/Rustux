#pragma once
#include <linux/types.h>
#include <limits.h>

#undef USHRT_MAX
#undef SHRT_MAX
#undef INT_MAX
#undef UINT_MAX
#undef LONG_MAX
#undef ULONG_MAX
#undef ULLONG_MAX
#define USHRT_MAX 0xffff
#define SHRT_MAX 0x7fff
#define INT_MAX 0x7fffffff
#define UINT_MAX 0xffffffffU
#define LONG_MAX 0x7fffffffffffffffL
#define ULONG_MAX 0xffffffffffffffffUL
#define ULLONG_MAX 0xffffffffffffffffULL
