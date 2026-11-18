#pragma once
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/bitops.h>
#include <linux/unaligned.h>
#include <linux/align.h>
#include <linux/minmax.h>
#include <linux/math.h>

#define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]))
