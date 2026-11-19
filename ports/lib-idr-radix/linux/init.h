#pragma once
/* linux/init.h shim: __init/__exit expand to nothing (compiler.h
 * already defines __init); there is no module machinery. */
#include <linux/compiler.h>

#define __exit
