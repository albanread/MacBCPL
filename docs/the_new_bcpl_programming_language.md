# The New BCPL Programming Language

### with Cocoa, for Apple Silicon

*A description of MacBCPL, in the tradition of Kernighan & Ritchie.*

---

## Preface

BCPL is a small language. Martin Richards designed it in 1967 as a language for
writing compilers; it gave C its braces (here written `$(` and `$)`), its
view of memory as a vector of words, and its conviction that a programmer who
is trusted will write better programs than one who is fenced in.

**MacBCPL** — *New BCPL* — is a modern BCPL for macOS on Apple Silicon. It keeps
the old language's spareness and its single-word-typeless heart, and adds three
things the old language never had: IEEE floating point as a first-class
citizen, a class system in which **an object is a real Cocoa object**, and a
memory model in which heap data follows the stack's scopes so that there is no
garbage collector to pause your program and no `free` you are required to call.

The compiler is a JIT. You write a program, the driver lexes, parses, checks,
lowers to LLVM, and runs it — there is no separate executable yet. Everything
in this book has been run.

This book is a tutorial and a reference. The first chapter teaches the language
by example; the middle chapters take up types, control flow, functions,
pointers, lists, and classes in turn; the appendices are the cold reference —
the lexis, the standard library, and the grammar. Read the first chapter
straight through; thereafter, dip.

A note on convention, settled once: **keywords are UPPER CASE, your names are
lower case.** The lexer enforces it — `LET` is the keyword, `let` is a variable.
This is not nostalgia; it is what lets a 66-word vocabulary live in the same
namespace as your identifiers without clashing.

---

## Chapter 0. Introduction

The only way to learn a new language is to write programs in it. The first
program to write is the same for all languages:

> Print the words `hello, world`

Here it is in MacBCPL. Put it in a file called `hello.bcl`:

```bcpl
LET START() BE
    WRITES("hello, world*N")
```

Run it:

```sh
newbcpl-driver run hello.bcl
```

and it prints

```
hello, world
```

Already there are things to explain. `START` is where execution begins — the
entry routine, by convention. `LET … BE` introduces a **routine**, a procedure
that does something but returns no value. `WRITES` writes a string. And the
string ends in `*N`, not `\n`: in BCPL the escape character is the asterisk.
`*N` is newline, `*T` is tab, `*"` is a quote, and `**` is an asterisk itself.

If you leave off the `*N`, the cursor stays on the same line; BCPL prints
exactly what you tell it to and nothing more.

A program is a sequence of **declarations**. There is no `main` ceremony beyond
naming a routine `START`. Comments are as in C — `//` to end of line, or
`/* … */` for a block:

```bcpl
// the customary greeting
LET START() BE
    WRITES("hello, world*N")   /* could also be {WRITES(...)} */
```

The body of `START` here is a single statement. When you need several, group
them with **section brackets**, `$(` and `$)`. Curly braces `{` and `}` are
exact synonyms, so use whichever you like:

```bcpl
LET START() BE $(
    WRITES("hello, world*N")
    WRITEN(2026)
    NEWLINE()
$)
```

`WRITEN` writes an integer in decimal; `NEWLINE()` writes a line break. Section
brackets may carry a tag for readability, matched at the close: `$(loop … $)loop`.

---

## Chapter 1. A Tutorial Introduction

### 1.1 Variables and arithmetic

A program that prints a small table of squares shows variables, a loop, and
arithmetic:

```bcpl
LET START() BE $(
    LET n = 0
    WHILE n <= 10 DO $(
        WRITEN(n)
        WRITES("  ")
        WRITEN(n * n)
        NEWLINE()
        n := n + 1
    $)
$)
```

`LET n = 0` declares a variable and gives it an initial value. `:=` is
assignment — `=` is reserved for equality and for the `=` of a `LET`. The
`WHILE … DO` loop repeats its body as long as the condition holds.

Every value in BCPL is a single **64-bit word**. An integer is a word; a
pointer is a word; a character is a word holding a small code; `TRUE` is the
word 1 and `FALSE` is 0. There is no separate boolean or byte type to declare —
there is the word, and what you do with it.

The arithmetic operators are `+ - * /` and `REM` for remainder. Comparison is
`= ~= < <= > >=` (note `~=` for "not equal"). They yield `TRUE` or `FALSE`,
which are just 1 and 0, so you can do arithmetic on them if you wish.

### 1.2 The FOR statement

The square table is more naturally a `FOR`:

```bcpl
LET START() BE
    FOR i = 0 TO 10 DO $(
        WRITEN(i)
        WRITES("  ")
        WRITEN(i * i)
        NEWLINE()
    $)
```

`FOR i = 0 TO 10 DO` counts `i` from 0 through 10 inclusive. A step other than 1
is written `BY`:

```bcpl
FOR i = 10 TO 0 BY -1 DO WRITEN(i)
```

The loop variable is local to the loop.

### 1.3 Floating point

Integers are not the whole story. MacBCPL has IEEE doubles, and — because every
value is one untyped word — it distinguishes integer arithmetic from float
arithmetic *by the operator*, not by the variable. The float operators are the
ordinary ones with a dot:

```bcpl
LET START() BE $(
    LET c = 100.0
    LET f = c *. 9.0 /. 5.0 +. 32.0
    FWRITE(f)               // float-write: prints 212
    NEWLINE()
$)
```

`+.` `-.` `*.` `/.` are floating add, subtract, multiply, divide; `<.` `>.`
`=.` and the rest are the floating comparisons. (If you prefer, `+#` `-#` … are
accepted as synonyms for the dotted forms.) `FWRITE` prints a double; `WRITEN`
prints an integer.

To cross between the two worlds, `FLOAT(n)` turns an integer into a double and
`FIX(x)` truncates a double back to an integer:

```bcpl
LET avg = FLOAT(total) /. FLOAT(count)
```

The reason for the separate operators is the typeless word: the bits `3.14` and
the bits of some large integer are indistinguishable in storage, so the
*operation* must declare which it means. Use `+` on doubles and you will add
their bit patterns as integers — nonsense. The compiler's type inference tries
to keep you honest by tracking which values are floats, but the operator is the
ground truth.

### 1.4 Strings and characters

A string literal `"…"` is a **Cocoa text object** — an `NSString`, not a raw
array of bytes (Chapter 9 tells the full story; it matters because a "character"
is a Unicode code point, not a byte). A character literal is in single quotes,
and the `*`-escapes apply in both:

```bcpl
LET START() BE $(
    LET nl = '*N'
    WRITES("tab*There, newline next")
    WRITEC(nl)
$)
```

`WRITEC` writes one character. The common escapes are `*N` newline, `*T` tab,
`*S` space, `*C` carriage return, `*B` backspace, `*P` form-feed, `*"` quote,
and `**` asterisk; `*c` for any other `c` is just that byte.

### 1.5 Functions

A function returns a value. Where a routine is `LET name(args) BE statement`, a
function is `LET name(args) = expression`:

```bcpl
LET square(x) = x * x

LET START() BE $(
    WRITEN(square(12))      // 144
    NEWLINE()
$)
```

When the answer is not a single expression, use a **VALOF** block and
**RESULTIS** to deliver the result:

```bcpl
LET factorial(n) = VALOF $(
    LET p = 1
    FOR i = 2 TO n DO p := p * i
    RESULTIS p
$)
```

`VALOF` says "this block computes a value"; `RESULTIS e` is its `return`. A
routine that wants to stop early uses `RETURN` (no value); `FINISH` stops the
whole program.

### 1.6 Arrays — vectors

BCPL's array is the **vector**, a block of consecutive words. `VEC n` makes a
vector with room for `n+1` words, indexed `0` to `n`. The subscript operator is
`!`:

```bcpl
LET START() BE $(
    LET v = VEC 9           // ten words, v!0 .. v!9
    FOR i = 0 TO 9 DO v!i := i * i
    FOR i = 0 TO 9 DO $(
        WRITEN(v!i)
        WRITES(" ")
    $)
    NEWLINE()
$)
```

`v!i` is "the word at offset `i` from `v`" — it is both how you read and how you
assign (`v!i := …`). A vector of doubles is `FVEC n`, subscripted the same way
but holding floats.

### 1.7 A first taste of objects

Here is the shape of things to come. A class bundles data with the procedures
that act on it, and in MacBCPL an instance is a genuine Cocoa object:

```bcpl
CLASS Counter $(
    DECL n
    ROUTINE CREATE(start) BE SELF.n := start
    ROUTINE bump() BE SELF.n := SELF.n + 1
    FUNCTION value() = SELF.n
$)

LET START() BE $(
    LET c = NEW Counter(10)
    c.bump()
    c.bump()
    WRITEN(c.value())       // 12
    NEWLINE()
$)
```

`NEW Counter(10)` allocates an object and runs its `CREATE`; `c.bump()` sends it
a message; `SELF` is the receiver inside a method. We will return to all of this
in Chapter 7 — including the fact that `c` is released automatically when it
falls out of scope, with no `free` for you to write.

