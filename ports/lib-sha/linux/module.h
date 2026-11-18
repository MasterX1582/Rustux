#pragma once
#include <linux/export.h>

/* The module-init blocks in the sha units are compiled out
 * (CONFIG_CRYPTO_FIPS and the *_mod_init_arch hooks are unset), but
 * MODULE_* at file scope and __initconst on the fips-sha.h vectors are
 * unconditional. */
#define MODULE_LICENSE(x)
#define MODULE_DESCRIPTION(x)
#define MODULE_AUTHOR(x)

#define __init
#define __exit
#define __initconst
#define subsys_initcall(fn)
#define module_exit(fn)
