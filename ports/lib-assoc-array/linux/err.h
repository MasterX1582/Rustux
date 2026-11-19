#pragma once
/* linux/err.h shim: the standard error-pointer encoding. */
#include <linux/types.h>
#include <linux/errno.h>

#define MAX_ERRNO	4095

#define ERR_PTR(error)		((void *)(long)(error))
#define PTR_ERR(ptr)		((long)(ptr))
#define IS_ERR_VALUE(x)		((unsigned long)(x) >= (unsigned long)-MAX_ERRNO)
#define IS_ERR(ptr)		IS_ERR_VALUE((unsigned long)(ptr))
