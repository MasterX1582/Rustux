/* Invariant self-test (not part of the shipped port): hand-computed
 * expectations for the seq_buf edge cases — 4-byte buffer overflow at
 * exactly capacity, strscpy tail, hex formatting, commit/pop/to_user. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/sprintf.h>
#include <linux/seq_buf.h>
#include <linux/seq_file.h>
#include <linux/printk.h>
#include <linux/uaccess.h>
#include <linux/errno.h>

static char klog[4096];
static size_t klog_len;
uintptr_t kshim_ptr_base;
size_t kshim_uaccess_ok;

int kshim_printk(const char *fmt, ...)
{
	va_list ap;
	size_t rem = sizeof(klog) - klog_len;
	int ret;
	va_start(ap, fmt);
	ret = kshim_vsnprintf(klog + klog_len, rem, fmt, ap);
	va_end(ap);
	klog_len += (size_t)ret < rem ? (size_t)ret : (rem ? rem - 1 : 0);
	return ret;
}
int seq_write(struct seq_file *m, const void *v, size_t len)
{ (void)m; (void)v; return (int)len; }
char *d_path(const struct path *path, char *buf, int buflen)
{ (void)path; if (buflen > 0) buf[0] = '\0'; return buf; }
char *seq_mangle_path(char *sp, const char *p, const char *esc)
{ (void)p; (void)esc; return sp; }

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); if (fails > 20) exit(1); } } while (0)

int main(void)
{
	static char buf[64];
	static char ub[32];
	struct seq_buf s;
	char *p;
	int ret;

	kshim_ptr_base = 0;
	kshim_uaccess_ok = (size_t)-1;

	/* ---- 4-byte buffer, putc at exactly capacity ---- */
	memset(buf, 'X', sizeof buf);
	seq_buf_init(&s, buf, 4);
	CHECK(s.len == 0 && s.size == 4, "init");
	CHECK(seq_buf_putc(&s, 'a') == 0, "putc a");
	CHECK(seq_buf_putc(&s, 'b') == 0, "putc b");
	CHECK(seq_buf_putc(&s, 'c') == 0, "putc c");
	CHECK(seq_buf_putc(&s, 'd') == 0, "putc d (exact fit)");
	CHECK(s.len == 4 && !seq_buf_has_overflowed(&s), "len==size");
	CHECK(seq_buf_putc(&s, 'e') == -1, "putc past capacity");
	CHECK(s.len == 5 && seq_buf_has_overflowed(&s), "len==size+1");
	CHECK(seq_buf_buffer_left(&s) == 0 && seq_buf_used(&s) == 4,
	      "left/used");
	CHECK(!memcmp(buf, "abcd", 4), "content");
	CHECK(seq_buf_pop(&s) == -1, "pop while overflowed");

	/* ---- puts: success then strscpy truncation ---- */
	seq_buf_init(&s, buf, 4);
	CHECK(seq_buf_puts(&s, "xy") == 0, "puts xy");
	CHECK(s.len == 2, "len after puts");	/* NUL not counted */
	CHECK(seq_buf_puts(&s, "zz") == -1, "puts zz overflow");
	/* strscpy wrote 'z' + NUL into the last two bytes */
	CHECK(!memcmp(buf, "xyz\0", 4), "truncated tail %02x %02x %02x %02x",
	      buf[0], buf[1], buf[2], buf[3]);
	CHECK(s.len == 5, "len after overflow");

	/* ---- putmem exact fit, partial copy ---- */
	seq_buf_init(&s, buf, 4);
	CHECK(seq_buf_putmem(&s, "ABCD", 4) == 0, "putmem exact");
	CHECK(!memcmp(buf, "ABCD", 4), "putmem content");
	CHECK(seq_buf_putmem(&s, "WXYZ", 4) == -1, "putmem full");
	CHECK(s.len == 5, "putmem overflow len");

	/* ---- get_buf + commit ---- */
	seq_buf_init(&s, buf, 6);
	CHECK(seq_buf_get_buf(&s, &p) == 6 && p == buf, "get_buf");
	p[0] = 'h'; p[1] = 'i';
	seq_buf_commit(&s, 2);
	CHECK(s.len == 2, "commit len");
	seq_buf_commit(&s, -1);
	CHECK(s.len == 7 && seq_buf_has_overflowed(&s), "commit -1");
	seq_buf_clear(&s);
	CHECK(s.len == 0 && !seq_buf_has_overflowed(&s), "clear");
	CHECK(seq_buf_pop(&s) == -1, "pop empty");

	/* ---- set_overflow zeroes the unclaimed tail ---- */
	seq_buf_init(&s, buf, 8);
	seq_buf_putmem(&s, "ab", 2);
	memset(buf + 2, 0x7f, 6);
	seq_buf_set_overflow(&s);
	CHECK(s.len == 9, "set_overflow len");
	CHECK(!memcmp(buf, "ab\0\0\0\0\0\0", 8), "overflow zeroed tail");

	/* ---- putmem_hex byte order: little-endian reverses in-group ---- */
	seq_buf_init(&s, buf, 64);
	CHECK(seq_buf_putmem_hex(&s, "\x61\x62\x63\x64", 4) == 0,
	      "putmem_hex");
	CHECK(!memcmp(buf, "64636261 ", 9), "memhex %.*s", 9, buf);
	CHECK(s.len == 9, "memhex len");

	/* ---- printf via the deterministic mini-printer ---- */
	seq_buf_init(&s, buf, 64);
	CHECK(seq_buf_printf(&s, "%d|%05x|%-4s|", -42, 0xab1, "xy") == 0,
	      "printf ret");
	CHECK(!strcmp(buf, "-42|00ab1|xy  |"), "printf got '%s'", buf);
	CHECK(s.len == 15, "printf len %zu", s.len);
	seq_buf_init(&s, buf, 8);
	CHECK(seq_buf_printf(&s, "%s", "0123456789") == -1, "printf ovf");
	/* vsnprintf truncated to 7 chars + NUL at buf[7] */
	CHECK(s.len == 9 && !memcmp(buf, "0123456\0", 8), "printf trunc");

	/* ---- hex_dump: 16B row, 1-byte groups, ascii ---- */
	seq_buf_init(&s, buf, 64);
	{
		/* "P: " + "61 62 63 64" + pad to ascii column 49 + "abcd" + \n */
		char exp[64];
		memcpy(exp, "P: 61 62 63 64", 14);
		memset(exp + 14, ' ', 38);
		memcpy(exp + 52, "abcd\n\0", 6);
		CHECK(seq_buf_hex_dump(&s, "P: ", DUMP_PREFIX_NONE,
				       16, 1, "abcd", 4, true) == 0,
		      "hex_dump ret");
		CHECK(!strcmp(buf, exp), "hex_dump '%s'", buf);
	}
	/* offset prefix, groupsize 4, no ascii */
	seq_buf_init(&s, buf, 64);
	CHECK(seq_buf_hex_dump(&s, "", DUMP_PREFIX_OFFSET, 16, 4,
			       "abcdefgh", 8, false) == 0, "hd2 ret");
	CHECK(!strcmp(buf, "00000000: 64636261 68676665\n"),
	      "hd2 '%s'", buf);

	/* ---- to_user: success, partial fault, EBUSY, EFAULT ---- */
	seq_buf_init(&s, buf, 8);
	seq_buf_putmem(&s, "hello", 5);
	memset(ub, 0, sizeof ub);
	CHECK(seq_buf_to_user(&s, ub, 0, 5) == 5, "to_user n");
	CHECK(!memcmp(ub, "hello", 5), "to_user bytes");
	CHECK(seq_buf_to_user(&s, ub, 3, 10) == 2, "to_user tail");
	CHECK(seq_buf_to_user(&s, ub, 5, 3) == -EBUSY, "to_user EBUSY");
	CHECK(seq_buf_to_user(&s, ub, 0, 0) == 0, "to_user cnt 0");
	kshim_uaccess_ok = 0;
	CHECK(seq_buf_to_user(&s, ub, 0, 3) == -EFAULT, "to_user EFAULT");
	kshim_uaccess_ok = 2;
	CHECK(seq_buf_to_user(&s, ub, 0, 5) == 2, "to_user partial");
	kshim_uaccess_ok = (size_t)-1;

	/* ---- init_append: with and without in-bounds NUL ---- */
	memset(buf, 'z', 8);
	buf[3] = '\0';
	seq_buf_init_append(&s, buf, 8);
	CHECK(s.len == 3 && !seq_buf_has_overflowed(&s), "init_append nul");
	memset(buf, 'z', 8);
	seq_buf_init_append(&s, buf, 8);
	CHECK(s.len == 9 && seq_buf_has_overflowed(&s), "init_append noNUL");

	/* ---- str/strlen/terminate on full buffer ---- */
	seq_buf_init(&s, buf, 4);
	seq_buf_putmem(&s, "wxyz", 4);
	CHECK(seq_buf_strlen(&s) == 3, "strlen full");
	CHECK(buf[3] == '\0' && s.len == 4, "strlen overwrote last byte");
	seq_buf_init(&s, buf, 0);
	CHECK(seq_buf_str(&s)[0] == '\0' && seq_buf_strlen(&s) == 0,
	      "size 0 str");
	CHECK(seq_buf_putc(&s, 'q') == -1 && s.len == 1, "size0 putc");
	CHECK(seq_buf_buffer_left(&s) == 0 && seq_buf_used(&s) == 0,
	      "size0 left/used");

	/* ---- do_printk into the klog ---- */
	seq_buf_init(&s, buf, 16);
	seq_buf_puts(&s, "aa\nbb\ncc");
	klog_len = 0; klog[0] = 0;
	seq_buf_do_printk(&s, "L>");
	CHECK(!strncmp(klog, "L>aa\nL>bb\nL>cc\n", 15), "printk '%.*s'",
	      15, klog);

	if (fails) {
		printf("selftest: %d FAILURES\n", fails);
		return 1;
	}
	printf("selftest: all checks passed\n");
	return 0;
}
