int body(void);
void _start(void) {
    register long a0 __asm__("a0") = body();
    register long a7 __asm__("a7") = 94;
    __asm__ volatile("ecall" :: "r"(a0), "r"(a7));
    for (;;) {}
}
