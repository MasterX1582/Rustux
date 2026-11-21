// Process-lifecycle proof from real code: fork() (glibc -> clone(56)
// -> our fork path), execve("/realbin", argv, envp) (bounded arg
// collection + image swap), waitpid() (wait4 status decode), plus the
// metadata ABI real programs touch: getcwd, access, stat, fstat,
// readlinkat(/proc/self/exe), clock_getres, sched_yield.
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <sys/utsname.h>
#include <sched.h>
#include <time.h>
#include <errno.h>

static int fails = 0;

static void check(int ok, const char *what) {
    char buf[128];
    int n = snprintf(buf, sizeof(buf), "parent: %-28s %s\n", what, ok ? "ok" : "FAIL");
    write(1, buf, n);
    if (!ok) fails++;
}

int main(void) {
    char buf[256];
    int n;

    n = snprintf(buf, sizeof(buf), "parent: start pid=%ld\n", (long)getpid());
    write(1, buf, n);

    // Metadata first — these don't depend on the child.
    check(getcwd(buf, sizeof(buf)) && buf[0] == '/', "getcwd");
    check(access("/realbin", F_OK) == 0, "access realbin");
    check(access("/nonexistent", F_OK) != 0, "access missing ENOENT");

    struct stat st;
    check(stat("/realbin", &st) == 0 && S_ISREG(st.st_mode), "stat realbin");
    check(fstat(1, &st) == 0 && S_ISCHR(st.st_mode), "fstat stdout chr");
    check(lstat("/realbin", &st) == 0 && S_ISREG(st.st_mode), "lstat realbin");

    n = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
    buf[n > 0 ? n : 0] = 0;
    check(n > 0 && strcmp(buf, "/parent") == 0, "readlink self exe");

    struct timespec res;
    check(clock_getres(CLOCK_MONOTONIC, &res) == 0 && res.tv_nsec == 10000000,
          "clock_getres 10ms");
    sched_yield(); // exercise the path; result unchecked by design

    // The lifecycle: fork a child that execs realbin with argv+envp.
    char *args[] = {"realbin", "from-parent", NULL};
    char *env[] = {"RUSTUX_ENV=child", NULL};
    pid_t p = fork();
    if (p < 0) {
        fprintf(stderr, "parent: fork() -> %d\n", errno);
        check(0, "fork");
        return 1;
    }
    if (p == 0) {
        execve("/realbin", args, env);
        _exit(111); // only reached if exec fails
    }
    check(p > 0, "fork child pid");

    int status = 0;
    pid_t w = waitpid(p, &status, 0);
    check(w == p, "waitpid returned child");
    check(WIFEXITED(status), "child exited (not killed)");
    check(WEXITSTATUS(status) == 42, "child status == 42");

    n = snprintf(buf, sizeof(buf), "parent: %d failures, done\n", fails);
    write(1, buf, n);
    return fails;
}
