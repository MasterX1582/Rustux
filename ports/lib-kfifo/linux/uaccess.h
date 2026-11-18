#pragma once
/* uaccess shim: __user tags are compile-time noise and the user/kernel
 * split does not exist in the harness, so the copy helpers are plain
 * memcpy returning 0 (all bytes copied — the success path). */
#include <string.h>
#include <linux/types.h>

#ifndef __user
#define __user
#endif

static inline unsigned long copy_from_user(void *to, const void __user *from,
					   unsigned long n)
{
	memcpy(to, from, n);
	return 0;
}

static inline unsigned long copy_to_user(void __user *to, const void *from,
					 unsigned long n)
{
	memcpy(to, from, n);
	return 0;
}
