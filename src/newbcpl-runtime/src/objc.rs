//! macOS Objective-C runtime bridge for MacBCPL.
//!
//! Ported from MacModula2's `src/newm2-runtime/src/objc.rs`, adapted to
//! BCPL's string ABI. Where MacModula2 passes `ARRAY OF CHAR` as a
//! UTF-16 open array `(ptr, high)`, BCPL strings are NUL-terminated
//! UTF-8 byte strings (`*const u8`) — matching the convention the
//! existing `iGui_*` / runtime builtins already use. So every
//! name-taking entry point here takes a `*const u8` and reads to the
//! first NUL.
//!
//! This is the substrate that makes BCPL objects *be* Objective-C
//! objects (the MacModula2 model): a BCPL `CLASS` is registered with
//! `objc_allocateClassPair` + `class_addIvar("__bcpl", size)` +
//! `class_addMethod`; `NEW Class` is `[[Class alloc] init]`; method
//! dispatch is `objc_msgSend`; fields live in the `__bcpl` ivar.
//!
//! Everything resolves through `dlsym(RTLD_DEFAULT, …)` at runtime, so
//! the JIT host gains no static link dependency on libobjc; `bootstrap`
//! `dlopen`s the umbrella frameworks first.

#![cfg(not(windows))]
#![allow(clippy::missing_safety_doc)]

use core::ffi::c_void;
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

const RTLD_DEFAULT: *mut c_void = (-2isize) as *mut c_void;
const RTLD_NOW: i32 = 0x2;

unsafe extern "C" {
    fn dlopen(path: *const i8, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
}

/// Map the umbrella frameworks into the process so libobjc, the `NS*`
/// classes, and the AppKit/Foundation/CoreGraphics exports resolve via
/// `dlsym(RTLD_DEFAULT, …)`. Idempotent; runs once.
pub fn bootstrap() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        for path in [
            "/System/Library/Frameworks/Cocoa.framework/Cocoa",
            "/System/Library/Frameworks/AppKit.framework/AppKit",
            "/System/Library/Frameworks/Foundation.framework/Foundation",
            "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics",
            "/usr/lib/libobjc.A.dylib",
        ] {
            if let Ok(c) = CString::new(path) {
                unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
            }
        }
    });
}

/// Resolve a symbol by name across everything loaded into the process.
/// After `bootstrap()`, every libSystem / libobjc / framework C entry
/// point (`objc_msgSend`, `CGColorCreate`, …) resolves here.
pub fn dlsym_default(name: &str) -> Option<*const ()> {
    bootstrap();
    let c = CString::new(name).ok()?;
    let p = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
    if p.is_null() { None } else { Some(p as *const ()) }
}

fn sym_or_null(name: &str) -> *mut c_void {
    dlsym_default(name)
        .map(|p| p as *mut c_void)
        .unwrap_or(std::ptr::null_mut())
}

/// Borrow a BCPL NUL-terminated UTF-8 string as a `&CStr` (no copy).
/// Returns `None` if the pointer is null.
///
/// # Safety
/// `ptr` must be null or point to a valid NUL-terminated C string that
/// outlives the returned borrow.
unsafe fn cstr<'a>(ptr: *const u8) -> Option<&'a CStr> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(ptr as *const i8) })
    }
}

// ─── class / selector lookup ────────────────────────────────────────

/// `objc_getClass(name)` — look up an Objective-C class by name.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_get_class(name: *const u8) -> *mut c_void {
    let Some(c) = (unsafe { cstr(name) }) else {
        return std::ptr::null_mut();
    };
    let f = sym_or_null("objc_getClass");
    if f.is_null() {
        return std::ptr::null_mut();
    }
    let f: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(f) };
    f(c.as_ptr())
}

/// `sel_registerName(name)` — intern a selector from a name.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_sel(name: *const u8) -> *mut c_void {
    let Some(c) = (unsafe { cstr(name) }) else {
        return std::ptr::null_mut();
    };
    let f = sym_or_null("sel_registerName");
    if f.is_null() {
        return std::ptr::null_mut();
    }
    let f: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(f) };
    f(c.as_ptr())
}

/// The address of `objc_msgSend`, which BCPL casts to a typed function
/// pointer per call site so each carries its own ABI signature (the
/// macOS analogue of a COM vtable slot). Returns null if unavailable.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_objc_msgsend_ptr() -> *mut c_void {
    sym_or_null("objc_msgSend")
}

/// The address of `objc_msgSendSuper` — for `SUPER.method(...)`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_objc_msgsend_super_ptr() -> *mut c_void {
    sym_or_null("objc_msgSendSuper")
}

// ─── NSString bridge ────────────────────────────────────────────────

/// Build an autoreleased `NSString*` from a BCPL UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring(text: *const u8) -> *mut c_void {
    bootstrap();
    let Some(c) = (unsafe { cstr(text) }) else {
        return std::ptr::null_mut();
    };
    let get_class = sym_or_null("objc_getClass");
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if get_class.is_null() || reg_sel.is_null() || msg_send.is_null() {
        return std::ptr::null_mut();
    }
    let get_class: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(get_class) };
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send: extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    let cls = get_class(c"NSString".as_ptr());
    let sel = reg_sel(c"stringWithUTF8String:".as_ptr());
    if cls.is_null() || sel.is_null() {
        return std::ptr::null_mut();
    }
    send(cls, sel, c.as_ptr())
}

