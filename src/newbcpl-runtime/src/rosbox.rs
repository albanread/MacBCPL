//! The BOX (64-bit RISC OS over Linux; design/BOX_PORT.md): the runtime's
//! way out to RISC OS. The runtime is linked into the box's `/init`, so it
//! must never touch the process's own stdin/stdout or exit it: the module
//! that runs programs (rosgd `modules/bcpl`) gives hooks, and the
//! terminal, file and SWI paths of the runtime go through them.
//!
//! FINISH, Escape and a SWI error end the program by a Rust panic carrying
//! a [`Finish`], which unwinds through the JIT's frames to the run's
//! `catch_unwind` (box/src/lib.rs).

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;

/// The hooks, as `bcpl_box.h` declares them.
#[repr(C)]
pub struct Hooks {
    /// Bytes for the VDU (Latin-1; "\n" is a new line).
    pub write: Option<unsafe extern "C" fn(*const u8, usize)>,
    /// A line of input (no terminator) into buf: its length, or -1 at the
    /// end of the input (Escape).
    pub read: Option<unsafe extern "C" fn(*mut u8, usize) -> isize>,
    /// SWI `number` (the X form) with r[0..9] in and out; *flags gets
    /// N Z C V as bits 3-0. An error: its number in *errnum and its block's
    /// address returned; else 0.
    pub swi: Option<unsafe extern "C" fn(u32, *mut u32, *mut u32, *mut u32) -> u32>,
    /// A SWI's number from its name: 0 if known, else -1.
    pub swi_number: Option<unsafe extern "C" fn(*const c_char, *mut u32) -> c_int>,
    /// A RISC OS file's bytes (malloc'd, freed with libc free) or NULL.
    pub file_load: Option<unsafe extern "C" fn(*const c_char, *mut usize) -> *mut u8>,
    /// 0 nothing, 1 a file, 2 a directory (OS_File 17's object type).
    pub file_kind: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
    /// Grows the run's dynamic area by n bytes: the old end (a RISC OS
    /// address), or 0.
    pub arena_grow: Option<unsafe extern "C" fn(usize) -> usize>,
}

static HOOKS: AtomicPtr<Hooks> = AtomicPtr::new(std::ptr::null_mut());
static STOP: AtomicBool = AtomicBool::new(false);

pub fn set_hooks(h: *const Hooks) {
    HOOKS.store(h as *mut Hooks, Ordering::SeqCst);
    // FINISH and Escape are panics that are meant: no message for them.
    // Any other panic is reported by the run's catch_unwind.
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::panic::set_hook(Box::new(|_| {})));
}

pub fn hooks() -> Option<&'static Hooks> {
    let p = HOOKS.load(Ordering::SeqCst);
    if p.is_null() { None } else { Some(unsafe { &*p }) }
}

// ── The end of a run ─────────────────────────────────────────────────────

/// The payload of the panic that ends a program early: its exit code and,
/// for an error, the text to report.
pub struct Finish {
    pub code: i64,
    pub why: Option<String>,
}

/// Ends the program (unwinding through the JIT's frames).
pub fn finish(code: i64, why: Option<String>) -> ! {
    std::panic::panic_any(Finish { code, why })
}

/// Escape was seen: the program ends at the next hook call.
pub fn request_stop() {
    STOP.store(true, Ordering::SeqCst);
}

pub fn clear_stop() {
    STOP.store(false, Ordering::SeqCst);
}

/// Ends the program if Escape was seen.
pub fn check_stop() {
    if STOP.swap(false, Ordering::SeqCst) {
        finish(17, Some("Escape".into()));
    }
}

// ── The terminal ─────────────────────────────────────────────────────────

fn vdu(b: &[u8]) {
    if let Some(w) = hooks().and_then(|h| h.write) {
        unsafe { w(b.as_ptr(), b.len()) };
    }
}

/// Text from the compiler (UTF-8) to the VDU as Latin-1 (a character
/// above 255 is "?").
pub fn write_str(s: &str) {
    let bytes: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect();
    vdu(&bytes);
}

/// The program's output: to the selected output stream, else the VDU.
pub fn write_bytes(b: &[u8]) {
    let to_file = {
        let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
        let out = s.output;
        match s.files.get_mut(out) {
            Some(Some(f)) if out >= FIRST_FILE => {
                f.write(b);
                true
            }
            _ => false,
        }
    };
    if !to_file {
        vdu(b);
    }
    check_stop();
}

static CONSOLE_IN: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());
static CONSOLE_EOF: AtomicBool = AtomicBool::new(false);

