#pragma once
/* linux/limits.h shim: upstream pulls SIZE_MAX etc. from the compiler's
 * limits headers; the harness does the same. */
#include <limits.h>
#include <stdint.h>

#ifndef SIZE_MAX
#define SIZE_MAX (~(size_t)0)
#endif
#ifndef ULONG_MAX
#define ULONG_MAX (~0UL)
#endif
