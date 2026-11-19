#pragma once
/* linux/cpu.h shim: cpu hotplug is stubbed. The states exist only to
 * let percpu_counter.c's startup notifier block compile; cpuhp_setup_*
 * succeed trivially and the callbacks never run (all 4 CPUs are
 * permanently online in the model). */
#include <linux/types.h>
#include <linux/kernel.h>
#include <linux/cpumask.h>
#include <linux/smp.h>

#define CPUHP_AP_ONLINE_DYN 512
#define CPUHP_PERCPU_CNT_DEAD 60

static inline int cpuhp_setup_state(unsigned int state, const char *name,
				    int (*startup)(unsigned int cpu),
				    int (*teardown)(unsigned int cpu))
{
	(void)state; (void)name; (void)startup; (void)teardown;
	return 0;
}

static inline int cpuhp_setup_state_nocalls(unsigned int state,
					    const char *name,
					    int (*startup)(unsigned int cpu),
					    int (*teardown)(unsigned int cpu))
{
	(void)state; (void)name; (void)startup; (void)teardown;
	return 0;
}