/// Extract an `NSString*`'s text into a BCPL UTF-8 buffer (NUL
/// terminated). `dest_cap` is the buffer capacity in bytes. Returns the
/// number of bytes written (excluding the NUL).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring_to_utf8(
    nsstr: *mut c_void,
    dest: *mut u8,
    dest_cap: u64,
) -> u64 {
    if nsstr.is_null() || dest.is_null() || dest_cap == 0 {
        return 0;
    }
    let msg = sym_or_null("objc_msgSend");
    let reg = sym_or_null("sel_registerName");
    if msg.is_null() || reg.is_null() {
        return 0;
    }
    let reg: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg) };
    let send: extern "C" fn(*mut c_void, *mut c_void) -> *const i8 = unsafe { std::mem::transmute(msg) };
    let utf8 = send(nsstr, reg(c"UTF8String".as_ptr()));
    if utf8.is_null() {
        return 0;
    }
    let bytes = unsafe { CStr::from_ptr(utf8) }.to_bytes();
    let n = bytes.len().min((dest_cap as usize).saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest, n);
        *dest.add(n) = 0;
    }
    n as u64
}

/// Build an **owned (+1)** `NSString*` from a BCPL UTF-8 string via
/// `[[NSString alloc] initWithUTF8String:]`. Unlike `bcpl_objc_nsstring`
/// (which uses the `+0`/autoreleased `stringWithUTF8String:` and would
/// dangle in this pool-less JIT process), the result is retain-count +1
/// and pool-independent. This single builder serves BOTH immortal string
/// literals (codegen caches the +1 forever in a `@.nsstr.N` slot) AND
/// owned operation results (JOIN etc. transfer the +1 to the caller, who
/// releases it via `bcpl_str_release`).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring_immortal(text: *const u8) -> *mut c_void {
    bootstrap();
    let Some(c) = (unsafe { cstr(text) }) else {
        return std::ptr::null_mut();
    };
    let get_class = sym_or_null("objc_getClass");
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if get_class.is_null() || reg_sel.is_null() || msg_send.is_null() {
        return std::ptr::null_mut();
    }
    let get_class: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(get_class) };
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send0: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    let send1: extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    let cls = get_class(c"NSString".as_ptr());
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    let obj = send0(cls, reg_sel(c"alloc".as_ptr()));
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    send1(obj, reg_sel(c"initWithUTF8String:".as_ptr()), c.as_ptr())
}

// ─── NSString byte access (`s % i`, LEN, WRITES) ────────────────────
//
// A BCPL `String` value is an NSString `id`. Byte-level access (`s % i`,
// `LEN s`) and the WRITES/WRITEF text sinks go through `-UTF8String`,
// which returns a buffer of undefined lifetime under a (here, absent)
// autorelease pool — so we COPY the bytes synchronously into runtime
// memory while the source string is provably alive (the caller holds it
// for the duration of the call). No raw `-UTF8String` pointer ever
// leaves a single runtime call frame, so nothing can dangle.

/// Copy an `NSString*`'s UTF-8 bytes into an owned `Vec`. Synchronous;
/// the source must be live for the call. `None` on nil / failure.
pub(crate) unsafe fn nsstring_utf8_bytes(nsstr: *mut c_void) -> Option<Vec<u8>> {
    if nsstr.is_null() {
        return None;
    }
    let msg = sym_or_null("objc_msgSend");
    let reg = sym_or_null("sel_registerName");
    if msg.is_null() || reg.is_null() {
        return None;
    }
    let reg: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg) };
    let send: extern "C" fn(*mut c_void, *mut c_void) -> *const i8 = unsafe { std::mem::transmute(msg) };
    let utf8 = send(nsstr, reg(c"UTF8String".as_ptr()));
    if utf8.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(utf8) }.to_bytes().to_vec())
}

/// Run a shell command via `/bin/sh -c` and return its combined
/// stdout+stderr as an autoreleased `NSString` id (a BCPL `String`).
///
/// This is the BCPL IDE's Run primitive: the IDE shells out to
/// `newbcpl-driver run <tempfile>` so a crash in the user's program
/// kills the SUBPROCESS, not the IDE (matching the MacModula2 IDE's
/// out-of-process build/run model). A non-zero exit or a kill is
/// appended as a `[exit N]` / `[killed]` footer. `cmd` is an NSString
/// id; the result is never null (errors come back as a diagnostic
/// string), so the caller can drop it straight into `[view setString:]`.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_run_capture(cmd: *mut c_void) -> *mut c_void {
    let cmd_str = match unsafe { nsstring_utf8_bytes(cmd) } {
        Some(b) => String::from_utf8_lossy(&b).into_owned(),
        None => return unsafe { nsstring_from_rust("[ide] run: empty command") },
    };
    let text = match std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(&cmd_str)
        .output()
    {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            if !o.stderr.is_empty() {
                s.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            match o.status.code() {
                Some(0) => {}
                Some(c) => s.push_str(&format!("\n[exit {c}]")),
                None => s.push_str("\n[killed]"),
            }
            s
        }
        Err(e) => format!("[ide] run failed: {e}"),
    };
    unsafe { nsstring_from_rust(&text) }
}

