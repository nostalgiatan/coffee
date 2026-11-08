typedef long L;
static double a[4096], b[4096];
static void step(double *a, double *b, L n) { for (L i = 0; i < n; i++) a[i] = a[i] * 0.5 + b[i]; }
int body(void) {
    L n = 4096;
    for (L i = 0; i < n; i++) { a[i] = (double)i; b[i] = 1.0; }
    for (L r = 0; r < 40; r++) step(a, b, n);
    double s = 0.0;
    for (L i = 0; i < n; i++) s = s + a[i];
    return (L)s % 251;
}
