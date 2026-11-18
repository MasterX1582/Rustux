/* <string.h> resolves here under -Ilinux; pull the real one first for
 * size_t and the libc declarations (memcpy, memset, memmove, memcmp).
 *
 * Unlike the lib-string wave, this unit does not DEFINE any libc-named
 * function: the memcpy/memset calls inside sha*.c are meant to bind to
 * libc, whose semantics are byte-exact C semantics here.  Declaring
 * them hidden would make them unresolvable (no in-module definition),
 * so they keep default visibility. */
#include_next <string.h>
#include <stdint.h>
#include <sys/types.h>
#include <linux/compiler.h>

#ifndef __KERNEL_SHA_STRING_SHIM_DECLS
#define __KERNEL_SHA_STRING_SHIM_DECLS

/* Kernel memzero_explicit(): memset the optimizer cannot remove.
 * memzero_explicit is not a libc symbol, so a header-local inline is
 * unambiguous. */
static __always_inline void memzero_explicit(void *s, size_t count)
{
	memset(s, 0, count);
	__asm__ volatile("" ::: "memory");
}

#endif /* __KERNEL_SHA_STRING_SHIM_DECLS */
