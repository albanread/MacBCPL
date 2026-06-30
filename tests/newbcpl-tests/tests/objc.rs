//! Objective-C `[receiver message: arg ...]` bracket message-send probes.
//!
//! These drive REAL Cocoa (Foundation) through the bracket sugar: raw,
//! un-mangled selectors via objc_msgSend, class-name and instance
//! receivers, per-arg ABI (incl. floats in d-registers), and the
//! `AS Type` return annotation. A BCPL string literal is itself an
//! NSString id, so it is a valid `id`/NSString argument.

use newbcpl_tests::expect_stdout as expect;

/// Zero-arg (unary) selector + a one-keyword selector, both returning an
/// NSString id that WRITES prints.
#[test]
fn unary_and_keyword_sends() {
    expect(
        "objc_unary_keyword",
        "LET START() BE $(\n  LET s = \"Hello\"\n  WRITES([s uppercaseString])\n  WRITES([s stringByAppendingString: \" World\"])\n$)\n",
        "HELLOHello World",
    );
}

/// `AS INT` return annotation: `[s length]` returns NSUInteger in x0.
#[test]
fn typed_int_return() {
    expect(
        "objc_int_return",
        "LET START() BE $(\n  WRITEN([\"Hello\" length] AS INT)\n$)\n",
        "5",
    );
}

/// Selector-DB return synthesis: NO annotation needed — `length`/`count`
/// (`u`→Int) and `doubleValue` (`d`→Float) get their hints from the DB.
#[test]
fn db_synthesizes_scalar_returns() {
    expect(
        "objc_db_scalars",
        "LET START() BE $(\n  WRITEN([\"Hello\" length])\n  FWRITE([[NSNumber numberWithDouble: 2.5] doubleValue])\n$)\n",
        "52.5",
    );
}

/// Class-name receiver -> a class method send (via bcpl_objc_get_class).
#[test]
fn class_receiver_send() {
    expect(
        "objc_class_send",
        "LET START() BE $(\n  WRITES([NSString stringWithString: \"from a class send\"])\n$)\n",
        "from a class send",
    );
}

/// Nested sends compose: the inner result is the outer receiver.
#[test]
fn nested_sends() {
    expect(
        "objc_nested",
        "LET START() BE $(\n  WRITES([[\"hi\" uppercaseString] stringByAppendingString: \"!\"])\n$)\n",
        "HI!",
    );
}

/// FLOAT arg (d-register) AND `AS FLOAT` return (d0) — the ABI-critical
/// path. numberWithDouble: takes a double; doubleValue returns one.
#[test]
fn float_arg_and_return() {
    expect(
        "objc_float",
        "LET START() BE $(\n  LET num = [NSNumber numberWithDouble: 2.5]\n  FWRITE([num doubleValue] AS FLOAT)\n$)\n",
        "2.5",
    );
}

/// Multi-keyword selector setObject:forKey: (two id args) + objectForKey:.
#[test]
fn multi_keyword_send() {
    expect(
        "objc_multi_keyword",
        "LET START() BE $(\n  LET d = [[NSMutableDictionary alloc] init]\n  [d setObject: \"the value\" forKey: \"k\"]\n  WRITES([d objectForKey: \"k\"])\n$)\n",
        "the value",
    );
}

/// DB-backed synthesis: a selector on a class the bundled 40-class JSON does
/// NOT cover (`NSProcessInfo.activeProcessorCount`) synthesizes to Int from
/// the live cocoa.sqlite mirror — no annotation. Gated on `COCOA_SQLITE`
/// (skips when the DB isn't configured; the env is inherited by the driver).
#[test]
fn db_synthesizes_non_json_selector() {
    if std::env::var("COCOA_SQLITE").is_err() {
        return; // DB not configured — covered by running the suite with COCOA_SQLITE set
    }
    expect(
        "objc_db_nonjson",
        "LET START() BE $(\n  WRITEN([[NSProcessInfo processInfo] activeProcessorCount] > 0)\n$)\n",
        "1",
    );
}

/// `bcpl_run_capture` shells out via `/bin/sh -c` and returns the
/// command's combined stdout/stderr as an NSString (a BCPL String). This
/// is the BCPL IDE's out-of-process Run primitive (crash isolation). A
/// zero-exit command gets no footer, so the output is verbatim.
#[test]
fn run_capture_shells_out_and_captures() {
    expect(
        "objc_run_capture",
        "LET START() BE $(\n  WRITES(bcpl_run_capture(\"printf hello\"))\n$)\n",
        "hello",
    );
}

