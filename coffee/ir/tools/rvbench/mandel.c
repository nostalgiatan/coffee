typedef long L;
static L mandel(double cr, double ci) {
    double zr = 0.0, zi = 0.0; L k = 0;
    while (k < 200) {
        double t = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci; zr = t;
        if (zr * zr + zi * zi > 4.0) return k;
        k++;
    }
    return k;
}
int body(void) {
    L total = 0;
    for (L y = 0; y < 100; y++) for (L x = 0; x < 100; x++)
        total += mandel((double)x * 3.0 / 100.0 - 2.0, (double)y * 2.0 / 100.0 - 1.0);
    return total % 256;
}
