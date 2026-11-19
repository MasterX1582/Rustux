#pragma once
#include <linux/stddef.h>

#define typeof_member(T, m) typeof(((T *)0)->m)

#define container_of(ptr, type, member) \
	((type *)((char *)(ptr) - offsetof(type, member)))

#define container_of_const(ptr, type, member) \
	((type *)((char *)(ptr) - offsetof(type, member)))
