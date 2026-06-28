//! The Cocoa selector database — return-kind data that lets Objective Forth's `->`
//! pick the right message-send shape (cell / double / NSRect / …) per selector.
//!
//! Ported from MacModula2's `cocoadb` (which generated it via `newm2-cocoa-gen`).
//! The JSON is our own generator's small, fixed shape, so it is parsed line-by-line
//! without a JSON dependency, and embedded into the binary with `include_str!`.
//! Each selector maps to a return *kind* and an argument count:
//!   @ id/ptr · : SEL · i int · u uint · d real · B bool · v void · { struct · ? other
//!   N NSRange · P NSPoint · S NSSize · R NSRect  (the named geometry structs)

use std::collections::{HashMap, HashSet};

/// The selector-return-kind database, baked from `data/cocoa-selectors.json`.
const DB_JSON: &str = include_str!("../data/cocoa-selectors.json");

#[derive(Debug, Clone)]
pub struct SelSig {
    /// Return kind: a scalar (`@ : i u d B v ?`), a named geometry struct
    /// (`N`/`P`/`S`/`R`), or a struct descriptor `{…}`.
    pub ret: String,
    /// Argument kinds, one char per argument (the leading char of each arg's
    /// encoding: `@ i u d B R P S N …`). Drives typed-argument send routing.
    pub args: String,
}

impl SelSig {
    pub fn argc(&self) -> usize {
        self.args.len()
    }
    /// True when every argument is a C `double` — the case the all-float sends
    /// (osend1f..osend4f) handle by pulling args off the Forth float stack.
    pub fn all_double(&self) -> bool {
        !self.args.is_empty() && self.args.bytes().all(|b| b == b'd')
    }
}

#[derive(Debug, Default)]
pub struct CocoaDb {
    pub selectors: HashMap<String, SelSig>,
    pub classes: HashSet<String>,
}

impl CocoaDb {
    /// Load the embedded database. A selector absent from the DB falls back to an
    /// `id`/cell result (the pre-typing behaviour), so coverage gaps are harmless.
    pub fn load() -> CocoaDb {
        let mut db = CocoaDb::default();
        for line in DB_JSON.lines() {
            let t = line.trim();
            if t.starts_with("\"classes\":") {
                // "classes": ["NSObject", "NSString", …] — names at odd splits ≥ 3
                for (i, seg) in t.split('"').enumerate() {
                    if i >= 3 && i % 2 == 1 {
                        db.classes.insert(seg.to_string());
                    }
                }
                continue;
            }
            if let Some((sel, sig)) = parse_selector_line(t) {
                db.selectors.insert(sel, sig);
            }
        }
        db
    }

    /// The return kind for a selector, if the DB covers it.
    pub fn ret_of(&self, selector: &str) -> Option<&str> {
        self.selectors.get(selector).map(|s| s.ret.as_str())
    }

    /// The full signature (return kind + arg kinds) for a selector.
    pub fn lookup(&self, selector: &str) -> Option<&SelSig> {
        self.selectors.get(selector)
    }

    pub fn is_empty(&self) -> bool {
        self.selectors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_return_and_arg_kinds() {
        let db = CocoaDb::load();
        assert!(!db.is_empty());
        // 4-double class method -> "dddd", all_double
        let c = db.lookup("colorWithRed:green:blue:alpha:").unwrap();
        assert_eq!((c.ret.as_str(), c.args.as_str(), c.all_double(), c.argc()), ("@", "dddd", true, 4));
        // single double
        assert!(db.lookup("setAlphaValue:").unwrap().all_double());
        assert!(db.lookup("addTimeInterval:").unwrap().all_double());
        // two ids -> not all_double
        let s = db.lookup("setObject:forKey:").unwrap();
        assert_eq!((s.args.as_str(), s.all_double()), ("@@", false));
        // an NSRect arg is a struct, not a double
        assert_eq!(db.lookup("setFrame:").unwrap().args, "R");
        // a zero-arg selector has no args
        assert_eq!(db.lookup("doubleValue").unwrap().argc(), 0);
    }
}

/// Parse one selector entry line: `"sel": {"ret": "R", "args": ["x", …]}`.
fn parse_selector_line(t: &str) -> Option<(String, SelSig)> {
    let rest = t.strip_prefix('"')?;
    const MID: &str = "\": {\"ret\": \"";
    let q = rest.find(MID)?;
    let sel = rest[..q].to_string();
    let after = &rest[q + MID.len()..];
    let rq = after.find('"')?; // closing quote of the ret value
    let ret = after[..rq].to_string();
    // Arg kinds: `"args": ["@", "u"]` -> the leading char of each quoted element.
    let args = match after.find("\"args\": [") {
        Some(ai) => {
            let arr = &after[ai + "\"args\": [".len()..];
            let end = arr.find(']').unwrap_or(0);
            arr[..end]
                .split('"')
                .enumerate()
                .filter(|(i, seg)| i % 2 == 1 && !seg.is_empty()) // odd splits = quoted bodies
                .filter_map(|(_, seg)| seg.chars().next())
                .collect()
        }
        None => String::new(),
    };
    Some((sel, SelSig { ret, args }))
}
