#pragma once
#include <linux/types.h>

static inline bool is_power_of_2(unsigned long n)
{
	return n != 0 && (n & (n - 1)) == 0;
}
