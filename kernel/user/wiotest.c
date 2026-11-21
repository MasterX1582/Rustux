// wiotest — compiled writable-file ABI proof: creat(85)/open with
// O_CREAT·O_EXCL·O_TRUNC·O_APPEND, write/read on a RAM-backed file,
// truncate(76)/ftruncate(77) incl. zero-fill, rename(82), unlink(87)
// with open-unlink-survives semantics, fsync(74), and the honest
// refusals (EROFS on the archive, EEXIST, EBADF on wrong accmode).
// Same check() convention as iotest — exit status = failure count.
#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <sys/syscall.h>

static int fails;

static void say(const char *s) {
    if (write(1, s, strlen(s)) < 0) { /* ignore */ }
}

static void check(int ok, const char *what) {
    char buf[96];
    int n = snprintf(buf, sizeof(buf), "wiotest: %-30s %s\n",
                     what, ok ? "ok" : "FAIL");
    say(buf);
    if (!ok) fails++;
}

int main(void) {
    char buf[64];
    struct stat st;

    say("wiotest: writable-file abi checks\n");

    /* creat + write + close + read back */
    int fd = creat("scratch.tmp", 0644);
    check(fd >= 0, "creat scratch.tmp");
    if (fd >= 0) {
        check(write(fd, "hello", 5) == 5, "write hello");
        errno = 0;
        check(read(fd, buf, 1) < 0 && errno == EBADF,
              "read O_WRONLY -> EBADF");
        check(fsync(fd) == 0, "fsync");
        check(close(fd) == 0, "close");
    }
    fd = open("scratch.tmp", O_RDONLY);
    check(fd >= 0, "reopen O_RDONLY");
    if (fd >= 0) {
        memset(buf, 0, sizeof(buf));
        check(read(fd, buf, 5) == 5 && memcmp(buf, "hello", 5) == 0,
              "read back hello");
        errno = 0;
        check(write(fd, "x", 1) < 0 && errno == EBADF,
              "write O_RDONLY -> EBADF");
        check(fstat(fd, &st) == 0 && st.st_size == 5, "fstat size 5");
        close(fd);
    }

    /* O_RDWR|O_TRUNC resets; O_APPEND lands at EOF every time */
    fd = open("scratch.tmp", O_RDWR | O_TRUNC);
    check(fd >= 0, "O_TRUNC open");
    if (fd >= 0) {
        check(write(fd, "abc", 3) == 3, "write abc");
        close(fd);
        fd = open("scratch.tmp", O_WRONLY | O_APPEND);
        check(fd >= 0, "O_APPEND open");
        if (fd >= 0) {
            /* pos starts at 0; append must still land at EOF */
            check(write(fd, "def", 3) == 3, "append def");
            close(fd);
        }
        fd = open("scratch.tmp", O_RDONLY);
        if (fd >= 0) {
            memset(buf, 0, sizeof(buf));
            check(read(fd, buf, 6) == 6 &&
                  memcmp(buf, "abcdef", 6) == 0, "append landed at EOF");
            close(fd);
        }
    }

    /* O_CREAT|O_EXCL on an existing file -> EEXIST */
    errno = 0;
    check(open("scratch.tmp", O_CREAT | O_EXCL | O_WRONLY, 0644) < 0 &&
          errno == EEXIST, "O_EXCL -> EEXIST");

    /* ftruncate shrink + grow zero-fill */
    fd = open("scratch.tmp", O_RDWR);
    check(fd >= 0, "open O_RDWR");
    if (fd >= 0) {
        check(ftruncate(fd, 3) == 0, "ftruncate shrink 3");
        check(ftruncate(fd, 8) == 0, "ftruncate grow 8");
        memset(buf, 0xAA, sizeof(buf));
        check(pread(fd, buf, 8, 0) == 8 &&
              memcmp(buf, "abc\0\0\0\0\0", 8) == 0,
              "grow zero-fills gap");
        close(fd);
    }
    /* truncate(2) by path */
    check(truncate("scratch.tmp", 2) == 0, "truncate path 2");
    check(stat("scratch.tmp", &st) == 0 && st.st_size == 2,
          "stat size 2 after truncate");

    /* rename: old name dies, new name carries the bytes */
    check(rename("scratch.tmp", "moved.tmp") == 0, "rename");
    errno = 0;
    check(stat("scratch.tmp", &st) < 0 && errno == ENOENT,
          "old name gone");
    check(stat("moved.tmp", &st) == 0 && st.st_size == 2,
          "new name stat");

    /* open-unlink-still-works: the OFD outlives the name */
    fd = open("moved.tmp", O_RDONLY);
    check(fd >= 0, "open before unlink");
    check(unlink("moved.tmp") == 0, "unlink moved.tmp");
    if (fd >= 0) {
        memset(buf, 0, sizeof(buf));
        check(read(fd, buf, 2) == 2 && memcmp(buf, "ab", 2) == 0,
              "unlinked fd still reads");
        close(fd);
    }
    errno = 0;
    check(stat("moved.tmp", &st) < 0 && errno == ENOENT,
          "unlinked -> ENOENT");

    /* refusals: archive is EROFS, missing is ENOENT */
    errno = 0;
    check(unlink("motd.txt") < 0 && errno == EROFS,
          "unlink archive -> EROFS");
    errno = 0;
    check(rename("motd.txt", "x.tmp") < 0 && errno == EROFS,
          "rename archive -> EROFS");
    errno = 0;
    check(unlink("nonexistent.tmp") < 0 && errno == ENOENT,
          "unlink missing -> ENOENT");

    /* a fresh O_CREAT file shows up in the directory stream */
    fd = open("listed.tmp", O_CREAT | O_WRONLY, 0644);
    check(fd >= 0, "creat listed.tmp");
    if (fd >= 0) close(fd);
    check(access("listed.tmp", F_OK) == 0, "access finds new file");
    {
        int d = open(".", O_RDONLY | O_DIRECTORY);
        char dbuf[1024];
        int found = 0;
        if (d >= 0) {
            long got = syscall(SYS_getdents64, d, dbuf, sizeof(dbuf));
            for (long off = 0; off + 19 <= got; ) {
                struct { unsigned long ino, off; unsigned short len;
                         unsigned char type; char name[]; } *e =
                    (void *)(dbuf + off);
                if (strcmp(e->name, "listed.tmp") == 0) found = 1;
                off += e->len;
            }
            close(d);
        }
        check(found, "getdents lists new file");
    }
    check(unlink("listed.tmp") == 0, "cleanup listed.tmp");

    int n = snprintf(buf, sizeof(buf), "wiotest: %d failures, done\n",
                     fails);
    say(buf);
    return fails;
}
