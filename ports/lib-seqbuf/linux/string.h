/* <linux/string.h> shim for the lib-seqbuf harness.  The shim dir is on
 * the -I path as `linux/`, so <string.h> would also resolve here —
 * pull the real one first for memcpy/memset/strlen/strnlen/strchr. */
#include_next <string.h>
#ifndef KSHIM_LINUX_STRING_H
#define KSHIM_LINUX_STRING_H
#include <stdint.h>
#include <sys/types.h>
#include <linux/errno.h>
#include <linux/bug.h>
#include <limits.h>

/* sized_strscpy() — byte-level semantics identical to lib/string.c's
 * word-at-a-time implementation: copies the NUL-terminated source,
 * writes at most count bytes (last byte forced NUL on truncation),
 * returns chars copied excluding NUL, or -E2BIG when count is 0 or the
 * source was truncated. */
static inline ssize_t sized_strscpy(char *dest, const char *src, size_t count)
{
	size_t res = 0;

	if (count == 0 || WARN_ON_ONCE(count > INT_MAX))
		return -E2BIG;

	while (count > 1) {
		char c = src[res];

		dest[res] = c;
		if (!c)
			return (ssize_t)res;
		res++;
		count--;
	}
	dest[res] = '\0';
	return src[res] ? -E2BIG : (ssize_t)res;
}

/* upstream strscpy(dst, src, size) is __strscpy1() -> sized_strscpy()
 * plus compile-time array/cstr checks that fold to +0; only the 3-arg
 * form is used by seq_buf.c. */
#define strscpy(dst, src, size) sized_strscpy(dst, src, size)

#endif /* KSHIM_LINUX_STRING_H */
