# Fork notes: MacBCPL vs. NewBCPL

MacBCPL is a **hard fork** of [NewBCPL](https://github.com/albanread/NewBCPL).
The standing directive is: **do not preserve Windows compatibility.** Windows /
SEH code paths are *deleted* rather than `cfg(windows)`-gated wherever
practical. This note records what diverged and what is still around as dead
code.

## What was dropped

- **`bcpl-wingui`** (Direct2D / DirectWrite `iGui_*`) — removed from the
  workspace members; the GUI is being reimplemented over Cocoa via the
  MacModula2 objc bridge.
- **Windows SEH JIT memory manager** + `RtlAddFunctionTable` — replaced by the
  MCJIT default MM (DWARF `.eh_frame`).
- **`RtlVirtualUnwind` stack walker / SEH crash machinery** — replaced by the
  POSIX signal-safe handler ([crash_handling.md](crash_handling.md)).
- **Tracing mark-sweep GC** — replaced by the arena + manual-heap + Cocoa model
  ([memory_model.md](memory_model.md)). No collector, no safepoints.
- **x86 inline `asm`** via `new-asm` — stubbed on arm64 (the 6 inline-asm probes
  are `#[ignore]`d).

## Build wiring

- `new-asm` points at the MacModula2 arm64 RASM port.
- The `NewAudio` dependency (runtime `audio.rs`) is gated/stubbed for now.
- `inkwell` target switched `x86` → `aarch64`; `llvm-sys` added against Homebrew
  LLVM 22.1 (`LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm`).

## Sibling reference port

MacBCPL reuses runtime infrastructure from **MacModula2**
(`/Users/oberon/claudeprojects/MacModula2`), the same LLVM/inkwell/llvm-sys
stack: the objc Cocoa bridge (`objc.rs`), AArch64 coroutine context switch
(`coroutine.rs`), `mmap`-based allocation (`win32_compat.rs`), and the
signal-safe crash handler (`crash.rs`). MacLocus is the precedent for stubbing
x86 inline asm on arm64.

## Dead code still present (safe to delete later)

These remain in the tree but are unreferenced or `cfg(windows)`-gated after the
retargets; left in place to keep diffs reviewable:

- `src/newbcpl-llvm/src/jit_mm.rs` — Windows SEH MM (only referenced under
  `cfg(windows)`).
- `src/newbcpl-runtime/src/igui*.rs` — Direct2D GUI (`cfg(windows)`).
- `gc.rs` `register_jit_vtable_methods` / `__newbcpl_lookup_method` — dead after
  the Obj-C object retarget; assorted `cfg(windows)` blocks in
  `gc.rs` / `builtins.rs`.
- `@Class.vtable` / `@Class.method_names` global emission in `emit.rs` — dead
  after dispatch moved to `objc_msgSend`.
- `VTABLE_HEADER_BYTES` and `FieldLayout.offset` — superseded by `own_offset`.

## Where the old docs went

The inherited Windows-era NewBCPL docs (manifesto, user guide, module system,
`wingui_bcpl_design`, corpus-sweep / test-matrix / reference-audit journals,
`fastload`, `jit_typedesc_lifetime`, `pair_and_multilane_types`) were moved out
of the repo to `../oldbcpldocs/`. The BCPL *surface language* is unchanged by the
port, so those remain the reference for language-level questions; only the
platform-specific claims (GC, Direct2D, SEH) in them are now superseded by the
docs in this folder.
