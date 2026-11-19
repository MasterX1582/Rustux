#pragma once
/* linux/cpu.h shim: the only consumer is radix_tree_init()'s
 * cpuhp_setup_state_nocalls() registration — a no-op success in the
 * single-CPU harness.  radix_tree_cpu_dead() is never invoked. */
#include <linux/types.h>

#define CPUHP_RADIX_DEAD 0x5d

#define cpuhp_setup_state_nocalls(state, name, startup, teardown) \
	((void)(state), (void)(name), (void)(startup), (void)(teardown), 0)
