// shelltest — the canonical shell pipeline shape, compiled:
// pipe() -> fork() -> child dup2(w,1)+execve(realbin) -> parent
// reads the child's stdout through the pipe. Exercises pipe OFDs
// shared across fork, dup2 fd replacement surviving exec, blocked
// pipe reads, and waitpid — all with a real glibc program at both
// ends. Same check() convention as parent.c/iotest.c.
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

static int fails;

static void say(const char *s) {
    if (write(1, s, strlen(s)) < 0) { /* ignore */ }
}

static void check(int ok, const char *what) {
    char buf[80];
    int n = snprintf(buf, sizeof(buf), "shelltest: %-28s %s\n",
                     what, ok ? "ok" : "FAIL");
    say(buf);
    if (!ok) fails++;
}

int main(void) {
    say("shelltest: pipe->fork->dup2->execve proof\n");

    int p[2] = {-1, -1};
    check(pipe(p) == 0 && p[0] >= 0 && p[1] == p[0] + 1,
          "pipe create");

    pid_t pid = fork();
    if (pid > 0) check(1, "fork");
    else check(pid == 0, "fork (child sees 0)");
    if (pid == 0) {
        /* child: stdout becomes the pipe write end, like a shell's
           left-hand side — then exec replaces the whole image but
           must keep the redirected fd. */
        if (dup2(p[1], 1) != 1) _exit(111);
        close(p[0]);
        close(p[1]);
        char *a[] = {"realbin", "via-shell", NULL};
        char *e[] = {"RUSTUX_ENV=pipe", NULL};
        execve("realbin", a, e);
        _exit(127); /* exec failed */
    }

    /* parent: the only read end left once the child exits. */
    close(p[1]);
    char all[2048];
    size_t off = 0;
    for (;;) {
        ssize_t n = read(p[0], all + off, sizeof(all) - off - 1);
        if (n <= 0) break;
        off += (size_t)n;
    }
    all[off] = '\0';
    check(off > 0, "parent got pipe bytes");
    check(strstr(all, "realbin:") != NULL, "realbin ran");
    check(strstr(all, "argv1=via-shell") != NULL, "argv survived exec");
    check(strstr(all, "env=pipe") != NULL, "env survived exec");

    int st = -1;
    pid_t w = waitpid(pid, &st, 0);
    check(w == pid, "waitpid");
    check(WIFEXITED(st) && WEXITSTATUS(st) == 42, "child exit 42");

    /* stdin redirect sanity: dup2 a file onto fd 0, read(0) sees it */
    int fd = open("motd.txt", O_RDONLY);
    check(fd >= 0, "open motd for stdin");
    if (fd >= 0) {
        check(dup2(fd, 0) == 0, "dup2 onto stdin");
        char b[6] = {0};
        check(read(0, b, 5) == 5 && memcmp(b, "MOTD:", 5) == 0,
              "stdin redirected");
        close(fd);
    }

    int n = snprintf(all, sizeof(all), "shelltest: %d failures, done\n",
                     fails);
    say(all);
    return fails;
}
