/* Userspace shim for <linux/kernel.h>: swap, WARN_ON, jump-label no-ops. */
#pragma once

#include <linux/compiler.h>
#include <linux/minmax.h>
#include <linux/bitops.h>
#include <linux/types.h>

#define swap(a, b) do { __typeof__(a) __t = (a); (a) = (b); (b) = __t; } while (0)
#define WARN_ON(x) (!!(x))
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))

/* Static keys: in userspace the "true" branch is always taken, matching a
 * kernel where the static key was never patched off. */
#define DEFINE_STATIC_KEY_TRUE(name) static int name = 1;
#define DEFINE_STATIC_KEY_FALSE(name) static int name = 0;
#define static_branch_likely(x) likely(x)
#define static_branch_unlikely(x) unlikely(x)
