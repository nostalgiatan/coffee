typedef long L;
static L s[400000];
int body(void) {
    L n = 400000, c = 0;
    for (L i = 2; i < n; i++)
        if (s[i] == 0) { c++; for (L j = i * 2; j < n; j += i) s[j] = 1; }
    return c % 256;
}
