#pragma once
/* linux/threads.h shim: NR_CPUS for the 4-cpu model. */
#include <linux/smp.h>

#define NR_CPUS KSHIM_NCPU
