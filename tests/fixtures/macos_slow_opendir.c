// Test-only interposer: make the first two synthetic source-directory opens slow.
#include <dirent.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static _Atomic int delayed = 0;

static DIR *delayed_opendir(const char *path) {
    const char *prefix = getenv("LIFEINVADER_TEST_SLOW_ROOT");
    const char *marker = getenv("LIFEINVADER_TEST_SLOW_MARKER");
    if (prefix && marker && strncmp(path, prefix, strlen(prefix)) == 0 &&
        atomic_fetch_add(&delayed, 1) < 2) {
        int fd = open(marker, O_WRONLY | O_CREAT | O_APPEND, 0600);
        if (fd >= 0) { (void)write(fd, "x", 1); close(fd); }
        sleep(12);
    }
    return opendir(path);
}

__attribute__((used)) static struct {
    const void *replacement;
    const void *original;
} interpose_opendir __attribute__((section("__DATA,__interpose"))) = {
    (const void *)delayed_opendir, (const void *)opendir
};
