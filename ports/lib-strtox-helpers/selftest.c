/* Independent sanity checks for the lib-strtox-helpers port units:
 * canonical values, not the recorded corpus.  Prints FAIL lines and
 * returns the failure count.  Covers kstrto* over valid/invalid strings,
 * whitespace, signs and every base; hex_dump_to_buffer row/group shapes
 * and truncation; string_escape_* on control/high bytes; unescape; and
 * the strreplace/sysfs_streq/match helpers. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <stdbool.h>
#include <linux/kernel.h>
#include <linux/string.h>
#include <linux/string_helpers.h>
#include <linux/kstrtox.h>
#include <linux/hex.h>
#include <linux/slab.h>
#include <linux/errno.h>
#include <linux/ctype.h>
#include "kstrtox.h"

static int fails;
#define CK(expr) do {						\
	if (!(expr)) {						\
		printf("FAIL %d: %s\n", __LINE__, #expr);	\
		fails++;					\
	}							\
} while (0)

int main(void)
{
	char buf[256];
	int i;
	bool b;
	u8 u8v;
	s8 s8v;
	u16 u16v;
	s16 s16v;
	u32 u32v;
	s32 s32v;
	u64 u64v;
	s64 s64v;
	unsigned int ui;
	int si;
	unsigned long ul;
	long sl;
	unsigned long long ull;
	long long ll;
	char *p;

	/* ---- kstrtox: valid decimal ----------------------------------- */
	CK(kstrtouint("123", 10, &ui) == 0 && ui == 123);
	CK(kstrtoint("-123", 10, &si) == 0 && si == -123);
	CK(kstrtoint("+123", 10, &si) == 0 && si == 123);
	CK(kstrtoull("18446744073709551615", 10, &ull) == 0 &&
	   ull == ~0ULL);
	CK(kstrtoll("-9223372036854775808", 10, &ll) == 0 &&
	   ll == (-9223372036854775807LL - 1));
	CK(kstrtoll("9223372036854775807", 10, &ll) == 0 &&
	   ll == 9223372036854775807LL);

	/* overflow -> -ERANGE, res untouched */
	ull = 77;
	CK(kstrtoull("18446744073709551616", 10, &ull) == -ERANGE);
	CK(ull == 77);
	CK(kstrtoull("99999999999999999999", 10, &ull) == -ERANGE);
	CK(kstrtoll("9223372036854775808", 10, &ll) == -ERANGE);
	CK(kstrtoll("-9223372036854775809", 10, &ll) == -ERANGE);
	CK(kstrtouint("4294967296", 10, &ui) == -ERANGE);
	CK(kstrtouint("4294967295", 10, &ui) == 0 && ui == 4294967295U);
	CK(kstrtoint("2147483648", 10, &si) == -ERANGE);
	CK(kstrtoint("-2147483648", 10, &si) == 0 && si == -2147483648);
	CK(kstrtoint("-2147483649", 10, &si) == -ERANGE);
	CK(kstrtou8("255", 10, &u8v) == 0 && u8v == 255);
	CK(kstrtou8("256", 10, &u8v) == -ERANGE);
	CK(kstrtos8("-128", 10, &s8v) == 0 && s8v == -128);
	CK(kstrtos8("128", 10, &s8v) == -ERANGE);
	CK(kstrtos8("-129", 10, &s8v) == -ERANGE);
	CK(kstrtou16("65535", 10, &u16v) == 0 && u16v == 65535);
	CK(kstrtou16("65536", 10, &u16v) == -ERANGE);
	CK(kstrtos16("-32768", 10, &s16v) == 0 && s16v == -32768);
	CK(kstrtos16("32768", 10, &s16v) == -ERANGE);

	/* minus on unsigned functions -> -EINVAL */
	CK(kstrtoull("-1", 10, &ull) == -EINVAL);
	CK(kstrtouint("-5", 0, &ui) == -EINVAL);
	/* leading whitespace is not skipped */
	CK(kstrtoint(" 12", 10, &si) == -EINVAL);
	CK(kstrtoint("\t12", 10, &si) == -EINVAL);
	/* single trailing newline accepted */
	CK(kstrtoint("12\n", 10, &si) == 0 && si == 12);
	/* anything else trailing is EINVAL */
	CK(kstrtoint("12\n\n", 10, &si) == -EINVAL);
	CK(kstrtoint("12x", 10, &si) == -EINVAL);
	CK(kstrtoint("12 ", 10, &si) == -EINVAL);
	CK(kstrtoint("", 10, &si) == -EINVAL);
	CK(kstrtoint("+", 10, &si) == -EINVAL);
	CK(kstrtoint("-", 10, &si) == -EINVAL);

	/* bases */
	CK(kstrtouint("ff", 16, &ui) == 0 && ui == 255);
	CK(kstrtouint("FF", 16, &ui) == 0 && ui == 255);
	CK(kstrtouint("0xff", 16, &ui) == 0 && ui == 255);  /* prefix eaten */
	CK(kstrtouint("0Xff", 16, &ui) == 0 && ui == 255);
	CK(kstrtouint("ff", 0, &ui) == -EINVAL);	/* base 0: not 0x */
	CK(kstrtouint("0xff", 0, &ui) == 0 && ui == 255); /* autodetect */
	CK(kstrtouint("017", 0, &ui) == 0 && ui == 15);   /* octal */
	CK(kstrtouint("018", 0, &ui) == -EINVAL);	/* '8' bad octal */
	CK(kstrtouint("17", 8, &ui) == 0 && ui == 15);
	CK(kstrtouint("101", 2, &ui) == 0 && ui == 5);
	CK(kstrtouint("102", 2, &ui) == -EINVAL);
	CK(kstrtouint("77", 10, &ui) == 0 && ui == 77);
	CK(kstrtouint("z", 36, &ui) == -EINVAL);	/* >base16 unsupported */
	CK(kstrtouint("10", 3, &ui) == 0 && ui == 3);
	CK(kstrtoint("-10", 16, &si) == 0 && si == -16);
	/* "0x" with base 0: leading '0' forces base 8 (isxdigit(s[2])
	 * fails on the NUL), 'x' is junk trailing -> EINVAL. */
	CK(kstrtouint("0x", 0, &ui) == -EINVAL);

	/* _parse_integer_limit: rv is consumed count | KSTRTOX_OVERFLOW */
	ull = 0;
	CK(_parse_integer_limit("123abc", 10, &ull, 20, 0) == 3 && ull == 123);
	CK(_parse_integer_limit("ff.", 16, &ull, 20, 0) == 2 && ull == 255);
	CK(_parse_integer_limit("18446744073709551616", 10, &ull, 20, 0) ==
	   (20 | KSTRTOX_OVERFLOW) && ull == ~0ULL);
	CK(_parse_integer_limit("12", 10, &ull, 1, 0) == 1 && ull == 1);
	CK(_parse_integer_limit("5", 10, &ull, 4, 100) == 1 && ull == 1005);

	/* _parse_integer_fixup_radix */
	{
		unsigned int bb;
		const char *r;
		bb = 0;  r = _parse_integer_fixup_radix("0x1f", &bb);
		CK(bb == 16 && r[0] == '1');
		bb = 0;  r = _parse_integer_fixup_radix("017", &bb);
		CK(bb == 8 && r[0] == '0');
		bb = 0;  r = _parse_integer_fixup_radix("17", &bb);
		CK(bb == 10 && r[0] == '1');
		bb = 16; r = _parse_integer_fixup_radix("0xff", &bb);
		CK(bb == 16 && r[0] == 'f');
		bb = 16; r = _parse_integer_fixup_radix("ff", &bb);
		CK(bb == 16 && r[0] == 'f');
		bb = 10; r = _parse_integer_fixup_radix("0x10", &bb);
		CK(bb == 10 && r[0] == '0');	/* explicit base: no eat */
	}

	/* kstrtobool */
	CK(kstrtobool("y", &b) == 0 && b);
	CK(kstrtobool("Y", &b) == 0 && b);
	CK(kstrtobool("1", &b) == 0 && b);
	CK(kstrtobool("t", &b) == 0 && b);
	CK(kstrtobool("e", &b) == 0 && b);
	CK(kstrtobool("n", &b) == 0 && !b);
	CK(kstrtobool("0", &b) == 0 && !b);
	CK(kstrtobool("f", &b) == 0 && !b);
	CK(kstrtobool("d", &b) == 0 && !b);
	CK(kstrtobool("on", &b) == 0 && b);
	CK(kstrtobool("ON", &b) == 0 && b);
	CK(kstrtobool("off", &b) == 0 && !b);
	CK(kstrtobool("OfF", &b) == 0 && !b);
	CK(kstrtobool("ox", &b) == -EINVAL);
	CK(kstrtobool("x", &b) == -EINVAL);
	CK(kstrtobool("yes", &b) == 0 && b);	/* only s[0] matters */
	CK(kstrtobool(NULL, &b) == -EINVAL);

	/* scaled decimal */
	CK(kstrtoudec64("123.45", 3, &u64v) == 0 && u64v == 123450);
	CK(kstrtoudec64("1.5", 0, &u64v) == 0 && u64v == 1);
	CK(kstrtoudec64(".5", 0, &u64v) == 0 && u64v == 0);
	CK(kstrtodec64("-3.141592", 4, &s64v) == 0 && s64v == -31415);
	CK(kstrtoudec64("0.999", 2, &u64v) == 0 && u64v == 99);
	CK(kstrtoudec64("", 2, &u64v) == -EINVAL);
	CK(kstrtoudec64(".", 2, &u64v) == -EINVAL);
	CK(kstrtoudec64("18446744073709551615", 0, &u64v) == 0 &&
	   u64v == ~0ULL);
	CK(kstrtoudec64("18446744073709551616", 0, &u64v) == -ERANGE);
	CK(kstrtoudec64("1.2x", 2, &u64v) == -EINVAL);
	CK(kstrtoudec64("1.2\n", 2, &u64v) == 0 && u64v == 120);

	/* from_user: truncation at sizeof(type)*8+3 bytes */
	CK(kstrtouint_from_user("123", 3, 10, &ui) == 0 && ui == 123);
	/* count truncates the input; the truncated prefix parses fine */
	CK(kstrtouint_from_user("12345", 3, 10, &ui) == 0 && ui == 123);
	CK(kstrtou8_from_user("255", 3, 10, &u8v) == 0 && u8v == 255);
	CK(kstrtobool_from_user("y", 1, &b) == 0 && b);
	CK(kstrtobool_from_user("on", 2, &b) == 0 && b);
	CK(kstrtobool_from_user("onx", 3, &b) == 0 && b);

	/* ---- hexdump --------------------------------------------------- */
	CK(hex_to_bin('0') == 0 && hex_to_bin('9') == 9);
	CK(hex_to_bin('a') == 10 && hex_to_bin('f') == 15);
	CK(hex_to_bin('A') == 10 && hex_to_bin('F') == 15);
	CK(hex_to_bin('g') == -1 && hex_to_bin(' ') == -1);
	CK(hex_to_bin(0) == -1 && hex_to_bin(0xff) == -1);

	memset(buf, 0, sizeof buf);
	CK(hex2bin((u8 *)buf, "0123456789abcdef", 8) == 0);
	CK(!memcmp(buf, "\x01\x23\x45\x67\x89\xab\xcd\xef", 8));
	CK(hex2bin((u8 *)buf, "01234g", 3) == -EINVAL);
	CK(hex2bin((u8 *)buf, "", 0) == 0);

	memset(buf, 0, sizeof buf);
	p = bin2hex(buf, "\x01\x23\xab\xef", 4);
	CK(p == buf + 8 && !memcmp(buf, "0123abef", 8));

	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCDEFGHIJKLMNOP", 16, 16, 1, buf,
			       sizeof buf, false);
	/* the loop appends ' ' after every byte then lx-- strips the
	 * final one: "41 42 ... 50" length 47 */
	CK(i == 47 && !strcmp(buf,
		"41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f 50"));
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCDEFGHIJKLMNOP", 16, 16, 1, buf,
			       sizeof buf, true);
	CK(!strcmp(buf,
		"41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f 50  ABCDEFGHIJKLMNOP"));
	/* groupsize 2: u16 little-endian per group */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCD", 4, 16, 2, buf, sizeof buf, false);
	CK(!strcmp(buf, "4241 4443"));
	/* groupsize 4 */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCDEFGH", 8, 16, 4, buf, sizeof buf, false);
	CK(!strcmp(buf, "44434241 48474645"));
	/* groupsize 8 */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCDEFGH", 8, 16, 8, buf, sizeof buf, false);
	CK(!strcmp(buf, "4847464544434241"));
	/* len % groupsize != 0 -> groupsize forced to 1 */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABC", 3, 16, 4, buf, sizeof buf, false);
	CK(!strcmp(buf, "41 42 43"));
	/* bad rowsize -> 16; bad groupsize -> 1 */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("ABCD", 4, 99, 3, buf, sizeof buf, false);
	CK(!strcmp(buf, "41 42 43 44"));
	/* truncation: linebuflen limits; ret is the would-be length */
	memset(buf, 0xa5, sizeof buf);
	i = hex_dump_to_buffer("ABCD", 4, 16, 1, buf, 5, false);
	CK(i == 11);		/* (1*2+1)*4-1 */
	/* ascii column padding for a short row: ascii_column = 16*2 +
	 * 16/1 + 1 = 49, then the 2 ascii chars: total 51 */
	memset(buf, 0, sizeof buf);
	i = hex_dump_to_buffer("AB", 2, 16, 1, buf, sizeof buf, true);
	CK(i == 51 && !memcmp(buf, "41 42", 5) && buf[5] == ' ' &&
	   buf[48] == ' ' && buf[49] == 'A' && buf[50] == 'B');

	/* ---- string_helpers -------------------------------------------- */
	memset(buf, 0, sizeof buf);
	CK(string_get_size(0, 1, STRING_UNITS_10, buf, sizeof buf) == 3 &&
	   !strcmp(buf, "0 B"));
	memset(buf, 0, sizeof buf);
	i = string_get_size(1536, 1, STRING_UNITS_2, buf, sizeof buf);
	CK(!strcmp(buf, "1.50 KiB"));
	memset(buf, 0, sizeof buf);
	i = string_get_size(1500, 1, STRING_UNITS_10, buf, sizeof buf);
	CK(!strcmp(buf, "1.50 kB"));
	memset(buf, 0, sizeof buf);
	i = string_get_size(4096, 512, STRING_UNITS_2, buf, sizeof buf);
	CK(!strcmp(buf, "2.00 MiB"));
	memset(buf, 0, sizeof buf);
	i = string_get_size(1024, 1, STRING_UNITS_2 | STRING_UNITS_NO_SPACE |
			   STRING_UNITS_NO_BYTES, buf, sizeof buf);
	CK(!strcmp(buf, "1.00Ki"));

	/* escape: every flag class */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("a\nb\tc", buf, sizeof buf, ESCAPE_SPACE, NULL);
	CK(!strcmp(buf, "a\\nb\\tc"));
	memset(buf, 0, sizeof buf);
	i = string_escape_str("a\"b\\c\a", buf, sizeof buf,
			      ESCAPE_SPECIAL, NULL);
	CK(!strcmp(buf, "a\\\"b\\\\c\\a"));
	memset(buf, 0, sizeof buf);
	i = string_escape_mem("a\0b", 3, buf, sizeof buf, ESCAPE_NULL, NULL);
	CK(i == 4 && !memcmp(buf, "a\\0b", 5));
	/* ESCAPE_OCTAL/ESCAPE_HEX escape EVERY byte, printable or not */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("a\x01""b\xff", buf, sizeof buf,
			      ESCAPE_OCTAL, NULL);
	CK(!strcmp(buf, "\\141\\001\\142\\377"));
	memset(buf, 0, sizeof buf);
	i = string_escape_str("a\x01""b\xff", buf, sizeof buf,
			      ESCAPE_HEX, NULL);
	CK(!strcmp(buf, "\\x61\\x01\\x62\\xff"));
	/* ESCAPE_NP: printable passthrough, others escaped via OCTAL.
	 * NOTE: kernel isprint() includes Latin-1 letters, so 0xff
	 * passes through under NP. */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("ab\x7f""c\xff", buf, sizeof buf,
			      ESCAPE_NP | ESCAPE_OCTAL, NULL);
	CK(!strcmp(buf, "ab\\177c\xff"));
	/* ESCAPE_NAP: needs ascii AND printable; 0x80+ always escaped */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("ab\t\xff""z", buf, sizeof buf,
			      ESCAPE_NAP | ESCAPE_HEX, NULL);
	CK(!strcmp(buf, "ab\\x09\\xffz"));
	/* `only` limits the class: 'a' escaped by OCTAL, 'b' literal */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("abc", buf, sizeof buf, ESCAPE_OCTAL, "a");
	CK(!strcmp(buf, "\\141bc"));
	/* ESCAPE_APPEND adds dict chars to the class; note non-dict chars
	 * are NOT passed through under APPEND, so OCTAL escapes all. */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("abc def", buf, sizeof buf,
			      ESCAPE_OCTAL | ESCAPE_APPEND, " ");
	CK(!strcmp(buf, "\\141\\142\\143\\040\\144\\145\\146"));
	/* ESCAPE_APPEND with NP: ' ' also escaped, letters pass through */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("a b", buf, sizeof buf,
			      ESCAPE_NP | ESCAPE_OCTAL | ESCAPE_APPEND, " ");
	CK(!strcmp(buf, "a\\040b"));
	/* truncation: ret is the would-be length */
	memset(buf, 0, sizeof buf);
	i = string_escape_str("abcdef", buf, 3, ESCAPE_SPACE, NULL);
	CK(i == 6 && buf[0] == 'a' && buf[1] == 'b' && buf[2] == 'c');

	/* unescape */
	memset(buf, 0, sizeof buf);
	CK(string_unescape("a\\nb\\tc", buf, 0, UNESCAPE_ANY) == 5 &&
	   !strcmp(buf, "a\nb\tc"));
	memset(buf, 0, sizeof buf);
	CK(string_unescape("\\x41\\x4", buf, 0, UNESCAPE_ANY) == 2 &&
	   !memcmp(buf, "A\x04", 2));
	memset(buf, 0, sizeof buf);
	CK(string_unescape("\\101\\77\\377", buf, 0, UNESCAPE_ANY) == 3 &&
	   !memcmp(buf, "A?\xff", 3));
	memset(buf, 0, sizeof buf);
	CK(string_unescape("\\q", buf, 0, UNESCAPE_ANY) == 2 &&
	   !strcmp(buf, "\\q"));
	memset(buf, 0, sizeof buf);
	CK(string_unescape("a\\", buf, 0, UNESCAPE_ANY) == 2 &&
	   !strcmp(buf, "a\\"));
	strcpy(buf, "a\\tb");
	CK(string_unescape(buf, buf, 0, UNESCAPE_ANY) == 3 &&
	   !strcmp(buf, "a\tb"));

	/* skip_spaces / strim */
	strcpy(buf, "   abc  ");
	CK(skip_spaces(buf) == buf + 3);
	CK(strim(buf) == buf + 3 && !strcmp(buf + 3, "abc"));
	strcpy(buf, "     ");
	CK(strim(buf) == buf && buf[0] == 0);	/* all spaces -> "" */
	strcpy(buf, "x");
	CK(strim(buf) == buf && !strcmp(buf, "x"));

	/* sysfs_streq */
	CK(sysfs_streq("abc", "abc"));
	CK(sysfs_streq("abc", "abc\n"));
	CK(sysfs_streq("abc\n", "abc"));
	CK(!sysfs_streq("abc", "abd"));
	CK(!sysfs_streq("abc", "abcd"));
	CK(!sysfs_streq("abc", "abc\n\n"));
	CK(sysfs_streq("", ""));
	CK(sysfs_streq("", "\n"));
	CK(!sysfs_streq("", "a"));

	/* strreplace */
	strcpy(buf, "a.b.c");
	CK(strreplace(buf, '.', '/') == buf && !strcmp(buf, "a/b/c"));
	strcpy(buf, "aaa");
	strreplace(buf, 'a', 'a');
	CK(!strcmp(buf, "aaa"));

	/* match_string / __sysfs_match_string */
	{
		const char *arr[] = { "alpha", "beta", "gamma", NULL };
		CK(match_string(arr, 3, "beta") == 1);
		CK(match_string(arr, 3, "delta") == -EINVAL);
		CK(match_string(arr, 3, "beta\n") == -EINVAL);
		CK(__sysfs_match_string(arr, 3, "beta\n") == 1);
		CK(__sysfs_match_string(arr, 3, "beta") == 1);
		CK(__sysfs_match_string(arr, 3, "delta\n") == -EINVAL);
		CK(match_string(arr, 1, "beta") == -EINVAL);
	}

	/* memcpy_and_pad */
	memset(buf, 0xa5, 16);
	memcpy_and_pad(buf, 8, "ab", 2, '!');
	CK(!memcmp(buf, "ab!!!!!!", 8));
	memcpy_and_pad(buf, 2, "abcdef", 6, '!');
	CK(!memcmp(buf, "ab", 2));

	/* string_is_terminated / upper / lower */
	CK(string_is_terminated("ab", 3));
	CK(!string_is_terminated("ab", 2));
	strcpy(buf, "aBc1");
	string_upper(buf + 8, "aBc1");
	CK(!strcmp(buf + 8, "ABC1"));
	string_lower(buf + 8, "aBc1");
	CK(!strcmp(buf + 8, "abc1"));

	/* kstrdup_quotable / kstrdup_and_replace */
	p = kstrdup_quotable("a\"b\n", GFP_KERNEL);
	CK(p && !strcmp(p, "a\\x22b\\x0a"));
	kfree(p);
	p = kstrdup_and_replace("a-b", '-', '+', GFP_KERNEL);
	CK(p && !strcmp(p, "a+b"));
	kfree(p);

	if (!fails)
		printf("selftest: all checks passed\n");
	return fails;
}
