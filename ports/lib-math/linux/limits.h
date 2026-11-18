/* Userspace shim for <linux/limits.h>: the limits int_sqrt.c reads. */
#pragma once

#include <linux/types.h>

#define ULONG_MAX (~0UL)
#define UINT_MAX (~0U)
#define U32_MAX ((u32)~0U)
#define U64_MAX ((u64)~0ULL)
