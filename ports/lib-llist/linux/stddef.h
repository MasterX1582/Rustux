/* This shim dir is on the -I path as `linux/`, so <stddef.h> resolves
 * here.  Pure pass-through, NO header guard of our own: glibc headers
 * re-include <stddef.h> with __need_* selectors for selective types and
 * a pragma-once wrapper would swallow those.  glibc's NULL/offsetof are
 * already the kernel's semantics ((void*)0 / __builtin_offsetof). */
#include_next <stddef.h>
