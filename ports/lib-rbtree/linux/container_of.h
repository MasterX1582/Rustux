#pragma once
/* include/linux/container_of.h semantics: recover the containing struct
 * from a member pointer. Upstream additionally type-checks the member;
 * the arithmetic is identical. */
#include <linux/stddef.h>

#define typeof_member(T, m)	typeof(((T *)0)->m)

#define container_of(ptr, type, member) \
	((type *)((char *)(ptr) - offsetof(type, member)))

#define container_of_const(ptr, type, member) \
	((type *)((char *)(ptr) - offsetof(type, member)))
