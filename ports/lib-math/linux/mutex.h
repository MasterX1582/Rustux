/* Userspace shim for <linux/mutex.h>: the harness is single-threaded, so
 * mutexes degrade to no-ops (the lockdep predicates still report held). */
#pragma once

struct mutex { int _unused; };

#define DEFINE_MUTEX(name) struct mutex name = { 0 }

static inline void mutex_lock(struct mutex *m) { (void)m; }
static inline void mutex_unlock(struct mutex *m) { (void)m; }
static inline int lockdep_is_held(struct mutex *m) { (void)m; return 1; }
