#pragma once
/* asm-generic errno values used by seq_buf.c and the shims — defined
 * directly because glibc's <errno.h> -> <bits/errno.h> -> <linux/errno.h>
 * would loop back here. */
#define EPERM 1
#define E2BIG 7
#define EFAULT 14
#define EBUSY 16
#define EINVAL 22
#define ENOMEM 12
#define ERANGE 34
