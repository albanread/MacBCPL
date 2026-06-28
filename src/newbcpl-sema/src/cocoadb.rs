//! The Cocoa selector database — return-kind data that lets Objective Forth's `->`
//! pick the right message-send shape (cell / double / NSRect / …) per selector.
//!
//! Ported from MacModula2's `cocoadb` (which generated it via `newm2-cocoa-gen`).
//! The JSON is our own generator's small, fixed shape, so it is parsed line-by-line
//! without a JSON dependency, and embedded into the binary with `include_str!`.
//! Each selector maps to a return *kind* and an argument count:
//!   @ id/ptr · : SEL · i int · u uint · d real · B bool · v void · { struct · ? other
//!   N NSRange · P NSPoint · S NSSize · R NSRect  (the named geometry structs)

use core::ffi::{c_char, c_int, c_void};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

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

/// The selector signature database. Two backends behind one interface:
///   * `Json`  — the bundled `cocoa-selectors.json` (~40 classes); the
///               deterministic default.
///   * `Sqlite`— the shared `cocoa.sqlite` mirror of the WHOLE Obj-C surface
///               (26k classes, 482k method encodings), opted into by setting
///               `COCOA_SQLITE=/path/to/cocoa.sqlite`. Signatures are derived
///               on demand by parsing `rt_methods.encoding` and aggregating
///               across classes (a selector consistent across classes — the
///               common case — resolves to that signature; outliers lose to
///               the dominant kind). Receiver-class-exact lookup is a
///               follow-up that needs sema to track the static receiver class.
pub struct CocoaDb {
    json: HashMap<String, SelSig>,
    classes: HashSet<String>,
    sqlite: Option<Mutex<SqliteState>>,
}

impl CocoaDb {
    /// Load the metadata source: the `COCOA_SQLITE` mirror if the env var is
    /// set and the file opens, else the bundled JSON. A selector absent from
    /// whichever source falls back to an `id`/Object result, so gaps are
    /// harmless.
    pub fn load() -> CocoaDb {
        if let Ok(path) = std::env::var("COCOA_SQLITE") {
            if let Some(state) = SqliteState::open(&path) {
                return CocoaDb {
                    json: HashMap::new(),
                    classes: HashSet::new(),
                    sqlite: Some(Mutex::new(state)),
                };
            }
            eprintln!("cocoa_data: COCOA_SQLITE={path} could not be opened; using bundled JSON");
        }
        Self::load_json()
    }

    /// The bundled-JSON backend (the deterministic default; also used by tests).
    pub fn load_json() -> CocoaDb {
        let mut db = CocoaDb { json: HashMap::new(), classes: HashSet::new(), sqlite: None };
        for line in DB_JSON.lines() {
            let t = line.trim();
            if t.starts_with("\"classes\":") {
                for (i, seg) in t.split('"').enumerate() {
                    if i >= 3 && i % 2 == 1 {
                        db.classes.insert(seg.to_string());
                    }
                }
                continue;
            }
            if let Some((sel, sig)) = parse_selector_line(t) {
                db.json.insert(sel, sig);
            }
        }
        db
    }

    /// The full signature (return kind + arg kinds) for a selector, or None if
    /// the source doesn't cover it. Owned (the sqlite backend caches lazily).
    pub fn lookup(&self, selector: &str) -> Option<SelSig> {
        if let Some(m) = &self.sqlite {
            let mut st = m.lock().expect("cocoa sqlite mutex");
            if let Some(hit) = st.cache.get(selector) {
                return hit.clone();
            }
            let sig = st.query(selector);
            st.cache.insert(selector.to_string(), sig.clone());
            sig
        } else {
            self.json.get(selector).cloned()
        }
    }

    /// The return kind for a selector, if covered.
    pub fn ret_of(&self, selector: &str) -> Option<String> {
        self.lookup(selector).map(|s| s.ret)
    }

    pub fn is_empty(&self) -> bool {
        self.sqlite.is_none() && self.json.is_empty()
    }
}

// ─── sqlite backend (libsqlite3 via dlopen, like the runtime FFI) ────

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;
const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READONLY: c_int = 1;
const SQLITE_TRANSIENT: isize = -1;

type FnOpen = unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type FnPrepare =
    unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut *mut c_void, *mut *const c_char) -> c_int;
type FnBindText =
    unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int, *mut c_void) -> c_int;
type FnStep = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnColumnText = unsafe extern "C" fn(*mut c_void, c_int) -> *const u8;
type FnFinalize = unsafe extern "C" fn(*mut c_void) -> c_int;

