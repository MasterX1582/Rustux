#pragma once
#include <linux/types.h>
#include <linux/err.h>

#define PAGE_SIZE 4096

struct task_struct;
extern int get_cmdline(struct task_struct *task, char *buffer, int buflen);