/// `bcpl_selector` reifies a `SEL` from an NSString name — used to wire
/// menu items / targets to standard Cocoa actions (`terminate:`, `cut:`).
/// A valid name interns to a non-null selector.
#[test]
fn selector_interns_a_sel() {
    expect(
        "objc_selector_intern",
        "LET START() BE $(\n  LET s = bcpl_selector(\"terminate:\")\n  TEST s = 0 THEN WRITES(\"no\") ELSE WRITES(\"yes\")\n$)\n",
        "yes",
    );
}

/// `bcpl_set_text_color` colours a character range of an NSTextStorage
/// in place (the NSColor + NSRange are built in the runtime, so BCPL
/// passes only ints/floats). The IDE's syntax colouriser uses it; here we
/// just confirm it runs and leaves the text/length intact.
#[test]
fn set_text_color_applies_to_range() {
    expect(
        "objc_set_text_color",
        "LET START() BE $(\n  LET ts = [[NSTextStorage alloc] initWithString: \"hello world\"]\n  bcpl_set_text_color(ts, 0, 5, 1.0, 0.2, 0.2)\n  WRITEN([ts length] AS INT)\n$)\n",
        "11",
    );
}

/// `bcpl_is_keyword` recognises BCPL keywords by `(string, loc, len)` —
/// the IDE colouriser uses it to paint keywords. "LET"→1, "foo"→0,
/// "WHILE"→1.
#[test]
fn is_keyword_recognises_bcpl_keywords() {
    expect(
        "objc_is_keyword",
        "LET START() BE $(\n  LET s = \"LET foo WHILE\"\n  WRITEN(bcpl_is_keyword(s, 0, 3))\n  WRITEN(bcpl_is_keyword(s, 4, 3))\n  WRITEN(bcpl_is_keyword(s, 8, 5))\n$)\n",
        "101",
    );
}

/// `bcpl_error_line` pulls the diagnostic line number out of compiler
/// output (`… at L:C`) for the IDE's red error mark; 0 when clean.
#[test]
fn error_line_parses_diagnostic() {
    expect(
        "objc_error_line",
        "LET START() BE $(\n  WRITEN(bcpl_error_line(\"run: parse: oops at 7:12\"))\n  WRITEN(bcpl_error_line(\"ran clean\"))\n$)\n",
        "70",
    );
}

/// `bcpl_line_numbers(n)` builds the IDE gutter's text — "1\n2\n…\nn".
#[test]
fn line_numbers_builds_gutter() {
    expect(
        "objc_line_numbers",
        "LET START() BE WRITES(bcpl_line_numbers(3))\n",
        "1\n2\n3",
    );
}

// ─── Tier B: struct returns materialized as vectors ─────────────────

/// NSRange return (DB tag N) -> a 2-word VEC via the arm64 integer-pair
/// (x0/x1) ABI. Field names resolve via seeded manifests. Exact values.
#[test]
fn struct_return_nsrange_to_vec() {
    expect(
        "objc_nsrange",
        "LET START() BE $(\n  LET r = [\"hello world\" rangeOfString: \"world\"]\n  WRITEN(r ! NSRange_location)\n  WRITEN(r ! NSRange_length)\n$)\n",
        "65",
    );
}

/// NSRect return (DB tag R, 32B) -> a 4-double FVEC via the hidden sret
/// (x8) ABI. A zero-initialised NSView has a zero frame -> deterministic.
/// Reads the float fields via the `.%` float subscript + seeded manifests.
#[test]
fn struct_return_nsrect_to_fvec() {
    expect(
        "objc_nsrect",
        "LET START() BE $(\n  LET v = [[NSView alloc] init]\n  LET fr = [v frame]\n  FWRITE(fr .% NSRect_x)\n  FWRITE(fr .% NSRect_y)\n  FWRITE(fr .% NSRect_width)\n  FWRITE(fr .% NSRect_height)\n$)\n",
        "0000",
    );
}

// ─── Struct ARGUMENTS (by-value structs passed from an FVEC) ─────────

/// An NSRect passed BY VALUE as a struct argument (`valueWithRect:`, DB
/// arg "R"): the FVEC's 4 doubles are loaded and placed per the arm64 HFA
/// ABI (v0..v3), then read back via `rectValue` (struct return). NSValue
/// round-trips with no window/screen constraints, so it is deterministic.
#[test]
fn struct_arg_nsvalue_rect_roundtrip() {
    expect(
        "objc_structarg_rect",
        "LET START() BE $(\n  LET r = [[NSValue valueWithRect: (FVEC [1.0, 2.0, 3.0, 4.0])] rectValue]\n  FWRITE(r .% NSRect_x) FWRITE(r .% NSRect_y) FWRITE(r .% NSRect_width) FWRITE(r .% NSRect_height)\n$)\n",
        "1234",
    );
}

