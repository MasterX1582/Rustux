#pragma once
/* linux/preempt.h shim: the sequential harness is never "in
 * interrupt", so the atomic-with-preload path in
 * radix_tree_node_alloc() is always eligible when the gfp mask is
 * non-blocking. */
#include <linux/types.h>

#define in_interrupt()		0
#define in_task()		1
#define preempt_disable()	do { } while (0)
#define preempt_enable()	do { } while (0)
