//! BCPL for the BOX: the compiler and runtime as a static library that
//! rosgd's `modules/bcpl` links into `/init` (design/BOX_PORT.md).
//!
//! The C API is `box/bcpl_box.h`. The module gives hooks (the VDU, the
//! keyboard, SWIs, RISC OS files, the run's dynamic area); `bcpl_box_run`
//! compiles a program with the library modules it names and runs it under
//! MCJIT.
//!
//! Names: the program is named by its RISC OS name and read through the
//! file hook, as Latin-1 (each byte one character). `GET "x"` looks beside
//! it (`x`, `x/bcl`, then `x` without a suffix it was given), then in the
//! library (`/usr/lib/bcpl/x.bcl`, read by Rust's std::fs). A library
//! module `foo` is linked if the program, or a module linked already,
//! names `foo_...`.

use std::ffi::{c_char, c_int, CStr};
use std::path::{Path, PathBuf};
use std::time::Instant;

use newbcpl_ir::Module as IrModule;
use newbcpl_runtime::rosbox;

pub use rosbox::Hooks;

const COMPILE_ONLY: u32 = 1;
const TIMES: u32 = 2;
const DEBUG: u32 = 4;

static LIBRARY: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

fn out(s: &str) {
    rosbox::write_str(s);
}

fn library() -> PathBuf {
    LIBRARY.lock().unwrap().clone().unwrap_or_else(|| PathBuf::from("/usr/lib/bcpl"))
}

/// A file's text, each byte one character (RISC OS's Latin-1).
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

// ── Where source comes from ──────────────────────────────────────────────

/// A source file: a RISC OS name (read through the hook) or a host path
/// (the ROM's library).
#[derive(Clone)]
enum Src {
    Ros(String),
    Host(PathBuf),
}

impl Src {
    fn read(&self) -> Option<String> {
        match self {
            Src::Ros(n) => rosbox::file_load(n).map(|b| latin1(&b)),
            Src::Host(p) => std::fs::read(p).ok().map(|b| latin1(&b)),
        }
    }

    fn shown(&self) -> String {
        match self {
            Src::Ros(n) => n.clone(),
            Src::Host(p) => p.display().to_string(),
        }
    }

    /// `GET "req"` from this file.
    fn get(&self, req: &str) -> Option<Src> {
        let stem = req.rsplit_once('.').map(|(s, _)| s).unwrap_or(req);
        if let Src::Ros(n) = self {
            let dir = match n.rfind(['.', ':']) {
                Some(i) if n.as_bytes()[i] == b'.' => &n[..=i],
                Some(i) => &n[..=i],
                None => "",
            };
            let ros_req = req.replace('.', "/");
            for c in [format!("{dir}{ros_req}"), format!("{dir}{req}/bcl"), format!("{dir}{stem}/bcl"), format!("{dir}{stem}")] {
                if rosbox::file_kind(&c) == 1 {
                    return Some(Src::Ros(c));
                }
            }
        }
        let lib = library();
        for c in [lib.join(format!("{req}.bcl")), lib.join(format!("{stem}.bcl")), lib.join(req)] {
            if c.is_file() {
                return Some(Src::Host(c));
            }
        }
        None
    }
}

/// A compile error: the file, its line, the message.
struct Diag {
    file: String,
    line: usize,
    text: String,
}

fn diag(src: &Src, line: usize, text: impl Into<String>) -> Diag {
    Diag { file: src.shown(), line, text: text.into() }
}

/// Parse `src` with its GETs expanded in place.
fn parse(src: &Src, text: &str, depth: u32) -> Result<newbcpl_parser::Program, Diag> {
    if depth > 32 {
        return Err(diag(src, 0, "GET nesting is more than 32 deep (a GET that includes itself?)"));
    }
    let prog = newbcpl_parser::parse_source(text).map_err(|e| diag(src, e.span.start.line, e.message))?;
    let mut items = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        if let newbcpl_parser::Decl::Get(g) = &item {
            let Some(inc) = src.get(&g.path) else {
                return Err(diag(src, 0, format!("GET \"{}\": not found beside it or in the library", g.path)));
            };
            let Some(t) = inc.read() else {
                return Err(diag(&inc, 0, "cannot be read"));
            };
            let sub = parse(&inc, &t, depth + 1)?;
            items.extend(sub.items);
            continue;
        }
        items.push(item);
    }
    Ok(newbcpl_parser::Program { items, ..prog })
}

/// Front end: source to IR, or the errors.
fn front(src: &Src, text: &str, name: &str) -> Result<IrModule, Vec<Diag>> {
    let prog = parse(src, text, 0).map_err(|d| vec![d])?;
    let sema = newbcpl_sema::analyze(&prog);
    if !sema.errors.is_empty() {
        return Err(sema.errors.iter().map(|e| diag(src, e.span.start.line, e.message.clone())).collect());
    }
    Ok(newbcpl_ir::lower(&prog, &sema, name))
}

/// Whether `text` names `stem_...` as a word.
fn names_module(text: &str, stem: &str) -> bool {
    let pat = format!("{stem}_");
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(i) = text[from..].find(&pat) {
        let at = from + i;
        let before_ok = at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_' || b[at - 1] == b'.');
        if before_ok {
            return true;
        }
        from = at + pat.len();
    }
    false
}

