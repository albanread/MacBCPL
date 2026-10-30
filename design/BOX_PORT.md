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
Not done (section 13): a program with a `CLASS` compiles and stops at
its first `NEW` with "classes and Objective-C are not in the box".

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

## 13. As built (30 Oct 2026)

Branches: this repository's `cli/bcpl-box`; A7232ToolChain's `bcpl` (from
`langs`, `origin/langs` merged in again on the way; worktree
`src/A7232ToolChain-bcpl`).

**Build.** `sh box/build.sh aarch64|x86_64` (cargo for
`<arch>-unknown-linux-musl`, zig 0.15 as the C compiler) makes
`BOX/caches/bcpl/out-<arch>/`: `libbcpl-box.a` (26.9 MB Arm, 26.0 MB Intel,
no LLVM inside: the build stops if an LLVM object is in the Rust archive),
`llvm-stamp.txt` (the NCL set's `.stamp`, checked by /init's link with
FasterBASIC's and Modula-2's), `bcpl_box.h`, and `lib/`, the ROM's
`/usr/lib/bcpl` (libhdr, maths, geom, riscos, declaritive). There is no
LLVM-inside build: the library has no dlopen or dlsym of its own.
(`BOX/caches/bcpl` is a link to `/Volumes/xb/BOX-overflow/bcpl`: the
`/Volumes/xc` disc ran out of space during this work.)

**Size.** /init's text, Arm: 78.6 MB on `langs`, 80.2 MB with BCPL
(+1.6 MB); Intel 85.6 to 87.3 MB (+1.7 MB). initramfs.cpio, Arm 216.9 to
219.0 MB; Intel 228.6 to 230.7 MB. The LLVM is the same one copy.

**What differed from the plan.**
- MacBCPL already ran programs with MCJIT, so boxjit.rs is the Mac's run
  path with Modula-2's memory manager and the box's checks; the Objective-C
  class registrar is not in it.
- `STATIC` is parsed but not lowered (a static is taken for an external
  routine): the library modules keep their state in `GLOBAL`s.
- Classes: not done. `NEW` of a class ends the program with "classes and
  Objective-C are not in the box"; a raw bracket send does the same.
- Program arguments: the C API takes them, but MacBCPL has no way for a
  program to read them, so they are dropped.

**Found and fixed on the way** (all for the Mac too, its tests unchanged:
`cargo test --workspace` passes but for `host_recovery`, a Windows-era
test of a panic through a JIT frame, which aborts on macOS as the README
says, before and after these changes):
- `VEC n` and `GETVEC(n)` gave n words, not n+1 as the language guide says:
  `v!n` was the next heap block's header (a SWI's r!9 read garbage).
- `FREEVEC` freed nothing: it looked for the heap header one word too high,
  so a GETVEC in a loop grew without end (the sieve ran out of memory).
- Every function polled a GC safepoint, at entry and at every loop
  back-edge, though there is no collector; and every function pushed and
  popped an arena whether or not it made anything in one. Both gone (the
  arena only where a VEC is routed to it): fib(30) 18.5 to 1.4 ms, the
  sieve 11.1 to 4.0 ms.
- An arena's first chunk was 64 KB, made, zeroed and freed on every call
  of a routine with a VEC: now 1 KB (dict 227 to 39 ms).

**Proved in the Arm box under QEMU** (slot A):
- `box/boxtest.sh` with `box/tests/go`: output through the VDU, WRITEF,
  JOIN, a library module; SYS and SYSN by number and name, registers out,
  XSYS's error block with V set, ERRNUM, ERRTEXT, a GETVEC buffer to a SWI,
  SYS's error ending the program; files written and read back as streams;
  STOP's code in Sys$ReturnCode; a compile error as `FILE:LINE: error:`;
  a fault ending the run and the next run normal; `-t`, `-O0`.
- `tools/build-arm-mac.sh --check`: 4 of 2050 checks failed, the known four
  (three share checks, one MimeMap); the 13 BCPL checks
  (boot/selftest_bcpl.c) and every FasterBASIC, Modula-2 and F66 check
  pass.
