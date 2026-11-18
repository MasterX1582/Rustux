#pragma once
/* linux/uaccess.h shim for the lib-seqbuf harness.
 *
 * copy_to_user() copies what it can and returns the number of bytes
 * NOT copied — the upstream contract.  Faults are deterministic:
 * kshim_uaccess_ok is the number of bytes the "user mapping" accepts
 * this call (SIZE_MAX = copy never faults); the harness sets it from
 * the seed to cover the success / partial / full-EFAULT paths of
 * seq_buf_to_user().
 */
#include <linux/types.h>
#include <linux/compiler.h>
#include <string.h>

extern size_t kshim_uaccess_ok;

static inline unsigned long
copy_to_user(void __user *to, const void *from, unsigned long n)
{
	size_t ok = n < kshim_uaccess_ok ? n : kshim_uaccess_ok;

	memcpy(to, from, ok);
	return n - ok;
}

static inline unsigned long
copy_from_user(void *to, const void __user *from, unsigned long n)
{
	size_t ok = n < kshim_uaccess_ok ? n : kshim_uaccess_ok;

	memcpy(to, from, ok);
	return n - ok;
}
