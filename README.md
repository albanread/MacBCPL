# MacBCPL

**Modern BCPL on Apple Silicon — a Rust + LLVM compiler for macOS arm64, with
both a JIT (`run`) and AOT standalone executables (`build`).**

> Under development. `run` JITs and executes any program; `build` emits a
> standalone signed Mach-O executable — for console programs, the full memory
> model, Cocoa (system classes and user-defined `CLASS`es), and inheritance.
> Linking `modules-active/` into an AOT build is the remaining gap.

MacBCPL is the macOS / Apple-Silicon (arm64) fork of
[NewBCPL](https://github.com/albanread/NewBCPL), itself a recreation of the
modern BCPL dialect from [NBCPL](https://github.com/albanread/NBCPL). It is a
**hard fork**: it does not preserve Windows compatibility. The Windows-only
machinery — the tracing GC, the Direct2D / DirectWrite GUI, and the SEH unwind
stack — has been replaced with a native macOS stack:

- **no garbage collector** — a three-tier memory model where heap objects follow
  stack scope ([docs/memory_model.md](docs/memory_model.md));
- **BCPL objects are real Cocoa / Objective-C objects**
  ([docs/cocoa_objects.md](docs/cocoa_objects.md));
- **POSIX signal-safe crash handling** with JIT-aware backtraces
  ([docs/crash_handling.md](docs/crash_handling.md)).

The end-to-end pipeline runs: source → lex → parse → sema → IR → LLVM emit →
MCJIT → execute. Console programs JIT and run today, and Cocoa is reachable now
through Objective-C bracket message sends — far enough that **the bundled IDE
([examples/bcpl-ide.bcl](examples/bcpl-ide.bcl)) is itself a native Cocoa app
written in BCPL that edits and runs BCPL**: an editable source pane over an
output pane, a native menu bar, file open/save, and `Cmd-R` to compile and run
the buffer out of process.

## Quick start

Requires a Rust toolchain and **LLVM 22.1.x** from Homebrew at
`/opt/homebrew/opt/llvm`.

```sh
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm   # also set in .claude/settings.local.json
cargo build --workspace
cargo test  --workspace                             # green on arm64

newbcpl-driver run prog.bcl                          # JIT and execute
```

See [docs/macos_build.md](docs/macos_build.md) for the full toolchain and JIT
notes.

## The pipeline

| Crate | Role |
|-------|------|
| `newbcpl-lexer` | full BCPL surface — classic and dotted-float operators, section brackets, `*`-escapes, `#`/`#X` numbers, `%%` bitfield, `?` null literal |
| `newbcpl-parser` | recursive-descent over the dialect — `LIST` / `MANIFESTLIST`, `CLASS` / `EXTENDS` / `VIRTUAL` / `FINAL` / `MANAGED` / `SUPER`, `FOREACH` with lane destructuring, `AS` annotations, `RETAIN`, `USING`, multi-target assignment, `VEC` / `FVEC` paren-init, `FUNCTION` / `ROUTINE` forms, `LET f(…)=e AND g(…)=e` mutual recursion |
| `newbcpl-sema` | register-class type inference; attaches a hint to every expression, never errors on type grounds; computes class layouts (per-class ivars, own-relative field offsets); hard-diagnostic channel rejects `PRIVATE` / `PROTECTED` / `FINAL` violations |
| `newbcpl-ir` / `newbcpl-llvm` | typed lowering then codegen via inkwell (LLVM 22), MCJIT. `NEW` / field access / `VALOF` / every loop form / `SWITCHON` / GEP / lane access / PAIR-as-`<2 x i64>` / SIMD packs. Object dispatch lowers to `objc_msgSend` |
| `newbcpl-runtime` | the no-GC runtime: manual free-list heap, per-function arenas, the objc Cocoa bridge, the signal-safe crash handler, and the standard library (`WRITES`/`WRITEF`/`WRITEN`/`WRITEC`, `FLOAT`/`FIX`/`TRUNC`/`ENTIER`, `FSIN`…`FSQRT`, list ops, `RAND`/`RND`/`FRND`, typed allocators) |
| `newbcpl-loader` | module discovery in `./modules-active/` (or `$NEWBCPL_MODULES_ACTIVE`); `<stem>_<routine>` mangling; cross-module references resolve at link time |
| `newbcpl-driver` | phase dumps (`dump-tokens` / `-ast` / `-sema` / `-ir` / `-llvm` / `-asm`), `run` (JIT + execute), `test-folder <dir>`, and `gui` (Cocoa editor/runner, in progress) |

## Memory: no GC

NewBCPL's precise mark-sweep tracing GC is **gone**. The design goal is
*"heap-allocated objects follow stack-scope semantics"* — heap objects with LIFO
lifetimes, freed automatically at scope exit, no collector and no safepoints.

- **Tier 0** — stack / registers / static (locals, params, `VALOF` slot, string
  literals).
- **Tier 1** — per-function **arena** (bump alloc, freed wholesale at function
  exit): the default for `VEC` / `FVEC` / `TABLE` transients proven non-escaping
  by an escape pre-pass.
- **Tier 2** — program-global **manual free-list heap**: explicit `GETVEC` /
  `FREEVEC`, all lists, and the promotion target for anything that escapes.
- **Tier 3** — **Cocoa**: `NEW Class` objects managed by `objc_retain` /
  `objc_release`.

Cleanup rides the same exit machinery as `USING`/`RELEASE`, firing on every true
function-exit edge, innermost-first. **Use-after-free is the cardinal sin**: a
missed arena enter degrades to a leak, never a wild pointer. Full design in
[docs/memory_model.md](docs/memory_model.md).

## Objects are Cocoa objects

A BCPL `CLASS` instance is a real Objective-C object.
`CLASS` / `EXTENDS` / `NEW` / `SUPER` lower onto the Obj-C runtime via the objc
bridge: classes are `objc_allocateClassPair`'d with per-class ivars, methods are
`class_addMethod`'d under mangled `bcpl_<m>` selectors, dispatch is
`objc_msgSend`, and inheritance is real Obj-C superclassing. This object
extension is a non-standard, greenfield feature — free to evolve to fit Cocoa.
Details and the arm64 msgSend ABI in [docs/cocoa_objects.md](docs/cocoa_objects.md).

## Resource cleanup

`USING name = expr DO body` binds `name` for the body and disposes it at scope
exit — running the user `RELEASE` method (if any) then releasing the underlying
memory — on every way out (fall-through, `RETURN`, `RESULTIS`, `FINISH`,
`BREAK`, `LOOP`, `ENDCASE`), innermost-first. Object ownership is **automatic
for the common case** (scope-local `LET o = NEW C()` auto-releases at the
epilogue) and **explicit at the edges** (`USING` for deterministic disposal),
and never crashes (only `+1`-owned direct-`NEW` bindings are auto-released, so
over-release is impossible). Reassigning an owned binding raises a compile-time
leak warning. The `MANAGED` keyword still parses but is advisory.

## Crash handling and `BRK`

A POSIX signal-safe handler (`SIGSEGV` / `SIGBUS` / `SIGILL` / `SIGFPE` /
`SIGABRT` / `SIGTRAP`) dumps async-signal-safely and names frames against a
lock-free JIT-symbol registry. `BRK` is a real statement: it prints a banner,
heap summary, arm64 register context, and a hand-rolled `x29` frame-pointer
stack walk (libunwind can't traverse JIT frames). The JIT sets
`NoFramePointerElim` so the x29 chain stays intact across JIT frames — see
[docs/crash_handling.md](docs/crash_handling.md).

## Tests

`cargo test --workspace` is green on arm64. The eight-tier synthetic probe
matrix is the quality gate; `tests/.../arena.rs` carries use-after-free and
over-release catchers for the memory model. Six inline-x86-ASM probes are
`#[ignore]`d — x86 inline `asm` is unsupported on the fork (stubbed, as MacLocus
did).

## Documentation

This README is the overview; the [docs/](docs) folder holds the Mac-specific
detail (build, memory model, Cocoa objects, crash handling, fork notes). The
inherited Windows-era NewBCPL language docs (manifesto, K&R user guide, module
system, etc.) live in `../oldbcpldocs/`; the BCPL *surface language* is unchanged
by the port, so they remain the language-level reference — only their
platform-specific claims (GC, Direct2D, SEH) are superseded by the docs here.
See [docs/fork_notes.md](docs/fork_notes.md) for the full Windows→Mac divergence.
