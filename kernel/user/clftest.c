// clftest — clone(CLONE_FS) shared fs_struct checks: a CLONE_FS|VM
// child's umask change is visible to the parent (shared table), a
// plain fork child's is not (private copy); CLONE_SYSVSEM is an
// honest no-op; and the FULL glibc pthread_create flag set —
// VM|FS|FILES|SIGHAND|THREAD|SYSVSEM|SETTLS|PARENT_SETTID|
// CHILD_CLEARTID — parses and runs as one clone call.
#define _GNU_SOURCE
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <sched.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <sys/syscall.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static volatile int fsdone;
static volatile int done4;
static volatile int ptid_w;
static volatile int join_w;
static char s1[8192] __attribute__((aligned(16)));
static char s2[8192] __attribute__((aligned(16)));
static char s3[8192] __attribute__((aligned(16)));
static char faketls[256] __attribute__((aligned(64)));

static int fs_child(void *arg) {
    umask(0077);           // shared table — parent must see it
    *(volatile int *)arg = 1;
    return 4;
}

static int sv_child(void *arg) {
    (void)arg;
    return 3;
}

static int thr_fn(void *arg) {
    unsigned long fsb = 0;
    syscall(158, 0x1003, &fsb); // arch_prctl(ARCH_GET_FS, &fsb)
    *(volatile unsigned long *)arg = fsb;
    done4 = 1;
    return 0; // THREAD exits are join-only — never waitable
}

int main(void) {
    printf("clftest: clone(CLONE_FS)/SYSVSEM checks\n");

    /* CLONE_FS|CLONE_VM: shared fs_struct — child's umask write is
       visible here after waitpid orders the exit. */
    fsdone = 0;
    umask(0022);
    pid_t p1 = clone(fs_child, s1 + sizeof(s1),
                     CLONE_FS | CLONE_VM, (void *)&fsdone);
    check(p1 > 0, "clone(FS|VM) returns child pid");
    int st = 0;
    check(waitpid(p1, &st, 0) == p1 && WEXITSTATUS(st) == 4,
          "fs child exited 4");
    check(fsdone == 1, "child ran on shared mm");
    mode_t seen = umask(0); // read-back: the shared mask
    check(seen == 0077, "child umask visible in parent");
    umask(0022);

    /* Plain fork: private fs_struct — child umask stays local. */
    pid_t p2 = fork();
    check(p2 >= 0, "fork for privacy check");
    if (p2 == 0) { umask(0111); _exit(9); }
    st = 0;
    check(p2 > 0 && waitpid(p2, &st, 0) == p2 && WEXITSTATUS(st) == 9,
          "fork child exited 9");
    seen = umask(0);
    check(seen == 0022, "fork child's umask NOT shared");
    umask(0022);

    /* CLONE_SYSVSEM: accepted as a no-op — no SysV sems exist to
       share, but the flag must not ENOSYS. */
    pid_t p3 = clone(sv_child, s2 + sizeof(s2),
                     CLONE_VM | CLONE_SYSVSEM, (void *)0);
    check(p3 > 0, "clone(VM|SYSVSEM) returns child pid");
    st = 0;
    check(p3 > 0 && waitpid(p3, &st, 0) == p3 && WEXITSTATUS(st) == 3,
          "sysvsem child exited 3");

    /* The real test: glibc pthread_create's full flag set in one
       clone — VM|FS|FILES|SIGHAND|THREAD|SYSVSEM|SETTLS|
       PARENT_SETTID|CHILD_CLEARTID. A rejected flag here is what
       breaks real pthread_create. */
    done4 = 0;
    ptid_w = 0;
    join_w = 0x5a5a; // sentinel — kernel CLEARTIDs to 0
    volatile unsigned long fsb_out = 0;
    int flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND |
                CLONE_THREAD | CLONE_SYSVSEM | CLONE_SETTLS |
                CLONE_PARENT_SETTID | CLONE_CHILD_CLEARTID;
    pid_t t = clone(thr_fn, s3 + sizeof(s3), flags, (void *)&fsb_out,
                    (void *)&ptid_w, faketls, (void *)&join_w);
    check(t > 0, "full pthread flag set accepted");
    /* pthread_join idiom: futex-wait on the ctid word. */
    while (join_w != 0)
        syscall(SYS_futex, (void *)&join_w, 0 /*FUTEX_WAIT*/,
                0x5a5a, 0, 0, 0);
    check(join_w == 0, "CLEARTID cleared the join word");
    check(done4 == 1, "pthread-shape child ran");
    check(ptid_w == (unsigned long)t, "PARENT_SETTID wrote tid");
    check(fsb_out == (unsigned long)faketls,
          "SETTLS fs_base live in thread");

    /* Still honest: an unimplemented flag class ENOSYSs. */
    errno = 0;
    long e = syscall(SYS_clone, CLONE_VM | 0x20000 /*NEWNS*/,
                     s3 + sizeof(s3), 0, 0, 0);
    check(e == -1 && errno == ENOSYS, "CLONE_NEWNS -> ENOSYS");

    printf("clftest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
