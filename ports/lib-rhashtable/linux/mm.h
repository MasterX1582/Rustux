#pragma once
#include <linux/types.h>

#define PAGE_SHIFT	12
#define PAGE_SIZE	(1UL << PAGE_SHIFT)

#define NUMA_NO_NODE	(-1)

#include <linux/spinlock.h>

unsigned int num_possible_cpus(void);
