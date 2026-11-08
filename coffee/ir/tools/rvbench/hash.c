typedef long L;
typedef unsigned long U;
#define SAR(v, n) ((L)(v) >> (n))
static L keys[262144];
int body(void) {
    L size = 262144, found = 0; L x = 88172645463325252L;
    for (L i = 0; i < 200000; i++) {
        x ^= (L)((U)x << 13); x ^= SAR(x, 7); x ^= (L)((U)x << 17);
        L k = (L)(x & 65535) + 1;
        L h = (k * 2654435761L) & (size - 1);
        for (;;) {
            if (keys[h] == 0) { keys[h] = k; break; }
            else if (keys[h] == k) { found++; break; }
            else h = (h + 1) & (size - 1);
        }
    }
    return found % 256;
}