- The desktop, driven with deskdrive (box/deskbox.sh): `!BCPL` in the Filer
  with its icon, its !Run opening Examples with the &A78 icons; Hello
  double-clicked running in a TaskWindow and reading a name; Escape ending
  an endless printing loop; Greeter double-clicked running as its own task:
  the Declaritive window, its tick ("27 seconds"), change events ("Hello,
  Box!"), Quit.
- !Code: Hello dropped on it, highlighted, Language BCPL; F1 on WRITEF
  giving its signature and description; Build file (`*BCPL -c`) putting a
  compile error in the problems and F8 going to its line; Run file.

**Benchmarks** (`box/tests/bench.bcl`, the box's language comparison,
A7232ToolChain rosgd/tests/bench/languages, best of three, Arm box under
QEMU, -O2):

| test | BCPL | Modula-2 | FasterBASIC LLVM | C |
|---|---|---|---|---|
| fib(30) | 1.40 ms | 1.67 ms | 1.37 ms | 1.4 ms |
| sieve, primes below 2,000,000 | 4.03 ms | 4.24 ms | 7.0 ms | 4.2 / 4.3 ms |
| dict, 200,000 string keys | 40.8 ms | | | 24 ms |

A program compiles in a few milliseconds (Mandelbrot 5 ms): MacBCPL has no
large library to compile with every program.

**The Intel box.** Built on the Studio: `box/build.sh x86_64` and
`make ARCH=x86_64` in A7232ToolChain `bcpl`: `build/init` links the module,
the library and the one LLVM (an x86-64 static PIE; 87.3 MB of text).
Host-side, `sh box/hostcheck.sh` compiles box/tests/*.bcl (hello, swi,
files, finish, fault, bench) with the box's front end and code generator
for x86_64-unknown-linux-musl, as the JIT does (the large code model), with
the library modules each names, and checks: ELF x86-64; no 32-bit
PC-relative reference out of an object; every name an object needs is a
runtime builtin or in /init's dlsym table. All pass. Nothing has run on
x86-64: MCJIT's placement of code above 4 GB on x86-64, the unwinding of
FINISH and Escape through x86-64 JIT frames, and the fault escape there are
unproved.

**What must be run on the Mac Pro** (`ssh macpro`, BOX root
/Users/Shared/BOX; one session at a time):

1. A7232ToolChain branch `bcpl` and this repository's branch
   `cli/bcpl-box` side by side in `BOX/src` (as `A7232ToolChain-bcpl` and
   `MacBCPL-cli-bcpl-box`), with zfb2026 `cli/fbllvm-box` and MacModula2
   `cli/m2-llvmshare` as `langs` needs; `rosgd/tools/box-root.sh` in the
   first.
2. The libraries and the x86-64 NCL set, copied from the Studio together
   (`caches/intel-mac/llvm-libs-box-x86_64-ncl`, `caches/bcpl/out-x86_64`,
   and FasterBASIC's and Modula-2's): each library's `llvm-stamp.txt` must
   be the set's `.stamp`, or the link stops.
3. In `rosgd`: `tools/build-intel-mac.sh --check`. Pass: no more than the 4
   known failures, and the 13 checks from "the BCPL module is in the ROM"
   to "the run after a fault is as good as the first" reading `ok`, with
   every FasterBASIC and Modula-2 check.
4. The bench: `box/tests/bench.bcl` on a share as `bench,a78`, then
   `*BCPL HostFS::Host.$.bench` in a box under HVF; or langbench.seq's
   `bcpl` step.
5. The desktop (`run/run-x86_64.sh` with `make ARCH=x86_64 usershare`):
   `!BCPL`, a double-click on Examples.Hello and Examples.Greeter, Escape
   in a loop, and !Code with a BCPL file (Build, F8, F1, Run).

**Left.**
- Everything Intel beyond building and the host check: the run above.
- Classes (CLASS, NEW, methods): a small object runtime behind the
  `bcpl_objc_*` entry points and a msgSend trampoline for each
  architecture (section 8).
- `STATIC` in the compiler (lowered as GLOBAL is).
- Escape is seen when a program prints, reads or calls a SWI: a loop that
  does none of these cannot be stopped (as the other languages).
- Locals are on /init's stack: `@` of a local is not a RISC OS address
  (ADDR says so); VEC, GETVEC, strings and globals are.
- A VEC in a loop body that is not a `{ }` block takes new arena space each
  turn until the function returns (MacBCPL's per-function arena).
- Strings that CSTR, ERRTEXT and declaritive_poll make are kept until the
  run ends.
- `|` and `&` are bitwise and do not short-circuit (MacBCPL's dialect):
  examples use nested tests.
- A fault inside the runtime while it holds a lock would leave it held.
- !Code: no completion, rename or formatter for BCPL (MacBCPL has no
  formatter or language server); no "New BCPL project".
