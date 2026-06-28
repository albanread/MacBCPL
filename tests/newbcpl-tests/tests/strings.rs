//! Cocoa NSString string probes.
//!
//! A BCPL `String` value is now an Obj-C `id` (an immutable NSString),
//! managed like every other Cocoa object: literals are immortal cached
//! constants; owned operation results (JOIN, string-returning calls) are
//! +1 and released at the scope epilogue / by USING / on strong-store
//! overwrite; `s % i` and LEN go through a runtime char-fetch over the
//! UTF-8 bytes. These probes pin the behaviour AND the use-after-free /
//! over-release / leak safety found by the design-verify pass — a wrong
//! ownership decision surfaces here as a crash or wrong output.

use newbcpl_tests::expect_stdout as expect;

/// Literal print: a literal is an immortal NSString; WRITES extracts its
/// UTF-8. Running the same literal twice reuses the one immortal (no
/// double-create, no release).
#[test]
fn literal_print() {
    expect(
        "str_literal",
        "LET START() BE $(\n  WRITES(\"hello*N\")\n  WRITES(\"hello*N\")\n$)\n",
        "hello\nhello\n",
    );
}

/// `s % i` (runtime char-fetch) and LEN (UTF-8 byte count) share the
/// same index domain, so a FOR scan round-trips the bytes.
#[test]
fn len_and_char_fetch() {
    expect(
        "str_len_char",
        "LET START() BE $(\n  LET s = \"abcde\"\n  WRITEN(LEN s)\n  WRITEC(s % 0)\n  WRITEC(s % 4)\n  FOR i = 0 TO LEN(s) - 1 DO WRITEC(s % i)\n$)\n",
        "5aeabcde",
    );
}

/// `s % i` out of range / negative returns 0 (tolerant read).
#[test]
fn char_fetch_out_of_range() {
    expect(
        "str_char_oob",
        "LET START() BE $(\n  LET s = \"hi\"\n  WRITEN(s % -1)\n  WRITEN(s % 2)\n  WRITEN(s % 0)\n$)\n",
        "00104",
    );
}

/// `s % i` returns Unicode CODE POINTS (not UTF-8 bytes); LEN counts code
/// points; WRITEC re-encodes a code point as UTF-8. "café" = 4 code points
/// (not 5 UTF-8 bytes); é = U+00E9 = 233.
#[test]
fn char_fetch_returns_code_points() {
    expect(
        "str_codepoints",
        "LET START() BE $(\n  LET s = \"café\"\n  WRITEN(LEN s)\n  WRITEN(s % 3)\n  FOR i = 0 TO LEN(s) - 1 DO WRITEC(s % i)\n$)\n",
        "4233café",
    );
}

/// Astral characters (beyond the BMP) are ONE code point — UTF-16
/// `characterAtIndex:` would split 😀 into a surrogate pair. "★😀" = 2 code
/// points; 😀 = U+1F600 = 128512; the FOR echo round-trips it.
#[test]
fn char_fetch_astral_code_point() {
    expect(
        "str_codepoints_astral",
        "LET START() BE $(\n  LET e = \"★😀\"\n  WRITEN(LEN e)\n  WRITEN(e % 1)\n  FOR i = 0 TO LEN(e) - 1 DO WRITEC(e % i)\n$)\n",
        "2128512★😀",
    );
}

/// JOIN a list of string ids → an owned +1 NSString; bind, print,
/// release at epilogue (no leak, no crash).
#[test]
fn join_list_of_strings() {
    expect(
        "str_join",
        "LET START() BE $(\n  LET j = JOIN(LIST(\"foo\", \"bar\", \"baz\"), \"-\")\n  WRITES(j)\n$)\n",
        "foo-bar-baz",
    );
}

/// Anonymous JOIN temp passed straight to WRITES: spilled-release after
/// the non-capturing call — correct output, no leak, no over-release.
#[test]
fn anonymous_join_temp() {
    expect(
        "str_join_anon",
        "LET START() BE $(\n  WRITES(JOIN(LIST(\"a\", \"b\", \"c\"), \",\"))\n$)\n",
        "a,b,c",
    );
}

/// WRITEF `%s` consumes an NSString id argument.
#[test]
fn writef_percent_s() {
    expect(
        "str_writef",
        "LET START() BE $(\n  WRITEF(\"name=%s n=%d\", \"Bob\", 42)\n$)\n",
        "name=Bob n=42",
    );
}

