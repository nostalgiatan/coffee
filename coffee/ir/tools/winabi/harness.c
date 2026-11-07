#include <stdarg.h>
#include <stdio.h>
#include <stdint.h>
#define MS __attribute__((ms_abi))

MS long c_sum10(long a, long b, long c, long d, long e, long f, long g, long h, long i, long j) {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6 + g * 7 + h * 8 + i * 9 + j * 10;
}
MS double c_mix(long a, double x, long b, double y, long c, double z, double d, long e) {
    return a + x * 2 + b * 3 + y * 4 + c * 5 + z * 6 + d * 7 + e * 8;
}
MS long c_vsum(long n, ...) {
    __builtin_ms_va_list ap; __builtin_ms_va_start(ap, n);
    long s = 0;
    for (long k = 0; k < n; k++) s += __builtin_va_arg(ap, long) * (k + 1);
    __builtin_ms_va_end(ap);
    return s;
}
MS double c_vsumd(long n, ...) {
    __builtin_ms_va_list ap; __builtin_ms_va_start(ap, n);
    double s = 0;
    for (long k = 0; k < n; k++) s += __builtin_va_arg(ap, double) * (k + 1);
    __builtin_ms_va_end(ap);
    return s;
}
MS long c_id(long a) { return a; }
/* the library calls the generated code makes by itself, renamed in the object (run.sh) */
#include <stdlib.h>
MS void *ms_malloc(size_t n) { return malloc(n); }
MS void ms_free(void *p) { free(p); }

MS long cf_sum10(long, long, long, long, long, long, long, long, long, long);
MS double cf_mix(long, double, long, double, long, double, double, long);
MS float cf_f32(float, long, float);
MS long cf_busy(long, long, long, long);
MS long cf_call_c(long);
MS long cf_big(long);

/* Calls fn(a, b, c, d) in the Win64 convention with markers in every register it has to keep; *bad gets a bit for each that changed.
   The SysV arguments: fn rdi, a rsi, b rdx, c rcx, d r8, bad r9. */
long chk_call(void *fn, long a, long b, long c, long d, unsigned long *bad);
__asm__(
".text\n.globl chk_call\nchk_call:\n"
".intel_syntax noprefix\n"
".macro CHKR reg, val, bit\n mov rax, \\val\n cmp \\reg, rax\n je 2f\n or r11, (1 << \\bit)\n2:\n.endm\n"
".macro CHKX x, val, bit\n movq rax, \\x\n mov rdx, \\val\n cmp rax, rdx\n je 2f\n or r11, (1 << \\bit)\n2:\n.endm\n"
"push rbp\n mov rbp, rsp\n push rbx\n push r12\n push r13\n push r14\n push r15\n sub rsp, 40\n"
"mov r12, r9\n mov rax, rdi\n mov r10, rcx\n mov r11, r8\n mov rcx, rsi\n mov r8, r10\n mov r9, r11\n"
"mov rsi, 0x1111111111111111\n mov rdi, 0x2222222222222222\n mov rbx, 0x3333333333333333\n"
"mov r13, 0x4444444444444444\n mov r14, 0x5555555555555555\n mov r15, 0x6666666666666666\n"
".irp n, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15\n mov r10, 0x7000 + \\n\n movq xmm\\n, r10\n.endr\n"
"mov r10, rax\n"
"call r10\n"
"mov r10, rax\n xor r11d, r11d\n"
"CHKR rsi, 0x1111111111111111, 0\n CHKR rdi, 0x2222222222222222, 1\n CHKR rbx, 0x3333333333333333, 2\n"
"CHKR r13, 0x4444444444444444, 3\n CHKR r14, 0x5555555555555555, 4\n CHKR r15, 0x6666666666666666, 5\n"
"CHKX xmm6, 0x7006, 6\n CHKX xmm7, 0x7007, 7\n CHKX xmm8, 0x7008, 8\n CHKX xmm9, 0x7009, 9\n CHKX xmm10, 0x700a, 10\n"
"CHKX xmm11, 0x700b, 11\n CHKX xmm12, 0x700c, 12\n CHKX xmm13, 0x700d, 13\n CHKX xmm14, 0x700e, 14\n CHKX xmm15, 0x700f, 15\n"
"mov [r12], r11\n mov rax, r10\n"
"add rsp, 40\n pop r15\n pop r14\n pop r13\n pop r12\n pop rbx\n pop rbp\n ret\n"
".att_syntax\n"
);

static int bad_total;
static void check(const char *what, long got, long want) {
    if (got != want) { printf("FAIL %s: got %ld want %ld\n", what, got, want); bad_total++; }
}
static void checkd(const char *what, double got, double want) {
    if (got != want) { printf("FAIL %s: got %g want %g\n", what, got, want); bad_total++; }
}
static void kept(const char *what, unsigned long bad) {
    if (bad) { printf("FAIL %s: registers not kept (mask %#lx)\n", what, bad); bad_total++; }
}

int main(void) {
    unsigned long bad;
    check("sum10", cf_sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), c_sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    checkd("mix", cf_mix(1, 1.5, 2, 2.5, 3, 3.5, 4.5, 4), c_mix(1, 1.5, 2, 2.5, 3, 3.5, 4.5, 4));
    checkd("f32", cf_f32(1.5f, 3, 0.25f), 4.75);
    {
        long a = 3, b = 4, c = 5, d = 6, s = a + 2 * b, t = 3 * c + 4 * d, u = s + t;
        double x = 1.5 * a + b, y = 2.5 * c + d;
        check("busy", cf_busy(a, b, c, d), s + t + u + (long)(x * y));
    }
    for (long n = -3; n < 40; n += 7) {
        long a = c_sum10(n, 2, 3, 4, 5, 6, 7, 8, 9, n + 10);
        long m = (long)c_mix(n, 1.5, 2, 2.5, 3, 3.5, 4.5, 4);
        long v = c_vsum(4, n, 20L, 30L, 40L);
        long w = (long)c_vsumd(3, 1.5, 2.5, (double)n);
        check("call_c", cf_call_c(n), a + m * 1000 + v * 100000 + w * 10000000);
        check("big", cf_big(n), n * 1024 * 1023 / 2);
    }
    chk_call((void *)cf_sum10, 1, 2, 3, 4, &bad); kept("sum10", bad);
    chk_call((void *)cf_mix, 1, 2, 3, 4, &bad); kept("mix", bad);
    chk_call((void *)cf_f32, 1, 2, 3, 4, &bad); kept("f32", bad);
    check("busy via chk", chk_call((void *)cf_busy, 3, 4, 5, 6, &bad), 3 + 8 + 15 + 24 + (3 + 8 + 15 + 24) + (long)((4.5 + 4) * (12.5 + 6)) + 0 * 0 + (3 + 8) * 0);
    kept("busy", bad);
    chk_call((void *)cf_call_c, 5, 0, 0, 0, &bad); kept("call_c", bad);
    chk_call((void *)cf_big, 5, 0, 0, 0, &bad); kept("big", bad);
    if (!bad_total) puts("winabi ok");
    return bad_total != 0;
}
