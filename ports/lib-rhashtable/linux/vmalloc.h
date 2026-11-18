#pragma once
#include <linux/slab.h>

#define vmalloc(size)		kshim_alloc((size), 0)
#define vfree(p)		kshim_free(p)
