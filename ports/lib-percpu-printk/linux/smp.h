#pragma once
/* linux/smp.h shim: the percpu model has NCPU=4 simulated processors;
 * kshim_cpu (defined in wrappers.c / selftest.c) is "the CPU this op
 * runs on", chosen per-op by the seeded driver. */
#include <linux/types.h>
#include <linux/cpumask.h>

#define KSHIM_NCPU 4

extern int kshim_cpu;

#define smp_processor_id() (kshim_cpu)
#define raw_smp_processor_id() (kshim_cpu)
#define get_cpu() (kshim_cpu)
#define put_cpu() do { } while (0)

/* Sequential model: all memory barriers degrade to compiler no-ops. */
#define smp_rmb() do { } while (0)
#define smp_wmb() do { } while (0)
#define smp_mb() do { } while (0)
#define smp_mb__before_atomic() do { } while (0)
#define smp_mb__after_atomic() do { } while (0)
