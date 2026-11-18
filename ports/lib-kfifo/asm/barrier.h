#pragma once
/* asm/barrier.h shim: smp_* become compiler barriers — ordering against
 * the optimizer is preserved; real fencing is meaningless here since
 * the harness is single-threaded. */
#define smp_wmb() __asm__ volatile("" ::: "memory")
#define smp_rmb() __asm__ volatile("" ::: "memory")
#define smp_mb()  __asm__ volatile("" ::: "memory")
#define barrier() __asm__ volatile("" ::: "memory")