> **Exercise 1-1.** Write a program that prints the Fahrenheit-to-Celsius table
> for 0, 20, 40, … 300 degrees, using floating point and a `FOR` loop with `BY`.
>
> **Exercise 1-2.** Write `power(base, n)` as a function using `VALOF`, then
> rewrite it with mutual recursion (see §4.3). Print a table of powers of 2.

---

## Chapter 2. Types, Operators, and Expressions

### 2.1 The word, and the things in it

There is one storage type — the 64-bit word — and several *interpretations* of
it that the compiler tracks as hints:

| Interpretation | What the word holds |
|----------------|---------------------|
| integer | a signed 64-bit value |
| float | the bits of an IEEE double |
| pointer | a memory address; `?` is the null word, 0 |
| string | a handle to a Cocoa `NSString` object (Chapter 9) |
| character | a small integer — a Unicode code point |
| boolean | `TRUE` (1) or `FALSE` (0) |

A variable does not have a fixed type you declare; the value in it is just a
word, and the operators you apply decide what it means. This is the source of
the language's economy and of its one discipline: **match the operator to the
data.** Integer `+` on float bits is a bug the machine will cheerfully execute.

The compiler's semantic pass attaches a *type hint* to every expression to pick
the right machine instruction (integer vs. floating, scalar vs. vector) and to
choose how to print or dispatch. It never rejects a program on type grounds —
at most it warns. The word is sovereign.

### 2.2 Constants

Integers may be written in decimal, in octal with a leading `#`, or in
hexadecimal with `#X`:

```bcpl
255        #377       #XFF       // the same value three ways
```

Floating constants have a decimal point or an exponent: `3.14`, `0.5`, `1e10`,
`6.022e23`, `2E-3`. A point must have a digit after it, so `3.` is not a float —
write `3.0`.

Character constants are one code point in single quotes, escapes allowed: `'a'`,
`'0'`, `'*N'`, `'*''`. String constants are in double quotes: `"abc*N"` — and
each is a Cocoa `NSString` object, not a byte array (Chapter 9). The
null/uninitialized value is `?`. The booleans are `TRUE` and `FALSE`.

Names for constants are made with **MANIFEST**, whose values are substituted at
compile time and occupy no storage:

```bcpl
MANIFEST $(
    maxline = 1000
    tab     = 9
    eof     = -1
$)
```

### 2.3 Declarations

`LET name = expr` introduces a variable with an initial value. Several may be
declared at once, and `LET … AND …` introduces them simultaneously (important
for mutually recursive functions, §4.3):

```bcpl
LET x = 1 AND y = 2 AND z = 3
```

`FLET` is `LET` for a value you intend to be floating — it nudges the type
inference toward float. `STATIC` names keep their storage for the life of the
program; `GLOBAL` names are visible across modules; `MANIFEST` names are
compile-time constants. A bare `?` initializer leaves a variable uninitialized:

```bcpl
LET scratch = ?            // value unspecified until assigned
```

### 2.4 Arithmetic and the floating twins

Integer operators: `+ - * /` and `REM` (remainder). Unary minus negates.

Floating operators wear a dot: `+. -. *. /.`. There is no float remainder
operator; use the library.

```bcpl
LET miles = km *. 0.621371
LET rem   = a REM b
```

### 2.5 Relational and logical operators

The comparisons are `= ~= < <= > >=` for integers and `=. ~=. <. <=. >. >=.`
for floats. They produce `TRUE` or `FALSE`.

The **logical** connectives are `AND`, `OR`, `NOT`, and `XOR`. They work on
truth values:

```bcpl
IF n > 0 AND n < limit THEN ...
UNLESS done OR failed DO ...
```

A caution for those coming from older BCPL: here **`OR` is purely a logical
operator**, never an "else" marker. The conditional that older texts wrote with
`OR` is written here with `->` (§2.8) or with `TEST … THEN … ELSE`.

The **bitwise** operators are separate words so they are never confused with the
logical ones: `BAND BOR BXOR BNOT`, plus the symbol forms `& | ^` and the shifts
`<< >>`. `EQV` and `NEQV` are bitwise equivalence and its negation.

```bcpl
LET masked = flags BAND #XFF
LET hi     = (w >> 32) BAND #XFFFFFFFF
```

### 2.6 Increment and the assignment

There is no `++`. Assignment is `:=`, and `n := n + 1` is the idiom. Assignment
is a statement, not an expression, which removes a whole class of `=`/`==`
confusion: you cannot accidentally assign inside a condition.

A useful extension is **multiple assignment** — several targets, several
sources, evaluated and then stored in parallel:

```bcpl
a, b := b, a               // swap, no temporary
x, y := 0, 0
```

### 2.7 Precedence

From loosest to tightest binding:

1. the conditional `e -> a, b`
2. logical / bitwise *or*: `OR XOR | BOR BXOR EQV NEQV`
3. logical / bitwise *and*: `AND & BAND`
4. relations: `= ~= < <= > >=` and their float twins
5. shifts: `<< >>`
6. additive: `+ - +. -.`
7. multiplicative: `* / REM *. /.`
8. postfix: call `f(…)`, subscript `v!i` `v%i` `v.%i`, bitfield `w %% (p,n)`, member `obj.field`, `obj OF field`, lane `p.|k|`
9. prefix unary: `- NOT ~ BNOT ! @ % HD TL REST LEN`

When in doubt, parenthesize; it costs nothing and the next reader will thank you.

### 2.8 Conditional expressions

The conditional *expression* yields one of two values:

```bcpl
LET sign = n < 0 -> -1, (n > 0 -> 1, 0)
LET label = ok -> "yes", "no"
```

`cond -> a, b` is `a` if `cond` is true, else `b`. It nests, and it is the
expression-level counterpart of the `TEST` statement.

> **Exercise 2-1.** Write `bitcount(w)` returning the number of 1-bits in a
> word, using `BAND`, `>>`, and a loop.
>
> **Exercise 2-2.** Using only `->`, write `max3(a,b,c)` as a single expression.

---

## Chapter 3. Control Flow

### 3.1 Statements and blocks

A statement is an expression-with-effect (a call, an assignment) or a control
construct. Several statements enclosed in `$( … $)` (or `{ … }`) form a block,
which is itself a statement and may declare its own local `LET` variables.

### 3.2 IF, UNLESS, TEST

`IF cond THEN stmt` runs `stmt` when the condition is true. There is no `ELSE`
on `IF`; when you need both arms, use `TEST`:

```bcpl
TEST n MOD 2 = 0
    THEN WRITES("even*N")
    ELSE WRITES("odd*N")
```

`UNLESS cond THEN stmt` is `IF NOT cond THEN stmt`, and reads well for guards:

```bcpl
UNLESS valid(p) THEN RETURN
```

So: `IF`/`UNLESS` for one arm, `TEST … THEN … ELSE` for two.

### 3.3 WHILE, UNTIL, REPEAT

`WHILE cond DO stmt` tests before each iteration; `UNTIL cond DO stmt` is its
negation. To test *after* the body, append `REPEATWHILE` / `REPEATUNTIL`; for an
unconditional loop, append `REPEAT`:

```bcpl
WHILE more(p) DO p := step(p)

p := first
$( process(p); p := next(p) $) REPEATUNTIL p = 0

$( tick() $) REPEAT            // forever (until BREAK/RETURN/FINISH)
```

### 3.4 FOR

```bcpl
FOR i = 1 TO n DO use(i)
FOR i = n TO 1 BY -1 DO use(i)
```

The limit and step are evaluated once. The control variable is local to the
loop.

### 3.5 BREAK and LOOP

Inside any loop, `BREAK` exits the innermost loop and `LOOP` jumps to its next
iteration:

```bcpl
FOR i = 0 TO n DO $(
    IF v!i = 0 THEN LOOP        // skip zeros
    IF v!i < 0 THEN BREAK       // stop at first negative
    total := total + v!i
$)
```

### 3.6 SWITCHON

A multi-way branch on an integer:

```bcpl
SWITCHON ch INTO $(
    CASE '+': op := add;  ENDCASE
    CASE '-': op := sub;  ENDCASE
    CASE '*':
    CASE 'x': op := mul;  ENDCASE        // fall-through: '*' and 'x' share
    DEFAULT:  error(ch)
$)
```

Each `CASE` is a constant label. `ENDCASE` leaves the switch; without it,
control falls through to the next case, which is occasionally what you want (as
with `'*'` and `'x'` above). `DEFAULT` catches everything else.

### 3.7 GOTO and labels

Labels and `GOTO` exist for the rare case — typically breaking out of deeply
nested loops — where structured forms are clumsier than the jump:

```bcpl
    FOR i = 0 TO n DO
        FOR j = 0 TO m DO
            IF grid!i!j = target THEN GOTO found
    WRITES("absent*N")
    RETURN
found:
    WRITEF("found at %d,%d*N", i, j)
```

Use it sparingly.

### 3.8 BRK — the breakpoint statement

`BRK` is a real statement, not a comment. When reached it prints a diagnostic
dump — a banner, the heap summary, the machine registers, and a back-trace that
names your BCPL routines — and then carries on. It is invaluable when a program
misbehaves and you want a snapshot without a debugger. (The same machinery turns
a fatal signal into a named back-trace; see the *crash handling* note in
`docs/crash_handling.md`.)

