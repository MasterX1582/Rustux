// rsh — the smallest real shell for Rustux. Reads lines byte-wise
// (canonical tty or a pipe both deliver them), splits on spaces,
// builtins "exit", otherwise fork + execve + waitpid with an exit-
// status report. Doubles as a boot task (blocks on stdin forever —
// a live shell for a guest keyboard) and as rshtest's exec target.
#include <unistd.h>
#include <sys/wait.h>
#include <string.h>
#include <stdio.h>

extern char **environ;

static void puts1(const char *s) {
    ssize_t r = write(1, s, strlen(s));
    (void)r;
}

static void putdec(int v) {
    char b[12]; int i = 12;
    if (v == 0) { puts1("0"); return; }
    if (v < 0) { puts1("-"); v = -v; }
    while (v) { b[--i] = '0' + v % 10; v /= 10; }
    ssize_t r = write(1, b + i, 12 - i);
    (void)r;
}

int main(void) {
    char line[256];
    puts1("rsh: rustux shell\n");
    for (;;) {
        puts1("rsh$ ");
        int n = 0;
        for (;;) {                        // a line, one byte at a time
            char c;
            int r = read(0, &c, 1);
            if (r <= 0) return 0;
            if (c == '\n') break;
            if (n < (int)sizeof(line) - 1) line[n++] = c;
        }
        line[n] = 0;

        char *argv[16]; int argc = 0;
        char *p = line;
        while (*p && argc < 15) {
            while (*p == ' ') p++;
            if (!*p) break;
            argv[argc++] = p;
            while (*p && *p != ' ') p++;
            if (*p) *p++ = 0;
        }
        argv[argc] = 0;
        if (argc == 0) continue;
        if (!strcmp(argv[0], "exit")) return 0;

        pid_t pid = fork();
        if (pid == 0) {
            char path[64];
            const char *prog = argv[0];
            if (prog[0] != '/') {          // flat namespace: /name
                path[0] = '/';
                strncpy(path + 1, prog, 62);
                path[63] = 0;
                prog = path;
            }
            execve(prog, argv, environ);
            _exit(127);                    // exec failed
        }
        int st = 0;
        waitpid(pid, &st, 0);
        puts1("rsh: exit ");
        if (WIFEXITED(st)) putdec(WEXITSTATUS(st));
        else putdec(st);
        puts1("\n");
    }
}
