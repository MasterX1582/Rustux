#pragma once
/* asm/barrier.h shim: single-threaded harness, barriers compile to
 * compiler barriers. */
#define smp_wmb()	__asm__ __volatile__("" ::: "memory")
#define smp_rmb()	__asm__ __volatile__("" ::: "memory")
#define smp_mb()	__asm__ __volatile__("" ::: "memory")
#define barrier()	__asm__ __volatile__("" ::: "memory")