/// Apply a foreground colour to a character range of an `NSTextStorage`
/// (or any `NSMutableAttributedString`): `[ts addAttribute:
/// NSForegroundColorAttributeName value:[NSColor colorWithRed:…] range:
/// {loc,len}]`. The `NSColor` and the `NSRange` are built HERE so BCPL
/// never has to pass a by-value struct arg — it just calls with plain
/// ints + floats. Used by the IDE's syntax colouriser. No-op on null.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_set_text_color(
    ts: *mut c_void,
    loc: i64,
    len: i64,
    r: f64,
    g: f64,
    b: f64,
) {
    if ts.is_null() {
        return;
    }
    let msg = sym_or_null("objc_msgSend");
    let getc = sym_or_null("objc_getClass");
    let reg = sym_or_null("sel_registerName");
    if msg.is_null() || getc.is_null() || reg.is_null() {
        return;
    }
    let getc: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(getc) };
    let reg: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg) };
    let nscolor = getc(c"NSColor".as_ptr());
    if nscolor.is_null() {
        return;
    }
    // color = [NSColor colorWithRed:r green:g blue:b alpha:1.0]  (doubles → d0..d3)
    let msg_color: extern "C" fn(*mut c_void, *mut c_void, f64, f64, f64, f64) -> *mut c_void =
        unsafe { std::mem::transmute(msg) };
    let color = msg_color(
        nscolor,
        reg(c"colorWithRed:green:blue:alpha:".as_ptr()),
        r,
        g,
        b,
        1.0,
    );
    let key = unsafe { nsstring_from_rust("NSColor") }; // = NSForegroundColorAttributeName
    // [ts addAttribute:key value:color range:NSMakeRange(loc,len)]
    // NSRange is two NSUInteger → two GP arg registers (x4,x5).
    let msg_attr: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void, u64, u64) =
        unsafe { std::mem::transmute(msg) };
    msg_attr(
        ts,
        reg(c"addAttribute:value:range:".as_ptr()),
        key,
        color,
        loc as u64,
        len as u64,
    );
}

/// Is the word `src[loc .. loc+len)` a BCPL keyword? (For the IDE's
/// syntax colouriser — the keyword set mirrors the lexer.) `loc`/`len` are
/// code-point indices; BCPL source is ASCII so they index UTF-8 bytes
/// directly. Returns 1 / 0.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_is_keyword(src: *mut c_void, loc: i64, len: i64) -> i64 {
    if src.is_null() || len <= 0 {
        return 0;
    }
    let Some(bytes) = (unsafe { nsstring_utf8_bytes(src) }) else {
        return 0;
    };
    let (loc, len) = (loc as usize, len as usize);
    if loc.saturating_add(len) > bytes.len() {
        return 0;
    }
    let word = &bytes[loc..loc + len];
    const KW: &[&[u8]] = &[
        b"AND", b"BAND", b"BE", b"BNOT", b"BOR", b"BREAK", b"BXOR", b"BY", b"CASE",
        b"CLASS", b"DECL", b"DEFAULT", b"DO", b"ELSE", b"ENDCASE", b"ENTIER", b"EQV",
        b"EXTENDS", b"FALSE", b"FINAL", b"FINISH", b"FIX", b"FLET", b"FLOAT", b"FOR",
        b"FOREACH", b"FREELIST", b"FREEVEC", b"FSQRT", b"FSTATIC", b"FTABLE", b"FUNCTION",
        b"FVALOF", b"FVEC", b"GET", b"GLOBAL", b"GLOBALS", b"GOTO", b"IF", b"IN", b"INTO",
        b"LET", b"LIST", b"LOOP", b"MANAGED", b"MANIFEST", b"NEQV", b"NEW", b"NOT", b"OF",
        b"OR", b"PRIVATE", b"PROTECTED", b"PUBLIC", b"REM", b"REPEAT", b"REPEATUNTIL",
        b"REPEATWHILE", b"RESULTIS", b"RETAIN", b"RETURN", b"ROUTINE", b"SELF", b"STATIC",
        b"SUPER", b"SWITCHON", b"TABLE", b"TEST", b"THEN", b"TO", b"TRUE", b"TRUNC",
        b"UNLESS", b"UNTIL", b"USING", b"VALOF", b"VEC", b"VIRTUAL", b"WHILE", b"XOR",
    ];
    if KW.iter().any(|k| *k == word) {
        1
    } else {
        0
    }
}

/// Intern an Obj-C selector from an NSString name (a BCPL `String`) and
/// return it as a `SEL`. Lets BCPL wire menu items / targets to STANDARD
/// Cocoa actions (`terminate:`, `cut:`, `selectAll:`, …) whose selectors
/// aren't otherwise nameable from source (a bare `[recv sel]` only
/// produces a *send*, not a reified selector value). Null on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_selector(name: *mut c_void) -> *mut c_void {
    let Some(bytes) = (unsafe { nsstring_utf8_bytes(name) }) else {
        return std::ptr::null_mut();
    };
    let Ok(c) = std::ffi::CString::new(bytes) else {
        return std::ptr::null_mut();
    };
    let reg = sym_or_null("sel_registerName");
    if reg.is_null() {
        return std::ptr::null_mut();
    }
    let reg: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg) };
    reg(c.as_ptr())
}