struct SqliteFns {
    prepare: FnPrepare,
    bind_text: FnBindText,
    step: FnStep,
    column_text: FnColumnText,
    finalize: FnFinalize,
}

/// An open read-only handle to cocoa.sqlite plus a per-selector cache. The
/// raw `sqlite3*` and statement pointers are confined to a single-threaded
/// compile and serialized by the enclosing `Mutex`, so `Send` is sound.
struct SqliteState {
    fns: SqliteFns,
    db: *mut c_void,
    cache: HashMap<String, Option<SelSig>>,
}
unsafe impl Send for SqliteState {}

impl SqliteState {
    fn open(path: &str) -> Option<SqliteState> {
        unsafe {
            let lib = dlopen(c"libsqlite3.dylib".as_ptr(), RTLD_NOW);
            if lib.is_null() {
                return None;
            }
            let sym = |name: &core::ffi::CStr| dlsym(lib, name.as_ptr());
            let open_p = sym(c"sqlite3_open_v2");
            let prepare_p = sym(c"sqlite3_prepare_v2");
            let bind_p = sym(c"sqlite3_bind_text");
            let step_p = sym(c"sqlite3_step");
            let col_p = sym(c"sqlite3_column_text");
            let fin_p = sym(c"sqlite3_finalize");
            if [open_p, prepare_p, bind_p, step_p, col_p, fin_p].iter().any(|p| p.is_null()) {
                return None;
            }
            let open: FnOpen = std::mem::transmute(open_p);
            let cpath = std::ffi::CString::new(path).ok()?;
            let mut db: *mut c_void = std::ptr::null_mut();
            if open(cpath.as_ptr(), &mut db, SQLITE_OPEN_READONLY, std::ptr::null()) != SQLITE_OK
                || db.is_null()
            {
                return None;
            }
            Some(SqliteState {
                fns: SqliteFns {
                    prepare: std::mem::transmute(prepare_p),
                    bind_text: std::mem::transmute(bind_p),
                    step: std::mem::transmute(step_p),
                    column_text: std::mem::transmute(col_p),
                    finalize: std::mem::transmute(fin_p),
                },
                db,
                cache: HashMap::new(),
            })
        }
    }

