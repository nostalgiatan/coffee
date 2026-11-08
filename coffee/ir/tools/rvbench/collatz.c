typedef long L;
int body(void) {
    L best = 0, at = 0;
    for (L start = 1; start < 20000; start++) {
        L n = start, steps = 0;
        while (n != 1) { if (n % 2 == 0) n = n / 2; else n = 3 * n + 1; steps++; }
        if (steps > best) { best = steps; at = start; }
    }
    return (at + best) % 256;
}