/// ADVERSARIAL A: reassign an owned-string slot to an immortal literal,
/// then the epilogue releases the slot. The strong store must release the
/// old JOIN result and RETAIN the immortal so it survives later uses — a
/// bug here over-releases the immortal → SIGABRT. The literal is long
/// enough to be a heap __NSCFString (not a tagged pointer), so an
/// over-release crashes hard rather than silently no-op'ing.
#[test]
fn reassign_owned_to_immortal_survives() {
    expect(
        "str_reassign",
        "LET START() BE $(\n  LET s = JOIN(LIST(\"x\", \"y\"), \"\")\n  WRITES(s)\n  s := \"a long heap-backed banner literal value\"\n  WRITES(s)\n  WRITES(\"a long heap-backed banner literal value\")\n$)\n",
        "xya long heap-backed banner literal valuea long heap-backed banner literal value",
    );
}

/// USING disposes an owned string deterministically at block exit.
#[test]
fn using_string() {
    expect(
        "str_using",
        "LET START() BE $(\n  USING u = JOIN(LIST(\"u\", \"sing\"), \"-\") DO WRITES(u)\n  WRITES(\" end\")\n$)\n",
        "u-sing end",
    );
}

/// ADVERSARIAL (escape): a string RETURNED from a factory escapes — it
/// must NOT be released in the callee, and must stay valid in the caller.
#[test]
fn resultis_escape_transfers_ownership() {
    expect(
        "str_escape",
        "LET mk() = VALOF $(\n  LET s = JOIN(LIST(\"deep\", \"value\"), \":\")\n  RESULTIS s\n$)\nLET START() BE $(\n  LET r = mk()\n  WRITES(r)\n$)\n",
        "deep:value",
    );
}

/// ADVERSARIAL B: many owned strings created in a loop; each iteration's
/// +1 is balanced by the release-on-overwrite of the next store and the
/// final one by the epilogue. A double-release would crash here.
#[test]
fn loop_owned_strings_no_crash() {
    expect(
        "str_loop",
        "LET use() BE $(\n  LET t = JOIN(LIST(\"loop\", \"item\"), \"/\")\n  WRITES(\"\")\n$)\nLET START() BE $(\n  FOR i = 1 TO 500 DO use()\n  WRITES(\"done\")\n$)\n",
        "done",
    );
}

// ─── Implementation-review regression probes (use >11-byte HEAP strings,
// not short tagged pointers, so over-release/raw-deref bugs actually fire)

/// REVIEW #1: a String-returning function that returns a BORROWED immortal
/// literal, bound to several locals. Must not be treated as a +1 producer
/// (that over-released the shared immortal → UAF/SIGSEGV). Heap-backed.
#[test]
fn string_returning_fn_borrow_no_crash() {
    expect(
        "str_fn_borrow",
        "LET NM() = \"a long heap-backed literal over eleven bytes\"\nLET START() BE $(\n  LET a = NM()\n  LET b = NM()\n  LET c = NM()\n  WRITES(a)\n  WRITES(b)\n  WRITES(c)\n$)\n",
        "a long heap-backed literal over eleven bytesa long heap-backed literal over eleven bytesa long heap-backed literal over eleven bytes",
    );
}

/// REVIEW #2/#4: pass a string into a helper that scans `s % i` and `LEN s`
/// via an `AS STRING` param (String hint propagates → char-fetch dispatch).
/// Heap string → previously a raw byte/VEC deref crash.
#[test]
fn string_param_scan_via_annotation() {
    expect(
        "str_param_scan",
        "LET upper(s AS STRING) = VALOF $(\n  LET n = 0\n  FOR i = 0 TO LEN(s) - 1 DO IF (s % i) >= 65 & (s % i) <= 90 THEN n := n + 1\n  RESULTIS n\n$)\nLET START() BE $(\n  WRITEN(upper(\"Hello Wonderful World\"))\n$)\n",
        "3",
    );
}

/// REVIEW #4: a string pulled from a list (HD → Word-hinted) then LEN'd.
/// The tagged-pointer-safe `__newbcpl_len` returns the byte length instead
/// of dereferencing the non-canonical id → SIGSEGV.
#[test]
fn string_from_list_len_no_crash() {
    expect(
        "str_hd_len",
        "LET START() BE $(\n  LET xs = LIST(\"Hello\")\n  LET h = HD xs\n  WRITEN(LEN h)\n$)\n",
        "5",
    );
}

/// REVIEW #5: assigning a non-object word into an owned-String slot must
/// not `retain`/`release` a bogus pointer. Heap-backed initial value.
#[test]
fn assign_int_to_owned_string_no_crash() {
    expect(
        "str_assign_int",
        "LET START() BE $(\n  LET s = JOIN(LIST(\"long enough\", \" to be heap\"), \"\")\n  WRITES(s)\n  s := 42\n  WRITES(\" after\")\n$)\n",
        "long enough to be heap after",
    );
}
