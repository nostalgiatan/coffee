typedef long L;
static L a[8192], b[8192];
__attribute__((noinline)) static L total(L *a, L n) { L s = 0; for (L i = 0; i < n; i++) s = s + a[i]; return s; }
__attribute__((noinline)) static void scale(L *a, L *b, L n, L k) { for (L i = 0; i < n; i++) a[i] = (a[i] ^ b[i]) + k; }
int body(void) {
    L n = 8192;
    for (L i = 0; i < n; i++) { a[i] = i * 3 + 1; b[i] = i; }
    L t = 0;
    for (L r = 0; r < 40; r++) { scale(a, b, n, r); t = t + total(a, n); }
    return t % 251;
}