> **Exercise 3-1.** Write a `SWITCHON`-based classifier that reads characters
> and counts letters, digits, and "other".
>
> **Exercise 3-2.** Rewrite the nested-loop search of §3.7 without `GOTO`, using
> a `VALOF`/`RESULTIS` helper function. Which reads better?

---

## Chapter 4. Functions and Program Structure

### 4.1 Functions and routines

The distinction is whether a value comes back:

```bcpl
LET twice(x) = x + x                 // function: has a value
LET greet(who) BE WRITEF("hi %s*N", who)   // routine: has an effect
```

A function's body is an expression; for more than an expression, it is a `VALOF`
block ending in `RESULTIS`. A routine's body is a statement; it ends by
falling off the end, by `RETURN`, or by `FINISH`.

Parameters are passed by value — each is a word. To let a callee modify the
caller's data, pass a pointer (Chapter 5) or an object (Chapter 7).

### 4.2 VALOF and RESULTIS

`VALOF` turns a block into an expression. It is the workhorse for any function
whose result needs local variables or a loop:

```bcpl
LET gcd(a, b) = VALOF $(
    WHILE b ~= 0 DO $(
        LET t = b
        b := a REM b
        a := t
    $)
    RESULTIS a
$)
```

`FVALOF` is the same, hinting that the result is a float.

### 4.3 Recursion and the AND chain

Functions may recurse. For *mutual* recursion, declare the partners together
with `AND`, so each is in scope for the other:

```bcpl
LET even(n) = n = 0 -> TRUE,  odd(n - 1)
 AND odd(n)  = n = 0 -> FALSE, even(n - 1)

LET START() BE
    WRITES(even(10) -> "even*N", "odd*N")
```

Without the `AND`, the first definition could not see the second.

### 4.4 Scope: local, static, global, manifest

A `LET` inside a routine or block is **local** — it lives on the stack and
vanishes at block exit. Four other lifetimes are available:

- **STATIC** — one fixed cell, retained for the whole run, private to the file:

  ```bcpl
  STATIC $( calls = 0 $)
  LET tick() BE calls := calls + 1
  ```

- **GLOBAL** — like static, but visible to other modules.
- **MANIFEST** — a compile-time constant, no storage (see §2.2).
- **TABLE** — a static, pre-initialized vector of constants:

  ```bcpl
  LET primes = TABLE(2, 3, 5, 7, 11, 13)
  ```
  `FTABLE` is the floating equivalent.

### 4.5 Type annotations

You may annotate a parameter or a binding with `AS Type`. The annotation guides
type inference and, for class types, tells the compiler the receiver's class so
that method calls can be resolved directly:

```bcpl
LET area(s AS Shape) = s.area()
LET name AS STRING = "anon"
```

Recognized type names include `INTEGER` (`INT`), `FLOAT`, `WORD`, `STRING`,
`CHAR`, `BYTE`, `LIST`, `VECTOR`, `OBJECT`, the pack types `PAIR FPAIR QUAD
FQUAD OCT FOCT`, and any class name. A pointer type is written `^Type` or
`POINTER TO Type`. Annotations are advisory — omit them and the word still
flows; supply them and you get sharper code and earlier warnings.

### 4.6 Programs in several files — modules

A program may be split across files in a `modules-active/` directory (override
the location with `$NEWBCPL_MODULES_ACTIVE`). Each file is a module named after
its stem; every top-level function it defines is exported under the mangled name
`<module>_<function>`. The loader JITs all active modules and links them before
`START` runs, so references resolve in any direction.

```bcpl
// modules-active/maths.bcl
LET sq(x)  = x * x
LET cube(x) = x * x * x
```

```bcpl
// program.bcl
LET START() BE $(
    WRITEN(maths_sq(7))         // 49 — note the module_ prefix
    NEWLINE()
$)
```

There is as yet no `EXPORT` qualifier; every top-level function is public.

> **Exercise 4-1.** Write `fib(n)` two ways: a recursive function, and an
> iterative `VALOF`. Time both with `TIMER_START`/`TIMER_END` (Appendix B).
>
> **Exercise 4-2.** Split a small program into a module of helpers and a main
> file; call across the boundary.

---

## Chapter 5. Pointers, Vectors, and the Word

BCPL's view of memory is direct: storage is a vector of words, and a pointer is
the index of a word. This chapter is about that view and the operators that
express it.

### 5.1 Indirection

The prefix `!` is "the word pointed at"; `@` is "the address of":

```bcpl
LET x = 10
LET p = @x          // p points at x
!p := 20            // store through p
WRITEN(x)           // 20
```

`%` is the *byte* indirection operator — `%p` is the byte at address `p`, for
stepping through raw memory and byte vectors. (On a string `%` means something
else entirely: `s % i` is the i-th *character* — a Unicode code point — because
strings are Cocoa objects, not byte arrays. See Chapter 9.)

### 5.2 Subscripting is indirection

`v!i` is exactly `!(v + i)` — the word `i` words past `v`. It is an lvalue, so
it works on both sides of `:=`. The byte subscript `v%i` reads or writes the
`i`-th byte of a memory block; `v.%i` indexes a float vector. (When the left
operand is a *string*, `s%i` is a character rather than a byte, and is read-only
— Chapter 9.)

```bcpl
LET v = VEC 100
v!0 := 1
FOR i = 1 TO 100 DO v!i := v!(i-1) * 2
```

Subscripts chain: `grid!i!j` is row `i`, column `j` of a vector of vectors.

### 5.3 Vectors: VEC, FVEC, TABLE

`VEC n` allocates room for indices `0 … n`; `FVEC n` does the same for doubles.
A `VEC` remembers the bound it was made with, retrievable as `LEN`:

```bcpl
LET v = VEC 9       // v!0 .. v!9 are valid
WRITEN(LEN(v))      // 9 — the bound n, not the element count
```

`TABLE(…)` and `FTABLE(…)` build static, constant-initialized vectors (§4.4).

Where a `VEC` lives in memory — on a scope-local arena that is freed wholesale
when the function returns, or on the long-lived heap — is decided for you by the
compiler's escape analysis. A vector you only use locally costs nothing to
reclaim; one you return is promoted to the heap automatically. Chapter 8 tells
the whole story.

### 5.4 The manual heap: GETVEC and FREEVEC

When you want a block whose lifetime you control yourself, allocate it with
`GETVEC(n)` and release it with `FREEVEC(p)`:

```bcpl
LET buf = GETVEC(255)
... use buf!0 .. buf!255 ...
FREEVEC(buf)
```

`GETVEC` returns a zeroed block on the manual heap; `FREEVEC` returns it to the
free list. (Passing `FREEVEC` a pointer that did not come from `GETVEC` — a
stack or arena address — is harmless; it is ignored.) Typed spellings
`IGETVEC SGETVEC PGETVEC QGETVEC` and the float `FGETVEC` allocate the same way
but document intent.

### 5.5 Bit-fields

The `%%` operator extracts or deposits a run of bits. `w %% (pos, width)` is the
`width` bits of `w` starting at bit `pos`:

```bcpl
LET green = pixel %% (8, 8)        // bits 8..15
color %% (0, 8) := 255             // set the low byte
```

### 5.6 SIMD packs: PAIR, QUAD, OCT

Apple Silicon has vector registers, and MacBCPL exposes them as small fixed
packs. `PAIR(a, b)` packs two lanes into a word-pair register; `QUAD` packs
four; `OCT` packs eight; the `F`-prefixed forms (`FPAIR FQUAD FOCT`) pack
floats. Arithmetic on a pack operates lane-wise on all lanes at once. Read a
single lane with `.|k|`:

```bcpl
LET p = PAIR(3, 4)
LET q = PAIR(10, 20)
LET r = p + q              // lane-wise: PAIR(13, 24)
WRITEN(r.|0|)              // 13
WRITEN(r.|1|)              // 24
```

These are the tool for geometry and signal work: four coordinates or eight
samples move and compute together. Allocate arrays of them with
`PAIRS(n) QUADS(n) OCTS(n)` and the float variants.

> **Exercise 5-1.** Write `reverse(v, n)` that reverses the first `n` words of a
> vector in place, using a multiple assignment to swap.
>
> **Exercise 5-2.** Pack an RGBA color into one word with `%%`, then unpack and
> print the four components.

---

## Chapter 6. Lists

A vector is a fixed block; a **list** is a chain. MacBCPL lists are classic cons
cells: a list value is a single word that is either `0` — the empty list, NIL —
or a pointer to a 16-byte cell holding a *head* (`hd`, at offset 0) and a *tail*
(`tl`, at offset 8). The tail is itself a list. Nothing more — no length field,
no type tag.

### 6.1 Building lists

`LIST(a, b, c)` builds a three-cell chain `a → b → c → NIL`. `MANIFESTLIST(…)`
builds one the same way (intended for read-only literal data). Because every
element is just a word, a list may freely mix integers, floats, and pointers:

```bcpl
LET xs = LIST(10, 20, 30)
LET ys = LIST()             // NIL, the empty list
```

### 6.2 Head and tail

The fundamental accessors are `HD` (head) and `TL` (tail); `REST` is a synonym
for `TL`:

