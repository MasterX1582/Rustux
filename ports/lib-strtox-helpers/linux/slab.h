#pragma once
/* kmalloc family over userspace malloc; allocation never fails. */
#include <stdlib.h>
#include <string.h>
#include <linux/types.h>
#include <linux/err.h>
#include <linux/errno.h>

/* Allocation flags are advisory and ignored by the harness anyway. */
#define GFP_KERNEL	0
#define GFP_ATOMIC	0

static inline void *kmalloc(size_t n, gfp_t gfp)
{
	(void)gfp;
	return malloc(n);
}

static inline void *kzalloc(size_t n, gfp_t gfp)
{
	(void)gfp;
	return calloc(1, n);
}

static inline void *kcalloc(size_t n, size_t s, gfp_t gfp)
{
	(void)gfp;
	return calloc(n, s);
}

static inline void kfree(const void *p)
{
	free((void *)p);
}

static inline char *kstrdup(const char *s, gfp_t gfp)
{
	char *d;
	(void)gfp;
	if (!s)
		return NULL;
	d = malloc(strlen(s) + 1);
	if (d)
		strcpy(d, s);
	return d;
}

static inline void *memdup_user_nul(const void *src, size_t n)
{
	char *d = malloc(n + 1);
	if (!d)
		return ERR_PTR(-ENOMEM);
	memcpy(d, src, n);
	d[n] = '\0';
	return d;
}

#define kzalloc_objs(P, COUNT, ...)	calloc((COUNT), sizeof(P))
