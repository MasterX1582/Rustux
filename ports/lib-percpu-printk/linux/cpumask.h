#pragma once
/* linux/cpumask.h shim: NCPU=4, all CPUs permanently online and never
 * dying. cpu_online_mask/cpu_dying_mask are dummy addresses only ever
 * consumed by the for_each_cpu_or macro below. */
#include <linux/types.h>
#include <linux/smp.h>

extern const unsigned long kshim_cpu_mask;

#define cpu_online_mask (&kshim_cpu_mask)
#define cpu_dying_mask (&kshim_cpu_mask)
#define cpu_possible_mask (&kshim_cpu_mask)

#define num_online_cpus() KSHIM_NCPU
#define num_possible_cpus() KSHIM_NCPU

#define for_each_cpu(cpu, mask) \
	for ((cpu) = 0; (cpu) < KSHIM_NCPU; (cpu)++)
#define for_each_cpu_or(cpu, mask1, mask2) \
	for ((cpu) = 0; (cpu) < KSHIM_NCPU; (cpu)++)
#define for_each_possible_cpu(cpu) \
	for ((cpu) = 0; (cpu) < KSHIM_NCPU; (cpu)++)
#define for_each_online_cpu(cpu) \
	for ((cpu) = 0; (cpu) < KSHIM_NCPU; (cpu)++)
