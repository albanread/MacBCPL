//! Fast cons-cell list probes.
//!
//! A list is a pointer to a 16-byte cons cell [hd@0, tl@8] (NIL=0).
//! HD/TL/REST and FOREACH are open-coded inline (GEP+load, no runtime
//! call); cells come from a recycling free-list reclaimed by FREELIST.

use newbcpl_tests::expect_stdout as expect;

/// LIST literal + inline HD / TL / LEN.
#[test]
fn list_literal_hd_tl_len() {
    expect(
        "list_hd_tl_len",
        "LET START() BE $(\n  LET xs = LIST(10, 20, 30, 40)\n  WRITEN(HD(xs))\n  WRITEN(HD(TL(xs)))\n  WRITEN(LEN xs)\n$)\n",
        "10204",
    );
}

/// FOREACH walks the cons chain in order (inline pointer walk).
#[test]
fn foreach_walks_cons_chain() {
    expect(
        "foreach_cons",
        "LET START() BE $(\n  LET xs = LIST(1, 2, 3, 4)\n  LET s = 0\n  FOREACH x IN xs DO s := s + x\n  WRITEN(s)\n$)\n",
        "10",
    );
}

/// Manual `p!0` / `p!1` traversal — the same inline shape HD/TL sugar to.
#[test]
fn manual_pointer_traversal() {
    expect(
        "list_manual_walk",
        "LET START() BE $(\n  LET xs = LIST(5, 6, 7)\n  LET p = xs\n  LET s = 0\n  UNTIL p = 0 DO $( s := s + p!0\n p := p!1 $)\n  WRITEN(s)\n$)\n",
        "18",
    );
}

/// APND returns the (possibly new) head; build by capturing it. Start
/// from LIST() so the binding is List-hinted (LEN dispatches on the
/// static hint: a List walks cons cells, a VEC reads the p[-1] length).
#[test]
fn apnd_returns_head() {
    expect(
        "apnd_head",
        "LET START() BE $(\n  LET xs = LIST()\n  xs := APND(xs, 7)\n  xs := APND(xs, 8)\n  xs := APND(xs, 9)\n  WRITEN(HD(xs))\n  WRITEN(HD(TL(TL(xs))))\n  WRITEN(LEN xs)\n$)\n",
        "793",
    );
}

/// FREELIST recycles cells; a subsequent build reuses them (no crash,
/// correct values — the recycling free-list works end to end).
#[test]
fn freelist_then_rebuild() {
    expect(
        "list_freelist_recycle",
        "LET START() BE $(\n  LET xs = LIST(1, 2, 3)\n  FREELIST xs\n  LET ys = LIST(40, 50)\n  WRITEN(HD(ys))\n  WRITEN(HD(TL(ys)))\n  WRITEN(LEN ys)\n$)\n",
        "40502",
    );
}
