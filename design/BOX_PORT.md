# BCPL on the BOX

How this compiler (MacBCPL: Rust, LLVM 22.1 through inkwell, driver
`newbcpl-driver`) goes onto the BOX, our 64-bit RISC OS translated to C
over Linux (A7232ToolChain/rosgd), the Arm box first and the Intel box
after.

Written 30 Oct 2026 from the code of this repository (branch
`cli/bcpl-box`) and of the ports made this week, whose designs this
follows and does not repeat: Modula-2 (MacModula2 `cli/m2-box`, the
closest model: Rust, inkwell, MCJIT in /init), the LLVM FasterBASIC
(zfb2026 `cli/fbllvm-box`), the Forth (MF66 `cli/forth-box`) and the QBE
FasterBASIC (FasterBASIC `cli/fasterbasic-box`), and the box's notes on
sharing LLVM (A7232ToolChain `langs`, rosgd/docs/llvm-in-the-box.md). The
box side is A7232ToolChain branch `bcpl`, made from `langs`.

## 1. What is wanted

- `*BCPL [-c] [-t] [-O0..3] [-run] <file> [<args>]` compiles a program
  and runs it in process, as `*Modula2` does: output through the VDU,
  input from the keyboard, files through RISC OS, Escape stops it, a fault
  ends the run and not the box.
- SWIs from BCPL by name and number, registers in and out, X forms that
  return the error; vectors and strings usable as SWI buffers.
- Declaritive windows (a library module and a Greeter example), the
  desktop application `!BCPL`, and BCPL in !Code (highlighting,
  Build/Run, problems, F1 on the standard library).
- Both boxes from one code base: LLVM emits AArch64 and x86-64.

## 2. Where the compiler runs: inside /init, with the one LLVM

As for Modula-2 and both FasterBASICs: programs call SWIs with registers
in and out, and the compiler and runtime assume 64-bit words and
pointers, so the compiler and runtime are a static library linked into
`/init` behind a small C API (`box/bcpl_box.h`), and rosgd's
`modules/bcpl` (the command, the VDU, the SWI hook, the fault escape) is
a near copy of `modules/modula2`.

The library is a new crate, `box/` here (`newbcpl-box`, a `staticlib`),
which uses the compiler's crates (lexer, parser, sema, ir, llvm, runtime)
and replaces only the driver.

**LLVM is not in the library.** On `langs` /init links the NCL set
(LLVM 22.1.2, `deps/build-llvm-libs-box.sh` with `LLVM_FOR_NCL=1`) once
for every language that JITs (Makefile `LLVM_NCL`, `LLVM_LIBS`). So
`box/build.sh` builds as MacModula2's does on `cli/m2-llvmshare`:

- the crate's `llvm-outside` feature turns on llvm-sys's
  `no-llvm-linking`: LLVM's calls stay undefined in `libbcpl-box.a`;
  the build stops if any LLVM object is found in the Rust archive;
- the same headers and options as the others: `LLVM_SYS_221_PREFIX` is
  the NCL set itself, and `llvm-stamp.txt` (the set's `.stamp`) is
  written beside the library; the Makefile's stamp check refuses a
  library built against another set;
- every symbol but `box/exports.txt` is made local (llvm-objcopy), so
  nothing of Rust's or the compiler's meets one of /init's;
- **no private static_dl**: MCJIT's `dlopen(NULL)` and `dlsym` are
  answered by /init's one copy, `runtime/llvm/llvm_dl.c` (the table with
  the Modula-2 port's fix: C routines resolving to null). The JIT's check
  for names it cannot bind asks that same `dlsym`, so it cannot disagree
  with what MCJIT will find;
- libc++ and libunwind are /init's (zig's), as for the others.

## 3. The JIT

MacBCPL already runs programs with MCJIT (`run_program_ir`), so the
engine is the same; what changes is the memory manager and the target,
as in MacModula2's `boxjit.rs`:

- the target machine from the host triple (aarch64- or
  x86_64-linux-musl), the large code model, frame pointers kept;
