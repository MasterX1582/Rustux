/* Userspace shim for <linux/module.h>: metadata macros are no-ops. */
#pragma once

#define MODULE_DESCRIPTION(x)
#define MODULE_AUTHOR(x)
#define MODULE_LICENSE(x)
#define module_init(x)
#define module_exit(x)
