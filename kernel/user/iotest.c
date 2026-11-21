// iotest — compiled file-I/O ABI proof: the syscalls a real glibc
// program actually uses for file work: openat(257) behind fopen,
// pread64(17), lseek(8), fcntl(72) FD flags, dup3(292), and the
// archive's honest EROFS/EINVAL answers. Same check() convention
// as parent.c — exit status is the failure count.
// motd.txt = "MOTD: greetings from the Rustux initrd filesystem\r\n"
// so "greetings" sits at byte 6 and "from" at byte 16.
#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

static int fails;

static void say(const char *s) {
    if (write(1, s, strlen(s)) < 0) { /* ignore */ }
}

static void check(int ok, const char *what) {
    char buf[80];
    int n = snprintf(buf, sizeof(buf), "iotest: %-28s %s\n",
                     what, ok ? "ok" : "FAIL");
    say(buf);
    if (!ok) fails++;
}

int main(void) {
    char buf[128];

    say("iotest: file-io abi checks\n");

    /* fopen -> openat(AT_FDCWD, "motd.txt", O_RDONLY|O_CLOEXEC) */
    FILE *f = fopen("motd.txt", "r");
    check(f != NULL, "fopen motd.txt");
    if (f) {
        char line[64] = {0};
        check(fgets(line, sizeof(line), f) != NULL &&
              strstr(line, "greetings") != NULL, "fgets motd line");
        check(fclose(f) == 0, "fclose");
    }

    /* openat + O_CLOEXEC + pread (no offset movement) */
    int fd = openat(AT_FDCWD, "motd.txt", O_RDONLY | O_CLOEXEC);
    check(fd >= 0, "openat O_CLOEXEC");
    if (fd >= 0) {
        memset(buf, 0, sizeof(buf));
        long n = pread(fd, buf, 9, 6);
        check(n == 9 && memcmp(buf, "greetings", 9) == 0, "pread offset 6");
        /* pread must NOT have moved the shared position */
        check(lseek(fd, 0, SEEK_CUR) == 0, "pread keeps pos");
        /* lseek SET/CUR/END + read */
        check(lseek(fd, 16, SEEK_SET) == 16, "lseek SEEK_SET");
        memset(buf, 0, sizeof(buf));
        n = read(fd, buf, 4);
        check(n == 4 && memcmp(buf, "from", 4) == 0, "read after lseek");
        check(lseek(fd, -1, SEEK_END) > 0, "lseek SEEK_END");

        /* fcntl: F_GETFD sees the CLOEXEC bit set at open */
        check(fcntl(fd, F_GETFD) == FD_CLOEXEC, "fcntl GETFD cloexec");
        check(fcntl(fd, F_SETFD, 0) == 0, "fcntl SETFD clear");
        check(fcntl(fd, F_GETFD) == 0, "fcntl GETFD clear");
        check(fcntl(fd, F_GETFL) == O_RDONLY, "fcntl GETFL rdonly");

        /* dup3 O_CLOEXEC: new fd carries the bit */
        int fd2 = dup3(fd, fd + 1, O_CLOEXEC);
        check(fd2 == fd + 1, "dup3 slot");
        check(fd2 >= 0 && fcntl(fd2, F_GETFD) == FD_CLOEXEC,
              "dup3 keeps cloexec");
        /* plain dup must NOT inherit the bit */
        int fd3 = dup(fd);
        check(fd3 >= 0 && fcntl(fd3, F_GETFD) == 0, "dup clears cloexec");
        close(fd2); close(fd3); close(fd);
    }

    /* openat relative through a real Dir dirfd (".") */
    int dfd = open(".", O_RDONLY | O_DIRECTORY);
    check(dfd >= 0, "open dir O_DIRECTORY");
    if (dfd >= 0) {
        int r = openat(dfd, "motd.txt", O_RDONLY);
        check(r >= 0, "openat via dirfd");
        if (r >= 0) close(r);
        close(dfd);
    }

    /* honest refusals */
    errno = 0;
    check(open("definitely-missing", O_RDONLY) < 0 && errno == ENOENT,
          "missing -> ENOENT");
    errno = 0;
    check(open("motd.txt", O_WRONLY) < 0 && errno == EROFS,
          "write open -> EROFS");
    errno = 0;
    check(openat(AT_FDCWD, "motd.txt", O_RDONLY | 0x2000) < 0 &&
          errno == EINVAL, "unknown flag -> EINVAL");
    check(pread(9999, buf, 1, 0) < 0, "pread bad fd");

    int n = snprintf(buf, sizeof(buf), "iotest: %d failures, done\n", fails);
    say(buf);
    return fails;
}