/// A line of input from the keyboard with its "\n", or None at the end of
/// the input.
fn read_line() -> Option<Vec<u8>> {
    let r = hooks().and_then(|h| h.read)?;
    let mut buf = vec![0u8; 1024];
    let n = unsafe { r(buf.as_mut_ptr(), buf.len()) };
    check_stop();
    if n < 0 {
        return None;
    }
    buf.truncate(n as usize);
    buf.push(b'\n');
    Some(buf)
}

/// `RDCH()`: the next byte of the selected input, or -1 at its end.
pub fn rdch() -> i64 {
    {
        let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
        let inp = s.input;
        if inp >= FIRST_FILE {
            return match s.files.get_mut(inp) {
                Some(Some(f)) => f.read_byte(),
                _ => -1,
            };
        }
    }
    let mut q = CONSOLE_IN.lock().unwrap_or_else(|p| p.into_inner());
    if q.is_empty() {
        if CONSOLE_EOF.load(Ordering::SeqCst) {
            return -1;
        }
        drop(q);
        match read_line() {
            Some(l) => {
                q = CONSOLE_IN.lock().unwrap_or_else(|p| p.into_inner());
                q.extend(l);
            }
            None => {
                CONSOLE_EOF.store(true, Ordering::SeqCst);
                return -1;
            }
        }
    }
    q.pop_front().map(|b| b as i64).unwrap_or(-1)
}

// ── SWIs ─────────────────────────────────────────────────────────────────

/// Calls a SWI (always the X form). Ok(flags) or Err(error block address).
pub fn swi(number: u32, r: &mut [u32; 10]) -> Result<u32, u32> {
    let Some(f) = hooks().and_then(|h| h.swi) else { return Err(0) };
    let mut flags = 0u32;
    let mut errnum = 0u32;
    let e = unsafe { f(number, r.as_mut_ptr(), &mut flags, &mut errnum) };
    check_stop();
    if flags & 1 != 0 { Err(e) } else { Ok(flags) }
}

pub fn swi_number(name: &str) -> Option<u32> {
    let f = hooks().and_then(|h| h.swi_number)?;
    let c = std::ffi::CString::new(name).ok()?;
    let mut n = 0u32;
    (unsafe { f(c.as_ptr(), &mut n) } == 0).then_some(n)
}

pub fn file_load(name: &str) -> Option<Vec<u8>> {
    let f = hooks().and_then(|h| h.file_load)?;
    let c = std::ffi::CString::new(name).ok()?;
    let mut n = 0usize;
    let p = unsafe { f(c.as_ptr(), &mut n) };
    if p.is_null() {
        return None;
    }
    let v = unsafe { std::slice::from_raw_parts(p, n) }.to_vec();
    unsafe { libc::free(p as *mut c_void) };
    Some(v)
}

pub fn file_kind(name: &str) -> i32 {
    let Some(f) = hooks().and_then(|h| h.file_kind) else { return 0 };
    let Ok(c) = std::ffi::CString::new(name) else { return 0 };
    unsafe { f(c.as_ptr()) }
}

/// The text of a RISC OS error block at `e` (a RISC OS address).
pub fn error_text(e: u32) -> String {
    if e == 0 {
        return "no SWI handler".into();
    }
    let p = (e as usize + 4) as *const c_char;
    unsafe { CStr::from_ptr(p) }.to_bytes().iter().map(|&b| b as char).collect()
}

pub fn error_number(e: u32) -> u32 {
    if e == 0 { 0 } else { unsafe { *(e as usize as *const u32) } }
}

// ── The program's data in the arena ──────────────────────────────────────
//
// dlmalloc over the run's dynamic area: the heap, the per-function arenas,
// lists' cells, strings and the JIT's data sections get RISC OS addresses,
// so the program can hand them to SWIs. The area only grows during a run;
// `arena_reset` forgets it at the end.

struct ArenaSys;

unsafe impl dlmalloc::Allocator for ArenaSys {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        let Some(g) = hooks().and_then(|h| h.arena_grow) else { return (std::ptr::null_mut(), 0, 0) };
        let size = (size + 0xFFFF) & !0xFFFF;
        let base = unsafe { g(size) };
        if base == 0 { (std::ptr::null_mut(), 0, 0) } else { (base as *mut u8, size, 0) }
    }
    fn remap(&self, _: *mut u8, _: usize, _: usize, _: bool) -> *mut u8 {
        std::ptr::null_mut()
    }
    fn free_part(&self, _: *mut u8, _: usize, _: usize) -> bool {
        false
    }
    fn free(&self, _: *mut u8, _: usize) -> bool {
        false
    }
    fn can_release_part(&self, _: u32) -> bool {
        false
    }
    fn allocates_zeros(&self) -> bool {
        false
    }
    fn page_size(&self) -> usize {
        4096
    }
}

static ARENA: Mutex<Option<dlmalloc::Dlmalloc<ArenaSys>>> = Mutex::new(None);

