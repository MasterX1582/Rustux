#pragma once
#include <string.h>
/* Native-endian unaligned load, same as the generic kernel version. */
#define get_unaligned(ptr) ({					\
	__auto_type __p = (ptr);				\
	typeof(*__p) __v;					\
	memcpy(&__v, __p, sizeof(__v));				\
	__v; })