/// Build an autoreleased `NSString` id from a Rust `&str` (NUL bytes
/// stripped, since a C string can't carry them). Thin wrapper over
/// `bcpl_objc_nsstring` (`+[NSString stringWithUTF8String:]`).
unsafe fn nsstring_from_rust(s: &str) -> *mut c_void {
    let cleaned = if s.as_bytes().contains(&0) {
        s.replace('\0', "")
    } else {
        s.to_owned()
    };
    match std::ffi::CString::new(cleaned) {
        Ok(c) => unsafe { bcpl_objc_nsstring(c.as_ptr() as *const u8) },
        Err(_) => std::ptr::null_mut(),
    }
}

thread_local! {
    // One-entry memo of the string's Unicode SCALAR VALUES (code points),
    // so `FOR i ... s % i` is O(n) per string, not O(n^2) of selector
    // dispatches. `s % i` returns the i-th CODE POINT and LEN(s) the
    // code-point count (NOT UTF-8 bytes or UTF-16 units), since a BCPL
    // String is now a Cocoa NSString. Keyed by the id value. Tagged-pointer
    // NSStrings encode their content in the pointer bits, so identical keys
    // ALWAYS mean identical content (no bleed). Heap NSStrings can be
    // reissued at a freed address — `bcpl_str_release` evicts this memo on
    // every owned-string release so a reused address never serves stale
    // data.
    static STR_MEMO: std::cell::RefCell<(usize, Vec<u32>)> =
        const { std::cell::RefCell::new((0usize, Vec::new())) };
}

#[inline]
fn str_memo_with<R>(nsstr: *mut c_void, f: impl FnOnce(&[u32]) -> R, dflt: R) -> R {
    if nsstr.is_null() {
        return dflt;
    }
    let key = nsstr as usize;
    STR_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.0 != key {
            match unsafe { nsstring_utf8_bytes(nsstr) } {
                Some(b) => {
                    // Decode UTF-8 to Unicode scalar values (Rust `char`
                    // IS a code point). `-UTF8String` is well-formed, so
                    // from_utf8_lossy is a no-op fast path here.
                    let cps: Vec<u32> = String::from_utf8_lossy(&b)
                        .chars()
                        .map(|c| c as u32)
                        .collect();
                    *m = (key, cps);
                }
                None => return dflt,
            }
        }
        f(&m.1)
    })
}

/// `LEN(s)` for a String: the **code-point** count (so the index domain
/// agrees with `s % i`). nil => 0.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_len(nsstr: *mut c_void) -> i64 {
    str_memo_with(nsstr, |c| c.len() as i64, 0)
}

/// `s % i` for a String: the i-th Unicode **code point** (scalar value).
/// Returns 0 for nil / out-of-range / negative index (BCPL's tolerant
/// read convention).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_char(nsstr: *mut c_void, idx: i64) -> i64 {
    if idx < 0 {
        return 0;
    }
    let i = idx as usize;
    str_memo_with(nsstr, |c| c.get(i).map(|&cp| cp as i64).unwrap_or(0), 0)
}

/// Release an owned String: EVICT the code-point memo for this id first
/// (so a later string reissued at the same address can't read stale data),
/// then send `release`. Used by the owned-string epilogue / USING / strong
/// store paths. Safe (and harmless) on any object id.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_release(obj: *mut c_void) {
    if obj.is_null() {
        return;
    }
    let key = obj as usize;
    STR_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.0 == key {
            *m = (0usize, Vec::new());
        }
    });
    unsafe { bcpl_objc_release(obj) };
}

// ─── object lifecycle ───────────────────────────────────────────────

/// `[[Class alloc] init]` — allocate and initialise an instance of a
/// (already-registered) class looked up by name. This is the lowering
/// target for `NEW Class`.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_new(name: *const u8) -> *mut c_void {
    bootstrap();
    let cls = unsafe { bcpl_objc_get_class(name) };
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { bcpl_objc_alloc_init(cls) }
}

/// `[[cls alloc] init]` on an already-resolved Class object.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_alloc_init(cls: *mut c_void) -> *mut c_void {
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if reg_sel.is_null() || msg_send.is_null() {
        return std::ptr::null_mut();
    }
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send0: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    let obj = send0(cls, reg_sel(c"alloc".as_ptr()));
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    send0(obj, reg_sel(c"init".as_ptr()))
}

/// Could `p` be a real heap Obj-C object id? A heap object is at least
/// 8-byte aligned and well above the zero page. This REJECTS: small
/// integers (a BCPL Word mistakenly stored into a managed slot —
/// `s := 42`), misaligned garbage, AND tagged-pointer NSStrings (whose
/// `retain`/`release` are no-ops anyway). It is the guard that keeps the
/// typeless BCPL ABI from turning a stray non-object word into an
/// `objc_msgSend` on a bogus pointer (SIGSEGV). Conservative: a non-objc
/// heap pointer (e.g. a raw VEC) still passes, but that only arises from
/// genuine type confusion the language can't prevent.
#[inline]
fn is_objc_pointer(p: *mut c_void) -> bool {
    let a = p as usize;
    a >= 0x1000 && (a & 0x7) == 0
}

