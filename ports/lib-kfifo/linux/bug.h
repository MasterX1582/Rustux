#pragma once
/* BUG_ON: assert-like abort, matching the documented shim contract. */
#include <stdlib.h>
#include <linux/compiler.h>
#define BUG_ON(condition) do { if (unlikely(condition)) abort(); } while (0)
#define BUG() abort()
