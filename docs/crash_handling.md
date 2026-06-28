# Crash handling, BRK, and JIT-aware backtraces

MacBCPL replaces NewBCPL's Windows SEH crash machinery (`RtlVirtualUnwind`
stack walker, `RtlAddFunctionTable`) with a **POSIX signal-safe crash handler**
ported from MacModula2 (`src/newbcpl-runtime/src/crash.rs`). It is mac-only.

## Signal handler

`bcpl_install_crash_handler` installs `sigaction` handlers for **SIGSEGV,
SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP**. The handler is **async-signal-safe**:
it formats into a fixed `Line` buffer and emits via raw `write(2)` — no
allocation, no locks on the hot path. It names frames via `dladdr` plus a
**lock-free frozen JIT-symbol registry**, then re-raises.

### JIT symbol registry

JIT'd code has no symbols `dladdr` can see, so the LLVM layer registers each
compiled function (its `LLVMGetFunctionAddress`) into the registry at run start
(`bcpl_register_jit_symbol`), then freezes it (`bcpl_finalize_jit_symbols`)
before `START`. `resolve_jit_symbol` turns a PC inside JIT'd code back into a
BCPL routine name for the dump.

## BRK

`BRK` is a real statement (not just a debugger trap). It lowers to a dump that
prints, mac-only:

- a banner,
- the heap summary (`gc::HEAP_COUNTERS`),
- the arm64 register context — `pc` / `sp` / `fp` read via inline asm
  (`x29` / `x30` / `sp`),
- a **hand-rolled `x29` frame-pointer stack walk**.

The stack walk is hand-rolled on purpose: `libunwind` / `libc::backtrace`
**cannot** traverse JIT frames, so the dump follows the `x29` chain manually,
naming each frame against the JIT symbol registry (inner → middle → `START`).

## The frame-pointer gotcha

The whole stack walk depends on JIT routines maintaining a valid `x29` chain.
MCJIT, left to itself, **elides frame pointers** for JIT'd code — it ignores the
per-function `"frame-pointer"="all"` attribute (which only the separate
dump-asm `TargetMachine` honors), emitting `stp x29,x30` **without**
`mov x29,sp`. That breaks the chain and the walk sees only the innermost BCPL
frame.

The fix is **`opts.NoFramePointerElim = 1`** on the `LLVMMCJITCompilerOptions`
(set alongside `OptLevel`). With it, every JIT routine links the fp chain and
`BRK` / the crash handler name the full BCPL call chain. (The
`"frame-pointer"="all"` function attribute is also set in `declare_function` as
belt-and-suspenders — honored by dump-asm.)

## Panic path

A runtime-helper Rust panic **cannot unwind through a JIT frame** on mac (MCJIT
registers no usable DWARF `.eh_frame` for the JIT'd code → *"failed to initiate
panic error 5"* → `SIGABRT`). This is intentional fork behaviour: the quiet
panic-hook suppression was removed so the panic message reaches stderr, then the
process aborts and the crash handler dumps it. (`expect_reject` tests only need
a non-zero exit and a substring match.)

## Verified

- `SIGSEGV` in a BCPL routine → `=== MacBCPL fatal signal: SIGSEGV ===` plus a
  backtrace naming `BCPL crash` / `START`.
- A runtime panic → the panic message followed by a `SIGABRT` dump.
- `BRK` → the full inner→`START` call chain.

The workspace test suite is green on arm64 (the 6 inline-x86-ASM probes are
`#[ignore]`d — unsupported on the fork).