```bcpl
WRITEN(HD(xs))              // 10
WRITEN(HD(TL(xs)))          // 20
WRITEN(HD(TL(TL(xs))))      // 30
```

There are two ways to write them, and the difference matters. The **call forms**
`HD(x)` and `TL(x)` are NIL-safe: applied to the empty list they return 0 rather
than crashing, so the classic walk is safe:

```bcpl
LET p = xs
WHILE p ~= 0 DO $(
    WRITEN(HD(p)); WRITES(" ")
    p := TL(p)
$)
NEWLINE()
```

The **prefix forms** `HD x` and `TL x` are open-coded as raw word loads
(`x!0` and `x!1`) — as fast as a subscript, but *unguarded*: do not apply them to
NIL. Use the prefix forms in hot loops where you have already checked for the
end, and the call forms everywhere else.

`LEN(xs)` counts the cells by walking the tail (it is O(n) — there is no stored
length); on NIL it is 0.

### 6.3 Appending and concatenating

`APND(xs, v)` adds `v` to the end of `xs` and **returns the new head**. You must
capture that result, because when `xs` was empty the head changes:

```bcpl
LET xs = LIST()
xs := APND(xs, 11)
xs := APND(xs, 22)
xs := APND(xs, 33)
WRITEN(LEN(xs))            // 3
```

`CONCAT(a, b)` returns a list that is `a` followed by `b`. It copies the cells of
`a` and then *shares* `b`'s cells — so the two lists now have cells in common.

### 6.4 Iterating with FOREACH

`FOREACH` walks a list, binding each head in turn:

```bcpl
LET xs = LIST(1, 2, 3, 4)
FOREACH e IN xs DO $(
    WRITEN(e); WRITES(" ")
$)
NEWLINE()                  // 1 2 3 4
```

If the elements are pairs, you can destructure the two lanes directly:

```bcpl
FOREACH (x, y) IN points DO
    WRITEF("(%d,%d) ", x, y)
```

### 6.5 Freeing lists, and the sharing rule

Lists live on the manual heap (never on an arena — see below), so a list you are
done with can be returned to the free list with `FREELIST(xs)`, which recycles
every cell.

But here is the rule that the sharing in §6.3 forces: **never free a list whose
cells are shared.** After `c := CONCAT(a, b)`, the cells of `b` belong to both
`b` and `c`; freeing `c` while `b` is still in use corrupts `b`. The same caution
applies whenever you have kept a `TL` of a list and freed the whole. Freeing is a
contract you keep, not a check the machine makes — which is precisely why lists
are never put on a scope arena, where bulk freeing could not honor the contract.

> **Exercise 6-1.** Write `member(x, xs)` returning `TRUE` if `x` is an element
> of `xs`, with a `WHILE` walk and the call forms of `HD`/`TL`.
>
> **Exercise 6-2.** Write `reverse(xs)` returning a new list with the elements in
> reverse order. Then write `length` without `LEN`, to feel the cons cell.

---

## Chapter 7. Classes and Cocoa Objects

This is where New BCPL departs furthest from the old. A `CLASS` groups fields
and methods — and on macOS, an instance is **a real Objective-C object**.
`NEW` calls into the Objective-C runtime; a method call is an `objc_msgSend`;
inheritance is real Cocoa subclassing. Your BCPL objects and Cocoa's objects are
the same kind of thing, which is what will let them share the Cocoa frameworks.

### 7.1 Declaring a class

```bcpl
CLASS Point $(
    DECL x, y
    ROUTINE CREATE(ix, iy) BE $(
        SELF.x := ix
        SELF.y := iy
    $)
    FUNCTION getx() = SELF.x
    FUNCTION gety() = SELF.y
    ROUTINE move(dx, dy) BE $(
        SELF.x := SELF.x + dx
        SELF.y := SELF.y + dy
    $)
$)
```

Fields are declared with `DECL` (uninitialized) or `LET`/`FLET` (with an
initializer that runs at construction). Each field is one word; you may annotate
with `AS Type`. Methods are `FUNCTION`s (returning a value) or `ROUTINE`s
(returning none), written just like top-level functions and routines. Inside a
method, `SELF` is the receiver; `SELF.field` reads or writes a field; bare field
names also resolve to `SELF`'s fields.

### 7.2 Construction: NEW and CREATE

`NEW Class(args)` allocates an instance and runs its `CREATE` method with those
arguments. `CREATE` is the constructor; if you declare field initializers but no
`CREATE`, the compiler synthesizes one that runs them.

```bcpl
LET p = NEW Point(3, 4)
p.move(1, 1)
WRITEN(p.getx())           // 4
```

### 7.3 Messages and members

A method call is `obj.method(args)`; a field access is `obj.field` (the classic
`obj OF field` is also accepted). Dispatch is by message send, so the actual
method run depends on the object's real class — the basis of polymorphism.

### 7.4 Inheritance, VIRTUAL, FINAL, SUPER

`CLASS Sub EXTENDS Base` makes `Sub` a subclass — a real Objective-C subclass of
`Base`. A method marked `VIRTUAL` may be overridden in a subclass; one marked
`FINAL` may not (the compiler rejects the attempt). `SUPER.method(args)` calls
the parent's version, the usual way for an override to extend rather than replace
inherited behavior.

Here is the canonical example — a base and a derived class sharing an inherited
field and adding their own, the program that the port was first verified against:

```bcpl
CLASS Base $(
    DECL a
    ROUTINE CREATE(v) BE SELF.a := v
    FUNCTION describe() = SELF.a
$)

CLASS Sub EXTENDS Base $(
    DECL b
    ROUTINE CREATE(v, w) BE $(
        SUPER.CREATE(v)            // initialize the inherited field
        SELF.b := w
    $)
    FUNCTION total() = SELF.describe() + SELF.b
$)

LET START() BE $(
    LET s = NEW Sub(30, 7)
    WRITEF("sum=%d*N", s.total())   // sum=37
$)
```

The inherited field `a` and the new field `b` occupy separate storage — each
class contributes its own fields and the Objective-C runtime composes them — so
there is no overlap and no double-allocation.

### 7.5 Visibility

Members may be grouped under `PUBLIC:`, `PRIVATE:`, or `PROTECTED:` sections.
`PUBLIC` members are reachable from anywhere; `PRIVATE` only from the class's own
methods; `PROTECTED` from the class and its subclasses. These are enforced: a
read of a private field from outside, or an override of a `FINAL` method, is a
hard compile error, and the driver will not generate code for it.

```bcpl
CLASS Account $(
    PRIVATE:
        DECL balance
    PUBLIC:
        ROUTINE CREATE(b) BE SELF.balance := b
        ROUTINE deposit(n) BE SELF.balance := SELF.balance + n
        FUNCTION value() = SELF.balance
$)
```

### 7.6 Object lifetime — automatic, with explicit control at the edges

The guiding rule is: **automatic for the common case, explicit at the edges,
never a crash.**

The common case is a scope-local object. When you write `LET o = NEW C()` and
`o` does not escape its scope, the compiler releases it automatically at the end
of the scope — there is no `free`, and you cannot over-release it, because only
the object you directly created and still own is released:

```bcpl
LET draw() BE $(
    LET p = NEW Point(0, 0)
    p.move(3, 4)
    WRITEN(p.getx())
$)                              // p released here, automatically
```

When an object must be disposed deterministically — a file, a window, anything
with a cleanup step — bind it with **USING**. At every exit from the `USING`
block (fall-through, `RETURN`, `BREAK`, an exception) it runs the object's
`RELEASE` method, if it has one, and then frees the memory:

```bcpl
CLASS File $(
    DECL fd
    ROUTINE CREATE(name) BE SELF.fd := open(name)
    ROUTINE RELEASE() BE close(SELF.fd)
$)

LET START() BE
    USING f = NEW File("data.txt") DO $(
        ... work with f ...
    $)                          // f.RELEASE() then memory freed, guaranteed
```

The edges:

- An object that **escapes** — returned with `RESULTIS`, stored in an outer
  variable, consed into a list, or marked `RETAIN` — transfers its ownership out
  and is *not* released at scope exit. The receiver becomes responsible (and will
  typically `USING` it).
- A plain `LET r = factory()` whose value came back from a *call* is not
  auto-released — ownership from a call is unknown, so the safe choice is to
  leak rather than risk freeing something still in use; wrap it in `USING` to
  dispose it.
- **Reassigning** a variable that owns a freshly-`NEW`ed object would lose the
  only reference, so the compiler warns you — the nudge is "always `USING` is
  better than sometimes," and `USING` is the one construct that disposes
  uniformly.

The older `MANAGED` keyword still parses but is now only advisory; any class with
a `RELEASE` method works in a `USING` block.

### 7.7 Under the hood

