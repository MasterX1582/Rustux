// ftest — clone(CLONE_FILES): the child shares the parent's fd
// TABLE. Two proofs: (1) FILES|VM — the child reports its new fd
// through shared memory and the parent uses it at the same number;
// (2) FILES alone — memory is copied so the child reports the fd
// through an inherited pipe, and the parent's slot still works
// after the child dies (the table outlives either user).
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/wait.h>
#include <sys/syscall.h>
#include <errno.h>
#include <string.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static volatile int opened_fd = -1;
static volatile int duped_fd = -1;
static char st1[8192] __attribute__((aligned(16)));
static char st2[8192] __attribute__((aligned(16)));

static int opener(void *a) {
    (void)a;
    opened_fd = open("/motd.txt", O_RDONLY); // lands in SHARED table
    duped_fd = dup(opened_fd);
    return 6;
}

// FILES alone (copied mm): the child reports its fd through an
// inherited pipe — and after it dies the shared TABLE keeps the
// fd alive for us. A raw SYS_clone call, not the clone() wrapper:
// FILES-without-VM is fork-shaped, so the child continues on the
// copied stack — no stack arg wanted.
static void files_child(int wfd) {
    int f = open("/motd.txt", O_RDONLY);
    _exit(write(wfd, &f, sizeof(f)) == sizeof(f) ? 7 : 8);
}

int main(void) {
    printf("ftest: clone(CLONE_FILES) checks\n");

    // FILES|VM: memory reporting — the pthread shape.
    pid_t p = clone(opener, st1 + sizeof(st1),
                    CLONE_FILES | CLONE_VM | SIGCHLD, 0);
    check(p > 0, "clone(FILES|VM) returns child pid");
    int st = 0;
    check(waitpid(p, &st, 0) == p && WEXITSTATUS(st) == 6,
          "files+vm child exited 6");
    check(opened_fd >= 3, "child open() visible to parent");
    char buf[8] = {0};
    check(read(opened_fd, buf, 5) == 5 && buf[0] != 0,
          "parent reads child's fd, real bytes");
    check(fcntl(duped_fd, F_GETFD) >= 0, "child's dup slot valid here");

    int pf[2];
    check(pipe(pf) == 0, "pipe for fd reporting");
    pid_t q = syscall(SYS_clone, CLONE_FILES | SIGCHLD, 0, 0, 0, 0);
    if (q == 0) files_child(pf[1]);
    check(q > 0, "clone(FILES) returns child pid");
    st = 0;
    check(waitpid(q, &st, 0) == q && WEXITSTATUS(st) == 7,
          "files-only child exited 7");
    close(pf[1]);
    int cf = -1;
    check(read(pf[0], &cf, sizeof(cf)) == sizeof(cf) && cf >= 3,
          "child fd number arrived over pipe");
    memset(buf, 0, sizeof(buf));
    check(pread(cf, buf, 5, 0) == 5 && buf[0] != 0,
          "fd alive in parent after child exit");

    // Honesty: bad pairings EINVAL, unknown flags ENOSYS.
    errno = 0;
    check(syscall(SYS_clone, 0x00000800 | SIGCHLD, 0, 0, 0, 0) == -1 &&
              errno == EINVAL,
          "CLONE_SIGHAND no-VM -> EINVAL");
    errno = 0;
    check(syscall(SYS_clone, 0x00010000 | SIGCHLD, 0, 0, 0, 0) == -1 &&
              errno == EINVAL,
          "CLONE_THREAD no-SIGHAND -> EINVAL");
    errno = 0;
    check(syscall(SYS_clone, 0x4000 /*CLONE_VFORK*/ | SIGCHLD, 0, 0, 0, 0) ==
                  -1 &&
              errno == ENOSYS,
          "CLONE_VFORK -> ENOSYS");

    printf("ftest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
