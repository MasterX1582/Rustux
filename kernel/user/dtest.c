// dtest — hierarchical path/VFS checks: mkdir/mkdirat create real
// dirs, chdir/fchdir move the shared cwd, relative + dirfd paths
// resolve, getdents lists per-dir components (not the flat world),
// rmdir honors ENOTEMPTY, and the honesty table stays sharp.
#define _GNU_SOURCE
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/syscall.h>

static int fails;
static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

static int dir_has(const char *dir, const char *want) {
    int d = open(dir, O_RDONLY | O_DIRECTORY);
    char buf[2048];
    int found = 0;
    if (d < 0) return -1;
    long got;
    while ((got = syscall(SYS_getdents64, d, buf, sizeof(buf))) > 0) {
        for (long off = 0; off + 19 <= got; ) {
            struct { unsigned long ino, off; unsigned short len;
                     unsigned char type; char name[]; } *e =
                (void *)(buf + off);
            if (strcmp(e->name, want) == 0) found = e->type;
            off += e->len;
        }
    }
    close(d);
    return found;
}

int main(void) {
    char buf[128];
    struct stat st;
    printf("dtest: path/VFS semantics\n");

    check(getcwd(buf, sizeof(buf)) && strcmp(buf, "/") == 0,
          "getcwd starts at /");
    check(syscall(SYS_mkdir, "/vtmp", 0755) == 0, "mkdir /vtmp");
    check(syscall(SYS_mkdirat, AT_FDCWD, "/vtmp/sub", 0755) == 0,
          "mkdirat /vtmp/sub");
    check(stat("/vtmp", &st) == 0 && S_ISDIR(st.st_mode),
          "stat /vtmp is S_IFDIR");
    check(stat("/vtmp/sub", &st) == 0 && S_ISDIR(st.st_mode),
          "stat /vtmp/sub is S_IFDIR");
    check(stat("/proc", &st) == 0 && S_ISDIR(st.st_mode),
          "stat /proc is S_IFDIR");

    check(chdir("/vtmp") == 0, "chdir /vtmp");
    check(getcwd(buf, sizeof(buf)) && strcmp(buf, "/vtmp") == 0,
          "getcwd = /vtmp");
    check(chdir("sub") == 0, "chdir sub (relative)");
    check(getcwd(buf, sizeof(buf)) && strcmp(buf, "/vtmp/sub") == 0,
          "getcwd = /vtmp/sub");
    check(chdir("..") == 0, "chdir .. pops");
    check(getcwd(buf, sizeof(buf)) && strcmp(buf, "/vtmp") == 0,
          "getcwd back to /vtmp");
    check(chdir("/") == 0 && chdir("..") == 0 &&
          getcwd(buf, sizeof(buf)) && strcmp(buf, "/") == 0,
          ".. at root stays root");

    /* cwd-relative + dirfd-relative creation. */
    check(chdir("/vtmp") == 0, "chdir back into /vtmp");
    int f = open("rel.tmp", O_CREAT | O_RDWR, 0644);
    check(f >= 0, "open rel.tmp cwd-relative");
    if (f >= 0) { write(f, "hi", 2); close(f); }
    check(access("/vtmp/rel.tmp", F_OK) == 0,
          "rel.tmp exists under /vtmp");
    int dd = open("/vtmp", O_RDONLY | O_DIRECTORY);
    check(dd >= 0, "open /vtmp O_DIRECTORY");
    int f2 = openat(dd, "rel2.tmp", O_CREAT | O_WRONLY, 0644);
    check(f2 >= 0, "openat rel2.tmp dirfd-relative");
    if (f2 >= 0) close(f2);
    check(access("/vtmp/rel2.tmp", F_OK) == 0,
          "rel2.tmp exists under /vtmp");
    check(access("sub", F_OK) == 0, "access sub cwd-relative");
    check(access("/vtmp/../motd.txt", F_OK) == 0,
          "/vtmp/../motd.txt resolves");

    /* getdents is per-dir: /vtmp lists its children only. */
    check(dir_has("/vtmp", ".") == 4, "vtmp lists . (DT_DIR)");
    check(dir_has("/vtmp", "..") == 4, "vtmp lists .. (DT_DIR)");
    check(dir_has("/vtmp", "sub") == 4, "vtmp lists sub (DT_DIR)");
    check(dir_has("/vtmp", "rel.tmp") == 8,
          "vtmp lists rel.tmp (DT_REG)");
    check(dir_has("/vtmp", "motd.txt") == 0,
          "vtmp does NOT list motd.txt");
    check(dir_has("/", "vtmp") == 4, "root lists vtmp (DT_DIR)");
    check(dir_has("/", "proc") == 4, "root lists proc (DT_DIR)");

    /* fchdir through a dir fd; unlinkat dirfd-relative. */
    check(fchdir(dd) == 0, "fchdir back to /vtmp");
    check(getcwd(buf, sizeof(buf)) && strcmp(buf, "/vtmp") == 0,
          "getcwd = /vtmp after fchdir");
    check(unlinkat(dd, "rel2.tmp", 0) == 0,
          "unlinkat dirfd-relative");
    check(access("/vtmp/rel2.tmp", F_OK) != 0, "rel2.tmp gone");
    check(unlink("rel.tmp") == 0, "unlink rel.tmp");

    /* Honesty table. */
    errno = 0;
    check(syscall(SYS_mkdir, "/vtmp", 0) == -1 && errno == EEXIST,
          "mkdir existing -> EEXIST");
    errno = 0;
    check(syscall(SYS_mkdir, "/vtmp/nope/kid", 0) == -1 &&
          errno == ENOENT, "mkdir missing parent -> ENOENT");
    errno = 0;
    check(rmdir("/vtmp") == -1 && errno == ENOTEMPTY,
          "rmdir non-empty -> ENOTEMPTY");
    errno = 0;
    check(chdir("/definitely/absent") == -1 && errno == ENOENT,
          "chdir missing -> ENOENT");
    errno = 0;
    check(chdir("/motd.txt") == -1 && errno == ENOTDIR,
          "chdir to file -> ENOTDIR");
    errno = 0;
    check(open("/vtmp", O_WRONLY) == -1 && errno == EISDIR,
          "open dir O_WRONLY -> EISDIR");
    errno = 0;
    check(open("/motd.txt/kid", O_RDONLY) == -1 && errno == ENOTDIR,
          "path through file -> ENOTDIR");
    errno = 0;
    check(unlink("/vtmp") == -1 && errno == EISDIR,
          "unlink a dir -> EISDIR");

    /* Cleanup: rmdir leaf-first, then back to root. */
    check(rmdir("/vtmp/sub") == 0, "rmdir /vtmp/sub");
    check(chdir("/") == 0 && rmdir("/vtmp") == 0,
          "rmdir /vtmp");
    check(access("/vtmp", F_OK) != 0, "/vtmp gone");
    close(dd);

    printf("dtest: %d failure%s, done\n", fails, fails == 1 ? "" : "s");
    return fails;
}