/// Send `release` to an object (BCPL `RELEASE` / end of `USING`).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_release(obj: *mut c_void) {
    if !is_objc_pointer(obj) {
        return;
    }
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if reg_sel.is_null() || msg_send.is_null() {
        return;
    }
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send0: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    send0(obj, reg_sel(c"release".as_ptr()));
}

/// Send `retain` to an object (BCPL `RETAIN`).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_retain(obj: *mut c_void) -> *mut c_void {
    if !is_objc_pointer(obj) {
        // Not a real object (small int / misaligned / tagged) — pass the
        // word through unchanged; retaining it would crash or be a no-op.
        return obj;
    }
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if reg_sel.is_null() || msg_send.is_null() {
        return std::ptr::null_mut();
    }
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send0: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(msg_send) };
    send0(obj, reg_sel(c"retain".as_ptr()))
}

// ─── runtime class definition ───────────────────────────────────────

/// `objc_allocateClassPair(super, name, 0)` — begin defining a new
/// Objective-C class. Add ivars/methods, then `register_class`.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_allocate_class(
    superclass: *mut c_void,
    name: *const u8,
) -> *mut c_void {
    bootstrap();
    let Some(c) = (unsafe { cstr(name) }) else {
        return std::ptr::null_mut();
    };
    let f = sym_or_null("objc_allocateClassPair");
    if f.is_null() {
        return std::ptr::null_mut();
    }
    let f: extern "C" fn(*mut c_void, *const i8, usize) -> *mut c_void =
        unsafe { std::mem::transmute(f) };
    f(superclass, c.as_ptr(), 0)
}

/// `class_addIvar(cls, name, size, alignment_log2, types)` — add an
/// instance variable to a class still being defined. BCPL classes use a
/// single ivar `__bcpl` holding the whole field block. Returns 1 on
/// success.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_add_ivar(
    cls: *mut c_void,
    name: *const u8,
    size: u64,
    alignment_log2: u8,
    types: *const u8,
) -> i32 {
    let (Some(n), Some(t)) = (unsafe { cstr(name) }, unsafe { cstr(types) }) else {
        return 0;
    };
    let f = sym_or_null("class_addIvar");
    if f.is_null() {
        return 0;
    }
    let f: extern "C" fn(*mut c_void, *const i8, usize, u8, *const i8) -> i8 =
        unsafe { std::mem::transmute(f) };
    f(cls, n.as_ptr(), size as usize, alignment_log2, t.as_ptr()) as i32
}

/// `class_addMethod(cls, sel, imp, types)` — install a method whose
/// implementation is `imp` (a plain C-ABI function — a JIT'd BCPL
/// routine works directly, since an Obj-C IMP is
/// `ret (*)(id self, SEL _cmd, …)`). `types` is the Obj-C type encoding,
/// e.g. "v@:@" for `-(void)act:(id)x`. Returns 1 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_add_method(
    cls: *mut c_void,
    sel: *mut c_void,
    imp: *mut c_void,
    types: *const u8,
) -> i32 {
    let Some(c) = (unsafe { cstr(types) }) else {
        return 0;
    };
    let f = sym_or_null("class_addMethod");
    if f.is_null() {
        return 0;
    }
    let f: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i8) -> i8 =
        unsafe { std::mem::transmute(f) };
    f(cls, sel, imp, c.as_ptr()) as i32
}

/// `objc_registerClassPair(cls)` — finalize a class begun with
/// `allocate_class`. After this, instances can be allocated and no more
/// ivars/methods may be added.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_register_class(cls: *mut c_void) {
    let f = sym_or_null("objc_registerClassPair");
    if f.is_null() {
        return;
    }
    let f: extern "C" fn(*mut c_void) = unsafe { std::mem::transmute(f) };
    f(cls);
}

/// Base pointer of a BCPL object's field block: the address of its
/// `__bcpl` ivar. Returns `obj` unchanged if the ivar isn't found.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_field_base(obj: *mut c_void) -> *mut c_void {
    if obj.is_null() {
        return obj;
    }
    let obj_get_class = sym_or_null("object_getClass");
    let class_get_ivar = sym_or_null("class_getInstanceVariable");
    let ivar_get_offset = sym_or_null("ivar_getOffset");
    if obj_get_class.is_null() || class_get_ivar.is_null() || ivar_get_offset.is_null() {
        return obj;
    }
    let obj_get_class: extern "C" fn(*mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(obj_get_class) };
    let class_get_ivar: extern "C" fn(*mut c_void, *const i8) -> *mut c_void =
        unsafe { std::mem::transmute(class_get_ivar) };
    let ivar_get_offset: extern "C" fn(*mut c_void) -> isize =
        unsafe { std::mem::transmute(ivar_get_offset) };
    let cls = obj_get_class(obj);
    let ivar = class_get_ivar(cls, c"__bcpl".as_ptr());
    if ivar.is_null() {
        return obj;
    }
    let off = ivar_get_offset(ivar);
    unsafe { (obj as *mut u8).offset(off) as *mut c_void }
}

