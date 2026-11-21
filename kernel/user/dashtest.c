// dashtest — a REAL Debian userspace shell: the Ubuntu dash
// package's /bin/dash (dynamic, PT_INTERP -> glibc ld.so) exec'd
// from our initrd and driven through pipes. Exercises vfork for
// real (dash forks via vfork), builtins, pipelines, substitution,
// redirection into the VFile layer, glob, and exec of other tasks.
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
    printf("  %-52s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

/* Run dash -c <script>; capture stdout into buf, return byte count. */
static int sh_run(const char *script, char *buf, size_t cap,
                  int *exit_code) {
    int p[2];
    if (pipe(p) < 0) {
        fprintf(stderr, "sh_run: pipe() -> %d\n", errno);
        goto fail;
    }
    pid_t pid = fork();
    if (pid < 0) {
        fprintf(stderr, "sh_run: fork() -> %d\n", errno);
        close(p[0]); close(p[1]);
        goto fail;
    }
    if (pid == 0) {
        dup2(p[1], 1);
        dup2(p[1], 2);
        close(p[0]); close(p[1]);
        char *argv[] = {"dash", "-c", (char *)script, NULL};
        execve("/bin/dash", argv, NULL);
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
    printf("dashtest: real Debian dash (dynamic) checks\n");
    char out[8192];
    int code = -1;

    /* The simplest possible proof: dash starts, echoes, exits 0. */
    int n = sh_run("echo DASH-OK", out, sizeof(out), &code);
    check(n > 0 && strstr(out, "DASH-OK") != NULL,
          "dash -c 'echo DASH-OK'");
    check(code == 0, "dash -c exits 0");

    /* Exit status plumbing — dash propagates builtin codes. */
    sh_run("exit 42", out, sizeof(out), &code);
    check(code == 42, "dash -c 'exit 42' gives 42");
    sh_run("false", out, sizeof(out), &code);
    check(code == 1, "false builtin gives 1");
    sh_run("true", out, sizeof(out), &code);
    check(code == 0, "true builtin gives 0");

    /* Variable assignment + expansion, arithmetic, conditionals. */
    n = sh_run("x=hello; echo $x; echo $((20+22))", out, sizeof(out), &code);
    check(strstr(out, "hello") != NULL && strstr(out, "42"),
          "var assign + arithmetic");
    n = sh_run("if true; then echo YES; else echo NO; fi",
               out, sizeof(out), &code);
    check(strstr(out, "YES") != NULL && !strstr(out, "NO"),
          "if/then/else");
    n = sh_run("for i in a b c; do echo $i; done", out, sizeof(out), &code);
    check(strstr(out, "a") != NULL && strstr(out, "b") && strstr(out, "c"),
          "for loop");

    /* Command substitution — dash forks a child into a pipe and
       reads its output back. The whole vfork->exec->wait4 pipe
       path in one builtin. */
    n = sh_run("echo sub-$(echo inner)-done", out, sizeof(out), &code);
    check(strstr(out, "sub-inner-done") != NULL, "command substitution");

    /* Builtin pipeline: echo feeds read inside a brace group —
       kernel pipe carries bytes between two vfork children. */
    n = sh_run("echo piped | { read x; echo got-$x; }",
               out, sizeof(out), &code);
    check(strstr(out, "got-piped") != NULL, "echo | read via pipe");

    /* test/[ builtin + a real file from the archive. */
    n = sh_run("[ -f /motd.txt ] && echo isfile; [ -f /nope ] || echo nofile",
               out, sizeof(out), &code);
    check(strstr(out, "isfile") != NULL && strstr(out, "nofile"),
          "test builtin + file stat");

    /* Redirection into the VFile layer: '>' creates, '<' reads. */
    n = sh_run("echo stored > /sh_tmp; read y < /sh_tmp; echo $y",
               out, sizeof(out), &code);
    check(strstr(out, "stored") != NULL, "redirect > file < read-back");
    unlink("/sh_tmp");

    /* Glob: pathname expansion through opendir/getdents. */
    n = sh_run("echo /mot*", out, sizeof(out), &code);
    check(strstr(out, "/motd.txt") != NULL, "glob /mot* -> motd.txt");

    /* pwd/cd through our cwd layer. */
    n = sh_run("pwd", out, sizeof(out), &code);
    check(strstr(out, "/") != NULL, "pwd");

    /* exec of a real sibling binary — dash resolves a PATH-less
       absolute exec through the same execve path. */
    n = sh_run("/init", out, sizeof(out), &code);
    check(strstr(out, "hello") != NULL && code == 0,
          "exec /init external command");

    printf("dashtest: %d failures, done\n", fails);
    return fails;
}