/// The library modules the program needs, with their IR.
fn library_modules(prog_text: &str) -> Result<Vec<(String, IrModule)>, Vec<Diag>> {
    let mut avail: Vec<(String, PathBuf)> = std::fs::read_dir(library())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "bcl"))
                .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p.clone())))
                .collect()
        })
        .unwrap_or_default();
    avail.sort();
    let mut texts = vec![prog_text.to_string()];
    let mut chosen: Vec<(String, PathBuf, String)> = Vec::new();
    loop {
        let mut added = false;
        for (stem, p) in &avail {
            if chosen.iter().any(|c| &c.0 == stem) {
                continue;
            }
            if texts.iter().any(|t| names_module(t, stem)) {
                let t = std::fs::read(p).map(|b| latin1(&b)).unwrap_or_default();
                texts.push(t.clone());
                chosen.push((stem.clone(), p.clone(), t));
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    let mut v = Vec::new();
    for (stem, p, t) in chosen {
        v.push((stem.clone(), front(&Src::Host(p), &t, &stem)?));
    }
    Ok(v)
}

fn report(diags: &[Diag]) {
    for (n, d) in diags.iter().enumerate() {
        if n == 50 {
            out("(more errors not shown)\n");
            break;
        }
        out(&format!("{}:{}: error: {}\n", d.file, d.line, d.text));
    }
}

// ── A run ────────────────────────────────────────────────────────────────

fn compile_and_run(name: &str, flags: u32) -> i32 {
    let t0 = Instant::now();
    newbcpl_llvm::boxjit::BOX_DEBUG.store(flags & DEBUG != 0, std::sync::atomic::Ordering::Relaxed);
    let src = Src::Ros(name.to_string());
    let Some(text) = src.read() else {
        out(&format!("{name}:0: error: cannot be read\n"));
        return -1;
    };
    let leaf = name.rsplit(['.', ':']).next().unwrap_or(name).replace('/', "_");
    let prog = match front(&src, &text, &leaf) {
        Ok(p) => p,
        Err(d) => {
            report(&d);
            return -1;
        }
    };
    let mods = match library_modules(&text) {
        Ok(m) => m,
        Err(d) => {
            report(&d);
            return -1;
        }
    };
    let t_front = t0.elapsed();
    let opt = match (flags >> 4) & 7 {
        0 => 2,
        n => n - 1,
    };
    if flags & COMPILE_ONLY != 0 {
        let r = newbcpl_llvm::boxjit::compile_box(&prog, &mods, opt);
        if flags & TIMES != 0 {
            out(&format!("Compiled in {} ms (front end {} ms)\n", t0.elapsed().as_millis(), t_front.as_millis()));
        }
        return match r {
            Ok(()) => 0,
            Err(e) => {
                out(&format!("{name}:0: error: {e}\n"));
                -1
            }
        };
    }
    let compiled = || {
        if flags & TIMES != 0 {
            out(&format!("Compiled in {} ms (front end {} ms)\n", t0.elapsed().as_millis(), t_front.as_millis()));
        }
    };
    match newbcpl_llvm::boxjit::run_box(&prog, &mods, opt, &compiled) {
        Err(e) => {
            out(&format!("{name}:0: error: {e}\n"));
            -1
        }
        Ok(Ok(v)) => v as i32,
        Ok(Err(p)) => match p.downcast::<rosbox::Finish>() {
            Ok(f) => match f.why {
                None => f.code as i32,
                Some(why) => {
                    out(&format!("{why}\n"));
                    if f.code == 17 && why == "Escape" { -6 } else { -5 }
                }
            },
            Err(p) => {
                let msg = p
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| p.downcast_ref::<&str>().copied())
                    .unwrap_or("a Rust panic");
                out(&format!("BCPL: the program stopped: {msg}\n"));
                -5
            }
        },
    }
}

/// The host check: `entry` (a host path) and the library modules it names,
/// compiled by the box's front end and code generator for `triple`, as an
/// object file.
pub fn host_emit(entry: &Path, lib: &Path, triple: &str, obj: &Path) -> Result<(), String> {
    *LIBRARY.lock().unwrap() = Some(lib.to_path_buf());
    let src = Src::Host(entry.to_path_buf());
    let text = src.read().ok_or("cannot be read")?;
    let msg = |d: Vec<Diag>| d.iter().map(|d| format!("{}:{}: {}", d.file, d.line, d.text)).collect::<Vec<_>>().join("; ");
    let prog = front(&src, &text, "hostcheck").map_err(msg)?;
    let mods = library_modules(&text).map_err(msg)?;
    newbcpl_llvm::boxjit::emit_object_box(&prog, &mods, triple, obj)
}

fn reset_run() {
    rosbox::close_all();
    rosbox::clear_stop();
    newbcpl_runtime::heap::reset_run_state();
    newbcpl_runtime::builtins::reset_run_state();
    rosbox::arena_reset();
}

// ── The C API (bcpl_box.h) ───────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcpl_box_set_hooks(h: *const Hooks) {
    rosbox::set_hooks(h);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcpl_box_set_library(dir: *const c_char) {
    *LIBRARY.lock().unwrap() = if dir.is_null() {
        None
    } else {
        Some(PathBuf::from(unsafe { CStr::from_ptr(dir) }.to_string_lossy().into_owned()))
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcpl_box_run(name: *const c_char, _argc: c_int, _argv: *const *const c_char, flags: u32) -> c_int {
    reset_run();
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
    match std::panic::catch_unwind(|| compile_and_run(&name, flags)) {
        Ok(rc) => rc,
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| p.downcast_ref::<&str>().copied())
                .unwrap_or("internal error");
            out(&format!("BCPL: internal error: {msg}\n"));
            -3
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bcpl_box_stop() {
    rosbox::request_stop();
}

#[unsafe(no_mangle)]
pub extern "C" fn bcpl_box_recover() {
    rosbox::recover();
    newbcpl_llvm::boxjit::box_recover();
    reset_run();
}

#[unsafe(no_mangle)]
pub extern "C" fn bcpl_box_end_run() {
    newbcpl_llvm::boxjit::box_end_run();
    reset_run();
}
