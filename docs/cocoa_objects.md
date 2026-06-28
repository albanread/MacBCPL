# BCPL objects are Cocoa objects

In MacBCPL, a BCPL `CLASS` instance **is a real Objective-C / Cocoa object**.
There is no parallel vtable scheme: `CLASS` / `EXTENDS` / `VIRTUAL` / `FINAL` /
`MANAGED` / `SUPER` / `NEW` are lowered directly onto the Objective-C runtime
through the objc bridge ported from MacModula2
(`src/newm2-runtime/src/objc.rs`, the `nm2_objc_*` / `bcpl_objc_*` C-ABI,
resolved by `dlsym`).

This object extension is treated as **our own non-standard greenfield feature** —
the fork is free to change its design, syntax, semantics and layout to fit
Cocoa, and is not required to preserve the upstream object scheme or pass the
old object-related corpus/matrix tests.

## Mapping

| BCPL construct | Objective-C runtime lowering |
|----------------|------------------------------|
| `CLASS C` | `objc_allocateClassPair(super, "BCPL_<run>_C", 0)` + `objc_registerClassPair` (in the JIT registrar) |
| `C EXTENDS B` | real Obj-C superclass (`BCPL_…_B`, else `NSObject`) |
| own fields | `class_addIvar("__bcpl_C", own_fields_size, …)` — **per-class** ivar of only this class's own fields |
| `NEW C(args)` | `[[C alloc] init]` via `objc_msgSend`, then `C_CREATE(obj, _cmd, args)` |
| `obj.method(args)` | `objc_msgSend(obj, sel, args)` via a per-call-site typed cast |
| `SUPER.method(args)` | `objc_msgSendSuper` (with a `Null` `_cmd` injected) |
| `SELF.field` / `obj.field` | `bcpl_objc_field_base_for(obj, "__bcpl_<defining_class>")` + own-relative offset |
| `RELEASE` / `USING` | user `RELEASE` method (if any) then `bcpl_objc_release`; `RETAIN` → `objc_retain` |

### Per-class ivar composition (not the offset trick)

Each BCPL class `class_addIvar`s **only its own fields**; the Obj-C runtime
composes inherited fields for us. This is deliberately *not* the single-`__bcpl`-
ivar-plus-offset scheme — that would double-allocate inherited fields when both
`Base` and `Sub` are BCPL classes. Sema computes an own-relative field offset
(`own_offset`, starts at 0 per class, +8 per field, no vtable header) and the
`defining_class` of each `FieldLoad` / `FieldStore`, so a field access resolves
to *that class's* ivar base.

### Selector mangling

A BCPL method `m` becomes the Obj-C selector **`bcpl_<m>`** (in both dispatch
and the registrar). This isolates BCPL methods from `NSObject`'s own selectors:
a BCPL method named `init` (or `release`, `alloc`, …) would otherwise collide
with `NSObject`'s — e.g. a 2-arg `init` returning `i64 0` makes
`[[Class alloc] init]` return `nil` and silently corrupts everything. Mangling
removes the collision.

> **Future GUI hook:** overriding a *real* Cocoa method (`drawRect:` etc.) will
> need an explicit opt-in escape hatch to register under the **raw** selector
> instead of the mangled one.

### No-op CREATE / RELEASE

The registrar binds `__newbcpl_default_method` (returns 0) to undefined
`CREATE` / `RELEASE` selectors on root BCPL classes, so an explicit
`obj.RELEASE()` on a class *without* a `RELEASE` method is safe (`NSObject`'s
`release` ≠ the mangled `bcpl_RELEASE`). `USING` cleanup is likewise guarded on
`has_release`.

## ABI notes

- arm64 has **no** `objc_msgSend_stret` / `_fpret`; a per-call-site typed
  `objc_msgSend` cast is sound (the snapshot path already returns a 4×`f64`
  `NSRect` this way).
- All method params are forced to `TypeHint::Word` → `i64` in LLVM, so `SELF`,
  `_cmd` and every argument are `i64` at the register level (ptr ≡ i64 there;
  MCJIT has no verifier, so the loose typing is fine). The msgSend cast uses
  `i64` for all args; the return type comes from the call-site hint.
- Type encodings are synthesised from signatures (`"q@:…"`): `Word`/`i64` → `q`,
  `f64` → `d`, ptr/obj → `@`. A routine's return encoding is `q`, **not** `v`
  (routines physically return `i64 0`).
- The class name carries a **per-run prefix** (`BCPL_<process-monotonic-runid>_`)
  because the JIT engine leaks across `run()` calls; the prefix prevents stale
  class reuse.

## Verified behaviour

`Base` + `Sub EXTENDS Base`, `SUPER.CREATE` / `describe`, reading an inherited
(Base ivar) field plus an own (Sub ivar) field → `sum=37`; the test matrix
remained green with no object-related failures. Scope-local objects are released
at the epilogue; returned objects stay valid in the caller; over-release would
trip `SIGABRT` (a deliberate regression catcher in `tests/.../arena.rs`).
