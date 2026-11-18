/* Userspace shim for <asm/bug.h>: kernel WARN_* log and continue; here they
 * are no-ops returning false, matching the non-warning execution path. */
#pragma once

#define WARN_ON(x) (!!(x))
#define BUG() __builtin_trap()
#define BUG_ON(x) do { if (x) __builtin_trap(); } while (0)
