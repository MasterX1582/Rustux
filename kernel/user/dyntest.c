// dyntest — first DYNAMIC userspace: a normal `gcc` binary with
// PT_INTERP /lib64/ld-linux-x86-64.so.2. The kernel maps ld.so into
// the mmap arena, starts at its entry, and glibc's loader maps
// libc.so.6 itself. getauxval(AT_BASE) nonzero proves the interp was
// mapped; gnu_get_libc_version proves libc init actually ran.
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/auxv.h>
#include <sys/wait.h>
#include <gnu/libc-version.h>

static int fails = 0;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

int main(int argc, char **argv) {
    /* exec'd child path: prove re-exec of a dynamic binary works
       from inside a dynamic process. */
    if (argc > 1 && !strcmp(argv[1], "--child"))
        return 7;

    printf("dyntest: dynamic ld.so + glibc checks\n");

    check(getauxval(AT_BASE) != 0, "AT_BASE names the interp base");
    check(getauxval(AT_ENTRY) != 0, "AT_ENTRY names the image entry");
    check(getauxval(AT_PHDR) != 0 && getauxval(AT_PHNUM) > 0,
          "AT_PHDR/AT_PHNUM set");
    const char *v = gnu_get_libc_version();
    check(v && v[0] >= '2', "gnu_get_libc_version()");

    char *p = malloc(1 << 20);
    check(p != NULL, "malloc 1MiB");
    if (p) {
        memset(p, 0xA5, 1 << 20);
        check(p[0] == (char)0xA5 && p[(1 << 20) - 1] == (char)0xA5,
              "heap bytes read back");
        free(p);
    }

    FILE *f = fopen("/motd.txt", "r");
    check(f != NULL, "fopen /motd.txt through libc");
    if (f) {
        char buf[64];
        check(fgets(buf, sizeof buf, f) != NULL, "fgets a line");
        fclose(f);
    }

    /* fork + execve re-exercises the whole interp path in a child:
       fresh mm, ld.so mapped again, exit code plumbed back. */
    pid_t pid = fork();
    if (pid == 0) {
        execl("/dyntest", "dyntest", "--child", NULL);
        _exit(127);
    }
    int st = 0;
    check(pid > 0 && waitpid(pid, &st, 0) == pid,
          "fork+waitpid a dynamic child");
    check(WIFEXITED(st) && WEXITSTATUS(st) == 7,
          "child exec'd /dyntest (exit 7)");

    printf("dyntest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
