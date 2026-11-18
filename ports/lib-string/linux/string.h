/* <string.h> resolves here under -Ilinux; pull the real one first for
 * size_t and the declarations this file does not cover. */
#include_next <string.h>
#include <stdint.h>
#include <sys/types.h>

/* Prototypes for the functions lib/string.c defines, mirroring
 * <linux/string.h>.  They are declared hidden so that every reference
 * inside the built shared object binds to our copy and cannot be
 * interposed by the libc symbols already loaded into the driver process
 * (glibc strcmp returns the full byte difference vs the kernel's -1/0/1,
 * glibc strncasecmp folds before the NUL check, and so on). */
#pragma GCC visibility push(hidden)
#ifndef __KERNEL_STRING_SHIM_DECLS
#define __KERNEL_STRING_SHIM_DECLS

int strncasecmp(const char *s1, const char *s2, size_t len);
int strcasecmp(const char *s1, const char *s2);
char *strcpy(char *dest, const char *src);
ssize_t sized_strscpy(char *dest, const char *src, size_t count);
char *stpcpy(char *__restrict__ dest, const char *__restrict__ src);
char *strcat(char *dest, const char *src);
char *strncat(char *dest, const char *src, size_t count);
size_t strlcat(char *dest, const char *src, size_t count);
int strcmp(const char *cs, const char *ct);
int strncmp(const char *cs, const char *ct, size_t count);
char *strchr(const char *s, int c);
char *strchrnul(const char *s, int c);
char *strnchrnul(const char *s, size_t count, int c);
char *strrchr(const char *s, int c);
char *strnchr(const char *s, size_t count, int c);
size_t strlen(const char *s);
size_t strnlen(const char *s, size_t count);
size_t strspn(const char *s, const char *accept);
size_t strcspn(const char *s, const char *reject);
char *strpbrk(const char *cs, const char *ct);
char *strsep(char **s, const char *ct);
void *memset(void *s, int c, size_t count);
void *memset16(uint16_t *s, uint16_t v, size_t count);
void *memset32(uint32_t *s, uint32_t v, size_t count);
void *memset64(uint64_t *s, uint64_t v, size_t count);
void *memcpy(void *dest, const void *src, size_t count);
void *memmove(void *dest, const void *src, size_t count);
int memcmp(const void *cs, const void *ct, size_t count);
int bcmp(const void *a, const void *b, size_t len);
void *memscan(void *addr, int c, size_t size);
char *strstr(const char *s1, const char *s2);
char *strnstr(const char *s1, const char *s2, size_t len);
void *memchr(const void *s, int c, size_t n);
void *memchr_inv(const void *start, int c, size_t bytes);

#endif /* __KERNEL_STRING_SHIM_DECLS */
#pragma GCC visibility pop
