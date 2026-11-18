#pragma once
/* Minimal userspace compiler shim for the lib-rhashtable harness.
 * WRITE_ONCE/READ_ONCE degrade to plain accesses: the recorded
 * reference is a single-threaded build where they compile to exactly
 * that.  All sparse/context annotations are no-ops. */

#undef __always_inline
#define __always_inline inline __attribute__((__always_inline__))
#define likely(x)	__builtin_expect(!!(x), 1)
#define unlikely(x)	__builtin_expect(!!(x), 0)
#define fallthrough	__attribute__((__fallthrough__))
#define noinline	__attribute__((__noinline__))
#define __noreturn	__attribute__((__noreturn__))
#define __aligned(x)	__attribute__((__aligned__(x)))
#define __packed	__attribute__((__packed__))
#define __maybe_unused	__attribute__((__unused__))
#define __always_unused	__attribute__((__unused__))
#define __must_check	__attribute__((__warn_unused_result__))
#define __cold
#define __pure		__attribute__((__pure__))
#define __init
#define __initdata
#define __exit
#define __force
#define __user
#define __kernel
#define __bitwise
#define __rcu
#define __percpu
#define __ro_after_init
#define ____cacheline_aligned_in_smp
#define __visible
#define __weak		__attribute__((__weak__))
#define __section(S)	__attribute__((__section__(S)))

/* sparse context annotations */
#define __acquires(x)
#define __releases(x)
#define __acquires_shared(x)
#define __releases_shared(x)
#define __must_hold(x)
#define __must_hold_shared(x)
#define __cond_acquires(x)
#define __cond_lock(x, c)	(c)
#define __bitlock(x)
#define __context__(x, y)
#define __release(x)
#define __acquire(x)
#define RCU
#define __bitlock__(x)

#define WRITE_ONCE(x, val)	((x) = (val))
#define READ_ONCE(x)		(x)

#define barrier()		__asm__ volatile("" ::: "memory")
#define smp_rmb()		barrier()
#define smp_wmb()		barrier()
#define smp_mb()		barrier()

#define TYPEOF_UNQUAL(exp)	__typeof__(exp)
#define _THIS_IP_		0UL

/* preempt/irq serialization is a no-op in the sequential harness. */
#define preempt_enable()	do { } while (0)
#define preempt_disable()	do { } while (0)
#define local_irq_save(flags)	((flags) = 0)
#define local_irq_restore(flags) do { } while (0)

#define SINGLE_DEPTH_NESTING	1
