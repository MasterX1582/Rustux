#pragma once
/* linux/percpu.h shim: the percpu model.
 *
 * A `__percpu` pointer is a plain pointer into kshim_pcpu_area, a flat
 * NCPU x KSHIM_PCPU_STRIDE byte array defined in wrappers.c/selftest.c.
 * An allocation reserves the same byte offset inside every CPU's
 * stride-sized chunk; per_cpu_ptr() slides the pointer to the chunk of
 * the requested CPU. this_cpu_*() operates on the chunk of the
 * current simulated CPU (kshim_cpu, set per-op by the driver).
 *
 * __alloc_percpu_gfp()/free_percpu() are defined in wrappers.c so the
 * deterministic allocation-failure injection lives at the unit
 * boundary (like kmem_cache_alloc_lru does in lib-xarray).
 */
#include <linux/types.h>
#include <linux/gfp.h>
#include <linux/smp.h>
#include <linux/kernel.h>

#define __percpu

/* bytes of per-cpu address space per simulated cpu */
#define KSHIM_PCPU_STRIDE 256

extern char kshim_pcpu_area[KSHIM_NCPU][KSHIM_PCPU_STRIDE];

#define per_cpu_ptr(ptr, cpu) \
	((__typeof__(ptr))((char *)(ptr) + (cpu) * KSHIM_PCPU_STRIDE))
#define per_cpu(var, cpu) (*per_cpu_ptr(&(var), cpu))
#define this_cpu_ptr(ptr) per_cpu_ptr(ptr, kshim_cpu)

#define __this_cpu_read(var) (*this_cpu_ptr(&(var)))
#define __this_cpu_write(var, val) (*this_cpu_ptr(&(var)) = (val))
#define __this_cpu_add(var, val) (*this_cpu_ptr(&(var)) += (val))
#define __this_cpu_sub(var, val) (*this_cpu_ptr(&(var)) -= (val))
#define __this_cpu_inc(var) __this_cpu_add(var, 1)
#define __this_cpu_dec(var) __this_cpu_sub(var, 1)

#define this_cpu_read(var) __this_cpu_read(var)
#define this_cpu_write(var, val) __this_cpu_write(var, val)
#define this_cpu_add(var, val) __this_cpu_add(var, val)
#define this_cpu_sub(var, val) __this_cpu_sub(var, val)
#define this_cpu_inc(var) __this_cpu_inc(var)
#define this_cpu_dec(var) __this_cpu_dec(var)

void *__alloc_percpu_gfp(size_t size, size_t align, gfp_t gfp);
void *__alloc_percpu(size_t size, size_t align);
void free_percpu(void __percpu *ptr);

#define alloc_percpu_gfp(type, gfp) \
	((type __percpu *)__alloc_percpu_gfp(sizeof(type), \
					   __alignof__(type), gfp))
#define alloc_percpu(type) \
	((type __percpu *)__alloc_percpu(sizeof(type), __alignof__(type)))
