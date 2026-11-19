#pragma once
/* linux/printk.h shim: kernel print output is folded into the
 * checksum stream rather than emitted — kshim_print() parses the
 * format string and folds each argument, canonicalising pointers via
 * the same encoding the wrappers use for entries (see wrappers.c).
 * This makes idr.c's ida_dump()/ida_dump_entry() deterministic and
 * observable to the differential run. */
#include <linux/types.h>

void kshim_print(const char *fmt, ...);

#define pr_cont(...)	kshim_print(__VA_ARGS__)
#define pr_debug(...)	kshim_print(__VA_ARGS__)
#define pr_info(...)	kshim_print(__VA_ARGS__)
#define pr_err(...)	kshim_print(__VA_ARGS__)
#define pr_warn(...)	kshim_print(__VA_ARGS__)