- a memory manager (`LLVMCreateSimpleMCJITMemoryManager`) whose code is
  mmap'd just above 4 GB (AArch64 MCJIT code reaches its data by ADRP,
  +-4 GB, whatever the code model) and whose data sections (the
  program's statics and globals) come from the run's dynamic area, so
  that `@` of a static is a RISC OS address;
- every runtime builtin bound by address (`LLVMAddGlobalMapping`, the
  existing `builtin_addresses` table); a declared name that is neither a
  builtin nor in `dlsym`'s table is a compile error, not a call to 0;
- the engine and its context live until the run ends and are disposed
  of then (the Mac leaks them): the code is unmapped and its `.eh_frame`
  deregistered (the `__register_frame` walk in llvm_dl.c registers it a
  FDE at a time, as zig's libunwind wants);
- `-O0..3` set LLVM's level (default 2); `-c` runs the whole back end
  without running the program, so that what only the back end sees is
  reported.

The Objective-C class registrar and the autorelease pool are left out of
the box's path.

## 4. Strings without Cocoa

A BCPL string is an `NSString` on the Mac: literals are made at first
use by `bcpl_objc_nsstring_immortal`, and the runtime reads them with
`nsstring_utf8_bytes`, `bcpl_str_len`, `bcpl_str_char`; `JOIN` makes
owned ones, freed by `bcpl_str_release`. In the box the same entry
points make and read **box strings**:

- a string is a pointer to its bytes, zero-terminated, in the run's
  dynamic area; a header just before them holds the length and whether
  it is immortal. So a string *is* the RISC OS address of a
  zero-terminated string and can be given to a SWI as it is;
- the bytes are Latin-1, RISC OS's alphabet: `LEN(s)` is the number of
  characters and `s % i` the i-th as a number, as on the Mac for any
  character below 256. Source files are read as Latin-1 (each byte one
  character), so a literal's characters come through unchanged; a
  character above 255 (from `WRITEC` or an escape) prints as "?";
- the compiler's emission does not change (the same calls), only the
  runtime behind the names.

## 5. Output, input, files, Escape, faults

The runtime writes through one function (`write_bytes`) and reads by
`RDCH`; in the box:

- **output**: the module's write hook (the VDU: OS_WriteC, OS_NewLine
  for "\n"), or the selected output stream (below);
- **input**: `RDCH` from a line read by OS_ReadLine32 through the read
  hook (`ENDSTREAMCH`, -1, at the end of input or on Escape), or from the
  selected input stream;
- **files**, new in the box (MacBCPL has none): classic BCPL streams,
  `FINDINPUT(name)`, `FINDOUTPUT(name)`, `FINDAPPEND(name)` (a stream or
  0), `SELECTINPUT(s)`, `SELECTOUTPUT(s)`, `INPUT()`, `OUTPUT()`,
  `ENDREAD()`, `ENDWRITE()`, `ENDSTREAM(s)`, `WRCH(c)`; with a stream
  selected, `RDCH` and every writer (`WRITES`, `WRITEF`, `WRITEN`, ...)
  use it. Names are RISC OS names; the bytes go through OS_Find and
  OS_GBPB with a buffer in the dynamic area. Every stream is closed when
  the run ends;
- **FINISH** ends the program, not /init (the Mac calls
  `process::exit`): it raises a Rust panic with a marker that unwinds
  through the JIT's frames (their `.eh_frame` is registered) to the
  run's `catch_unwind`, as Modula-2's HALT does. `FINISH`'s code (or
  START's result) goes to `Sys$ReturnCode`;
- **Escape** is checked in the write, read and SWI hooks (with a turn for
  other tasks in a TaskWindow every 10 cs, as `*Modula2`); when it is
  seen the program ends as by FINISH, with "Escape";
- **faults**: the module's `ros_fault_escape` around each run; after a
  fault `bcpl_box_recover` disposes of the engine and resets the runtime
  (streams, the heap's state, the arena). The runtime's own crash
  handler (POSIX signals, crash.rs) is not installed in the box.
  `BRK` prints its banner and heap summary and continues.

## 6. Memory the SWIs can see

RISC OS addresses are 32-bit: the arena is the low 4 GB of /init's
address space. As for the Forth and Modula-2, the program's **data is in
a RISC OS dynamic area** ("BCPL", made at the first run, grown as needed,
started again at its base by each run):

- the tier-2 heap (`GETVEC`, `FREEVEC`, lists' cells, escaping vectors),
  the tier-1 per-function arenas (`VEC`, `TABLE`), the record allocator
  and strings: their system memory is a dlmalloc whose pages are the
  dynamic area (the runtime's `std::alloc` calls in heap.rs, gc.rs and
  the cons slabs go through one function that uses it in the box);
- the program's statics and globals: the JIT's data sections (above).

So every vector and string a program has is a RISC OS address and can
be a SWI's buffer. Only code, locals in registers or on /init's stack,
and Rust's own memory stay outside; a register value for a SWI that does
not fit in 32 bits is refused with an error, never truncated.

## 7. SWIs from BCPL

Builtins in the runtime (riscos.rs), on BCPL words:

```bcpl
SYS(n, r)        // calls SWI n with r!0..r!9 in and out; an error ends
                 // the program with the error's text (as FINISH)
XSYS(n, r)       // the X form: 0, or the error block's address (V set)
SYSN(name, r)    // by name: "OS_Write0", "Wimp_Initialise", ...
XSYSN(name, r)
SWINUMBER(name)  // OS_SWINumberFromString, -1 if there is none
SYSFLAGS()       // N Z C V of the last call, bits 3-0
ERRNUM(e)        // an error block's number
ERRTEXT(e)       // its text, as a new string
CSTR(a)          // a string from a control-terminated string at address a
ADDR(p)          // p checked to be a RISC OS address (an error if not)
```

`r` is a vector of at least 10 words (`LET r = VEC 9`); each word in is
taken as 32 bits (from -2^31 to 2^32-1, else an error), each word out is
the register zero-extended. A string can go into a register as it is
(`r!0 := "Hello*N"; SYSN("OS_Write0", r)`); so can a vector (`VEC`,
`GETVEC`) and `@` of a static, which are in the arena; byte buffers are
vectors indexed with `%`.

A library module `riscos` (BCPL source in the ROM's library, below)
gives common calls over these: `riscos_write0(s)`, `riscos_time()`,
`riscos_cli(s)`, `riscos_vdu(...)`, `riscos_mode(...)` and the like.

## 8. The language in the box

Kept: the whole surface language (lexer, parser, sema and lowering are
unchanged), floats and the F-operators, SIMD packs, lists, `VALOF`,
`SWITCHON`, `GET`, `USING`/`RELEASE`, the three-tier memory model
(tiers 0-2), modules.

**Modules**: on the Mac every `.bcl` in `modules-active/` is linked into
every program. In the box the library is in the ROM, `/usr/lib/bcpl`
(`libhdr`, `maths`, `geom`, and the box's `riscos` and `declaritive`),
and only the modules a program refers to are linked: a module `foo` is
linked if the program (or a linked module) names `foo_...`. `GET "x"`
looks beside the program through RISC OS (`x`, `x/bcl`, `x/h`), then in
the library.

**Classes**: on the Mac a `CLASS` is an Objective-C class and dispatch is
`objc_msgSend`. The box has no Objective-C runtime. Planned: a small
object runtime in Rust behind the same `bcpl_objc_*` entry points (a
class table, methods by selector, reference counts, instances in the
arena) and a `msgSend` trampoline in assembly for AArch64 and x86-64,
which looks up the method and jumps to it with the arguments untouched.
Until then a program with a `CLASS` compiles and stops at its first `NEW`
with "classes are not in the box yet" (section 12 says which).

Gone, and left out of the build: Cocoa, Objective-C bracket sends
(`[obj sel: x]`: a compile error in the box), the autorelease pool and
`POOL` (a plain block in the box), the IDE's Cocoa helpers
(`bcpl_run_*`, `bcpl_set_text_color` ...), iGui, audio, inline x86 `ASM`
routines, `build` (AOT executables: the box has no linker for them), and
the POSIX crash handler.

## 9. Declaritive

`declaritive.bcl` in the library, over the module's SWIs (chunk &C0380,
as declaritive/bindings/c): `declaritive_create(json)`,
`declaritive_poll(ev)`, `declaritive_get(win, id)`,
`declaritive_set(win, id, text)`, `declaritive_destroy(win)`. A program
passes its JSON, becomes a Wimp task and handles events in a loop.
Example: `Greeter`, as !Modula2's and !F66's.

## 10. The desktop and !Code

`!BCPL` (in this repository, `box/app/!BCPL`; `make usershare` in
A7232ToolChain copies it): an icon, a file type, `!Help`, Examples
(Hello, SWIs, Files, Greeter, Bench, and a game).

- &A78 "BCPL": a BCPL program or module (`.bcl`). (Free in the box: &A73
  is Python's, &A75 FasterBASIC's, &A77 the Forth's, &A79 CUE, &A7A
  MJPEG, &A7B and &A7C Modula-2's, &A7D !Studio's, &A7E and &A7F
  Office's.)

Double-clicking a program runs it with `*BCPL -run`: in a TaskWindow, or
as a task of its own if it makes a Declaritive window.

!Code (rosapps/code-engine): a BCPL highlighter in syntax.rs (keywords,
`//` and `/* */` comments, strings with `*` escapes, `$( $)`, numbers
with `#`/`#X`), Build (`*BCPL -c`) and Run of the file in front, the
compiler's `file:line: error: text` lines parsed into the problems list,
F1 help from the library's modules and a table of the builtins. MacBCPL
has no formatter, so there is none here.

## 11. The Intel box

`/init` is x86_64-linux-musl there, so the same library is built for
x86-64 against `caches/intel-mac/llvm-libs-box-x86_64-ncl` (the X86
backend only). What the Studio can prove: the library and the Intel ROM
build and link; host-side (`box/hostcheck.sh`, as MacModula2's), the
box's code generator makes x86-64 objects for the test programs, checked
to be ELF x86-64 with no 32-bit PC-relative reference out of an object
and every name bound by the JIT. What must be run on the Mac Pro is
written down when that step is done.

## 12. Steps

1. This design.
2. `box/build.sh aarch64`; A7232ToolChain `bcpl` links it; `*BCPL` runs a
   program that prints and calls SWIs in the Arm box under QEMU.
3. SWIs, streams, `riscos` and `declaritive`; `!BCPL` with examples;
   self-test checks (`boot/selftest_bcpl.c`);
   `tools/build-arm-mac.sh --check` with only its 4 known failures and
   every other language's checks passing; the desktop driven with
   deskdrive. Benchmarks: fib(30) and the sieve beside the others.
4. !Code.
5. Intel: build, host checks, the Mac Pro run written down.
