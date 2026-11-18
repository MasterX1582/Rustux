#pragma once
/* libc string prototypes declared directly — a <string.h> passthrough
 * resolves back to this file when the record build adds -I linux.
 * Signatures match glibc (and kernel string.h semantics). */
#include <linux/types.h>

void *memset(void *s, int c, size_t n);
void *memcpy(void *dest, const void *src, size_t n);
void *memmove(void *dest, const void *src, size_t n);
int memcmp(const void *s1, const void *s2, size_t n);
size_t strlen(const char *s);
size_t strnlen(const char *s, size_t n);
