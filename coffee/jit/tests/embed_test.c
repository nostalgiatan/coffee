/* A host program using libcfjit: it offers host_scale, loads two modules (the second calls the first),
 * calls them, replaces the first, unloads. argv: a1.cir b.cir a2.cir bad.cir other.cir */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cfjit.h"

static long host_scale(long x) { return x * 10; }

static char *slurp(const char *path, long *n) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); exit(90); }
    fseek(f, 0, SEEK_END); *n = ftell(f); fseek(f, 0, SEEK_SET);
    char *b = malloc(*n + 1);
    if (fread(b, 1, *n, f) != (size_t)*n) exit(91);
    fclose(f);
    return b;
}

#define CHECK(c) do { if (!(c)) { printf("FAIL line %d: %s\n", __LINE__, #c); return 1; } } while (0)

int main(int argc, char **argv) {
    long n1, n2, n3, n4, n5;
    char *a1 = slurp(argv[1], &n1), *b = slurp(argv[2], &n2), *a2 = slurp(argv[3], &n3), *bad = slurp(argv[4], &n4), *other = slurp(argv[5], &n5);
    char err[200];
    cfjit_ctx c = cfjit_new();
    CHECK(c != 0);
    CHECK(cfjit_register(c, "host_scale", (void *)host_scale) == 0);
    CHECK(cfjit_register(c, "host_scale", (void *)host_scale) == -1);
    /* a module that calls a name nobody offers is refused with the name */
    long r = cfjit_load(c, bad, n4, err, sizeof err);
    CHECK(r == -4);
    CHECK(strstr(err, "nothing_here") != NULL);
    long ma = cfjit_load(c, a1, n1, err, sizeof err);
    CHECK(ma > 0);
    long (*compute)(long) = (long (*)(long))cfjit_sym(c, "compute");
    CHECK(compute != NULL);
    CHECK(compute(3) == 31);
    long mb = cfjit_load(c, b, n2, err, sizeof err);
    CHECK(mb > 0);
    long (*run)(long) = (long (*)(long))cfjit_sym(c, "run");
    CHECK(run(3) == 62);
    CHECK(cfjit_sym(c, "no_such") == NULL);
    /* b calls into a: a cannot go yet */
    CHECK(cfjit_unload(c, ma) == -2);
    /* replace a by a version that adds 100; b's call is rebound, the old code stays mapped */
    long ma2 = cfjit_replace(c, ma, a2, n3, err, sizeof err);
    CHECK(ma2 > 0 && ma2 != ma);
    CHECK(run(3) == 260);
    CHECK(compute(3) == 31);                       /* the old address still works until it is freed */
    long (*compute2)(long) = (long (*)(long))cfjit_sym(c, "compute");
    CHECK(compute2 != compute && compute2(3) == 130);
    CHECK(cfjit_unload(c, ma) == 0);               /* the retired module */
    /* a replacement that lacks what b calls is refused and changes nothing */
    long ma3 = cfjit_replace(c, ma2, other, n5, err, sizeof err);
    CHECK(ma3 == -5);
    CHECK(strstr(err, "compute") != NULL);
    CHECK(run(3) == 260);
    CHECK(cfjit_unload(c, ma2) == -2);
    CHECK(cfjit_unload(c, mb) == 0);
    CHECK(cfjit_unload(c, ma2) == 0);
    CHECK(cfjit_sym(c, "compute") == NULL);
    cfjit_free(c);
    printf("ok\n");
    return 0;
}
