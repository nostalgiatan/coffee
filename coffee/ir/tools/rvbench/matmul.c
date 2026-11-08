typedef long L;
static L a[64*64], b[64*64], c[64*64];
int body(void) {
    L n = 64;
    for (L i = 0; i < n * n; i++) { a[i] = i % 17; b[i] = i % 13; }
    for (L i = 0; i < n; i++) for (L j = 0; j < n; j++) {
        L s = 0;
        for (L k = 0; k < n; k++) s += a[i * n + k] * b[k * n + j];
        c[i * n + j] = s;
    }
    L sum = 0;
    for (L i = 0; i < n * n; i++) sum = (sum * 31 + c[i]) % 1000000007;
    return sum % 256;
}
