// fmaptest — file-backed mmap checks: MAP_PRIVATE pages are
// eager-filled private copies — content matches the file, writes
// don't reach the file, offsets index into it, past-EOF is zeros,
// and the honesty table is sharp: MAP_SHARED -> ENODEV, unaligned
// off -> EINVAL, write-only fd -> EACCES, non-regular -> ENODEV.
#define _GNU_SOURCE
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>

static int fails = 0;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

int main(void) {
    printf("fmaptest: file-backed mmap checks\n");
    struct stat st;
    char buf[8192];
    int fd = open("/motd.txt", O_RDONLY);
    check(fd >= 0 && fstat(fd, &st) == 0 && st.st_size > 0,
          "open + fstat motd.txt");
    size_t sz = st.st_size;

    /* Content: mmap bytes equal read() bytes. */
    void *m = mmap(NULL, sz, PROT_READ, MAP_PRIVATE, fd, 0);
    check(m != MAP_FAILED, "mmap file MAP_PRIVATE");
    int rd = open("/motd.txt", O_RDONLY);
    ssize_t n = read(rd, buf, sizeof(buf) - 1);
    check(n == (ssize_t)sz && memcmp(m, buf, sz) == 0,
          "mmap content == file content");
    close(rd);

    /* MAP_PRIVATE write is a private copy — file unchanged. */
    void *mw = mmap(NULL, sz, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0);
    check(mw != MAP_FAILED, "mmap PROT_WRITE private");
    char first = ((char *)mw)[0];
    ((char *)mw)[0] = 'X';
    check(((char *)mw)[0] == 'X', "private write lands in map");
    char c = 0;
    check(pread(fd, &c, 1, 0) == 1 && c == first,
          "file byte unchanged after private write");
    munmap(mw, sz);

    /* A >4K RAM file: offset mapping reads file+off. */
    int wf = open("/fmap.tmp", O_CREAT | O_RDWR, 0644);
    check(wf >= 0, "create /fmap.tmp");
    for (int i = 0; i < 5000; i++) buf[i] = 'a' + (i % 26);
    check(write(wf, buf, 5000) == 5000, "write 5000 pattern bytes");
    void *mo = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, wf, 4096);
    check(mo != MAP_FAILED, "mmap with off=4096");
    check(mo != MAP_FAILED &&
          memcmp(mo, buf + 4096, 5000 - 4096) == 0 &&
          ((char *)mo)[5000 - 4096] == 0,
          "off content matches + tail zeroed");
    munmap(mo, 4096);

    /* Past-EOF: whole tail reads as zeros (documented: Linux SIGBUS). */
    void *me = mmap(NULL, 8192, PROT_READ, MAP_PRIVATE, fd, 0);
    check(me != MAP_FAILED, "mmap len > file size");
    int zero_tail = 1;
    for (size_t i = sz; i < 8192 && zero_tail; i++)
        zero_tail &= (((char *)me)[i] == 0);
    check(zero_tail, "past-EOF bytes are zeros");
    munmap(me, 8192);

    /* Honesty table. */
    errno = 0;
    check(mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0) == MAP_FAILED &&
          errno == ENODEV, "MAP_SHARED -> ENODEV (no writeback)");
    errno = 0;
    check(mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, fd, 100) == MAP_FAILED &&
          errno == EINVAL, "unaligned off -> EINVAL");
    int wo = open("/fmap.tmp", O_WRONLY);
    errno = 0;
    check(wo >= 0 &&
          mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, wo, 0) == MAP_FAILED &&
          errno == EACCES, "O_WRONLY fd -> EACCES");
    close(wo);
    int dd = open("/", O_RDONLY | O_DIRECTORY);
    errno = 0;
    check(dd >= 0 &&
          mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, dd, 0) == MAP_FAILED &&
          errno == ENODEV, "dir fd -> ENODEV");
    close(dd);
    int pp[2];
    errno = 0;
    check(pipe(pp) == 0 &&
          mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, pp[0], 0) == MAP_FAILED &&
          errno == ENODEV, "pipe fd -> ENODEV");
    close(pp[0]); close(pp[1]);
    errno = 0;
    check(mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, 99, 0) == MAP_FAILED &&
          errno == EBADF, "bad fd -> EBADF");

    /* MAP_FIXED over an existing file map replaces it. */
    void *hint = mmap(NULL, 8192, PROT_READ, MAP_PRIVATE, fd, 0);
    check(hint != MAP_FAILED, "hint map for FIXED test");
    void *fx = mmap(hint, 4096, PROT_READ, MAP_PRIVATE | MAP_FIXED, wf, 0);
    check(fx == hint && memcmp(fx, buf, 4096) == 0,
          "MAP_FIXED replaces + refills");
    munmap(hint, 4096);
    munmap(m, sz);
    unlink("/fmap.tmp");
    close(wf); close(fd);

    printf("fmaptest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
