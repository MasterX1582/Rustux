#pragma once
/* Userspace <linux/string.h>: libc string ops plus the prototypes for the
 * helpers that live in lib/string.c / lib/string_helpers.c. */
#include <string.h>
#include <strings.h>
#include <linux/types.h>

extern char *skip_spaces(const char *);
extern char *strim(char *);

extern bool sysfs_streq(const char *s1, const char *s2);
extern int match_string(const char * const *array, size_t n,
			const char *string);
extern int __sysfs_match_string(const char * const *array, size_t n,
				const char *s);
#define sysfs_match_string(_a, _s) __sysfs_match_string(_a, ARRAY_SIZE(_a))

extern char *strreplace(char *str, char old, char new);
extern void memcpy_and_pad(void *dest, size_t dest_len, const void *src,
			   size_t count, int pad);
