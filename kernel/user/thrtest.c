// thrtest — CLONE_VM|SIGHAND|THREAD: a real thread-group member.
// Proves shared dispositions (sigaction lands in the shared table),
// tgid semantics (getpid matches the leader, gettid is distinct),
// wait4 invisibility (threads are never waitable zombies — join is
// the ctid futex, same as run-76), and the EINVAL pairing rules.
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <sys/wait.h>
#include <sys/syscall.h>
#include <errno.h>

#define CLONE_VM 0x00000100
#define CLONE_FILES 0x00000400
#define CLONE_SIGHAND 0x00000800
#define CLONE_THREAD 0x00010000
#define CHILD_SETTID 0x01000000
#define CHILD_CLEARTID 0x00200000
#define FUTEX_WAIT 0

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static void usr1(int s) { (void)s; }

// Shared-mm report block the thread fills in.
static volatile long r_pid, r_tid, r_handler;
static volatile int join_word;
static char st1[8192] __attribute__((aligned(16)));

static int thread_fn(void *a) {
    (void)a;
    r_pid = getpid();
    r_tid = (long)syscall(SYS_gettid);
    struct sigaction sa;
    sigaction(SIGUSR1, 0, &sa); // read back — sees SHARED table
    r_handler = (long)sa.sa_handler;
    return 9;
}

int main(void) {
    printf("thrtest: CLONE_THREAD group checks\n");

    struct sigaction sa = {0};
    sa.sa_handler = usr1;
    sigaction(SIGUSR1, &sa, 0); // installs into OUR sighand table
    long mypid = getpid();

    join_word = 0;
    long flags = CLONE_VM | CLONE_SIGHAND | CLONE_THREAD |
                 CHILD_SETTID | CHILD_CLEARTID;
    pid_t t = clone(thread_fn, st1 + sizeof(st1), flags, 0,
                    0, 0, (void *)&join_word);
    check(t > 0, "clone(VM|SIGHAND|THREAD) returns tid");
    check(join_word == (int)t, "SETTID wrote the thread tid");

    // Join the pthread way: futex-wait on the ctid word.
    int tid = join_word;
    while (join_word == tid && tid != 0)
        syscall(SYS_futex, (void *)&join_word, FUTEX_WAIT, tid, 0, 0, 0);
    check(join_word == 0, "ctid cleared + futex-woke the join");

    check(r_pid == mypid, "thread getpid() == leader tgid");
    check(r_tid == (long)t, "thread gettid() == clone tid");
    check(r_handler == (long)usr1,
          "thread sees leader's sigaction (shared table)");

    // Threads are wait4-invisible: ECHILD, not a reaped zombie.
    errno = 0;
    check(waitpid(t, 0, 0) == -1 && errno == ECHILD,
          "waitpid(thread) -> ECHILD (never a zombie)");

    // Leader-visible change: thread's disposition IS ours — swap the
    // handler, both see the same table either direction.
    struct sigaction back;
    sa.sa_handler = SIG_IGN;
    sigaction(SIGUSR1, &sa, &back);
    check(back.sa_handler == usr1, "sigaction swap sees shared table");

    // EINVAL pairing: SIGHAND needs VM, THREAD needs SIGHAND.
    errno = 0;
    check(syscall(SYS_clone, CLONE_SIGHAND | SIGCHLD, 0, 0, 0, 0) == -1 &&
              errno == EINVAL,
          "SIGHAND without VM -> EINVAL");
    errno = 0;
    check(syscall(SYS_clone,
                  CLONE_VM | CLONE_THREAD | SIGCHLD, 0, 0, 0, 0) == -1 &&
              errno == EINVAL,
          "THREAD without SIGHAND -> EINVAL");

    printf("thrtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