/// `n` bytes aligned to `align` in the arena, or null.
pub fn arena_alloc(n: usize, align: usize) -> *mut u8 {
    let mut g = ARENA.lock().unwrap_or_else(|p| p.into_inner());
    let d = g.get_or_insert_with(|| dlmalloc::Dlmalloc::new_with_allocator(ArenaSys));
    unsafe { d.malloc(n.max(1), align.max(16)) }
}

pub fn arena_free(p: *mut u8, n: usize, align: usize) {
    if p.is_null() {
        return;
    }
    let mut g = ARENA.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(d) = g.as_mut() {
        unsafe { d.free(p, n.max(1), align.max(16)) };
    }
}

/// Whether the arena is in use (a hook to grow it is set).
pub fn arena_on() -> bool {
    hooks().and_then(|h| h.arena_grow).is_some()
}

/// The run is over and its dynamic area will be started again: forget the
/// allocator's state.
pub fn arena_reset() {
    let mut g = ARENA.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(d) = g.take() {
        std::mem::forget(d); // its memory was the area's
    }
}

/// The runtime's memory for the program (heap blocks, arena chunks, list
/// slabs, strings): zeroed, in the arena when there is one.
pub fn sys_alloc_zeroed(n: usize, align: usize) -> *mut u8 {
    if arena_on() {
        let p = arena_alloc(n, align);
        if p.is_null() {
            finish(-2, Some("BCPL: no room for the program's data".into()));
        }
        unsafe { std::ptr::write_bytes(p, 0, n) };
        return p;
    }
    let layout = std::alloc::Layout::from_size_align(n.max(1), align.max(16)).unwrap();
    unsafe { std::alloc::alloc_zeroed(layout) }
}

pub fn sys_free(p: *mut u8, n: usize, align: usize) {
    if p.is_null() {
        return;
    }
    if arena_on() {
        arena_free(p, n, align);
        return;
    }
    let layout = std::alloc::Layout::from_size_align(n.max(1), align.max(16)).unwrap();
    unsafe { std::alloc::dealloc(p, layout) };
}

// ── Streams: files through RISC OS ───────────────────────────────────────
//
// Classic BCPL streams. A stream is a small number: 1 the keyboard, 2 the
// VDU, 3 and up a RISC OS file (OS_Find, OS_GBPB through a buffer in the
// arena, which a SWI can address).

pub const CONSOLE_INPUT: usize = 1;
pub const CONSOLE_OUTPUT: usize = 2;
const FIRST_FILE: usize = 3;
const OS_ARGS: u32 = 0x09;
const OS_GBPB: u32 = 0x0C;
const OS_FIND: u32 = 0x0D;
const BUF: usize = 4096;

pub struct RosStream {
    handle: u32,
    buf: *mut u8,
    writing: bool,
    /// reading: the bytes in buf and the next one; writing: the bytes waiting
    len: usize,
    pos: usize,
    eof: bool,
}

unsafe impl Send for RosStream {}

impl RosStream {
    fn flush(&mut self) {
        if self.writing && self.len > 0 {
            let mut r = [0u32; 10];
            r[0] = 2;
            r[1] = self.handle;
            r[2] = self.buf as usize as u32;
            r[3] = self.len as u32;
            let _ = swi_quiet(OS_GBPB, &mut r);
            self.len = 0;
        }
    }

    fn write(&mut self, b: &[u8]) {
        for chunk in b.chunks(BUF) {
            if self.len + chunk.len() > BUF {
                self.flush();
            }
            unsafe { std::ptr::copy_nonoverlapping(chunk.as_ptr(), self.buf.add(self.len), chunk.len()) };
            self.len += chunk.len();
        }
    }

    fn read_byte(&mut self) -> i64 {
        if self.writing {
            return -1;
        }
        if self.pos >= self.len {
            if self.eof {
                return -1;
            }
            let mut r = [0u32; 10];
            r[0] = 4;
            r[1] = self.handle;
            r[2] = self.buf as usize as u32;
            r[3] = BUF as u32;
            if swi_quiet(OS_GBPB, &mut r).is_err() {
                self.eof = true;
                return -1;
            }
            self.len = BUF - r[3] as usize;
            self.pos = 0;
            if self.len < BUF {
                self.eof = true;
            }
            if self.len == 0 {
                return -1;
            }
        }
        let b = unsafe { *self.buf.add(self.pos) };
        self.pos += 1;
        b as i64
    }

    fn close(&mut self) {
        self.flush();
        let mut r = [0u32; 10];
        r[1] = self.handle;
        let _ = swi_quiet(OS_FIND, &mut r);
        arena_free(self.buf, BUF, 16);
    }
}

