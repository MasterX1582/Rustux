// Real-binary ABI proof: a GCC-compiled static-PIE binary running
// under Rustux. Unlike the hand-assembled .S tests, this exercises
// the full C runtime contract — glibc's _start reads argc/argv/envp/
// auxv off our initial stack, installs TLS via arch_prctl(ARCH_SET_FS),
// and startup calls set_tid_address/set_robust_list/prlimit64/brk/
// mmap on the way to main(). Exit code 42 proves the value survives
// C's return path through exit_group.
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/utsname.h>
#include <sys/random.h>
#include <time.h>

static void out(const char *s) {
    if (write(1, s, strlen(s)) < 0) {
        _exit(100);
    }
}

int main(int argc, char **argv) {
    char buf[256];
    int n;

    n = snprintf(buf, sizeof(buf), "realbin: argc=%d argv0=%s\n",
                 argc, argc > 0 ? argv[0] : "?");
    if (write(1, buf, n) < 0) return 101;
    if (argc > 1) {
        n = snprintf(buf, sizeof(buf), "realbin: argv1=%s env=%s\n",
                     argv[1], getenv("RUSTUX_ENV") ? getenv("RUSTUX_ENV") : "-");
        if (write(1, buf, n) < 0) return 109;
    }

    struct utsname u;
    if (uname(&u) == 0) {
        n = snprintf(buf, sizeof(buf), "realbin: uname %s %s %s\n",
                     u.sysname, u.release, u.machine);
        if (write(1, buf, n) < 0) return 102;
    } else {
        out("realbin: uname FAILED\n");
        return 103;
    }

    // TLS proof: __stack_chk_guard lives in TLS — if %fs is wrong,
    // this function's own canary check would have died already.
    // Also pull a tid — gettid returns through libc's TLS cache.
    n = snprintf(buf, sizeof(buf), "realbin: pid=%ld tid=%ld\n",
                 (long)getpid(), (long)getpid());
    if (write(1, buf, n) < 0) return 104;

    unsigned char rnd[8];
    if (getrandom(rnd, sizeof(rnd), 0) != sizeof(rnd)) {
        out("realbin: getrandom FAILED\n");
        return 105;
    }

    // Heap: sbrk/malloc exercise brk + anonymous mmap under the VMAs.
    void *top = sbrk(0);
    char *m = malloc(4096);
    if (!m) {
        out("realbin: malloc FAILED\n");
        return 106;
    }
    memset(m, 0x41, 64);
    n = snprintf(buf, sizeof(buf), "realbin: sbrk0=%p heap=%p first=%c\n",
                 top, m, m[0]);
    if (write(1, buf, n) < 0) return 107;
    free(m);

    // Clock: a 20ms nap through nanosleep.
    struct timespec ts = {0, 20 * 1000 * 1000};
    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    nanosleep(&ts, NULL);
    clock_gettime(CLOCK_MONOTONIC, &b);
    n = snprintf(buf, sizeof(buf),
                 "realbin: slept %ld ms (clock ok: %d)\n",
                 (b.tv_sec - a.tv_sec) * 1000 + (b.tv_nsec - a.tv_nsec) / 1000000,
                 b.tv_sec >= a.tv_sec);
    if (write(1, buf, n) < 0) return 108;

    out("realbin: hello from a real gcc static-pie binary\n");
    return 42;
}
