// aptest — the Debian stub batch: umask roundtrip, sysinfo/times/
// getrusage shape, statfs magic, getgroups, sched_getaffinity,
// prctl(PR_SET_NAME), vfork-as-fork. Each syscall answers with a
// real or honestly-scoped value, never ENOSYS.
#define _GNU_SOURCE
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/syscall.h>
#include <sys/stat.h>
#include <sys/statfs.h>
#include <sys/sysinfo.h>
#include <sys/times.h>
#include <sys/resource.h>
#include <sys/prctl.h>
#include <sched.h>
#include <errno.h>
#include <string.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

int main(void) {
    printf("aptest: debian stub batch\n");

    check(umask(0077) == 0022, "umask returns default 022");
    check(umask(0022) == 0077, "umask roundtrip restores 022");

    struct sysinfo si;
    memset(&si, 0, sizeof(si));
    errno = 0;
    int sir = sysinfo(&si);
    /* uptime can legitimately be 0 in the first boot second */
    check(sir == 0 && si.uptime >= 0 && si.uptime < 86400 &&
          si.freeram > 0,
          "sysinfo: real uptime + freeram");

    struct tms tm;
    memset(&tm, 0, sizeof(tm));
    check(times(&tm) >= 0, "times answers with ticks");

    /* glibc's __times turns kernel EFAULT into a real SIGSEGV via
       a buf probe — raw syscall keeps the EFAULT observable. */
    errno = 0;
    check(syscall(SYS_times, 1) == -1 && errno == EFAULT,
          "times bad buf -> EFAULT");

    struct rusage ru;
    memset(&ru, 0, sizeof(ru));
    check(getrusage(RUSAGE_SELF, &ru) == 0, "getrusage RUSAGE_SELF");

    errno = 0;
    check(syscall(SYS_getrusage, 99, &ru) == -1 && errno == EINVAL,
          "getrusage bad who -> EINVAL");

    struct statfs sf;
    memset(&sf, 0, sizeof(sf));
    check(statfs("/", &sf) == 0 && sf.f_type == 0x858458f6,
          "statfs: RAMFS_MAGIC, 4KiB blocks");
    memset(&sf, 0, sizeof(sf));
    check(fstatfs(0, &sf) == 0 && sf.f_bsize == 4096,
          "fstatfs answers on a fd");

    check(getgroups(0, 0) == 0, "getgroups: no supplementary groups");

    cpu_set_t cs;
    memset(&cs, 0, sizeof(cs));
    errno = 0;
    long ar = sched_getaffinity(0, sizeof(cs), &cs);
    check(ar == 0 && CPU_ISSET(0, &cs),
          "affinity mask: single CPU bit 0");

    check(prctl(PR_SET_NAME, "aptnm", 0, 0, 0) == 0, "PR_SET_NAME ok");

    pid_t v = (pid_t)syscall(SYS_vfork);
    if (v == 0) _exit(11); // child path
    int st = 0;
    check(v > 0 && waitpid(v, &st, 0) == v && WEXITSTATUS(st) == 11,
          "vfork acts like fork, waitpid works");

    errno = 0;
    check(syscall(SYS_prctl, 9999, 0, 0, 0, 0) == -1 && errno == EINVAL,
          "unknown prctl -> EINVAL");

    /* reboot with wrong magic must EINVAL — a real reset would end
       the boot, so the magic gate is the only testable surface. */
    errno = 0;
    check(syscall(SYS_reboot, 0, 0, 0x1234567, 0) == -1 &&
          errno == EINVAL, "reboot bad magic -> EINVAL");
    check(syscall(SYS_reboot, 0xfee1dead, 672274793,
                  0x89abcdef /* CAD_ON */, 0) == 0,
          "reboot CAD_ON acknowledged");
    errno = 0;
    check(syscall(SYS_reboot, 0xfee1dead, 672274793, 9999, 0) == -1 &&
          errno == EINVAL, "reboot unknown cmd -> EINVAL");

    printf("aptest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
