#pragma once
/* atomic.h shim: the recorded reference is a single-threaded build, so
 * cmpxchg/xchg are the plain compare-exchange/swap they compile to. */
#define cmpxchg_release(p, o, n) __sync_val_compare_and_swap((p), (o), (n))
#define cmpxchg(p, o, n) __sync_val_compare_and_swap((p), (o), (n))
#define xchg(p, v) __sync_lock_test_and_set((p), (v))
