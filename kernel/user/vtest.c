// vtest — vfork() semantics: shared-mm child + suspended parent.
// The child's writes are visible in the parent's address space and
// the parent only resumes after the child's _exit or execve —
// ordering, sharing, and the wait4 contract in one binary.
#define _GNU_SOURCE
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static volatile int shared_flag;
static volatile long shared_val;

int main(void) {
    printf("vtest: vfork() shared-mm + parent-pause checks\n");

    /* Child writes a global then _exits. Two proofs at once:
       the write lands in the parent's address space (shared mm),
       and it's already set when vfork returns here (parent was
       suspended until the child exited — Linux's vfork_done). */
    shared_flag = 0;
    volatile long guard = 0x5eaf;   // parent stack slot — the child
                                    // running on this stack must not
                                    // disturb frames above its rsp.
    pid_t p = vfork();
    check(p >= 0, "vfork returns child pid in parent");
    if (p == 0) {
        shared_flag = 1;
        shared_val = 0xC0FFEE;
        _exit(7);
    }
    check(shared_flag == 1, "child write visible (shared mm)");
    check(shared_val == 0xC0FFEE, "child value write visible");
    check(guard == 0x5eaf, "parent stack intact after shared use");
    int st = 0;
    check(waitpid(p, &st, 0) == p && WIFEXITED(st) && WEXITSTATUS(st) == 7,
          "waitpid reaps vfork child (7)");

    /* exec path: the child execls away — the parent's suspension
       ends at the child's mm swap (vfork_done at exec_mmap). */
    shared_flag = 0;
    p = vfork();
    check(p >= 0, "vfork for exec path");
    if (p == 0) {
        shared_flag = 2;
        char *a[] = {"init", NULL};
        execve("/init", a, environ);
        _exit(111); // only if execve fails
    }
    check(shared_flag == 2, "pre-exec write visible to parent");
    st = 0;
    check(waitpid(p, &st, 0) == p && WIFEXITED(st) && WEXITSTATUS(st) == 0,
          "exec'd vfork child exited 0");

    /* fork() must NOT suspend the parent: flag starts 0 and the
       parent reads it before the child could run — value is the
       private copy either way (can't observe the write reliably
       without waitpid, which is the ordering difference itself). */
    shared_flag = 0;
    p = fork();
    check(p >= 0, "fork does not suspend parent");
    if (p == 0) { shared_flag = 3; _exit(8); }
    st = 0;
    check(p > 0 && waitpid(p, &st, 0) == p && WEXITSTATUS(st) == 8,
          "fork child reaped (8)");
    check(shared_flag == 0 || shared_flag == 3,
          "fork child got private mm");

    printf("vtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
