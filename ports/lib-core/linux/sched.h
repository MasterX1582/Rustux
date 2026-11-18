/* Userspace shim for <linux/sched.h>: cond_resched is a no-op outside the
 * kernel's scheduler. */
#pragma once

static inline void cond_resched(void) {}
