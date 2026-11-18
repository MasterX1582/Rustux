/* Userspace shim for <asm/bug.h>: kernel WARN_* log and continue; here they
 * are no-ops returning false, matching the non-warning execution path. */
#pragma once

#define WARN_ON(x) 0
#define BUG() do {} while (0)
#define BUG_ON(x) do {} while (0)
