# Building and running MacBCPL on Apple Silicon

MacBCPL targets `aarch64-apple-darwin` (Apple Silicon, arm64). There is no
cross-compile story to Windows — the fork deletes the Windows paths rather than
gating them.

## Toolchain

- **Rust** (stable) with the host `aarch64-apple-darwin` target.
- **LLVM 22.1.x** from Homebrew at `/opt/homebrew/opt/llvm`. The build links
  against it through `inkwell` (target `aarch64`) and `llvm-sys`.
- Standard macOS frameworks (Cocoa / AppKit / CoreGraphics / CoreText / Metal)
  resolved at runtime via `dlsym` through the ported objc bridge — no link-time
  framework flags needed for the console path.

## Build environment

The required env var is set in the repo-local `.claude/settings.local.json`:

```
LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm
```

(If building outside this harness, export `LLVM_SYS_221_PREFIX` to the same
prefix before `cargo build`.)

Two optional environment variables tune the compiler/runtime at use time:

- `COCOA_SQLITE=/path/to/cocoa.sqlite` — point the Cocoa type-synthesis layer
  (Chapter 10 of the language manual) at the full shared `cocoa.sqlite` mirror
  (the sister `cocoa_data` DB, tens of thousands of classes) instead of the
  bundled curated subset. Read during type checking; unset is fine and
  deterministic.
- `NEWBCPL_MODULES_ACTIVE=/path/to/dir` — override the `./modules-active/`
  location the loader scans for library modules.

```sh
cargo build --workspace
cargo test  --workspace      # suite is green on arm64; 6 inline-x86-ASM probes are #[ignore]d
```

The inline-`asm` probes are `#[ignore]`d on purpose: x86 text assembly via
`new-asm` is unsupported on the arm64 fork, so `asm` procedures are stubbed
(the same approach MacLocus took).

## Driver

`newbcpl-driver` exposes the full phase pipeline. Source flows
`lex → parse → sema → IR → LLVM emit → MCJIT → run`:

```sh
newbcpl-driver dump-tokens  prog.bcl
newbcpl-driver dump-ast     prog.bcl
newbcpl-driver dump-sema    prog.bcl
newbcpl-driver dump-ir      prog.bcl     # shows @arena vs @heap alloc tags (see memory_model.md)
newbcpl-driver dump-llvm    prog.bcl
newbcpl-driver dump-asm     prog.bcl
newbcpl-driver run          prog.bcl     # JIT + execute (console programs verified)
newbcpl-driver build        prog.bcl     # AOT: emit a standalone Mach-O exe
newbcpl-driver build        prog.bcl --out myprog   # ... to a chosen path
newbcpl-driver test-folder  dir/         # JIT every .bcl under dir, emit a report
```

`gui` (the Cocoa editor/runner over the objc bridge) is the in-progress GUI
phase; the console `run` path is fully working.

## AOT executables (`build`)

`build` compiles ahead of time to a standalone, signed `Mach-O 64-bit
executable arm64` — no runtime, no JIT, just the program:

```sh
newbcpl-driver build hello.bcl --out hello
./hello                                   # runs on its own
```

Under the hood it emits a relocatable object (the program's code plus a C
`main` that installs the crash handler, opens an autorelease pool, calls
`START`, and pops the pool), then links it with `clang` against the runtime
static library `libnewbcpl_runtime.a` (built alongside the driver by
`cargo build -p newbcpl-runtime`) and the macOS frameworks
(Foundation/AppKit/CoreGraphics/…). `clang` ad-hoc-signs the arm64 binary, so it
runs without a separate `codesign` step.

Works today for **console programs**, the full **memory model** (arena, `{ }` /
`POOL` reclaim scopes, lists, `GETVEC`), **Cocoa bracket sends** (system classes
and user-defined ones), and **user `CLASS`es with inheritance** — the object
model works because `main` calls a generated `__bcpl_register_classes` (the AOT
analogue of the JIT's registrar: `objc_allocateClassPair` + per-class ivars +
`class_addMethod` with the emitted methods as IMPs) before `START`. The
signal-safe crash handler is active in the built binary. **Remaining gap:**
linking `modules-active/` modules into an AOT build (they are JIT-linked today);
a single-file program with classes and Cocoa is fully AOT.

**Global option `--no-autorelease-pool`** (valid anywhere on the line). Each
`run` is wrapped in an Objective-C autorelease pool by default, giving +0 /
convenience-constructor Cocoa objects a defined lifetime (drained at run end —
see Chapter 10 of the language manual). This flag turns that off, reverting to
"no pool in place" so +0 objects leak; useful for isolating allocation
behavior. +1 owned objects (`alloc`/`init`/`copy` and BCPL `NEW`) are released
deterministically at their scope and are unaffected either way.

## JIT specifics that matter on macOS

- **Memory manager:** MCJIT uses the default MM, which registers DWARF
  `.eh_frame` for unwinding. There is no Windows SEH MM and no
  `RtlAddFunctionTable`.
- **`opts.NoFramePointerElim = 1`** is set on the `LLVMMCJITCompilerOptions`.
  This is load-bearing: MCJIT otherwise elides frame pointers for JIT'd code
  (it ignores the per-function `"frame-pointer"="all"` string attribute that
  the separate dump-asm `TargetMachine` *does* honor), which would leave JIT
  routines doing `stp x29,x30` **without** `mov x29,sp` — a broken x29 chain
  that defeats the `BRK` / crash-handler stack walk. With it set, every JIT
  routine links the fp chain and the backtrace names the full BCPL call chain.
- The JIT registers each compiled function address into the crash-handler
  symbol registry (so dumps name BCPL routines, not raw addresses) — see
  [crash_handling.md](crash_handling.md).
