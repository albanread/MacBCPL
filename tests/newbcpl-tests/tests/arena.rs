//! Phase-2 arena / no-GC memory model probes.
//!
//! These pin the "heap objects follow stack-scope semantics" behaviour
//! and — crucially — the use-after-free safety of the escape analysis: a
//! value that outlives its scope MUST be promoted to the manual heap, so
//! a wrong "doesn't escape" classification would surface here as a wrong
//! value or a crash, not silently.

use newbcpl_tests::expect_stdout as expect;

/// A scratch vector used only via local subscripting is arena-routed
/// (stack-scope lifetime). It must still compute correctly.
#[test]
fn scratch_vector_is_correct() {
    expect(
        "arena_scratch_vector",
        "LET START() BE $(\n  LET v = VEC 10\n  FOR i = 0 TO 10 DO v!i := i * i\n  WRITEN(v!5)\n$)\n",
        "25",
    );
}

/// A vector RETURNED from a function (RESULTIS v) escapes its scope, so
/// it must live on the manual heap and survive the callee's arena free.
/// If it were arena-routed this would read freed memory.
#[test]
fn returned_vector_survives_callee_arena_free() {
    expect(
        "arena_returned_vector",
        "LET MK() = VALOF $(\n  LET v = VEC 4\n  v!0 := 99\n  RESULTIS v\n$)\nLET START() BE $(\n  LET p = MK()\n  WRITEN(p!0)\n$)\n",
        "99",
    );
}

/// A vector passed as a call argument may be captured by the callee, so
/// it escapes → heap. Must read correctly inside the callee.
#[test]
fn vector_passed_as_arg_is_correct() {
    expect(
        "arena_arg_vector",
        "LET sink(p) BE WRITEN(p!0)\nLET START() BE $(\n  LET v = VEC 4\n  v!0 := 5\n  sink(v)\n$)\n",
        "5",
    );
}

/// Aliasing: `LET w = v` then using `w` makes `v` escape (same pointer).
/// Both must observe the same, valid storage.
#[test]
fn aliased_vector_is_correct() {
    expect(
        "arena_alias_vector",
        "LET sink(p) BE WRITEN(p!0)\nLET START() BE $(\n  LET v = VEC 4\n  v!0 := 7\n  LET w = v\n  sink(w)\n$)\n",
        "7",
    );
}

/// Recursion gives each call frame its own arena; a scratch vector per
/// frame must not be clobbered by deeper calls. Prints innermost-first.
#[test]
fn recursive_scratch_vectors_are_independent() {
    expect(
        "arena_recursion",
        "LET rec(n) BE $(\n  LET v = VEC 4\n  v!0 := n\n  IF n > 0 DO rec(n - 1)\n  WRITEN(v!0)\n$)\nLET START() BE $(\n  rec(3)\n$)\n",
        "0123",
    );
}

/// A vector declared before a loop and read after it must not be freed
/// by any per-iteration reset (there is none in v1 — the function arena
/// lives until function exit).
#[test]
fn vector_before_loop_survives_loop() {
    expect(
        "arena_loop",
        "LET START() BE $(\n  LET acc = VEC 100\n  FOR i = 0 TO 99 DO acc!i := i * i\n  WRITEN(acc!99)\n$)\n",
        "9801",
    );
}

// ─── Cocoa-object scope lifetimes ───────────────────────────────────
//
// A `LET o = NEW C()` is a +1-owned Obj-C object. Scope-local ones are
// `release`d at the procedure epilogue (stack-scope object lifetime);
// escaping ones transfer ownership and are NOT released. A wrong
// classification crashes (over-release → SIGABRT) or reads freed memory,
// so these correct-value probes are the safety guard.

const PT_CLASS: &str =
    "CLASS Pt $(\n  DECL x\n  LET CREATE(v) BE SELF.x := v\n  LET get() = SELF.x\n$)\n";

/// A scope-local object: created, a method called, released at epilogue.
/// Must compute correctly and not crash (no over-release).
#[test]
fn scope_local_object_released_cleanly() {
    expect(
        "obj_scope_local",
        &format!(
            "{PT_CLASS}LET START() BE $(\n  LET p = NEW Pt(42)\n  WRITEN(p.get())\n$)\n"
        ),
        "42",
    );
}

/// An object RETURNED (RESULTIS p) escapes — must NOT be released in the
/// creating function, and must stay valid in the caller.
#[test]
fn returned_object_survives_callee() {
    expect(
        "obj_returned",
        &format!(
            "{PT_CLASS}LET MK() = VALOF $(\n  LET p = NEW Pt(7)\n  RESULTIS p\n$)\nLET START() BE $(\n  LET q = MK()\n  WRITEN(q.get())\n$)\n"
        ),
        "7",
    );
}

/// Many scope-local objects created in a loop, each released at the
/// helper's epilogue — a double-release or leak-then-reuse would surface
/// as a crash here.
#[test]
fn looped_scope_local_objects() {
    expect(
        "obj_loop",
        &format!(
            "{PT_CLASS}LET use(n) BE $(\n  LET p = NEW Pt(n)\n  WRITEN(p.get())\n$)\nLET START() BE $(\n  FOR i = 1 TO 3 DO use(i)\n$)\n"
        ),
        "123",
    );
}

// ─── USING: deterministic object disposal (the explicit edge-case tool)
//
// USING runs the user RELEASE method (if any) AND frees the object's
// memory at scope exit, deterministically. It's the explicit ownership
// tool for objects the automatic scope-release can't prove (e.g. ones
// returned from a factory call). The user RELEASE printing is the
// observable signal that cleanup ran in order.

const RES_CLASS: &str = "CLASS Res $(\n  DECL id\n  LET CREATE(v) BE SELF.id := v\n  LET get() = SELF.id\n  LET RELEASE() BE $( WRITES(\"rel \") ; WRITEN(SELF.id) $)\n$)\n";

/// USING on a freshly-NEW'd object: body runs, then RELEASE at scope
/// exit (before code after the USING block).
#[test]
fn using_disposes_new_object() {
    expect(
        "using_new_object",
        &format!(
            "{RES_CLASS}LET START() BE $(\n  USING r = NEW Res(9) DO WRITES(\"use \")\n  WRITES(\"end\")\n$)\n"
        ),
        "use rel 9end",
    );
}

/// USING on a factory-returned object (direct `= NEW` factory): the
/// returned object's class is inferred, so USING disposes it.
#[test]
fn using_disposes_factory_object() {
    expect(
        "using_factory_object",
        &format!(
            "{RES_CLASS}LET MK(v) = NEW Res(v)\nLET START() BE $(\n  USING r = MK(4) DO WRITEN(r.get())\n  WRITES(\" end\")\n$)\n"
        ),
        "4rel 4 end",
    );
}

/// Explicit GETVEC/FREEVEC round-trip on the manual heap still works
/// (the manual tier is independent of the arena tier).
#[test]
fn getvec_freevec_roundtrip() {
    expect(
        "arena_getvec",
        "LET START() BE $(\n  LET v = GETVEC(4)\n  v!0 := 42\n  WRITEN(v!0)\n  FREEVEC(v)\n$)\n",
        "42",
    );
}
