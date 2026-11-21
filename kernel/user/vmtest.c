// vmtest — clone(CLONE_VM) shared-address-space checks: a child task
// on its own stack shares the parent's page tables — it must read
// the parent's pre-clone write, write back through the same memory,
// and the parent sees the result after waitpid. SETTLS is proven by
// arch_prctl(ARCH_GET_FS) readback inside the child; unsupported
// flag combos keep their honest ENOSYS/EINVAL answers.
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/syscall.h>
#include <errno.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static volatile int shared_val;
static volatile unsigned long child_fs;
static char cstack[8192] __attribute__((aligned(16)));
static char tstack[8192] __attribute__((aligned(16)));
// A fake TCB: pthreads pass a mapped TLS block; a bogus fs_base
// faults instantly because gcc's stack protector reads fs:0x28.
static char faketls[256] __attribute__((aligned(64)));

static int child_fn(void *arg) {
    if (shared_val != 1) return 11; // must see parent's pre-clone value
    shared_val = 0x5a;
    *(int *)arg = 99;
    return 7;
}

static int tls_fn(void *arg) {
    unsigned long fsb = 0;
    syscall(158, 0x1003, &fsb); // arch_prctl(ARCH_GET_FS, &fsb)
    *(volatile unsigned long *)arg = fsb;
    return 9;
}

int main(void) {
    printf("vmtest: clone(CLONE_VM) shared-address checks\n");

    int seen = 0;
    shared_val = 1;
    pid_t pid = clone(child_fn, cstack + sizeof(cstack), CLONE_VM, &seen);
    check(pid > 0, "clone(CLONE_VM) returns child pid");
    int st = 0;
    check(waitpid(pid, &st, 0) == pid, "waitpid collected the thread");
    check(WIFEXITED(st) && WEXITSTATUS(st) == 7,
          "thread saw shared_val==1 and exited 7");
    check(shared_val == 0x5a, "child write landed in shared memory");
    check(seen == 99, "arg-pointer write visible to parent");

    // CLONE_SETTLS: the glibc clone() wrapper passes its variadic
    // (ptid, tls, ctid) args to the syscall — the child runs tls_fn
    // with fs_base=faketls and reads it back through arch_prctl.
    child_fs = 0;
    pid_t t2 = clone(tls_fn, tstack + sizeof(tstack),
                     CLONE_VM | CLONE_SETTLS, (void *)&child_fs,
                     (void *)0, faketls, (void *)0);
    check(t2 > 0, "clone(CLONE_VM|SETTLS) returns child pid");
    st = 0;
    check(waitpid(t2, &st, 0) == t2 && WEXITSTATUS(st) == 9,
          "settls thread exited 9");
    check(child_fs == (unsigned long)faketls,
          "ARCH_GET_FS readback = clone tls arg");

    // Honest refusals: unsupported combos stay ENOSYS, a bare
    // CLONE_VM with no child stack is EINVAL.
    errno = 0;
    long e1 = syscall(SYS_clone, 0x800 /*CLONE_SIGHAND*/, 0, 0, 0, 0);
    check(e1 == -1 && errno == EINVAL, "CLONE_SIGHAND no-VM -> EINVAL");
    errno = 0;
    long e2 = syscall(SYS_clone, CLONE_VM | 0x10000 /*THREAD*/,
                      tstack + sizeof(tstack), 0, 0, 0);
    check(e2 == -1 && errno == EINVAL, "CLONE_VM|THREAD no-SIGHAND -> EINVAL");
    errno = 0;
    long e3 = syscall(SYS_clone, CLONE_VM, 0, 0, 0, 0);
    check(e3 == -1 && errno == EINVAL, "CLONE_VM no-stack -> EINVAL");

    printf("vmtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