You do not need this to use classes, but it explains the shape of the rules. A
`CLASS C` becomes a class registered with `objc_allocateClassPair`; each class
adds an instance variable holding *its own* fields, and the runtime composes the
inherited ones. Each BCPL method `m` is installed under the selector `bcpl_m` —
the `bcpl_` prefix keeps your method named `init` or `release` from colliding
with Cocoa's own `init`/`release`, a collision that would otherwise make
`[[C alloc] init]` return nil and corrupt the object silently. `NEW` is
`[[C alloc] init]` followed by your `CREATE`; a method call is an
`objc_msgSend`; `SUPER` is `objc_msgSendSuper`; `RETAIN`/release are
`objc_retain`/`objc_release`. This is also the door to the Cocoa frameworks: the
same object that runs your BCPL methods can be handed to AppKit. To call Cocoa's
*own* methods — under their real selectors, not the mangled `bcpl_` ones — use
the bracket message send of Chapter 10.

> **Exercise 7-1.** Add a `Shape` base class with a `VIRTUAL FUNCTION area()`,
> and `Circle`/`Square` subclasses. Write a routine that takes a `Shape` and
> prints its area, and call it with each.
>
> **Exercise 7-2.** Give `File` of §7.6 a real `RELEASE` that prints a message,
> and show by its output that `USING` runs it on every exit path — fall-through,
> early `RETURN`, and `BREAK` out of a loop.

---

## Chapter 8. Memory and Resource Management

Old BCPL gave you `GETVEC` and `FREEVEC` and left the rest to you. NewBCPL on
Windows added a tracing garbage collector. MacBCPL does neither: it has **no
collector**, yet you rarely call `free`. The idea is that heap data should follow
the stack's scopes — born in a scope, released when the scope ends — so that
lifetime is mostly automatic and always predictable, with no pauses.

### 8.1 The tiers

Every allocation lands in one of four tiers, and for the common cases the
compiler chooses for you:

- **Stack / static** — locals, parameters, the `VALOF` result, string literals,
  `STATIC`/`GLOBAL`/`MANIFEST` data. Tier 0; nothing to manage.
- **Scope arena** — a per-function bump region. Scratch `VEC`/`FVEC`/`TABLE` data
  that the compiler proves does not escape lives here and is freed *wholesale*
  when the function returns. This is the tier that makes "heap data with stack
  lifetime" true. No `free`, no collector, no pause.
- **Manual heap** — a global free-list heap. This backs explicit
  `GETVEC`/`FREEVEC`, all lists, and anything that *escapes* its scope and so
  cannot live on an arena. You free `GETVEC` blocks and owned lists yourself;
  escaped vectors are promoted here automatically.
- **Cocoa** — `NEW` objects, managed by retain/release as Chapter 7 describes.

### 8.2 Escape analysis chooses the tier

Before generating code, the compiler asks of each locally-built vector or object:
does it escape? A value escapes if it is returned, stored into an outer or global
variable, consed into a list, passed somewhere that keeps it, or marked `RETAIN`.
A value that escapes is put on the heap (or, for objects, has its ownership
transferred); a value that demonstrably does not is put on the scope arena and
freed for free at return.

The bias is always toward safety. If the compiler cannot prove a value stays
local, it treats it as escaping and heap-allocates — a missed arena placement
costs at worst a little memory, never a dangling pointer. **Use-after-free is the
one outcome the design refuses**; leaks are merely discouraged.

### 8.3 What you actually do

In practice:

- Local scratch arrays and short-lived objects: write them naturally and forget
  them — the arena and the auto-release handle it.
- Long-lived buffers you size yourself: `GETVEC` / `FREEVEC`.
- Resources needing deterministic cleanup: `USING`.
- Lists: build them, and `FREELIST` them only if you own them outright and have
  not shared their cells (§6.5).

The result is a language with manual control available when you reach for it, but
with the everyday 90% — the scratch vector, the local object — handled without a
collector and without a single `free`.

---

# Part II — Cocoa

*The chapters so far describe New BCPL as a language. This part describes it as a
**macOS language** — one whose values are, where it counts, Cocoa objects, so
that a BCPL program is a citizen of the Objective-C runtime and, in time, of the
Apple frameworks.*

The foundation was laid in Chapter 7: a `CLASS` instance is a real Objective-C
object, allocated by the runtime, dispatched by `objc_msgSend`, freed by
retain/release. This part takes up the Cocoa **types and facilities** that build
on that foundation. It begins with the one every program already uses without
thinking about it — the string — which is now an `NSString` (Chapter 9). Chapter
10 then opens the whole of Cocoa: the **bracket message send**, `[receiver
selector: arg …]`, which lets a BCPL program call any Objective-C method on any
object. With it the frameworks are reachable today — the worked examples drive
`NSProcessInfo`, `NSMutableArray`, `NSFont` geometry, an `NSAlert` dialog, and a
real `NSWindow`, and culminate in a complete native IDE written in BCPL that
edits and runs BCPL (§10.9). A higher-level, BCPL-flavored wrapping of AppKit
will grow here on top of the message send.

---

## Chapter 9. Strings

In old BCPL a string was a packed vector of bytes, and you walked it byte by
byte. In New BCPL on macOS a string is a **Cocoa `NSString`** — a real
Objective-C text object. You have been using them since Chapter 0; this chapter
says what they are.

The change buys three things: correct Unicode (a "character" is a code point, so
accents and emoji are single characters, not byte fragments), automatic
memory management on the same model as every other object, and a path to the
Cocoa frameworks, which speak `NSString` everywhere.

### 9.1 A string is an object

The value of `"hello"` is an `NSString` pointer — the same kind of word that
`NEW SomeClass()` yields. Everything Chapter 7 said about object identity and
lifetime therefore applies to strings, with one simplification: strings are
**immutable**. You never change a string in place; you build a new one.

```bcpl
LET s = "hello, world*N"
WRITES(s)                  // the object's text, to standard output
```

`WRITES` writes a string; `WRITEF`'s `%s` splices one into formatted output:

```bcpl
WRITEF("name=%s  n=%d*N", "Ada", 42)      // name=Ada  n=42
```

### 9.2 Literals are immortal, and shared

A string literal is created once and lives for the whole run — it is *immortal*.
Equal literals share one object, so identity comparison does the obvious thing:

```bcpl
LET a = "hello"
WRITES(a = "hello" -> "same*N", "diff*N")    // same
WRITES(a = "world" -> "same*N", "diff*N")    // diff
```

Because a literal is immortal, binding it, copying it, and letting it fall out
of scope cost nothing; there is no release to do.

Note that `=` on strings compares *identity*, not contents. For literals that is
the same thing (equal literals are one object), but two strings built
independently at run time may hold the same text and still compare unequal.
Content comparison is not yet a built-in.

### 9.3 Characters are code points

This is the substantive change from classic BCPL, so it deserves a clear
statement: **`s % i` is the i-th Unicode code point of `s`, and `LEN(s)` is the
number of code points** — not bytes, not UTF-16 units.

```bcpl
LET s = "café"
WRITEN(LEN(s))             // 4   — four characters
WRITEN(s % 3)              // 233 — U+00E9, the 'é'
```

`WRITEC` is the inverse: it takes a code point and writes its UTF-8 encoding, so
the canonical loop reconstructs any string, accents and emoji included:

```bcpl
LET s = "★😀"
WRITEN(LEN(s))             // 2 — two characters, though many bytes
FOR i = 0 TO LEN(s) - 1 DO WRITEC(s % i)     // prints  ★😀
NEWLINE()
```

Here `s % 1` is `128512` (U+1F600) — one integer for the whole emoji, not a
broken pair of bytes. Indexing past the end, or indexing the empty string,
yields 0.

`s % i` is read-only. Strings are immutable, so an assignment to it is rejected
at compile time:

```bcpl
s % 0 := 'H'
// sema error: cannot assign to `s % i` on a String — NSStrings are
// immutable; build a new string, or use a byte VEC for mutable storage
```

When you genuinely need mutable character storage, use a byte vector
(`GETVEC` and `%`), and build a string from it when you are done.

### 9.4 Building strings

Since you cannot mutate a string, you assemble text by **building a new one**.
The primitive is `JOIN`, which concatenates a list of strings with a separator
between them:

```bcpl
LET parts = LIST("foo", "bar", "baz")
LET path  = JOIN(parts, "-")
WRITES(path)               // foo-bar-baz
```

Plain concatenation is `JOIN(LIST(a, b), "")`. There is, deliberately, no `+`
operator on strings — `+` is integer addition, and overloading it on text
objects would invite exactly the type confusion the typeless word already asks
you to avoid.

A string `JOIN` builds is a *fresh, owned* object — which brings us to lifetime.

### 9.5 String lifetime

Strings obey the Cocoa stack-scope rules of §7.6, specialized by whether the
string is a literal or freshly built:

- **Literals are immortal** — never owned, never released, free to pass around.
- **A built string** (the result of `JOIN`) is **owned**, and is released
  automatically when its binding goes out of scope, exactly like a scope-local
  `NEW` object:

  ```bcpl
  LET report() BE $(
      LET line = JOIN(LIST("x=", "y="), " ")
      WRITES(line)
  $)                         // `line` released here, automatically
  ```

- **USING** disposes a built string deterministically at block exit:

  ```bcpl
  USING msg = JOIN(parts, ", ") DO WRITES(msg)
  ```

- **Escape transfers ownership.** A built string returned with `RESULTIS`,
  stored in an outer or global variable, consed into a list, or marked `RETAIN`
  is not released in the scope that produced it — the receiver inherits it.

