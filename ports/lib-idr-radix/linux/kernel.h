#pragma once
/* linux/kernel.h shim for the lib-idr-radix harness: min/max,
 * DIV_ROUND_UP and the printk family (which folds its arguments into
 * the harness checksum — see linux/printk.h). */
#include <linux/types.h>
#include <linux/minmax.h>
#include <linux/math.h>
#include <linux/printk.h>
