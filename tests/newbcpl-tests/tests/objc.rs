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

/// A send used as a bare statement (result discarded) runs and continues.
#[test]
fn statement_form_send() {
    expect(
        "objc_stmt",
        "LET START() BE $(\n  LET d = [[NSMutableArray alloc] init]\n  [d addObject: \"x\"]\n  [d removeAllObjects]\n  WRITEN([d count] AS INT)\n$)\n",
        "0",
    );
}
