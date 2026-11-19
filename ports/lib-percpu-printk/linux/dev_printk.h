#pragma once
/* linux/dev_printk.h shim: only the message-meta container is needed
 * by printk_ringbuffer.h. */
#include <linux/types.h>
#include <linux/compiler.h>

#ifndef dev_fmt
#define dev_fmt(fmt) fmt
#endif

struct device;

#define PRINTK_INFO_SUBSYSTEM_LEN 16
#define PRINTK_INFO_DEVICE_LEN 48

struct dev_printk_info {
	char subsystem[PRINTK_INFO_SUBSYSTEM_LEN];
	char device[PRINTK_INFO_DEVICE_LEN];
};
