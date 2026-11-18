#pragma once
/* Simple min/max shims: upstream does strict type checking, the
 * semantics exercised by this unit are the plain comparisons. */
#define min(x, y)	((x) < (y) ? (x) : (y))
#define max(x, y)	((x) > (y) ? (x) : (y))
#define min_t(type, x, y) ((type)(x) < (type)(y) ? (type)(x) : (type)(y))
#define max_t(type, x, y) ((type)(x) > (type)(y) ? (type)(x) : (type)(y))
#define swap(a, b)	do { typeof(a) __tmp = (a); (a) = (b); (b) = __tmp; } while (0)