- **Reassignment is safe.** Overwriting a binding that owns a built string
  releases the old object and retains the new one, so a loop that rebuilds a
  string each pass does not leak:

  ```bcpl
  LET acc = ""
  FOR i = 1 TO n DO
      acc := JOIN(LIST(acc, item(i)), ",")   // previous `acc` released each pass
  ```

As always the bias is toward never crashing: assigning a non-string word into a
string-typed binding does not attempt to release it, and the runtime's
retain/release calls are guarded so a stray integer can never be mistaken for an
object and sent a message.

One edge mirrors the object rule of §7.6: a string returned from an ordinary
*call* (not a literal, not a direct `JOIN`) is treated as borrowed and not
auto-released, since its ownership is unknown. If such a value is genuinely
yours to free, bind it with `USING`.

### 9.6 Passing strings through untyped places

Because a string is just a word, it can travel through an un-annotated
parameter, a list cell, or an object field, where the compiler has lost track of
its type. Operations that must know it is a string — `LEN`, `%`, `WRITES` — stay
safe at run time (the runtime recognizes a string handle and does the right
thing), but you get sharper code, and earlier checking, by annotating with
`AS STRING`:

```bcpl
LET initials(s AS STRING) = VALOF $(
    LET n = 0
    FOR i = 0 TO LEN(s) - 1 DO
        IF (s % i) >= 'A' & (s % i) <= 'Z' THEN n := n + 1
    RESULTIS n
$)
```

### 9.7 A string is a Cocoa object you can message

Because a string is a genuine `NSString`, you can send it any Cocoa message with
the **bracket syntax** of Chapter 10 — the whole `NSString` interface is open to
you:

```bcpl
LET up     = [s uppercaseString]              // a new string: "CAFÉ"
LET joined = [s stringByAppendingString: " and more"]
LET n      = [s length]                       // UTF-16 unit count
```

and a string literal is a valid `NSString` argument or collection element
anywhere Cocoa expects one:

```bcpl
[dict setObject: "London" forKey: "city"]
```

One subtlety pairs with §9.3. Cocoa counts text in UTF-16 code units, so
`[s length]` is the UTF-16 unit count — which differs from BCPL's `LEN(s)`
(code points) for characters outside the Basic Multilingual Plane: an emoji is
one `LEN` but two `[s length]`. Use the code-point-correct BCPL built-ins
(`LEN`, `%`, `WRITEC`) when the unit should be the character; reach for the
Cocoa methods when you want Cocoa's own operations — case folding, searching,
path manipulation. Chapter 10 covers the messaging syntax, the synthesized
return types, and — the part that bites — who owns the result a message hands
back.

> **Under the hood.** A literal's bytes are cooked at compile time (escapes
> resolved to UTF-8) and the `NSString` is built once, on first use, with
> `[[NSString alloc] initWithUTF8String:]` and cached forever — that is what
> "immortal" means. Short ASCII strings become *tagged pointers* (Apple encodes
> the characters in the pointer bits); the runtime detects these and never
> dereferences them. `LEN` and `s % i` decode UTF-8 to code points once and
> memoize the result per string, so an indexing loop is O(n), not O(n²) of
> repeated Objective-C calls; the memo is dropped when a string is released so a
> recycled address can never serve stale text.

> **Exercise 9-1.** Write `reverse(s AS STRING)` returning a new string with the
> code points of `s` in reverse order. Build it with `WRITEC` into output first,
> then (harder) return an actual reversed string. What gets awkward without
> string mutation, and how does that push you toward `JOIN`?
>
> **Exercise 9-2.** Write `count(s, c)` returning how many times code point `c`
> occurs in `s`. Test it on `"mississippi"` and on a string with an emoji, to
> convince yourself the unit of counting is the character, not the byte.

---

## Chapter 10. Sending Cocoa Messages

A `CLASS` (Chapter 7) lets you *define* objects, and the dot call `obj.method()`
sends them your own BCPL methods. But the macOS frameworks are millions of lines
of Objective-C you did not write, reachable only through their *real* selectors —
`length`, `setObject:forKey:`, `initWithContentRect:styleMask:backing:defer:`.
For those, New BCPL has the **bracket message send**, borrowed straight from
Objective-C:

```bcpl
[receiver selector: arg  selector: arg  …]
```

Everything in this chapter compiles to a single `objc_msgSend`. With it, a BCPL
program can drive any Cocoa object.

### 10.1 The forms

A message names a *receiver* and a *selector*, optionally with arguments. The
selector is read straight from the brackets, unmangled, exactly as Objective-C
spells it:

```bcpl
[obj removeAllObjects]                         // zero-argument selector
[s stringByAppendingString: " World"]          // one keyword + argument
[d setObject: v forKey: k]                      // multi-keyword: setObject:forKey:
[NSString stringWithString: other]             // class as receiver (class method)
[[NSMutableArray alloc] init]                  // nesting: send to a send's result
```

A multi-keyword selector is assembled from the keyword parts in order, so the
third line above sends `setObject:forKey:` with two arguments. A message may
stand alone as a statement when you want only its effect:

```bcpl
[a addObject: "alpha"]
```

### 10.2 Receivers

The receiver before the selector may be:

- **a Cocoa class**, written as a bare capitalized name — `NSString`,
  `NSProcessInfo`, `NSWindow`. An unbound name in receiver position is resolved
  to the Objective-C class of that name, so class methods just work:
  `[NSProcessInfo processInfo]`.
- **an instance** — any expression that evaluates to an object: a variable, a
  field, a nested bracket send, a parenthesized expression.
- **`SELF`**, inside a class method, to message your own object with a raw
  selector.

`SUPER` is *not* a bracket receiver — for a superclass call use the
`SUPER.method()` form of Chapter 7; the compiler will tell you so if you try
`[SUPER …]`.

### 10.3 Return types are synthesized

A message's result type matters: an integer comes back in a different machine
register than a double, a struct must be unpacked, and a string must be treated
as an `NSString`, not a raw word. New BCPL **synthesizes the return type from a
database of Objective-C method encodings**, so the common getters need no
annotation at all:

```bcpl
LET n   = [arr count]              // -> INT
LET x   = [num doubleValue]        // -> FLOAT
LET up  = [s uppercaseString]      // -> a String (NSString)
LET obj = [[NSObject alloc] init]  // -> Object
```

The synthesis is **receiver-class-exact**: when the receiver's class is known,
the compiler looks up *that class's* signature for the selector. This matters
because one selector can mean different things on different classes — `frame` is
a rectangle on a window or a view, but an object elsewhere. The class flows
through allocation, since `alloc` / `init` / `new` / `copy` return the
receiver's own class:

```bcpl
LET w = [[NSWindow alloc] init]    // w is known to be an NSWindow
LET f = [w frame]                  // -> FVEC: exactly NSWindow's frame, an NSRect
```

so `f` is typed as the rectangle it is. A class-name receiver
(`[NSColor clearColor]`) is resolved against that class the same way.

> The bundled database covers a curated subset of the frameworks. To synthesize
> against the *entire* Objective-C surface — tens of thousands of classes — set
> the environment variable `COCOA_SQLITE` to the path of the shared
> `cocoa.sqlite` mirror; the compiler reads it during type checking. Without it
> the bundled data is used: deterministic, and enough for the common classes.

When the database does not cover a selector, or you want to override it, append
a trailing **`AS Type`**:

```bcpl
LET clicked = [alert runModal] AS INT
LET val     = [thing somethingExotic] AS FLOAT
```

The same `AS Type` works on an *argument* when a selector expects a particular
machine type — most often a `double`, which must travel in a floating register:

```bcpl
LET f = [NSFont systemFontOfSize: 14 AS FLOAT]
```

The default — absent any annotation, known class, or database entry — is
`Object`.

### 10.4 Strings and collections

Because a BCPL string is an `NSString` (Chapter 9), strings are first-class in
this world: they are receivers, arguments, and collection elements without any
conversion. A small program that builds and queries Cocoa collections:

```bcpl
LET START() BE $(
    LET a = [[NSMutableArray alloc] init]
    [a addObject: "alpha"]
    [a addObject: "beta"]
    [a addObject: "gamma"]
    WRITEN([a count] AS INT)                         // 3
    WRITES([a objectAtIndex: 0]); NEWLINE()          // alpha
    WRITES([a componentsJoinedByString: " - "])      // alpha - beta - gamma
    NEWLINE()

    LET d = [[NSMutableDictionary alloc] init]
    [d setObject: "Ada Lovelace" forKey: "name"]
    WRITES([d objectForKey: "name"]); NEWLINE()      // Ada Lovelace
$)
```

String methods chain as you would expect, each returning a new string:

```bcpl
LET who = [["macbcpl" uppercaseString] stringByAppendingString: " on Cocoa"]
WRITES(who)                                          // MACBCPL on Cocoa
```

