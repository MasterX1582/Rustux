#pragma once
#include <stdio.h>
#include <linux/types.h>
#include <linux/printk.h>
#include <linux/log2.h>

/* Build the hexdump.c print_hex_dump() block; the harness supplies a
 * printk() that captures into a deterministic log buffer. */
#define CONFIG_PRINTK 1

#define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]))

/* lib/cmdline.c is a separate unit; the harness supplies an unreachable
 * stub so parse_int_array() links. */
extern int get_options(const char *str, int nints, int *ints);

extern int hex_dump_to_buffer(const void *buf, size_t len, int rowsize,
			      int groupsize, char *linebuf,
			      size_t linebuflen, bool ascii);
