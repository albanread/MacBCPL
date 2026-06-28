# MacBCPL documentation

This folder documents **MacBCPL** — the macOS / Apple-Silicon (arm64) fork of
[NewBCPL](https://github.com/albanread/NewBCPL). It contains only the docs that
are *specific to this port*. The original Windows-era NewBCPL docs (language
manifesto, K&R user guide, module system, Direct2D GUI design, corpus-sweep and
test-matrix journals, etc.) were inherited verbatim and are **not** repeated
here; they have been moved out of the repo to `../oldbcpldocs/` (mapped from the
`e:\oldbcpldocs` archive request). Pull any of them back if a language-level
reference is needed — the BCPL *surface language* is unchanged by the port.

## What changed in the Mac fork

MacBCPL is a hard fork: it does **not** preserve Windows compatibility. The
big departures from upstream NewBCPL are:

| Area | NewBCPL (Windows) | MacBCPL (macOS arm64) |
|------|-------------------|------------------------|
| Memory | precise mark-sweep tracing **GC** | **no GC** — per-function arenas + manual free-list heap + Cocoa retain/release ([memory_model.md](memory_model.md)) |
| Objects | vtable-based, GC-allocated | BCPL `CLASS` objects **are real Obj-C/Cocoa objects** ([cocoa_objects.md](cocoa_objects.md)) |
| GUI | Direct2D / DirectWrite `iGui_*` | Cocoa via the MacModula2 objc bridge (in progress) |
| Crash / `BRK` | Windows SEH + `RtlVirtualUnwind` stack walk | POSIX signal-safe handler + arm64 fp-chain walk ([crash_handling.md](crash_handling.md)) |
| Backend | MCJIT + Windows SEH unwind tables | MCJIT, default MM (DWARF `.eh_frame`), `NoFramePointerElim` ([macos_build.md](macos_build.md)) |
| Inline `asm` | x86 text asm via `new-asm` | stubbed on arm64 (no x86 inline asm) |

## Index

- **[the_new_bcpl_programming_language.md](the_new_bcpl_programming_language.md)** — the language manual: a K&R-style tutorial and reference for New BCPL (with Cocoa), covering the lexis, types, control flow, functions, pointers/vectors, lists, classes, and the memory model, with a grammar and standard-library appendix.
- **[macos_build.md](macos_build.md)** — toolchain, build env, and how to run the JIT on Apple Silicon.
- **[memory_model.md](memory_model.md)** — the no-GC, three-tier "heap objects with stack scope" model.
- **[cocoa_objects.md](cocoa_objects.md)** — `CLASS`/`EXTENDS`/`NEW`/`SUPER` lowered onto the Obj-C runtime; object ownership.
- **[crash_handling.md](crash_handling.md)** — signal-safe crash dumps, `BRK`, and JIT-aware stack walking.
- **[fork_notes.md](fork_notes.md)** — the directive trail and which Windows code paths were deleted vs. left dead.
