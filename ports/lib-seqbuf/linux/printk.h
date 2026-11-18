#pragma once
/* linux/printk.h shim for the lib-seqbuf harness.
 *
 * printk() becomes kshim_printk(): a deterministic sink that formats
 * with the same mini-printer as kshim_vsnprintf and appends the text to
 * the harness's klog buffer (folded into checksums by wrappers.c).  Only
 * the formats seq_buf_do_printk uses (%s, %.*s) ever reach it.
 */
#include <linux/types.h>
#include <linux/compiler.h>

enum {
	DUMP_PREFIX_NONE,
	DUMP_PREFIX_ADDRESS,
	DUMP_PREFIX_OFFSET
};

/* Implemented in wrappers.c; formats into a bounded deterministic log. */
int kshim_printk(const char *fmt, ...);

#define printk kshim_printk
