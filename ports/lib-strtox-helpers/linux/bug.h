#pragma once
#include <stdlib.h>
/* WARN_ON must still evaluate its argument; it returns it like the kernel. */
#define WARN_ON(cond)		(cond)
#define WARN_ON_ONCE(cond)	(cond)
#define WARN(cond, ...)		(cond)
#define BUG()			abort()
#define BUG_ON(cond)		do { if (cond) abort(); } while (0)
