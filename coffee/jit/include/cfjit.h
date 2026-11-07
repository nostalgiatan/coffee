/* cfjit: the Coffee JIT as a library. Link libcfjit.a (and libc); see jit/README.md.
 *
 * A context holds the functions the host offers and the modules loaded. A module is a .cir file
 * (coffee2 prog.cf --emit-cir) linked in memory. Its `c fn` functions keep their names and the C calling
 * convention (System V on x86-64): the host calls them through cfjit_sym, and the module calls the
 * host's functions by the names given to cfjit_register (declared in the module's source by a .cfc file,
 * `c fn log_line(msg: string) => void:`, and `use log_line in host of c`).
 *
 * No function may be called from two threads at once for one process (the compiler's switches are
 * global). Code of a module runs on the caller's thread and stack. */
#ifndef CFJIT_H
#define CFJIT_H
#include <stddef.h>

typedef long cfjit_ctx;   /* 0 is no context */

/* A new context; 0 when there are too many (16). */
cfjit_ctx cfjit_new(void);
/* Unmaps every module of the context. Nothing of it may run any more. */
void cfjit_free(cfjit_ctx);

/* Offers the C function at `fn` to modules loaded later, under `name`. 0, or -1 (name taken, bad context). */
long cfjit_register(cfjit_ctx, const char *name, void *fn);

/* Loads the module in data[0..n). Its number (> 0), or a negative code with the reason in err[0..errn):
 * -1 not a usable .cir, -2 the code could not be made, -3 out of memory, -4 a call nothing offers (the
 * reason names it). */
long cfjit_load(cfjit_ctx, const void *data, long n, char *err, long errn);

/* The address of a function or datum called `name` that a module defines or the host registered; NULL
 * when there is none. A `c fn` is callable through it with the C convention. */
void *cfjit_sym(cfjit_ctx, const char *name);

/* Takes module `n` out and unmaps it. 0; -1 no such module; -2 another module calls into it. */
long cfjit_unload(cfjit_ctx, long n);

/* Replaces module `n` by the .cir in data[0..len): the new one is linked first, then the other modules'
 * calls into the old one are rebound to the functions of the same names in the new one. The old module
 * stays mapped (code running in it finishes); free it with cfjit_unload when nothing runs there. The
 * number of the new module, or a negative code: -1 as for cfjit_load, -5 the new module lacks a function
 * the others call (nothing changed). */
long cfjit_replace(cfjit_ctx, long n, const void *data, long len, char *err, long errn);

#endif
