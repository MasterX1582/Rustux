#pragma once
#include <linux/types.h>
#define MAX_ERRNO	4095
#define IS_ERR_VALUE(x) ((unsigned long)(x) >= (unsigned long)-MAX_ERRNO)
static inline void *ERR_PTR(long e) { return (void *)e; }
static inline long PTR_ERR(const void *p) { return (long)p; }
static inline bool IS_ERR(const void *p)
{
	return IS_ERR_VALUE((unsigned long)p);
}
