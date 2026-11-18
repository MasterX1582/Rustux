/* Userspace shim for <asm/div64.h>: LP64 do_div from <asm-generic/div64.h>. */
#pragma once

#include <stdint.h>

#define do_div(n, base) ({					\
	uint32_t __base = (base);				\
	uint32_t __rem;						\
	__rem = ((uint64_t)(n)) % __base;			\
	(n) = ((uint64_t)(n)) / __base;				\
	__rem;							\
})
