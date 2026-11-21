// bbtest — real third-party userspace: the Ubuntu busybox-static
// binary (2MiB, glibc-free, static ET_EXEC) exec'd from our initrd
// and driven through pipes. Proves exec of a big foreign ELF,
// multicall argv[0] dispatch, sh -c, cat, and ls applets.
#define _GNU_SOURCE
#include <sys/wait.h>
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

/* Run busybox with the given argv; capture stdout into buf. */
static int bb_run(char *const argv[], char *buf, size_t cap,
                  int *exit_code) {
    int p[2];
    if (pipe(p) < 0) {
        fprintf(stderr, "bb_run: pipe() -> %d\n", errno);
        goto fail;
    }
    pid_t pid = fork();
    if (pid < 0) {
        fprintf(stderr, "bb_run: fork() -> %d\n", errno);
        close(p[0]); close(p[1]);
        goto fail;
    }
    if (pid == 0) {
        dup2(p[1], 1);
        close(p[0]); close(p[1]);
        execve("/busybox", argv, NULL);
        _exit(127);
    }
    close(p[1]);
    size_t n = 0;
    ssize_t r;
    while (n < cap - 1 &&
           (r = read(p[0], buf + n, cap - 1 - n)) > 0)
        n += (size_t)r;
    buf[n] = 0;
    close(p[0]);
    int st = 0;
    waitpid(pid, &st, 0);
    if (exit_code) *exit_code = WIFEXITED(st) ? WEXITSTATUS(st) : -1;
    return (int)n;
fail:
    /* Leave no stale result behind: a failed harness must never
       let a check pass on the previous run's buffer. */
    buf[0] = 0;
    if (exit_code) *exit_code = -1;
    return -1;
}

int main(void) {
    printf("bbtest: busybox-static real-binary checks\n");
    char out[8192];
    int code = -1;

    /* sh -c echo — the multicall sh applet over a pipe. */
    char *a1[] = {"sh", "-c", "echo BB-OK", NULL};
    int n = bb_run(a1, out, sizeof(out), &code);
    check(n > 0 && strstr(out, "BB-OK") != NULL,
          "busybox sh -c 'echo BB-OK'");
    check(code == 0, "sh -c exits 0");

    /* Exit code plumbing: false -> 1, explicit exit 42 -> 42. */
    char *a2[] = {"sh", "-c", "exit 42", NULL};
    bb_run(a2, out, sizeof(out), &code);
    check(code == 42, "sh -c 'exit 42' gives 42");
    char *a3[] = {"false", NULL};
    bb_run(a3, out, sizeof(out), &code);
    check(code == 1, "false applet gives 1");

    /* cat reads through our path layer. */
    char *a4[] = {"cat", "/motd.txt", NULL};
    n = bb_run(a4, out, sizeof(out), &code);
    check(n > 0 && code == 0, "cat /motd.txt exits 0 + bytes");
    int fd = open("/motd.txt", O_RDONLY);
    char want[4096];
    ssize_t wn = read(fd, want, sizeof(want) - 1);
    close(fd);
    want[wn > 0 ? wn : 0] = 0;
    check(wn > 0 && n == (int)wn && memcmp(out, want, wn) == 0,
          "cat output == file bytes");

    /* ls exercises getdents64 + stat from a foreign binary. /lib64
       holds exactly ld.so and nothing else mutates it — a root
       listing can legitimately lose a transient name between
       getdents and stat (wiotest's listed.tmp raced this once:
       "ls: /listed.tmp: No such file or directory" -> exit 1). */
    char *a5[] = {"ls", "/lib64", NULL};
    n = bb_run(a5, out, sizeof(out), &code);
    check(n > 0 && code == 0, "ls /lib64 runs");
    check(strstr(out, "ld-linux-x86-64.so.2") != NULL,
          "ls /lib64 sees ld-linux");
    char *a6[] = {"mkdir", "/bbdir", NULL};
    bb_run(a6, out, sizeof(out), &code);
    char *a7[] = {"ls", "/bbdir", NULL};
    n = bb_run(a7, out, sizeof(out), &code);
    struct stat st;
    check(code == 0 && stat("/bbdir", &st) == 0 &&
          S_ISDIR(st.st_mode), "busybox mkdir -> real VDir");
    char *a8[] = {"rmdir", "/bbdir", NULL};
    bb_run(a8, out, sizeof(out), &code);
    check(access("/bbdir", F_OK) != 0, "busybox rmdir removes it");

    /* A real kernel pipe between sh builtins — read is a
       builtin so no applet re-exec (vfork shared-mm) is needed. */
    char *a9[] = {"sh", "-c",
        "echo bb-pipe | { read v; echo got:$v; }", NULL};
    n = bb_run(a9, out, sizeof(out), &code);
    check(n > 0 && strstr(out, "got:bb-pipe") != NULL,
          "sh pipe: echo | read (builtins)");

    /* Non-builtin applet in a pipe: shellexec re-execs
       /proc/self/exe with argv[0]="cat" — exercises our
       proc-self-exe resolution AND a second pipeline child. */
    char *a10[] = {"sh", "-c", "echo bb-pipe2 | cat", NULL};
    n = bb_run(a10, out, sizeof(out), &code);
    check(n > 0 && strstr(out, "bb-pipe2") != NULL,
          "sh pipe: echo | cat (re-exec via /proc/self/exe)");

    printf("bbtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
