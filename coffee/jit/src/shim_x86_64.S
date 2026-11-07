/* The `syscall` of Coffee code is a call of cf_syscall(n, a, b, c, d, e, f); `cfld` supplies it in a
 * program of its own (ld's syscall_stub). The library gets the same code here, so it does not call
 * the host's C library for it. */
        .text
        .globl  cf_syscall
        .hidden cf_syscall
        .type   cf_syscall, @function
cf_syscall:
        movq    %rdi, %rax
        movq    %rsi, %rdi
        movq    %rdx, %rsi
        movq    %rcx, %rdx
        movq    %r8, %r10
        movq    %r9, %r8
        movq    8(%rsp), %r9
        syscall
        ret
        .size   cf_syscall, .-cf_syscall
        .section .note.GNU-stack,"",@progbits
