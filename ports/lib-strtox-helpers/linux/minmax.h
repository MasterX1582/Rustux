#pragma once
/* Simple userspace min/max; harness operands are always same-typed. */
#define min(x, y)	((x) < (y) ? (x) : (y))
#define max(x, y)	((x) > (y) ? (x) : (y))
#define umin(x, y)	((x) < (y) ? (x) : (y))
