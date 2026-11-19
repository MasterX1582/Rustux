#pragma once
#include <linux/types.h>

struct device;
typedef void (*dr_release_t)(struct device *dev, void *res);

extern void *devres_alloc(dr_release_t release, size_t size, gfp_t gfp);
extern void devres_free(void *res);
extern void devres_add(struct device *dev, void *res);
