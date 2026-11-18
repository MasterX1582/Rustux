#pragma once
/* Minimal linux/kernel.h shim for the lib-llist harness: llist.c
 * includes it but uses nothing from it beyond what llist.h's own
 * includes already provide. */
#include <linux/types.h>
#include <linux/compiler.h>
