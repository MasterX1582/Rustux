/* Userspace shim for <linux/export.h>: symbol exports are no-ops outside the kernel. */
#pragma once

#define EXPORT_SYMBOL(sym)
#define EXPORT_SYMBOL_GPL(sym)
