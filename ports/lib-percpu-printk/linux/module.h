#pragma once
/* linux/module.h stub: exports and module_init() are no-ops in the
 * harness build. */
#include <linux/types.h>
#include <linux/export.h>

#define module_init(fn) static int __mod_init_##fn __attribute__((unused)) = 0
#define MODULE_IMPORT_NS(ns)
