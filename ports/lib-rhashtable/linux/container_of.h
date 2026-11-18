#pragma once
/* container_of, identical arithmetic to upstream (without the
 * type-checking assert that needs kernel internals). */
#include <stddef.h>

#define container_of(ptr, type, member) ({			\
	const typeof(((type *)0)->member) *__mptr = (ptr);	\
	(type *)((char *)__mptr - offsetof(type, member)); })