/// Base pointer of the field block belonging to a SPECIFIC class in the
/// receiver's inheritance chain, identified by that class's unique ivar
/// name (e.g. "__bcpl_Base"). This is what makes per-class field
/// composition correct: a method defined on `Base`, running on a `Sub`
/// instance, must read `Base`'s fields from `Base`'s ivar — NOT the
/// most-derived ivar that `bcpl_objc_field_base` would find.
///
/// `class_getInstanceVariable(object_getClass(obj), name)` searches up
/// the superclass chain, and because each BCPL class's ivar name is
/// unique, it resolves unambiguously to the intended class's block.
/// Returns `obj` unchanged if the ivar isn't found.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_field_base_for(
    obj: *mut c_void,
    ivar_name: *const u8,
) -> *mut c_void {
    if obj.is_null() {
        return obj;
    }
    let Some(name) = (unsafe { cstr(ivar_name) }) else {
        return obj;
    };
    let obj_get_class = sym_or_null("object_getClass");
    let class_get_ivar = sym_or_null("class_getInstanceVariable");
    let ivar_get_offset = sym_or_null("ivar_getOffset");
    if obj_get_class.is_null() || class_get_ivar.is_null() || ivar_get_offset.is_null() {
        return obj;
    }
    let obj_get_class: extern "C" fn(*mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(obj_get_class) };
    let class_get_ivar: extern "C" fn(*mut c_void, *const i8) -> *mut c_void =
        unsafe { std::mem::transmute(class_get_ivar) };
    let ivar_get_offset: extern "C" fn(*mut c_void) -> isize =
        unsafe { std::mem::transmute(ivar_get_offset) };
    let cls = obj_get_class(obj);
    let ivar = class_get_ivar(cls, name.as_ptr());
    if ivar.is_null() {
        return obj;
    }
    let off = ivar_get_offset(ivar);
    unsafe { (obj as *mut u8).offset(off) as *mut c_void }
}

/// `[obj isKindOfClass: objc_getClass(name)]` — runtime type test.
/// Returns 1 (true) / 0 (false).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_is_kind_of(obj: *mut c_void, name: *const u8) -> i64 {
    if obj.is_null() {
        return 0;
    }
    let cls = unsafe { bcpl_objc_get_class(name) };
    if cls.is_null() {
        return 0;
    }
    let reg_sel = sym_or_null("sel_registerName");
    let msg_send = sym_or_null("objc_msgSend");
    if reg_sel.is_null() || msg_send.is_null() {
        return 0;
    }
    let reg_sel: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg_sel) };
    let send: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool =
        unsafe { std::mem::transmute(msg_send) };
    if send(obj, reg_sel(c"isKindOfClass:".as_ptr()), cls) {
        1
    } else {
        0
    }
}

// ─── Objective-C blocks ─────────────────────────────────────────────

#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
}

#[repr(C)]
struct BlockLiteral {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: *const c_void,
    descriptor: *const BlockDescriptor,
}

const BLOCK_IS_GLOBAL: i32 = 1 << 28;

/// Wrap a plain C-ABI function as a capture-free *global* Objective-C
/// block, so a BCPL routine can be passed to any Cocoa API taking a
/// block. `invoke` must have the block invoke ABI —
/// `ret invoke(void *block, <args…>)` — i.e. its first parameter is the
/// block itself (usually ignored). The block lives for the program's
/// lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_make_block(invoke: *mut c_void) -> *mut c_void {
    bootstrap();
    let isa = sym_or_null("_NSConcreteGlobalBlock");
    if isa.is_null() || invoke.is_null() {
        return std::ptr::null_mut();
    }
    let descriptor = Box::into_raw(Box::new(BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<BlockLiteral>(),
    }));
    let literal = Box::into_raw(Box::new(BlockLiteral {
        isa: isa as *const c_void,
        flags: BLOCK_IS_GLOBAL,
        reserved: 0,
        invoke: invoke as *const c_void,
        descriptor,
    }));
    literal as *mut c_void
}

// ─── headless snapshot (the native way to *see* the UI) ──────────────

