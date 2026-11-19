#pragma once
/* linux/math.h shim: round_up/round_down/DIV_ROUND_UP and swap(), the
 * pieces generic-radix-tree.{c,h} uses. Semantics match upstream. */
#define __round_mask(x, y) ((__typeof__(x))((y)-1))
#define round_up(x, y) ((((x)-1) | __round_mask(x, y))+1)
#define round_down(x, y) ((x) & ~__round_mask(x, y))
#define DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))

#define swap(a, b) do { __typeof__(a) __tmp = (a); (a) = (b); (b) = __tmp; } while (0)
