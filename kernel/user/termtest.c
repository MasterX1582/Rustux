// termtest — compiled terminal-ABI proof: isatty()/tcgetattr through
// the TCGETS ioctl, tcsetattr round-trip, TIOCGWINSZ grid, TIOCGPGRP,
// FIONREAD on a pipe and a file, and honest ENOTTY/EBADF/EINVAL
// refusals. Same check() convention — exit status = failure count.
#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <termios.h>
#include <sys/ioctl.h>

static int fails;

static void say(const char *s) {
    if (write(1, s, strlen(s)) < 0) { /* ignore */ }
}

static void check(int ok, const char *what) {
    char buf[96];
    int n = snprintf(buf, sizeof(buf), "termtest: %-28s %s\n",
                     what, ok ? "ok" : "FAIL");
    say(buf);
    if (!ok) fails++;
}

int main(void) {
    struct termios tio, tio2;
    struct winsize ws;
    int pfd[2], n;

    say("termtest: terminal abi checks\n");

    /* isatty = TCGETS ioctl under the hood */
    check(isatty(0) == 1, "isatty stdin");
    check(isatty(1) == 1, "isatty stdout");
    int fd = open("motd.txt", O_RDONLY);
    check(fd >= 0, "open motd");
    if (fd >= 0) {
        errno = 0;
        check(isatty(fd) == 0 && errno == ENOTTY,
              "isatty file -> ENOTTY");
    }

    /* termios get/set round-trip on the console fd */
    check(tcgetattr(1, &tio) == 0, "tcgetattr stdout");
    check((tio.c_lflag & ICANON) != 0, "lflag ICANON");
    check((tio.c_lflag & ECHO) != 0, "lflag ECHO");
    check(tio.c_cc[VMIN] == 1 && tio.c_cc[VTIME] == 0,
          "cc VMIN/VTIME");
    tio.c_lflag &= ~(tcflag_t)ECHO;
    check(tcsetattr(1, TCSANOW, &tio) == 0, "tcsetattr ECHO off");
    check(tcgetattr(1, &tio2) == 0 &&
          (tio2.c_lflag & ECHO) == 0, "tcgetattr sees change");
    tio.c_lflag |= ECHO;
    tcsetattr(1, TCSANOW, &tio); /* restore */

    /* winsize: the fbcon grid — nonzero rows/cols */
    check(ioctl(1, TIOCGWINSZ, &ws) == 0 &&
          ws.ws_col > 0 && ws.ws_row > 0, "TIOCGWINSZ grid");

    /* foreground pgrp is our own pid in the single-pgrp model */
    int pgrp = -1;
    check(ioctl(0, TIOCGPGRP, &pgrp) == 0 && pgrp == getpid(),
          "TIOCGPGRP = pid");

    /* FIONREAD: pipe fill + file remainder */
    check(pipe(pfd) == 0, "pipe");
    if (pfd[0] >= 0) {
        n = -1;
        check(ioctl(pfd[0], FIONREAD, &n) == 0 && n == 0,
              "FIONREAD empty pipe");
        if (write(pfd[1], "abcde", 5) == 5) {
            n = -1;
            check(ioctl(pfd[0], FIONREAD, &n) == 0 && n == 5,
                  "FIONREAD 5 queued");
        }
        close(pfd[0]); close(pfd[1]);
    }
    if (fd >= 0) {
        n = -1;
        check(ioctl(fd, FIONREAD, &n) == 0 && n > 0,
              "FIONREAD file remaining");
        close(fd);
    }

    /* refusals */
    n = -1;
    errno = 0;
    check(ioctl(9999, FIONREAD, &n) < 0 && errno == EBADF,
          "ioctl bad fd -> EBADF");
    errno = 0;
    check(ioctl(0, FIONBIO, &n) < 0 && errno == EINVAL,
          "FIONBIO -> EINVAL");

    char buf[64];
    int m = snprintf(buf, sizeof(buf),
                     "termtest: %d failures, done\n", fails);
    (void)m;
    say(buf);
    return fails;
}
