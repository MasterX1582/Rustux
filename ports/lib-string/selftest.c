/* Independent sanity checks for the lib/string.c port unit: canonical
 * values, not the recorded corpus.  Prints FAIL lines and returns the
 * failure count. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <strings.h>
#include <stdio.h>
#include <sys/types.h>

ssize_t sized_strscpy(char *dest, const char *src, size_t count);
char *strnchrnul(const char *s, size_t count, int c);
char *strnchr(const char *s, size_t count, int c);
char *strnstr(const char *s1, const char *s2, size_t len);
void *memscan(void *addr, int c, size_t size);
void *memchr_inv(const void *start, int c, size_t bytes);
void *memset16(uint16_t *s, uint16_t v, size_t count);
void *memset32(uint32_t *s, uint32_t v, size_t count);
void *memset64(uint64_t *s, uint64_t v, size_t count);

static int fails;
#define CK(expr) do {						\
	if (!(expr)) {						\
		printf("FAIL %d: %s\n", __LINE__, #expr);	\
		fails++;					\
	}							\
} while (0)

int main(void)
{
	char d[64], e[64];
	char *p;
	void *q;
	uint16_t b16[8];
	uint32_t b32[8];
	uint64_t b64[8];
	size_t i;

	/* strcmp / strncmp */
	CK(strcmp("abc", "abc") == 0);
	CK(strcmp("abc", "abd") < 0);
	CK(strcmp("abd", "abc") > 0);
	CK(strcmp("", "") == 0);
	CK(strcmp("a", "ab") < 0);
	CK(strncmp("abc", "abd", 2) == 0);
	CK(strncmp("abc", "abd", 3) < 0);
	CK(strncmp("abc", "abc", 0) == 0);

	/* case-insensitive */
	CK(strcasecmp("AbC", "aBc") == 0);
	CK(strcasecmp("abc", "abd") < 0);
	CK(strncasecmp("AbX", "aBx", 3) == 0);
	CK(strncasecmp("AbX", "aBy", 3) < 0);
	CK(strncasecmp("a", "b", 0) == 0);

	/* strlen / strnlen */
	CK(strlen("") == 0);
	CK(strlen("hello") == 5);
	CK(strnlen("hello", 10) == 5);
	CK(strnlen("hello", 3) == 3);
	CK(strnlen("", 0) == 0);

	/* strcpy / stpcpy */
	memset(d, 'x', sizeof d);
	CK(strcpy(d, "abc") == d);
	CK(!memcmp(d, "abc\0", 4));
	CK(d[5] == 'x');
	p = stpcpy(d, "xy");
	CK(p == d + 2);
	CK(!memcmp(d, "xy\0\0x", 5));	/* d[3]=old NUL, d[4]=fill */

	/* strcat / strncat / strlcat */
	strcpy(d, "ab");
	CK(strcat(d, "cd") == d);
	CK(!strcmp(d, "abcd"));
	strcpy(d, "ab");
	CK(strncat(d, "cdef", 2) == d);
	CK(!strcmp(d, "abcd"));
	strcpy(d, "ab");
	CK(strncat(d, "cd", 9) == d);
	CK(!strcmp(d, "abcd"));
	strcpy(d, "ab");
	CK(strncat(d, "cd", 0) == d);
	CK(!strcmp(d, "ab"));
	strcpy(d, "ab");
	CK(strlcat(d, "cdef", sizeof d) == 6);
	CK(!strcmp(d, "abcdef"));
	strcpy(d, "ab");
	CK(strlcat(d, "cdef", 4) == 6);	/* reports full length */
	CK(!strcmp(d, "abc"));		/* truncated, NUL-terminated */

	/* sized_strscpy */
	memset(d, 'x', sizeof d);
	CK(sized_strscpy(d, "hello", 6) == 5);
	CK(!strcmp(d, "hello"));
	CK(sized_strscpy(d, "hello", 4) == -7);	/* -E2BIG: truncated */
	CK(!strcmp(d, "hel"));
	CK(sized_strscpy(d, "hello", 0) == -7);
	CK(sized_strscpy(d, "", 8) == 0);
	CK(d[0] == 0);
	CK(sized_strscpy(d, "abcdefghijklmnop", 9) == -7); /* word loop E2BIG */

	/* strchr family */
	CK(strchr("abc", 'b') - (char *)"abc" == 1);
	CK(strchr("abc", 'z') == NULL);
	CK(strchr("abc", 0) - (char *)"abc" == 3);
	CK(strchrnul("abc", 'z') - (char *)"abc" == 3);
	CK(strchrnul("abc", 'b') - (char *)"abc" == 1);
	CK(strnchrnul("abc", 2, 'c') - (char *)"abc" == 2);
	CK(strnchrnul("abc", 2, 'z') - (char *)"abc" == 2);
	CK(strrchr("abcb", 'b') - (char *)"abcb" == 3);
	CK(strrchr("abc", 'z') == NULL);
	CK(strrchr("abc", 0) - (char *)"abc" == 3);
	CK(strnchr("abc", 3, 'c') - (char *)"abc" == 2);
	CK(strnchr("abc", 2, 'c') == NULL);
	CK(strnchr("abc", 4, 0) - (char *)"abc" == 3);

	/* spans */
	CK(strspn("aabbcc", "ab") == 4);
	CK(strspn("xyz", "ab") == 0);
	CK(strcspn("abcdef", "de") == 3);
	CK(strcspn("abc", "") == 3);
	CK(strpbrk("abcdef", "de") - (char *)"abcdef" == 3);
	CK(strpbrk("abc", "xyz") == NULL);

	/* strsep */
	strcpy(d, "a,b,,c");
	p = d;
	CK(!strcmp(strsep(&p, ","), "a"));
	CK(!strcmp(strsep(&p, ","), "b"));
	CK(!strcmp(strsep(&p, ","), ""));	/* empty token kept */
	CK(!strcmp(strsep(&p, ","), "c"));
	CK(p == NULL);
	CK(strsep(&p, ",") == NULL);

	/* memset / memcpy / memmove */
	memset(d, 'q', 8);
	CK(d[0] == 'q' && d[7] == 'q');
	memcpy(e, "01234567", 9);
	CK(!strcmp(e, "01234567"));
	strcpy(d, "12345678");
	memmove(d + 2, d, 6);			/* dst > src overlap */
	CK(!memcmp(d, "12123456", 9));
	strcpy(d, "12345678");
	memmove(d, d + 2, 6);			/* dst < src overlap */
	CK(!memcmp(d, "34567878", 9));
	memmove(d, d, 4);
	CK(!memcmp(d, "34567878", 9));

	/* memcmp / bcmp */
	CK(memcmp("abc", "abc", 3) == 0);
	CK(memcmp("abc", "abd", 3) < 0);
	CK(memcmp("abd", "abc", 3) > 0);
	CK(memcmp("abcdefgh", "abcdefgi", 8) < 0); /* word path */
	CK(memcmp("abcdefgh", "abcdefgh", 8) == 0);
	CK(bcmp("abc", "abc", 3) == 0);
	CK(bcmp("abc", "abd", 3) != 0);

	/* memscan / memchr / memchr_inv */
	CK((char *)memscan((char *)"abc", 'c', 3) - (char *)"abc" == 2);
	CK((char *)memscan((char *)"abc", 'z', 3) - (char *)"abc" == 3);
	CK(memchr("abc", 'b', 3) == (char *)"abc" + 1);
	CK(memchr("abc", 'z', 3) == NULL);
	CK(memchr("abc", 'c', 0) == NULL);
	memset(d, 'v', 32);
	CK(memchr_inv(d, 'v', 32) == NULL);
	d[17] = 'w';
	CK((char *)memchr_inv(d, 'v', 32) - d == 17);
	CK(memchr_inv(d, 'v', 0) == NULL);
	CK((char *)memchr_inv(d, 'v', 20) - d == 17); /* word loop + prefix */

	/* strstr / strnstr */
	CK(strstr("aabcbc", "bc") - (char *)"aabcbc" == 2);
	CK(strstr("abc", "bcde") == NULL);
	CK(strstr("abc", "") - (char *)"abc" == 0);
	CK(strnstr("aabcbc", "bc", 6) - (char *)"aabcbc" == 2);
	CK(strnstr("aabcbc", "bc", 3) == NULL);
	CK(strnstr("abc", "", 0) - (char *)"abc" == 0);

	/* memset16/32/64 */
	memset16(b16, 0x1234, 8);
	for (i = 0; i < 8; i++)
		CK(b16[i] == 0x1234);
	memset32(b32, 0xdeadbeef, 8);
	for (i = 0; i < 8; i++)
		CK(b32[i] == 0xdeadbeef);
	memset64(b64, 0x0123456789abcdefULL, 8);
	for (i = 0; i < 8; i++)
		CK(b64[i] == 0x0123456789abcdefULL);

	if (!fails)
		printf("selftest: all checks passed\n");
	return fails;
}
