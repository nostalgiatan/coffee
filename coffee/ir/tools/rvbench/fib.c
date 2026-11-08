typedef long L;
L fib(L n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }
int body(void) { return fib(27) % 256; }
