#pragma once
/* asm-generic errno values used by xarray.c — defined directly because
 * glibc's <errno.h> -> <bits/errno.h> -> <linux/errno.h> loops back here. */
#define EPERM 1
#define ENOMEM 12
#define EBUSY 16
#define EEXIST 17
#define EINVAL 22
#define ERANGE 34