/// An `NSRect` / `CGRect` — four CGFloat (f64), passed in v0–v3 on arm64.
#[repr(C)]
#[derive(Clone, Copy)]
struct NsRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Render an `NSView` (and its subviews) offscreen into a bitmap and
/// write it as a PNG at `path` (a BCPL UTF-8 string). Works without a
/// window server (`cacheDisplayInRect:` draws into a CGBitmapContext),
/// so a Cocoa UI can be captured headlessly. Returns nonzero on success.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_cocoa_snapshot_view(
    view: *mut c_void,
    path: *const u8,
) -> i32 {
    bootstrap();
    if view.is_null() {
        return 0;
    }
    let msg = sym_or_null("objc_msgSend");
    let reg = sym_or_null("sel_registerName");
    if msg.is_null() || reg.is_null() {
        return 0;
    }
    let sel = |s: &CStr| -> *mut c_void {
        let f: extern "C" fn(*const i8) -> *mut c_void = unsafe { std::mem::transmute(reg) };
        f(s.as_ptr())
    };

    let send_rect_ret: extern "C" fn(*mut c_void, *mut c_void) -> NsRect =
        unsafe { std::mem::transmute(msg) };
    let bounds = send_rect_ret(view, sel(c"bounds"));
    if bounds.w < 1.0 || bounds.h < 1.0 {
        return 0;
    }

    let send_rect_arg: extern "C" fn(*mut c_void, *mut c_void, NsRect) -> *mut c_void =
        unsafe { std::mem::transmute(msg) };
    let rep = send_rect_arg(view, sel(c"bitmapImageRepForCachingDisplayInRect:"), bounds);
    if rep.is_null() {
        return 0;
    }

    let send_rect_rep: extern "C" fn(*mut c_void, *mut c_void, NsRect, *mut c_void) =
        unsafe { std::mem::transmute(msg) };
    send_rect_rep(view, sel(c"cacheDisplayInRect:toBitmapImageRep:"), bounds, rep);

    let send_png: extern "C" fn(*mut c_void, *mut c_void, u64, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(msg) };
    let data = send_png(rep, sel(c"representationUsingType:properties:"), 4, std::ptr::null_mut());
    if data.is_null() {
        return 0;
    }

    let path_str = unsafe { bcpl_objc_nsstring(path) };
    if path_str.is_null() {
        return 0;
    }
    let send_write: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, bool) -> bool =
        unsafe { std::mem::transmute(msg) };
    let ok = send_write(data, sel(c"writeToFile:atomically:"), path_str, false);
    if ok { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_class_finds_nsobject() {
        let cls = unsafe { bcpl_objc_get_class(c"NSObject".as_ptr() as *const u8) };
        assert!(!cls.is_null(), "objc_getClass(NSObject) should resolve");
    }

    #[test]
    fn nsstring_roundtrips() {
        let ns = unsafe { bcpl_objc_nsstring(c"hello, cocoa".as_ptr() as *const u8) };
        assert!(!ns.is_null(), "NSString creation should succeed");
        let mut buf = [0u8; 64];
        let n = unsafe { bcpl_objc_nsstring_to_utf8(ns, buf.as_mut_ptr(), buf.len() as u64) };
        let got = std::str::from_utf8(&buf[..n as usize]).unwrap();
        assert_eq!(got, "hello, cocoa");
    }

    #[test]
    fn define_instantiate_class() {
        // Dynamically build a class — the exact mechanism BCPL CLASS
        // lowering will use: allocateClassPair + addIvar + register +
        // alloc/init + field_base.
        let sup = unsafe { bcpl_objc_get_class(c"NSObject".as_ptr() as *const u8) };
        assert!(!sup.is_null());
        let cls =
            unsafe { bcpl_objc_allocate_class(sup, c"BcplTestPoint".as_ptr() as *const u8) };
        assert!(!cls.is_null(), "allocateClassPair should succeed");
        // one ivar "__bcpl" holding a 24-byte field block, 8-byte aligned.
        let ok = unsafe {
            bcpl_objc_add_ivar(
                cls,
                c"__bcpl".as_ptr() as *const u8,
                24,
                3,
                c"[24c]".as_ptr() as *const u8,
            )
        };
        assert_eq!(ok, 1, "class_addIvar(__bcpl) should succeed");
        unsafe { bcpl_objc_register_class(cls) };

        let obj = unsafe { bcpl_objc_alloc_init(cls) };
        assert!(!obj.is_null(), "[[cls alloc] init] should succeed");

        // field_base must land inside the object and be writable.
        let base = unsafe { bcpl_objc_field_base(obj) } as *mut i64;
        assert!(!base.is_null());
        unsafe {
            *base = 0x1234_5678;
            assert_eq!(*base, 0x1234_5678, "field block must be writable");
        }
        assert_eq!(
            unsafe { bcpl_objc_is_kind_of(obj, c"NSObject".as_ptr() as *const u8) },
            1,
            "instance should be kind-of NSObject"
        );
        unsafe { bcpl_objc_release(obj) };
    }

    // THE LINCHPIN: per-class unique-named ivar composition. Base and
    // Sub (EXTENDS Base) each add their OWN field block under a unique
    // ivar name; the Obj-C runtime must compose them so a Sub instance
    // has BOTH blocks at distinct, non-overlapping, writable offsets,
    // and bcpl_objc_field_base_for must resolve each by name. If this
    // fails the whole per-class-ivar retarget scheme must change.
    #[test]
    fn per_class_ivar_composition() {
        let nsobject = unsafe { bcpl_objc_get_class(c"NSObject".as_ptr() as *const u8) };
        assert!(!nsobject.is_null());

        // Base: own field block of 16 bytes under "__bcpl_LBase".
        let base = unsafe { bcpl_objc_allocate_class(nsobject, c"BCPL_LBase".as_ptr() as *const u8) };
        assert!(!base.is_null(), "allocate BCPL_LBase");
        assert_eq!(
            unsafe {
                bcpl_objc_add_ivar(
                    base,
                    c"__bcpl_LBase".as_ptr() as *const u8,
                    16,
                    3,
                    c"[16c]".as_ptr() as *const u8,
                )
            },
            1,
            "addIvar __bcpl_LBase"
        );
        unsafe { bcpl_objc_register_class(base) };

        // Sub EXTENDS Base: own field block of 16 bytes under "__bcpl_LSub".
        let sub = unsafe { bcpl_objc_allocate_class(base, c"BCPL_LSub".as_ptr() as *const u8) };
        assert!(!sub.is_null(), "allocate BCPL_LSub (super=BCPL_LBase)");
        assert_eq!(
            unsafe {
                bcpl_objc_add_ivar(
                    sub,
                    c"__bcpl_LSub".as_ptr() as *const u8,
                    16,
                    3,
                    c"[16c]".as_ptr() as *const u8,
                )
            },
            1,
            "addIvar __bcpl_LSub"
        );
        unsafe { bcpl_objc_register_class(sub) };

        // Instantiate Sub; resolve both blocks by their unique names.
        let obj = unsafe { bcpl_objc_alloc_init(sub) };
        assert!(!obj.is_null(), "[[Sub alloc] init]");
        let base_blk =
            unsafe { bcpl_objc_field_base_for(obj, c"__bcpl_LBase".as_ptr() as *const u8) } as *mut u8;
        let sub_blk =
            unsafe { bcpl_objc_field_base_for(obj, c"__bcpl_LSub".as_ptr() as *const u8) } as *mut u8;
        let obj_u8 = obj as *mut u8;
        assert!(base_blk != obj_u8, "Base block must resolve to its ivar, not obj");
        assert!(sub_blk != obj_u8, "Sub block must resolve to its ivar, not obj");
        assert_ne!(base_blk, sub_blk, "Base and Sub blocks must be DISTINCT");

        // Non-overlapping: the two 16-byte blocks must not overlap.
        let (lo, hi) = if base_blk < sub_blk { (base_blk, sub_blk) } else { (sub_blk, base_blk) };
        let gap = (hi as usize) - (lo as usize);
        assert!(gap >= 16, "blocks overlap: gap {gap} < 16 bytes");

        // Both writable and independent.
        unsafe {
            *(base_blk as *mut i64) = 0x1111;
            *(base_blk.add(8) as *mut i64) = 0x2222;
            *(sub_blk as *mut i64) = 0x3333;
            *(sub_blk.add(8) as *mut i64) = 0x4444;
            assert_eq!(*(base_blk as *mut i64), 0x1111);
            assert_eq!(*(base_blk.add(8) as *mut i64), 0x2222);
            assert_eq!(*(sub_blk as *mut i64), 0x3333, "Sub write must not clobber Base");
            assert_eq!(*(sub_blk.add(8) as *mut i64), 0x4444);
        }
        unsafe { bcpl_objc_release(obj) };
    }
}

