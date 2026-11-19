#pragma once
#include <linux/compiler.h>
#include <linux/types.h>

/* Kernel log levels are a SOH + ascii digit prefix. */
#define KERN_SOH	"\001"
#define KERN_EMERG	KERN_SOH "0"
#define KERN_ALERT	KERN_SOH "1"
#define KERN_CRIT	KERN_SOH "2"
#define KERN_ERR	KERN_SOH "3"
#define KERN_WARNING	KERN_SOH "4"
#define KERN_NOTICE	KERN_SOH "5"
#define KERN_INFO	KERN_SOH "6"
#define KERN_DEBUG	KERN_SOH "7"

enum {
	DUMP_PREFIX_NONE,
	DUMP_PREFIX_ADDRESS,
	DUMP_PREFIX_OFFSET
};

extern __printf(1, 2) int printk(const char *fmt, ...);
extern __printf(2, 3) char *kasprintf(gfp_t gfp, const char *fmt, ...);

extern void print_hex_dump(const char *level, const char *prefix_str,
			   int prefix_type, int rowsize, int groupsize,
			   const void *buf, size_t len, bool ascii);
