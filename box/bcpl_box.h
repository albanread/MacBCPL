/* bcpl_box.h -- BCPL for the BOX: the compiler and runtime as a static
 * library (libbcpl-box.a) for /init.  MacBCPL's box/ (design/BOX_PORT.md);
 * rosgd's modules/bcpl is its one user.
 *
 * The library knows nothing of RISC OS: it is given hooks.  One run at a
 * time; bcpl_box_end_run after each (or bcpl_box_recover after a fault). */
#ifndef BCPL_BOX_H
#define BCPL_BOX_H
#include <stddef.h>
#include <stdint.h>

struct bcpl_box_hooks {
    /* bytes for the VDU (Latin-1, "\n" a new line) */
    void (*write)(const char *p, size_t n);
    /* a line of input without its terminator: its length, or -1 at the
     * end of the input */
    long (*read)(char *buf, size_t max);
    /* SWI number (the X form) with r[0..9] in and out; *flags gets N Z C V
     * as bits 3-0.  An error: its number in *errnum and its block's RISC OS
     * address returned; else 0 */
    uint32_t (*swi)(uint32_t number, uint32_t r[10], uint32_t *flags, uint32_t *errnum);
    /* a SWI's number from its name: 0 if it is known, else -1 */
    int (*swi_number)(const char *name, uint32_t *number);
    /* a RISC OS file's bytes, malloc'd (*n its length), or NULL */
    char *(*file_load)(const char *name, size_t *n);
    /* 0 nothing, 1 a file, 2 a directory */
    int (*file_kind)(const char *name);
    /* grows the run's dynamic area by n bytes: the old end (a RISC OS
     * address), or 0 */
    size_t (*arena_grow)(size_t n);
};

void bcpl_box_set_hooks(const struct bcpl_box_hooks *h);
/* the library's directory (a Linux path); NULL: /usr/lib/bcpl */
void bcpl_box_set_library(const char *dir);

#define BCPL_COMPILE_ONLY 1u
#define BCPL_TIMES        2u
#define BCPL_DEBUG        4u   /* the JIT's sections and entry point written */
#define BCPL_OPT(n)       ((uint32_t)((n) + 1) << 4)
/* Compiles and runs the program `name` (a RISC OS name), argv[1..] its
 * arguments: its exit code (START's result, FINISH's 0 or STOP's code), or
 * -1 it did not compile (errors written), -3 it could not be started,
 * -5 it stopped with an error (written), -6 Escape */
int bcpl_box_run(const char *name, int argc, const char *const *argv, uint32_t flags);
/* Escape: the program ends at its next output, input or SWI */
void bcpl_box_stop(void);
/* after a fault jumped out of bcpl_box_run */
void bcpl_box_recover(void);
/* after every run */
void bcpl_box_end_run(void);
#endif
