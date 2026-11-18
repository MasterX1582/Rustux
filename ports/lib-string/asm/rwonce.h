#pragma once
/* Single-threaded userspace harness: READ_ONCE/WRITE_ONCE are volatile
 * accesses, matching the kernel semantics for scalar types. */
#define READ_ONCE(x) (*(volatile __typeof__(&(x)) *)&(x))
#define WRITE_ONCE(x, v) (*(volatile __typeof__(&(x)) *)&(x) = (v))
