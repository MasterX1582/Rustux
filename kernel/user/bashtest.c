// bashtest — a REAL GNU userspace shell: the Ubuntu bash 5.2
// package's /usr/bin/bash (dynamic, PT_INTERP -> glibc ld.so,
// plus a second DT_NEEDED libtinfo.so.6 — ld.so's first
// multi-library resolution) exec'd from our initrd and driven
// through pipes. Exercises the dash surface plus bash-only
// features: brace expansion, arrays, [[ ]], functions, here-
// strings, traps, parameter expansion, and the wait builtin.
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

/* Run bash -c <script>; capture stdout+stderr into buf. */
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
        char *argv[] = {"bash", "-c", (char *)script, NULL};
        execve("/bin/bash", argv, NULL);
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
    buf[0] = 0;
    if (exit_code) *exit_code = -1;
    return -1;
}

int main(void) {
    printf("bashtest: real GNU bash (dynamic, +libtinfo) checks\n");
    char out[8192];
    int code = -1;

    /* Proof of life: bash starts, echoes, exits 0. */
    int n = sh_run("echo BASH-OK", out, sizeof(out), &code);
    check(n > 0 && strstr(out, "BASH-OK") != NULL,
          "bash -c 'echo BASH-OK'");
    check(code == 0, "bash -c exits 0");

    /* Exit status plumbing. */
    sh_run("exit 42", out, sizeof(out), &code);
    check(code == 42, "bash -c 'exit 42' gives 42");
    sh_run("false", out, sizeof(out), &code);
    check(code == 1, "false builtin gives 1");

    /* Brace expansion — bash-only, dash can't do it. */
    n = sh_run("echo x{a,b,c}y", out, sizeof(out), &code);
    check(strstr(out, "xay") != NULL && strstr(out, "xby") != NULL &&
          strstr(out, "xcy") != NULL, "brace expansion {a,b,c}");

    /* [[ ]] conditional — bash's own evaluator. */
    n = sh_run("[[ -f /motd.txt ]] && echo isfile; "
               "[[ hello == h* ]] && echo globmatch",
               out, sizeof(out), &code);
    check(strstr(out, "isfile") != NULL && strstr(out, "globmatch"),
          "[[ file test + pattern match ]]");

    /* Indexed arrays. */
    n = sh_run("a=(alpha beta gamma); echo ${a[1]} ${a[2]} ${#a[@]}",
               out, sizeof(out), &code);
    check(strstr(out, "beta gamma 3") != NULL, "indexed arrays");

    /* Functions + local. */
    n = sh_run("f() { local v=inner; echo fn-$v; }; f",
               out, sizeof(out), &code);
    check(strstr(out, "fn-inner") != NULL, "function + local var");

    /* Richer parameter expansion: substring, length, default,
       pattern replace — all beyond POSIX/dash. */
    n = sh_run("s=abcdef; echo ${s:1:3}; echo ${#s}; "
               "echo ${unset:-dflt}; echo ${s/bcd/XYZ}",
               out, sizeof(out), &code);
    check(strstr(out, "bcd") != NULL && strstr(out, "6") != NULL &&
          strstr(out, "dflt") != NULL && strstr(out, "aXYZef") != NULL,
          "param expansion substr/len/default/replace");

    /* case statement. */
    n = sh_run("case beta in a*) echo A;; b*) echo B;; *) echo Z;; esac",
               out, sizeof(out), &code);
    check(strstr(out, "B") != NULL && !strstr(out, "A") &&
          !strstr(out, "Z"), "case pattern dispatch");

    /* printf builtin with format specifiers. */
    n = sh_run("printf '%s-%03d-%x\\n' item 7 255",
               out, sizeof(out), &code);
    check(strstr(out, "item-007-ff") != NULL, "printf builtin formats");

    /* Command substitution + arithmetic in place. */
    n = sh_run("echo sub-$(echo inner)-done; echo $((6*7))",
               out, sizeof(out), &code);
    check(strstr(out, "sub-inner-done") != NULL &&
          strstr(out, "42") != NULL, "substitution + arithmetic");

    /* Here-string: bash feeds <<< to a command's stdin. read is a
       builtin — no external cat needed. */
    n = sh_run("read x <<< 'here-string-data'; echo $x",
               out, sizeof(out), &code);
    check(strstr(out, "here-string-data") != NULL, "here-string <<<");

    /* trap on EXIT — bash's deferred signal path. */
    n = sh_run("trap 'echo trap-fired' EXIT; echo body",
               out, sizeof(out), &code);
    check(strstr(out, "body") != NULL && strstr(out, "trap-fired") != NULL,
          "trap EXIT handler");

    /* Background job + wait builtin: fork child, waitpid(-1). */
    n = sh_run("{ echo bg-child; } & wait; echo waited",
               out, sizeof(out), &code);
    check(strstr(out, "bg-child") != NULL && strstr(out, "waited") != NULL,
          "background job + wait builtin");

    /* Pipeline through a brace group (subshell fork). */
    n = sh_run("echo piped | { read x; echo got-$x; }",
               out, sizeof(out), &code);
    check(strstr(out, "got-piped") != NULL, "echo | read via pipe");

    /* Redirection create/append/read-back through the VFile layer.
       while-read is all builtins. */
    n = sh_run("echo one > /bash_tmp; echo two >> /bash_tmp; "
               "while read l; do echo got-$l; done < /bash_tmp",
               out, sizeof(out), &code);
    check(strstr(out, "got-one") != NULL && strstr(out, "got-two") != NULL,
          "redirect > and >> read-back");
    unlink("/bash_tmp");

    /* Glob through getdents. */
    n = sh_run("echo /mot*", out, sizeof(out), &code);
    check(strstr(out, "/motd.txt") != NULL, "glob /mot* -> motd.txt");

    /* while/until loops. */
    n = sh_run("i=0; while [ $i -lt 3 ]; do i=$((i+1)); done; echo done-$i",
               out, sizeof(out), &code);
    check(strstr(out, "done-3") != NULL, "while loop counting");

    /* exec of a real sibling binary. */
    n = sh_run("/init", out, sizeof(out), &code);
    check(strstr(out, "hello") != NULL && code == 0,
          "exec /init external command");

    /* type builtin identifies builtins without external help. */
    n = sh_run("type echo", out, sizeof(out), &code);
    check(strstr(out, "shell builtin") != NULL, "type identifies builtin");

    printf("bashtest: %d failures, done\n", fails);
    return fails;
}
