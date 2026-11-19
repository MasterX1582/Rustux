#pragma once
/* linux/errno.h shim: a <errno.h> passthrough would make glibc's
 * bits/errno.h resolve <linux/errno.h> back to this file, so define the
 * kernel constants the unit uses directly (values are the asm-generic
 * ones, identical on x86-64). */
#define EPERM		1
#define ENOENT		2
#define ESRCH		3
#define EINTR		4
#define EIO		5
#define ENOMEM		12
#define EACCES		13
#define EFAULT		14
#define EBUSY		16
#define EEXIST		17
#define ENODEV		19
#define EINVAL		22
#define ENOSPC		28
#define EROFS		30
#define ERANGE		34
#define ENOSYS		38
#define ENOTEMPTY	39
#define ELOOP		40
#define EAGAIN		11
#define EDEADLOCK	35
#define EOPNOTSUPP	95
#define ENOTSUP		EOPNOTSUPP
