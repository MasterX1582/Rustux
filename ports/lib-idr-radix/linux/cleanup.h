#pragma once
/* linux/cleanup.h shim: include/linux/idr.h declares one
 * DEFINE_CLASS() for the scoped idr_alloc guard — unused by idr.c —
 * so the class macros need only parse, not implement RAII. */
#include <linux/types.h>
#include <linux/compiler.h>

#define DEFINE_FREE(_name, _type, _free)
#define __free(_name)
#define __get_and_null(p, nullvalue)	((p) = (nullvalue), (p))
#define take_ptr(p)			__get_and_null(p, NULL)
#define return_ptr(p)			return take_ptr(p)
#define retain_and_null_ptr(p)		((void)__get_and_null(p, NULL))
#define DEFINE_CLASS(_name, _type, _exit, _init, _init_args...)
#define DEFINE_GUARD(...)
#define CLASS(...)
#define guard(...)
