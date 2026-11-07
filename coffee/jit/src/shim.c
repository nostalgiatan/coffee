/* The `syscall` of Coffee code is a call of cf_syscall(n, a, b, c, d, e, f); `cfld` supplies it in a
 * program of its own. In the library the host's C library does: libc's syscall(). */
#define _GNU_SOURCE
#include <unistd.h>
#include <sys/syscall.h>

__attribute__((visibility("hidden"))) long cf_syscall(long n, long a, long b, long c, long d, long e, long f) {
    return syscall(n, a, b, c, d, e, f);
}
