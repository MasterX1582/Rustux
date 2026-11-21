// rshtest — drives the real rsh shell through pipes: the child
// dup2's one pipe onto stdin and another onto stdout, execs /rsh;
// the parent types a script in and reads the whole session back.
// Proves the interactive-process shape end-to-end with no keyboard:
// prompt -> parse -> fork -> execve -> waitpid -> status report ->
// builtin exit.
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

extern char **environ;

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

int main(void) {
    printf("rshtest: shell-driven checks\n");

    int in[2], out[2];
    check(pipe(in) == 0 && pipe(out) == 0, "stdin/stdout pipes created");

    pid_t pid = fork();
    check(pid >= 0, "fork");
    if (pid == 0) {
        // Wire this half as the shell's terminal: stdin from in[0],
        // stdout to out[1], then become /rsh.
        dup2(in[0], 0);
        dup2(out[1], 1);
        close(in[0]); close(in[1]);
        close(out[0]); close(out[1]);
        char *argv[] = { "rsh", 0 };
        execve("/rsh", argv, environ);
        _exit(127);
    }

    close(in[0]);
    close(out[1]);
    // The session: a real binary with an arg, a bad name (child
    // exec fails -> 127), then the exit builtin. Closing the pipe
    // gives EOF as a second exit path.
    const char script[] = "realbin arg1\nnotacommand\nexit\n";
    check(write(in[1], script, sizeof(script) - 1) ==
          (int)sizeof(script) - 1, "script written to shell stdin");
    close(in[1]);

    char buf[8192]; int total = 0;
    for (;;) {
        int n = read(out[0], buf + total, sizeof(buf) - 1 - total);
        if (n <= 0) break;
        total += n;
        if (total >= (int)sizeof(buf) - 1) break;
    }
    close(out[0]);
    buf[total] = 0;

    int st = 0;
    check(waitpid(pid, &st, 0) == pid, "waitpid reaped the shell");
    check(WIFEXITED(st) && WEXITSTATUS(st) == 0, "rsh exited 0");

    check(strstr(buf, "rsh$") != NULL, "prompt appeared");
    check(strstr(buf, "realbin:") != NULL, "realbin output came back");
    check(strstr(buf, "arg1") != NULL, "argv reached realbin");
    check(strstr(buf, "rsh: exit 42") != NULL,
          "waitpid status 42 reached the shell");
    check(strstr(buf, "rsh: exit 127") != NULL, "bad exec reports 127");

    printf("rshtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
