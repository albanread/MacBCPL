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
