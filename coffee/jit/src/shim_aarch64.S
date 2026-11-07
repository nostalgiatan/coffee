/* cf_syscall(n, a, b, c, d, e, f) for aarch64 Linux: the number goes in x8, the arguments in x0..x5. */
        .text
        .globl  cf_syscall
        .hidden cf_syscall
        .type   cf_syscall, %function
cf_syscall:
        mov     x8, x0
        mov     x0, x1
        mov     x1, x2
        mov     x2, x3
        mov     x3, x4
        mov     x4, x5
        mov     x5, x6
        svc     #0
        ret
        .size   cf_syscall, .-cf_syscall
        .section .note.GNU-stack,"",%progbits
