#pragma once
/* Minimal cleanup.h: bitmap.h only needs DEFINE_FREE for
 * DEFINE_FREE(bitmap, unsigned long *, if (_T) bitmap_free(_T)).
 * Upstream's version is much richer; this is the semantic core. */
#include <linux/compiler.h>

#define DEFINE_FREE(_name, _type, _free) \
	static inline void __free_##_name(void *p) \
	{ _type _T = *(_type *)p; _free; }

#define __free(_name) __attribute__((__cleanup__(__free_##_name)))