/// A SWI that does not look at Escape (closing files must not be stopped).
fn swi_quiet(number: u32, r: &mut [u32; 10]) -> Result<u32, u32> {
    let Some(f) = hooks().and_then(|h| h.swi) else { return Err(0) };
    let mut flags = 0u32;
    let mut errnum = 0u32;
    let e = unsafe { f(number, r.as_mut_ptr(), &mut flags, &mut errnum) };
    if flags & 1 != 0 { Err(e) } else { Ok(flags) }
}

struct Streams {
    /// index = stream number; 0, 1 and 2 are not files
    files: Vec<Option<RosStream>>,
    input: usize,
    output: usize,
}

static STREAMS: Mutex<Streams> = Mutex::new(Streams { files: Vec::new(), input: CONSOLE_INPUT, output: CONSOLE_OUTPUT });

/// Opens `name` (a RISC OS name): OS_Find reason &4C read, &8C create, &CC
/// update (for append, at its end). The stream's number, or 0.
pub fn open(name: &[u8], how: u8) -> i64 {
    let buf = arena_alloc(BUF, 16);
    if buf.is_null() || name.len() >= BUF {
        arena_free(buf, BUF, 16);
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
        *buf.add(name.len()) = 0;
    }
    let find = |reason: u32| -> u32 {
        let mut r = [0u32; 10];
        r[0] = reason;
        r[1] = buf as usize as u32;
        match swi_quiet(OS_FIND, &mut r) {
            Ok(_) => r[0],
            Err(_) => 0,
        }
    };
    let handle = match how {
        b'r' => find(0x4C),
        b'w' => find(0x8C),
        _ => match find(0xCC) {
            0 => find(0x8C),
            h => {
                let mut r = [0u32; 10];
                r[0] = 2;
                r[1] = h;
                if swi_quiet(OS_ARGS, &mut r).is_ok() {
                    let ext = r[2];
                    let mut r = [0u32; 10];
                    r[0] = 1;
                    r[1] = h;
                    r[2] = ext;
                    let _ = swi_quiet(OS_ARGS, &mut r);
                }
                h
            }
        },
    };
    if handle == 0 {
        arena_free(buf, BUF, 16);
        return 0;
    }
    let st = RosStream { handle, buf, writing: how != b'r', len: 0, pos: 0, eof: false };
    let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
    while s.files.len() < FIRST_FILE {
        s.files.push(None);
    }
    let slot = (FIRST_FILE..s.files.len()).find(|&i| s.files[i].is_none());
    let n = match slot {
        Some(i) => {
            s.files[i] = Some(st);
            i
        }
        None => {
            s.files.push(Some(st));
            s.files.len() - 1
        }
    };
    n as i64
}

fn valid(s: &Streams, n: i64, writing: bool) -> bool {
    match n {
        1 => !writing,
        2 => writing,
        n if n >= FIRST_FILE as i64 => {
            matches!(s.files.get(n as usize), Some(Some(f)) if f.writing == writing)
        }
        _ => false,
    }
}

pub fn select_input(n: i64) -> i64 {
    let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
    if !valid(&s, n, false) {
        return 0;
    }
    s.input = n as usize;
    n
}

pub fn select_output(n: i64) -> i64 {
    let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
    if !valid(&s, n, true) {
        return 0;
    }
    s.output = n as usize;
    n
}

pub fn input() -> i64 {
    STREAMS.lock().unwrap_or_else(|p| p.into_inner()).input as i64
}

pub fn output() -> i64 {
    STREAMS.lock().unwrap_or_else(|p| p.into_inner()).output as i64
}

/// Closes stream `n` (a file); the selection goes back to the console if
/// it was the selected one.
pub fn end_stream(n: i64) {
    let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
    if n < FIRST_FILE as i64 {
        return;
    }
    if let Some(slot) = s.files.get_mut(n as usize) {
        if let Some(mut f) = slot.take() {
            f.close();
        }
    }
    if s.input == n as usize {
        s.input = CONSOLE_INPUT;
    }
    if s.output == n as usize {
        s.output = CONSOLE_OUTPUT;
    }
}

/// Every stream closed; the console selected; the keyboard's line buffer
/// emptied (the end of a run).
pub fn close_all() {
    let mut s = STREAMS.lock().unwrap_or_else(|p| p.into_inner());
    for slot in s.files.iter_mut() {
        if let Some(mut f) = slot.take() {
            f.close();
        }
    }
    s.files.clear();
    s.input = CONSOLE_INPUT;
    s.output = CONSOLE_OUTPUT;
    drop(s);
    CONSOLE_IN.lock().unwrap_or_else(|p| p.into_inner()).clear();
    CONSOLE_EOF.store(false, Ordering::SeqCst);
}

/// After a fault: the locks may be poisoned or held by the frame that
/// faulted; start them again.
pub fn recover() {
    STREAMS.clear_poison();
    ARENA.clear_poison();
    CONSOLE_IN.clear_poison();
}
