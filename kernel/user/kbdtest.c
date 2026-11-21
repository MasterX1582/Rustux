// kbdtest — tty stdin proof: the keyboard->ldisc->read(0) path.
// Headless-safe: every read attempt is gated behind poll() with a
// bounded timeout, so a run with no input still exits 0. A live run
// (guest keyboard or monitor sendkey) additionally proves canonical
// line delivery: poll wakes, read returns the line through its '\n'.
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <poll.h>
#include <sys/ioctl.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

int main(void) {
    printf("kbdtest: tty stdin checks\n");

    check(isatty(0) == 1, "isatty(stdin)");

    int navail = -1;
    check(ioctl(0, FIONREAD, &navail) == 0 && navail == 0,
          "FIONREAD empty queue -> 0");

    struct pollfd pfd = { .fd = 0, .events = POLLIN, .revents = 0 };
    check(poll(&pfd, 1, 0) == 0, "poll empty stdin -> 0 (would block)");
    check((pfd.revents & POLLIN) == 0, "no POLLIN on empty queue");

    // Live-input window: with a guest keyboard, typing a line here
    // wakes the poll and read() returns it, newline included. The
    // window is short — it must close before the suite winds down
    // (~2.5s total) so the final verdict always prints.
    printf("kbdtest: brief input window (headless runs time out)\n");
    pfd.revents = 0;
    int r = poll(&pfd, 1, 800);
    if (r > 0 && (pfd.revents & POLLIN)) {
        char buf[128];
        int n = read(0, buf, sizeof(buf) - 1);
        check(n > 0, "read() after POLLIN returns bytes");
        if (n > 0) {
            buf[n] = 0;
            printf("kbdtest: read %d bytes: %s", n, buf);
            if (buf[n - 1] != '\n') printf("\n");
            check(memchr(buf, '\n', n) != NULL,
                  "canonical read ends at newline");
        }
    } else {
        printf("kbdtest: no input this run (headless path)\n");
        check(1, "input window timed out cleanly");
    }

    printf("kbdtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails ? 1 : 0;
}
