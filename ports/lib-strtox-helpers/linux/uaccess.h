#pragma once
#include <string.h>
/* Userspace copy_from_user never faults: copy and report full success. */
static inline unsigned long copy_from_user(void *to, const void *from,
					   unsigned long n)
{
	memcpy(to, from, n);
	return 0;
}