/// An NSSize struct argument (`valueWithSize:`, DB arg "S") round-trips via
/// `sizeValue`. Exercises a 2-double HFA struct arg interleaved with the
/// receiver/sel in x0/x1.
#[test]
fn struct_arg_nsvalue_size_roundtrip() {
    expect(
        "objc_structarg_size",
        "LET START() BE $(\n  LET s = [[NSValue valueWithSize: (FVEC [12.0, 34.0])] sizeValue]\n  FWRITE(s .% NSSize_width) FWRITE(s .% NSSize_height)\n$)\n",
        "1234",
    );
}

// ─── Implementation-review regression probes ────────────────────────

/// REVIEW #1: `%` / LEN on a bracket-send NSString result must not deref
/// the id as raw bytes. A known string selector is synthesized to a String
/// hint, so it routes to the safe char-fetch path. (Was a SIGSEGV.)
#[test]
fn string_returning_send_indexes_safely() {
    expect(
        "objc_str_index",
        "LET START() BE $(\n  LET u = [\"abcdef\" uppercaseString]\n  WRITEN(u % 0)\n  WRITEN(LEN u)\n$)\n",
        "656",
    );
}

/// REVIEW #1: `AS String` (mixed case) must behave like `AS STRING`.
#[test]
fn as_string_annotation_is_case_insensitive() {
    expect(
        "objc_as_string_case",
        "LET START() BE $(\n  LET v = [\"xy\" lowercaseString] AS String\n  WRITEN(LEN v)\n$)\n",
        "2",
    );
}

/// REVIEW #3: an int arg to a `double` selector param via a per-arg
/// `AS FLOAT` must ride a d-register (else garbage). 7 -> 7.0 round-trips.
#[test]
fn per_arg_as_float_routes_to_fp_register() {
    expect(
        "objc_arg_as_float",
        "LET START() BE $(\n  LET num = [NSNumber numberWithDouble: 7 AS FLOAT]\n  FWRITE([num doubleValue] AS FLOAT)\n$)\n",
        "7",
    );
}

/// A send used as a bare statement (result discarded) runs and continues.
#[test]
fn statement_form_send() {
    expect(
        "objc_stmt",
        "LET START() BE $(\n  LET d = [[NSMutableArray alloc] init]\n  [d addObject: \"x\"]\n  [d removeAllObjects]\n  WRITEN([d count] AS INT)\n$)\n",
        "0",
    );
}

/// Ownership (Cocoa Create Rule), consistent with `NEW`: a scope-local
/// `[[C alloc] init]` is a +1-owned object the compiler releases at the
/// epilogue, while a +0 borrowed result (`objectAtIndex:`) must NOT be
/// released. A further `[a ...]` send is a receiver USE, not an escape, so
/// `a` stays local and is released. This loop would crash (over-release /
/// use-after-free) if any of those rules were wrong; it must just finish.
#[test]
fn bracket_alloc_init_owned_released() {
    expect(
        "objc_owned_release",
        "LET START() BE $(\n  FOR i = 1 TO 300 DO $(\n    LET a = [[NSMutableArray alloc] init]\n    [a addObject: \"x\"]\n    LET first = [a objectAtIndex: 0]\n  $)\n  WRITES(\"ok\")\n$)\n",
        "ok",
    );
}

/// A +0 convenience constructor (`[NSMutableArray array]`) returns an
/// autoreleased object. The autorelease pool wrapping every run (on by
/// default) gives it a defined lifetime — valid for the run — so it is safe
/// to build and use without an explicit `alloc`/`init`.
#[test]
fn plus_zero_convenience_constructor_under_pool() {
    expect(
        "objc_autorelease_pool",
        "LET START() BE $(\n  LET a = [NSMutableArray array]\n  [a addObject: \"x\"]\n  [a addObject: \"y\"]\n  WRITEN([a count] AS INT)\n  WRITES([a componentsJoinedByString: \",\"])\n$)\n",
        "2x,y",
    );
}

/// A +1 bracket object RETURNED from a function escapes its scope, so the
/// callee must NOT release it — ownership transfers to the caller. If the
/// escape analysis released it in the callee, the caller's read would be a
/// use-after-free.
#[test]
fn bracket_owned_result_escapes() {
    expect(
        "objc_owned_escape",
        "LET mk() = VALOF $(\n  LET a = [[NSMutableArray alloc] init]\n  [a addObject: \"kept\"]\n  RESULTIS a\n$)\nLET START() BE $(\n  LET a = mk()\n  WRITES([a objectAtIndex: 0])\n$)\n",
        "kept",
    );
}
