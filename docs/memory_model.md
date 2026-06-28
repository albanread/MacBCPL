# Memory model: no GC, stack-scoped heap

> **Supersedes** the upstream manifesto §5 GC contract. NewBCPL's precise
> mark-sweep tracing GC is **gone**. MacBCPL's design goal is *"heap-allocated
> objects follow stack-scope semantics"* — heap objects with LIFO lifetimes,
> freed automatically at scope exit, with no collector and no safepoints.

## The three tiers (plus tier 0)

Every allocation lands in exactly one tier. The choice is made at compile time
behind the single allocation choke point (`__newbcpl_alloc_rec` /
`__newbcpl_new_rec`).

- **Tier 0 — stack / registers / static.** Locals, params, the `VALOF` result
  slot, SIMD packs, string literals. Unchanged.
- **Tier 1 — per-function arena (default for proven scope-local transients).**
  Bump allocation; freed *wholesale* at function exit. This is the "stack-scope
  semantics" tier. v1 granularity is **per function** (one arena per function),
  which makes `GOTO` and `RESULTIS`-in-`VALOF` safe *by construction* — there are
  no inner arenas to skip, and the single arena outlives every intra-function
  jump. Routable sites: a direct `LET name = VEC/FVEC/TABLE(…)` where `name` is
  proven non-escaping (`is_vector_kind`). Lists are **never** arena-allocated.
- **Tier 2 — program-global manual free-list heap.** First-fit / split /
  coalesce (the reused free-list code from the old `gc.rs`). This backs explicit
  `GETVEC` / `FREEVEC`, **all lists** (cons cells alias via `TL`/`REST`, so they
  can never be arena-freed), and is the **promotion target** for anything that
  escapes its scope. `FREEVEC` / `FREELIST` are now *real* (they were no-ops
  upstream).
- **Tier 3 — Cocoa.** `NEW Class` objects are real Obj-C objects managed by
  `objc_retain` / `objc_release`. See [cocoa_objects.md](cocoa_objects.md).

## Scope automation

Arena and object cleanup ride the **same exit machinery** that `USING`/`RELEASE`
already used. `enter_function_arena` is emitted at function entry; cleanup fires
on the **seven true function-exit edges** only:

1. `RETURN`
2. `RESULTIS` *outside* a `VALOF` (the fallback form)
3. `FINISH`
4. routine fall-through
5. synthetic-`CREATE` fall-through
6. expression-body function epilogue (bypasses `lower_stmt` — highest risk site)
7. method `Function`-arm exit

Cleanup runs innermost-first and is idempotent (handle-guarded, so a
double-fire is a no-op). It is **never** fired at `BREAK` / `LOOP` / `ENDCASE` /
`GOTO` / `VALOF`-internal `RESULTIS` — freeing the single per-function arena
there would be a use-after-free.

## Escape analysis

A sema/lowering pre-pass decides arena-vs-heap *before* bytes are placed
(marking a store or return site is too late). Each binding starts **escaping =
true** (conservative) and is cleared to `false` only when provably scope-local.

A binding **escapes** (⇒ Tier 2 heap) if it, an alias of it, or anything
reachable from it is ever:

- the value of a `GlobalStore` / indirect store (`v!i:=`, `!p:=`, `%p:=`) / `FieldStore`;
- an element of a list cons (`APND` / `CONCAT`);
- in a `RETURN` / `RESULTIS` value position (including the expression-body tail);
- a `RETAIN` operand;
- stored into an outer / `GLOBAL` / `STATIC` binding.

Escape is closed transitively over **alias edges** (`LET y = x`, destructuring)
and **reachability** (promotion is a shallow copy, so an escaping struct with
interior arena pointers is heaped *itself plus its referents*). Anything
unknown, or an anonymous/unbound `TypedConstruct`, defaults to escaping ⇒ heap.

**Use-after-free is the cardinal sin.** The runtime fallback (no active arena ⇒
`heap_alloc`) means a *missed* arena enter degrades to a leak, never a wild
pointer. A debug IR-verifier asserts no arena-tagged value is an operand of
`Return` / `GlobalStore` / `IndirectStore` / `FieldStore` / `APND`.

## Object ownership (Cocoa tier)

Design principle (from the user): **automatic for the common case, explicit for
the edges, never crash.**

- Scope-local `LET o = NEW C()` → auto `objc_release` at the epilogue (only
  `+1`-owned direct-`NEW` bindings, so over-release is impossible).
- `USING o = NEW C()` (or `USING r = MK()`) → runs the user `RELEASE` method if
  present, then releases the memory, on every exit edge — the explicit
  deterministic-disposal tool.
- Escapers (`RESULTIS` / `RETAIN` / store / alias) → ownership transferred, not
  released.
- Plain `LET r = MK()` call-return → **not** auto-released (safe leak; the user
  is steered toward `USING`).
- Reassigning an owned binding (`owns_new`) emits a compile-time **leak
  warning** ("reassigning `p` leaks the object it owns … own objects with
  USING…"), per *"always USING is better than sometimes."*

## Lists

Lists live on the Tier-2 manual heap and never in an arena. The fast-list
redesign moves them to tagless **cons cells** (a list value is a 64-bit word:
`NIL = 0`, else a pointer to a 16-byte cell `[hd@0, tl@8]`), with `HD` / `TL` /
`REST` open-coded as inline `!0` / `!1` loads (no runtime calls). `APND` returns
the new head (callers must write `xs := APND(xs, v)`). `FREELIST` is by-contract:
do not free a list whose cells are shared via `TL` / `REST` / `CONCAT`.

## Status

Phase-2 core is **done and verified**: the tracing GC is replaced by the
Tier-1 arena + Tier-2 manual heap + Tier-3 Cocoa model, including Cocoa objects
being stack-scoped. Remaining follow-ons: landing the fast-list cons-cell
representation, per-block (v2) arenas, and `bcpl_promote`-based `RETAIN`
promotion (v1 simply heaps `RETAIN`ed bindings).
