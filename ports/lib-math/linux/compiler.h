/* Userspace shim for <linux/compiler.h>. */
#pragma once

#define likely(x) __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)

/* Address-space annotations are meaningless in userspace. */
#define __rcu

/* Kernel definition verbatim: make the optimizer believe the variable can be
 * manipulated arbitrarily (prevents strength-reducing the subtraction loop). */
#ifndef OPTIMIZER_HIDE_VAR
#define OPTIMIZER_HIDE_VAR(var)						\
	__asm__ ("" : "=r" (var) : "0" (var))
#endif
