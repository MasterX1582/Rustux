#pragma once
/* linux/stddef.h: the record build adds -I linux, so a <stddef.h>
 * passthrough would resolve back to this file — define the needed
 * pieces directly, same as upstream does via uapi/linux/stddef.h.
 * wchar_t too: glibc's <stdlib.h> family declares functions on it. */
#include <stdbool.h>

#undef NULL
#define NULL ((void *)0)

#undef offsetof
#define offsetof(TYPE, MEMBER)	__builtin_offsetof(TYPE, MEMBER)

typedef __SIZE_TYPE__ size_t;
typedef __PTRDIFF_TYPE__ ptrdiff_t;
typedef __WCHAR_TYPE__ wchar_t;
