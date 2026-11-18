#pragma once
/* linux/kernel.h shim: arithmetic helpers kfifo.c gets transitively. */
#include <linux/compiler.h>
#include <linux/minmax.h>
#include <linux/bug.h>

#define DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))
