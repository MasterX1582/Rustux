/* Userspace shim for <linux/reciprocal_div.h>. */
#pragma once

#include <linux/types.h>
#include <asm/div64.h>

struct reciprocal_value {
	u32 m;
	u8 sh1, sh2;
};

struct reciprocal_value_adv {
	u32 m;
	u8 sh, exp;
	bool is_wide_m;
};

struct reciprocal_value reciprocal_value(u32 d);
struct reciprocal_value_adv reciprocal_value_adv(u32 d, u8 prec);

/* Verbatim from upstream <linux/reciprocal_div.h>: the consumer-facing
 * divide-by-reciprocal helper (not yet exercised by any harness unit). */
static inline u32 reciprocal_divide(u32 a, struct reciprocal_value R)
{
	u32 t = (u32)(((u64)a * R.m) >> 32);
	return (t + ((a - t) >> R.sh1)) >> R.sh2;
}
