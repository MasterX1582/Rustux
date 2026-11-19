#pragma once
#include <linux/types.h>
#include <linux/err.h>

struct file;
extern char *file_path(struct file *filp, char *buf, int buflen);