    /// Resolve a selector's signature by parsing every `rt_methods.encoding`
    /// row for it and aggregating to the dominant return + arg kinds.
    fn query(&self, selector: &str) -> Option<SelSig> {
        let sql = c"SELECT encoding FROM rt_methods WHERE selector=?1 AND encoding IS NOT NULL";
        let mut rets: HashMap<char, usize> = HashMap::new();
        let mut argsets: HashMap<String, usize> = HashMap::new();
        unsafe {
            let mut stmt: *mut c_void = std::ptr::null_mut();
            if (self.fns.prepare)(self.db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut())
                != SQLITE_OK
                || stmt.is_null()
            {
                return None;
            }
            let Ok(csel) = std::ffi::CString::new(selector) else {
                (self.fns.finalize)(stmt);
                return None;
            };
            (self.fns.bind_text)(stmt, 1, csel.as_ptr(), -1, SQLITE_TRANSIENT as *mut c_void);
            while (self.fns.step)(stmt) == SQLITE_ROW {
                let txt = (self.fns.column_text)(stmt, 0);
                if txt.is_null() {
                    continue;
                }
                let enc = core::ffi::CStr::from_ptr(txt as *const c_char).to_string_lossy();
                let (ret, args) = parse_method_encoding(&enc);
                *rets.entry(ret).or_insert(0) += 1;
                *argsets.entry(args).or_insert(0) += 1;
            }
            (self.fns.finalize)(stmt);
        }
        if rets.is_empty() {
            return None;
        }
        // Dominant kind wins (consistent selectors resolve exactly; rare
        // outliers lose). Receiver-class-exact lookup is the future refinement.
        let ret = rets.iter().max_by_key(|(_, n)| **n).map(|(c, _)| *c).unwrap();
        let args = argsets.into_iter().max_by_key(|(_, n)| *n).map(|(a, _)| a).unwrap_or_default();
        Some(SelSig { ret: ret.to_string(), args })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_method_encodings() {
        // getter returning NSRect (by name): {CGRect=...}16@0:8
        assert_eq!(
            parse_method_encoding("{CGRect={CGPoint=dd}{CGSize=dd}}16@0:8"),
            ('R', String::new())
        );
        // setFrame:display: -> void, args NSRect + BOOL
        assert_eq!(
            parse_method_encoding("v52@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16B48"),
            ('v', "RB".to_string())
        );
        // count -> unsigned long
        assert_eq!(parse_method_encoding("Q16@0:8"), ('u', String::new()));
        // doubleValue -> double
        assert_eq!(parse_method_encoding("d16@0:8"), ('d', String::new()));
        // an id getter
        assert_eq!(parse_method_encoding("@16@0:8"), ('@', String::new()));
        // NSRange by shape (anonymous): {?=QQ}
        assert_eq!(parse_method_encoding("{?=QQ}16@0:8"), ('N', String::new()));
        // NSPoint by name as an arg: setFrameOrigin: -> v, arg P
        assert_eq!(
            parse_method_encoding("v36@0:8{CGPoint=dd}16"),
            ('v', "P".to_string())
        );
        // a string arg + int arg: setObject:forKey: style (two ids)
        assert_eq!(
            parse_method_encoding("v32@0:8@16@24"),
            ('v', "@@".to_string())
        );
        // CGAffineTransform (6 doubles) is NOT a geometry tag -> complex {
        assert_eq!(parse_method_encoding("{CGAffineTransform=dddddd}48@0:8").0, '{');
    }

    #[test]
    fn parses_return_and_arg_kinds() {
        let db = CocoaDb::load_json();
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

// ─── Obj-C @encode parser (ported from cocoa_data/encoding.py) ──────
//
// Turns a runtime method encoding (e.g. `{CGRect={CGPoint=dd}{CGSize=dd}}16@0:8`
// or `v52@0:8{CGRect=...}16B48`) into the SAME kind vocabulary the selector
// JSON uses, so the synthesis layer is unchanged whether the data came from
// the bundled JSON or the live cocoa.sqlite mirror. Kinds:
//   @ id/ptr · i int · u uint · d real · B bool/char · v void · : SEL
//   R NSRect · P NSPoint · S NSSize · N NSRange (geometry, by name OR shape)
//   { other struct/union/array (complex — synthesis treats as Object)
//   ? anything unmodelable

/// A parsed @encode type (only the structure the kind mapper needs).
#[derive(Debug, Clone)]
enum Ty {
    Scalar(char), // normalized leaf code: q i I etc. (see SCALAR_CHARS)
    Ptr,
    Struct { name: Option<String>, fields: Vec<Ty> },
    Other, // union / array / bitfield / unknown — not modelled
}

/// @encode scalar char → a normalized leaf code (arm64/LP64 widths collapsed
/// to a signedness/width class we care about): d=float, q=signed-int,
/// Q=unsigned-int, B=bool/char, @=pointer/id.
fn scalar_norm(c: char) -> Option<char> {
    Some(match c {
        'c' | 's' | 'i' | 'l' | 'q' => 'q', // signed integer
        'C' | 'S' | 'I' | 'L' | 'Q' => 'Q', // unsigned integer
        'B' => 'B',                          // bool
        'f' | 'd' | 'D' => 'd',             // float/double
        '@' | '#' | '*' => '@',             // id / Class / char*
        ':' => ':',                          // SEL
        'v' => 'v',                          // void
        _ => return None,
    })
}

struct EncParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> EncParser<'a> {
    fn new(s: &'a str) -> Self {
        EncParser { b: s.as_bytes(), i: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }
    fn skip_quals(&mut self) {
        while matches!(self.peek(), Some(b'r' | b'n' | b'N' | b'o' | b'O' | b'R' | b'V' | b' ')) {
            self.i += 1;
        }
    }
    fn skip_number(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
    }
    fn tag(&mut self) -> Option<String> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == b'=' || c == b'}' || c == b')' {
                break;
            }
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        if s.is_empty() || s == "?" {
            None
        } else {
            Some(s.to_string())
        }
    }
    fn skip_quoted(&mut self) {
        self.bump(); // opening "
        while !matches!(self.peek(), None | Some(b'"')) {
            self.i += 1;
        }
        self.bump(); // closing "
    }
    fn parse_type(&mut self) -> Ty {
        self.skip_quals();
        match self.peek() {
            Some(b'{') => self.parse_agg(b'}'),
            Some(b'(') => self.parse_agg(b')'),
            Some(b'[') => {
                // array: [N elem]
                self.bump();
                self.skip_number();
                let _ = self.parse_type();
                if self.peek() == Some(b']') {
                    self.bump();
                }
                Ty::Other
            }
            Some(b'^') => {
                self.bump();
                let _ = self.parse_type(); // consume pointee
                Ty::Ptr
            }
            Some(b'b') => {
                self.bump();
                self.skip_number();
                Ty::Other // bitfield
            }
            None => Ty::Other,
            Some(c) => {
                self.bump();
                match scalar_norm(c as char) {
                    Some(n) => Ty::Scalar(n),
                    None => Ty::Other,
                }
            }
        }
    }
    fn parse_agg(&mut self, close: u8) -> Ty {
        let openc = self.bump(); // { or (
        let name = self.tag();
        if self.peek() != Some(b'=') {
            // bodyless / opaque
            if self.peek() == Some(close) {
                self.bump();
            }
            return Ty::Other;
        }
        self.bump(); // =
        let mut fields = Vec::new();
        loop {
            self.skip_quals();
            match self.peek() {
                None => break,
                Some(c) if c == close => {
                    self.bump();
                    break;
                }
                Some(b'"') => {
                    self.skip_quoted();
                    fields.push(self.parse_type());
                }
                Some(_) => fields.push(self.parse_type()),
            }
        }
        if fields.is_empty() {
            return Ty::Other;
        }
        if openc == Some(b'{') {
            Ty::Struct { name, fields }
        } else {
            Ty::Other // union — not modelled as a value
        }
    }
}

/// Flatten a struct's leaf scalar codes (recursing nested structs). Returns
/// None if any leaf isn't a plain scalar/ptr (union/array/bitfield/unknown).
fn flatten_leaves(ty: &Ty, out: &mut Vec<char>) -> Option<()> {
    match ty {
        Ty::Scalar(c) => {
            out.push(*c);
            Some(())
        }
        Ty::Ptr => {
            out.push('@');
            Some(())
        }
        Ty::Struct { fields, .. } => {
            for f in fields {
                flatten_leaves(f, out)?;
            }
            Some(())
        }
        Ty::Other => None,
    }
}

/// Map a parsed top-level type to a kind char in the selector vocabulary.
fn type_kind(ty: &Ty) -> char {
    match ty {
        Ty::Scalar('q') => 'i',
        Ty::Scalar('Q') => 'u',
        Ty::Scalar('B') => 'B',
        Ty::Scalar('d') => 'd',
        Ty::Scalar('@') => '@',
        Ty::Scalar(':') => ':',
        Ty::Scalar('v') => 'v',
        Ty::Scalar(_) => '?',
        Ty::Ptr => '@',
        Ty::Other => '{',
        Ty::Struct { name, .. } => {
            // Geometry by NAME first (runtime encodings carry names).
            if let Some(n) = name {
                match n.as_str() {
                    "CGRect" | "NSRect" => return 'R',
                    "CGPoint" | "NSPoint" => return 'P',
                    "CGSize" | "NSSize" => return 'S',
                    "NSRange" | "_NSRange" | "CFRange" => return 'N',
                    _ => {}
                }
            }
            // Else by SHAPE: a flat homogeneous all-double struct of 2 or 4
            // leaves is Point/Size/Rect-shaped; 2 word leaves is Range-shaped.
            let mut leaves = Vec::new();
            if flatten_leaves(ty, &mut leaves).is_some() {
                let all_d = leaves.iter().all(|&c| c == 'd');
                let all_w = leaves.iter().all(|&c| matches!(c, 'q' | 'Q'));
                match (leaves.len(), all_d, all_w) {
                    (4, true, _) => return 'R',
                    (2, true, _) => return 'P', // P/S both -> FVec(2)
                    (2, _, true) => return 'N',
                    _ => {}
                }
            }
            '{'
        }
    }
}

/// Parse a full method encoding into (return kind, arg kinds). The format is
/// `<ret><framesize>@<o>:<o><arg><o>…`; we drop the offsets and the implicit
/// self(@)/`_cmd`(:) and keep the real argument kinds.
pub fn parse_method_encoding(enc: &str) -> (char, String) {
    let mut p = EncParser::new(enc);
    let ret = type_kind(&p.parse_type());
    p.skip_number(); // frame size
    let mut types = Vec::new();
    while p.peek().is_some() {
        p.skip_quals();
        if matches!(p.peek(), Some(c) if c.is_ascii_digit()) {
            p.skip_number(); // an offset
            continue;
        }
        types.push(type_kind(&p.parse_type()));
    }
    // Drop the implicit self (@) and _cmd (:) — the first two parsed types.
    let args: String = types.into_iter().skip(2).collect();
    (ret, args)
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