/// Table of `(symbol_name, address)` for every bridge entry point, so
/// the JIT symbol resolver can bind them the same way as the other
/// runtime builtins.
pub fn builtin_addresses() -> Vec<(&'static str, usize)> {
    vec![
        ("bcpl_objc_get_class", bcpl_objc_get_class as *const () as usize),
        ("bcpl_objc_sel", bcpl_objc_sel as *const () as usize),
        ("bcpl_objc_msgsend_ptr", bcpl_objc_msgsend_ptr as *const () as usize),
        (
            "bcpl_objc_msgsend_super_ptr",
            bcpl_objc_msgsend_super_ptr as *const () as usize,
        ),
        ("bcpl_objc_nsstring", bcpl_objc_nsstring as *const () as usize),
        (
            "bcpl_objc_nsstring_to_utf8",
            bcpl_objc_nsstring_to_utf8 as *const () as usize,
        ),
        (
            "bcpl_objc_nsstring_immortal",
            bcpl_objc_nsstring_immortal as *const () as usize,
        ),
        ("bcpl_str_len", bcpl_str_len as *const () as usize),
        ("bcpl_str_char", bcpl_str_char as *const () as usize),
        ("bcpl_str_release", bcpl_str_release as *const () as usize),
        ("bcpl_run_capture", bcpl_run_capture as *const () as usize),
        ("bcpl_selector", bcpl_selector as *const () as usize),
        ("bcpl_set_text_color", bcpl_set_text_color as *const () as usize),
        ("bcpl_is_keyword", bcpl_is_keyword as *const () as usize),
        ("bcpl_objc_new", bcpl_objc_new as *const () as usize),
        ("bcpl_objc_alloc_init", bcpl_objc_alloc_init as *const () as usize),
        ("bcpl_objc_release", bcpl_objc_release as *const () as usize),
        ("bcpl_objc_retain", bcpl_objc_retain as *const () as usize),
        (
            "bcpl_objc_allocate_class",
            bcpl_objc_allocate_class as *const () as usize,
        ),
        ("bcpl_objc_add_ivar", bcpl_objc_add_ivar as *const () as usize),
        ("bcpl_objc_add_method", bcpl_objc_add_method as *const () as usize),
        (
            "bcpl_objc_register_class",
            bcpl_objc_register_class as *const () as usize,
        ),
        ("bcpl_objc_field_base", bcpl_objc_field_base as *const () as usize),
        (
            "bcpl_objc_field_base_for",
            bcpl_objc_field_base_for as *const () as usize,
        ),
        ("bcpl_objc_is_kind_of", bcpl_objc_is_kind_of as *const () as usize),
        ("bcpl_objc_make_block", bcpl_objc_make_block as *const () as usize),
        (
            "bcpl_cocoa_snapshot_view",
            bcpl_cocoa_snapshot_view as *const () as usize,
        ),
    ]
}