(Remember §9.7: `[s length]` counts UTF-16 units, not code points. For
character-accurate work use BCPL's `LEN` and `%`.)

### 10.5 Methods that return C structs

Many Cocoa methods return a small C structure — a rectangle, a point, a size, a
range. New BCPL recognizes the geometry types in the selector database and
**materializes the struct as a BCPL vector** you index by field name. The field
names are predeclared constants:

```bcpl
// NSRange is an integer pair -> a VEC, read with the word subscript `!`
LET r = ["the quick brown fox" rangeOfString: "brown"]
WRITEN(r ! NSRange_location)        // 10
WRITEN(r ! NSRange_length)          // 5

// NSSize / NSPoint are two doubles, NSRect four -> an FVEC, read with `.%`
LET box = [someFont boundingRectForFont]
FWRITE(box .% NSRect_width)
FWRITE(box .% NSRect_height)
```

The seeded names are `NSRect_x`, `NSRect_y`, `NSRect_width`, `NSRect_height`;
`NSPoint_x`, `NSPoint_y`; `NSSize_width`, `NSSize_height`; and
`NSRange_location`, `NSRange_length`. A floating struct (rect, point, size)
comes back as an `FVEC` read with `.%`; the integer `NSRange` as a `VEC` read
with `!`. You never deal with the calling-convention details — the arm64 backend
applies the platform struct-return ABI for you.

### 10.6 Passing a struct by value

The reverse direction works too: where a selector takes a struct *argument* —
`setFrame:`, `initWithContentRect:`, `valueWithRect:` — you pass an `FVEC` (or
`VEC`) of the right shape and it is handed over by value:

```bcpl
LET rect = FVEC [200.0, 200.0, 720.0, 480.0]        // x, y, width, height
LET w = [[NSWindow alloc] initWithContentRect: rect styleMask: 15
                          backing: 2 defer: 0]
[w setTitle: "MacBCPL — Cocoa from BCPL"]
[w center]
[w makeKeyAndOrderFront: 0]
```

That is, in full, how you open a native window from BCPL.

The vector must match the struct exactly. Passing a scalar where a struct is
expected, or a vector of the wrong field count, is caught at compile time rather
than left to fault at run time:

```bcpl
[w setFrame: 0]                  // error: setFrame: expects a struct value
                                 //        (an FVEC/VEC of 4 fields), not a scalar
[w setFrame: (FVEC [1.0, 2.0])]  // error: setFrame: expects 4 fields, got 2
```

### 10.7 Who owns the result — and a hazard

This is the part to get right. Unlike `NEW` and `JOIN`, **the result of a
bracket send is not tracked for you.** The compiler does not know, in general,
whether a message returns something you own or merely borrow, so it does
nothing automatic — no release at scope exit. You manage these objects
explicitly, with `[x retain]` / `[x release]` or by binding with `USING`.

There is a sharper edge behind this. The JIT runs with **no autorelease pool**.
In normal Cocoa, the `+0` "convenience" constructors — `stringWithUTF8String:`,
`[NSNumber numberWithDouble:]`, and friends — return an object registered with
the current autorelease pool, which drains later. With no pool, such an object's
lifetime is undefined: it may be valid now and gone an instant later. The rule
that keeps you safe is the classic Objective-C one:

> Prefer the **`alloc`/`init`** (`+1`-owned) form over the convenience
> constructor, and `[x release]` it (or `USING` it) when done.

So write `[[NSMutableArray alloc] init]`, not `[NSMutableArray array]`; build a
string with `JOIN` or a kept `NSString`, not an untracked `stringWithFormat:`
result you stash for later. Within a single expression a convenience result is
fine; it is *keeping* one across time that is unsafe until a pool exists.

### 10.8 Dot or bracket?

Both send Objective-C messages; the difference is whose selectors they reach.

- `obj.method(args)` — Chapter 7 — calls **your** BCPL methods. The selector is
  mangled to `bcpl_method`, which is what isolates your methods from Cocoa's.
- `[obj method: args]` — this chapter — calls **Cocoa's** methods under their
  real, unmangled selectors.

Use the dot for objects you defined with `CLASS`; use brackets to drive the
frameworks. A single program mixes them freely — a BCPL object can be passed to
AppKit, and a Cocoa object can be stored in a BCPL field.

### 10.9 A worked whole: the BCPL IDE

Everything in Part II comes together in `examples/bcpl-ide.bcl`: a native macOS
IDE, **written in BCPL, that edits and runs BCPL**. In a couple of hundred lines
it builds a real Cocoa application — an editable `NSTextView` source pane over a
read-only output pane in an `NSSplitView`, a native menu bar, file open/save
through `NSOpenPanel`/`NSSavePanel`, and `Cmd-R` to compile and run the buffer —
with no Objective-C anywhere; it is bracket message sends and a BCPL class all
the way down.

The crux is how a BCPL object becomes a Cocoa **action target**. The IDE's
controller is an ordinary BCPL class that subclasses `NSObject` (Chapter 7), so
it is a real Objective-C object whose methods are installed as IMPs:

```bcpl
CLASS IdeController EXTENDS NSObject $(
    DECL editor, outv, win, cur
    ROUTINE doRun(s) BE $(
        [[editor string] writeToFile: tmp atomically: 1 encoding: 4 error: 0]
        [outv setString: bcpl_run_capture(cmd)]      // run the buffer, show output
    $)
    ...
$)
```

A menu item then targets one of those methods. Because a BCPL method `m` lives
under the Objective-C selector `bcpl_m` (§10.8), the IDE names that selector with
the runtime primitive `bcpl_selector` and hands it to the menu item:

```bcpl
LET it = [[NSMenuItem alloc] initWithTitle: "Run"
                             action: bcpl_selector("bcpl_doRun") keyEquivalent: "r"]
[it setTarget: ctrl]                                 // ctrl is a NEW IdeController
```

Standard editor commands — Cut, Copy, Undo, Select All — need no controller:
they are named the same way (`cut:`, `undo:`, …) with a *nil* target, so Cocoa's
responder chain dispatches them to the focused `NSTextView`. The program then
hands control to the ordinary Cocoa run loop, `[app run]`, which drives the key
window, the editor, and every menu action — no hand-written event pump.

Two small runtime primitives make this possible, both returning `NSString`s or
`SEL`s the BCPL code treats as ordinary words:

- `bcpl_selector(name)` — reify an Objective-C selector by name, so BCPL can
  name an action (its own `bcpl_…` method, or a standard one like `cut:`).
- `bcpl_run_capture(cmd)` — run a shell command and return its combined
  stdout+stderr as a string. The IDE writes the buffer to a temp file and runs
  `newbcpl-driver run` on it, **out of process** — so a crash in the program
  under test kills the subprocess, not the IDE.

The lesson of the example is the thesis of Part II: with classes that are Cocoa
objects (Chapter 7), strings that are `NSString`s (Chapter 9), and the bracket
send to reach the frameworks (this chapter), a complete native application — even
the development environment itself — is an ordinary BCPL program.

> **Exercise 10-1.** Using `NSProcessInfo`, print the process name, the active
> processor count (`AS INT`), and the operating-system version string. Which
> results needed an annotation, and why?
>
> **Exercise 10-2.** Build an `NSMutableArray` of three strings, then print them
> joined by `", "` with `componentsJoinedByString:`. Now do the same with BCPL's
> `JOIN` over a `LIST`. Which owns its result?
>
> **Exercise 10-3.** Call a method that returns an `NSRect` (e.g. a font's
> bounding rectangle) and print its width and height by field name. What kind of
> vector did you get back, and which subscript reads it?
>
> **Exercise 10-4.** Read `examples/bcpl-ide.bcl` (§10.9). Add a "Clear output"
> command: a method on `IdeController` that empties the output pane, a
> `bcpl_selector` for it, and a menu item under Program targeting the
> controller. Why does this need a controller method, where Cut and Copy did
> not?

---

## Appendix A — Lexical Reference

### A.1 Keywords

All keywords are upper case and reserved. Identifiers are lower-case (or mixed)
runs of letters, digits, and underscore, beginning with a letter or underscore;
they may not contain a dot.

```
LET AND BE VALOF RESULTIS MANIFEST STATIC GLOBAL GLOBALS VEC TABLE OF
IF UNLESS TEST THEN ELSE OR DO WHILE UNTIL REPEAT REPEATWHILE REPEATUNTIL
FOR TO BY SWITCHON INTO CASE DEFAULT ENDCASE GOTO RETURN FINISH BREAK LOOP
TRUE FALSE NOT XOR BAND BOR BXOR BNOT REM EQV NEQV GET
FLET FSTATIC FVEC FTABLE FVALOF FUNCTION ROUTINE
CLASS EXTENDS DECL NEW VIRTUAL FINAL MANAGED PUBLIC PRIVATE PROTECTED
SELF SUPER RETAIN FREEVEC FREELIST USING
FLOAT TRUNC FIX FSQRT ENTIER FOREACH IN
LIST MANIFESTLIST HD TL REST LEN TYPEOF TYPE AS POINTER DEFER BRK
PAIR FPAIR QUAD FQUAD OCT FOCT ASM
```

### A.2 Operators and punctuation

| Group | Lexemes |
|-------|---------|
| arithmetic | `+ - * /` `REM` |
| float arithmetic | `+. -. *. /.` (or `+# -# *# /#`) |
| comparison | `= ~= < <= > >=` |
| float comparison | `=. ~=. <. <=. >. >=.` (or `=# …`) |
| logical | `AND OR NOT XOR` |
| bitwise | `BAND BOR BXOR BNOT` `& \| ^` `<< >>` `EQV NEQV` |
| indirection | `!` (word) `%` (byte) `@` (address-of) `.%` (float subscript) |
| bit-field | `%%` |
| assignment | `:=` |
| conditional | `->` |
| lane access | `.\|k\|` |
| section brackets | `$( $)` and synonyms `{ }`, optionally tagged |
| grouping / sep. | `( )` `[ ]` `,` `;` |
| null literal | `?` |

### A.3 Literals

- **Integer**: decimal `255`; octal `#377`; hexadecimal `#XFF` / `#xff`.
- **Float**: `3.14`, `0.5`, `1e10`, `2E-3` (a point must be followed by a digit).
- **Character**: `'a'`, `'*N'` — one byte, escapes allowed.
- **String**: `"text*N"` — escapes allowed.
- **Null**: `?`. **Booleans**: `TRUE`, `FALSE`.

### A.4 Escapes (the `*` convention)

| Escape | Meaning |
|--------|---------|
| `*N` | newline |
| `*T` | tab |
| `*S` | space |
| `*B` | backspace |
| `*P` | form feed |
| `*C` | carriage return |
| `*"` | double quote |
| `**` | asterisk |
| `*c` | the byte `c`, for any other `c` |

### A.5 Comments

`// …` to end of line; `/* … */` for a block (does not nest).

---

## Appendix B — Standard Library

All functions are callable by the names below. Integer-returning unless noted;
float results are IEEE doubles.

### B.1 Console I/O

| Call | Effect |
|------|--------|
| `WRITES(s)` | write a string |
| `WRITEN(n)` | write an integer in decimal |
| `WRITEC(c)` | write one character — `c` is a Unicode code point, encoded UTF-8 |
| `FWRITE(x)` | write a float |
| `NEWLINE()` | write a line break |
| `WRITEF(fmt, …)` | formatted write; specifiers `%d %x %X %o %c %s %f %%` |
| `RDCH()` | read one byte from input, or −1 at end |
| `FINISH()` | terminate the program |

### B.2 Numbers and floating point

| Call | Result |
|------|--------|
| `FLOAT(n)` | integer → double |
| `FIX(x)` / `TRUNC(x)` | double → integer (truncate) |
| `ENTIER(x)` | floor of a double |
| `FSQRT(x)` | square root |
| `FSIN FCOS FTAN(x)` | trigonometry (radians) |
| `FLOG(x) FEXP(x)` | natural log, exponential |
| `FABS(x)` | float absolute value |
| `ABS(n)` | integer absolute value |
| `MIN(a,b) MAX(a,b)` | integer min / max |

### B.3 Randomness

| Call | Result |
|------|--------|
| `RAND(max)` | integer in `0 … max` |
| `FRND()` | double in `[0, 1)` |
| `RND(max)` | double in `[0, max)` |

### B.4 Vectors and the manual heap

| Call | Result |
|------|--------|
| `GETVEC(n)` / `FGETVEC(n)` | allocate an `n`-word (float) block |
| `IGETVEC SGETVEC PGETVEC QGETVEC` | typed aliases of `GETVEC` |
| `PAIRS(n) QUADS(n) OCTS(n)` (+ `F…`) | allocate arrays of packs |
| `FREEVEC(p)` | free a heap block |
| `LEN(v)` | length of a vector |

### B.5 Lists

| Call | Result |
|------|--------|
| `LIST(…)` / `MANIFESTLIST(…)` | build a cons-cell list |
| `HD(x) TL(x)` / `REST(x)` | head, tail (NIL-safe call forms) |
| `LEN(x)` | element count (O(n)) |
| `APND(x, v)` | append `v`, **returns new head** |
| `CONCAT(a, b)` | `a` then `b` (shares `b`'s cells) |
| `FREELIST(x)` | recycle an owned, unshared list |

### B.6 Reducers, timing, diagnostics

| Call | Result |
|------|--------|
| `SUM(v1, v2)` | element-wise sum of two vectors → new vector |
| `PAIRWISE_MIN/MAX/ADD(p)` | reduce a pack's lanes to a scalar |
| `TIMER_START()` | a monotonic timestamp (ns) |
| `TIMER_END(t)` | elapsed ns since `t` |
| `TIMER_DISPLAY(ns)` | print a duration |
| `SLEEP(ms)` | pause |
| `HEAP_INFO()` | print allocator statistics |
| `GC()` | request collection (no-op in the no-GC model) |

### B.7 Strings (NSString)

A string is a Cocoa `NSString` (Chapter 9). Characters are Unicode code points.

| Call / form | Result |
|------|--------|
| `"…"` | an immortal `NSString` literal (escapes cooked at compile time) |
| `s % i` | the i-th character of `s`, as a code point (read-only); 0 past the end |
| `LEN(s)` | number of code points in `s` |
| `WRITES(s)` | write `s`'s text |
| `WRITEF("…%s…", s)` | splice `s` into formatted output |
| `WRITEC(c)` | write code point `c` as UTF-8 |
| `JOIN(list, sep)` | build a new (owned) string from a list of strings + separator |
| `=` `~=` | compare string *identity* (equal literals share one object) |

Built (`JOIN`ed) strings follow the same Cocoa ownership as objects: released at
scope exit, or with `USING`; ownership transferred on escape. Literals are
immortal. Strings are immutable — there is no in-place edit and no `+`.

---

## Appendix C — Grammar Summary

An informal grammar of the surface forms. `[x]` is optional, `{x}` is zero or
more, `|` is alternation.

```
program     ::= { declaration }

declaration ::= "LET" binding { "AND" binding }
              | "MANIFEST" namedblock
              | "STATIC" namedblock  | "GLOBAL" namedblock
              | "CLASS" name ["EXTENDS" name] ["MANAGED"] "$(" { member } "$)"
              | "GET" string

binding     ::= name "=" expr                          // value
              | name "(" [params] ")" "=" expr          // function
              | name "(" [params] ")" "BE" stmt         // routine
params      ::= param { "," param }
param       ::= name ["AS" type]

member      ::= visibility ":"
              | "DECL" name { "," name } ["AS" type]
              | "LET" name "=" expr  | "FLET" name "=" expr
              | [modifier] "FUNCTION" name "(" [params] ")" "=" expr
              | [modifier] "ROUTINE"  name "(" [params] ")" "BE" stmt
visibility  ::= "PUBLIC" | "PRIVATE" | "PROTECTED"
modifier    ::= "VIRTUAL" | "FINAL"

stmt        ::= "$(" { stmt } "$)"   |  "{" { stmt } "}"
              | lvalue { "," lvalue } ":=" expr { "," expr }
              | "IF" expr "THEN" stmt
              | "UNLESS" expr "THEN" stmt
              | "TEST" expr "THEN" stmt "ELSE" stmt
              | "WHILE" expr "DO" stmt   | "UNTIL" expr "DO" stmt
              | stmt "REPEAT"  | stmt "REPEATWHILE" expr | stmt "REPEATUNTIL" expr
              | "FOR" name "=" expr "TO" expr ["BY" expr] "DO" stmt
              | "FOREACH" name ["," name] ["AS" type] "IN" expr "DO" stmt
              | "SWITCHON" expr "INTO" "$(" { case } "$)"
              | "RESULTIS" expr | "RETURN" | "FINISH"
              | "BREAK" | "LOOP" | "ENDCASE" | "GOTO" name | name ":"
              | "USING" name "=" expr "DO" stmt
              | "RETAIN" name ["=" expr]
              | "BRK"
              | expr                                    // call, etc.
case        ::= "CASE" const ":" { stmt } | "DEFAULT" ":" { stmt }

expr        ::= expr "->" expr "," expr                 // conditional
              | expr binop expr | unop expr
              | expr "(" [args] ")"                      // call
              | expr "!" expr | expr "%" expr | expr ".%" expr
              | expr "%%" "(" expr "," expr ")"          // bit-field
              | expr "." name | expr "OF" name           // member
              | expr ".|" expr "|"                       // lane
              | "VALOF" stmt | "FVALOF" stmt
              | "NEW" name ["(" [args] ")"]
              | objcmsg                                   // Cocoa message send
              | "VEC" expr | "FVEC" expr
              | "TABLE" "(" args ")" | "FTABLE" "(" args ")"
              | "LIST" "(" [args] ")" | "MANIFESTLIST" "(" [args] ")"
              | "PAIR" "(" args ")"  | "QUAD" "(" args ")" | "OCT" "(" args ")"
              | constant | name | "SELF" | "SUPER" | "?"
              | "(" expr ")"

objcmsg     ::= "[" receiver name "]" ["AS" type]                       // unary
              | "[" receiver { name ":" expr ["AS" type] }+ "]" ["AS" type]  // keyword
receiver    ::= "SELF" | name | expr        // a bare capitalized name = a Cocoa class
```

---

*End of manual. Every program in this book runs under `newbcpl-driver run`. When
the language and this description disagree, the language is right and the
description is a bug — report it.*
