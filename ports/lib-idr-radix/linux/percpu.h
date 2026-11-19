#pragma once
/* linux/percpu.h shim: single-"CPU" model — DEFINE_PER_CPU collapses
 * to a plain global, this_cpu_ptr is &self, per_cpu ignores the cpu.
 * The preload stack therefore behaves exactly like the real one on
 * the only CPU. */
#include <linux/types.h>

#define DECLARE_PER_CPU(type, name)	extern type name
#define DEFINE_PER_CPU(type, name)	type name
#define EXPORT_PER_CPU_SYMBOL_GPL(name)

#define this_cpu_ptr(ptr)		(ptr)
#define this_cpu_read(name)		(name)
#define this_cpu_write(name, val)	((name) = (val))
#define per_cpu(name, cpu)		(name)
