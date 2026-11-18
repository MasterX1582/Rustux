#pragma once
/* linux/seq_file.h shim: seq_buf.h only needs `struct seq_file` for the
 * seq_buf_print_seq() prototype, and seq_buf.c needs declarations for
 * seq_write/d_path/seq_mangle_path so the TU links.  All three are
 * implemented as unreachable stubs in wrappers.c — the harness never
 * calls seq_buf_print_seq (needs a real seq_file) or seq_buf_path
 * (needs the VFS d_path), so any deterministic stub is fine; if one is
 * ever reached it aborts rather than silently diverging. */
#include <linux/types.h>
#include <linux/compiler.h>

struct seq_file;
struct path;

int seq_write(struct seq_file *seq, const void *data, size_t len);
char *d_path(const struct path *path, char *buf, int buflen);
char *seq_mangle_path(char *s, const char *p, const char *esc);
