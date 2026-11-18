/* Userspace shim for <linux/bug.h>: kernel WARN_* log and continue; here they
 * evaluate the condition and return it (the warn path itself is a no-op). */
#pragma once

#define WARN(condition, ...) (!!(condition))
#define WARN_ON(x) (!!(x))
