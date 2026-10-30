//! The BOX's stand-in for objc.rs (design/BOX_PORT.md, sections 4 and 8):
//! the same entry points, with no Objective-C behind them.
//!
//! **Strings.** A BCPL string is a pointer to its bytes, zero-terminated,
//! with a 16-byte header just before them; it is made in the run's dynamic
//! area, so the string is itself the RISC OS address of a zero-terminated
//! string and can be given to a SWI. The bytes are Latin-1 (RISC OS's
//! alphabet): `LEN(s)` counts them and `s % i` is the i-th. Literals are
//! made once (the compiler caches them) and never released; strings that
//! `JOIN` makes are owned and freed when their reference count falls to 0.
//!
//! **Objects.** There is no Objective-C runtime in the box: `NEW` of a
//! class ends the program with a message.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counters shown by `HEAP_INFO` (as objc.rs's).
pub struct CocoaCounters {
    pub objects_new: AtomicU64,
    pub strings_built: AtomicU64,
    pub retains: AtomicU64,
    pub releases: AtomicU64,
    pub autorelease_pools: AtomicU64,
}

pub static COCOA_COUNTERS: CocoaCounters = CocoaCounters {
    objects_new: AtomicU64::new(0),
    strings_built: AtomicU64::new(0),
    retains: AtomicU64::new(0),
    releases: AtomicU64::new(0),
    autorelease_pools: AtomicU64::new(0),
};

pub fn autorelease_pool_push() -> *mut c_void {
    std::ptr::null_mut()
}

pub fn autorelease_pool_pop(_token: *mut c_void) {}

// ── Strings ──────────────────────────────────────────────────────────────

const MAGIC: u32 = 0x5354_4243; // "CBTS"
const HDR: usize = 16;

#[repr(C)]
struct StrHdr {
    magic: u32,
    len: u32,
    refs: i32,
    cap: u32,
}

fn hdr(p: *mut c_void) -> Option<*mut StrHdr> {
    if p.is_null() || (p as usize) < HDR || (p as usize) & 7 != 0 {
        return None;
    }
    let h = (p as usize - HDR) as *mut StrHdr;
    (unsafe { (*h).magic } == MAGIC).then_some(h)
}

/// A new string of these Latin-1 bytes (reference count 1).
pub fn box_string(bytes: &[u8]) -> *mut c_void {
    let cap = HDR + bytes.len() + 1;
    let base = crate::rosbox::sys_alloc_zeroed(cap, 16);
    if base.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let h = base as *mut StrHdr;
        (*h).magic = MAGIC;
        (*h).len = bytes.len() as u32;
        (*h).refs = 1;
        (*h).cap = cap as u32;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(HDR), bytes.len());
    }
    COCOA_COUNTERS.strings_built.fetch_add(1, Ordering::Relaxed);
    unsafe { base.add(HDR) as *mut c_void }
}

/// UTF-8 (the compiler's literals) to Latin-1: a character above 255 is
/// "?"; bytes that are not UTF-8 are kept as they are.
fn latin1_of_utf8(b: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(b) {
        Ok(s) => s.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect(),
        Err(_) => b.to_vec(),
    }
}

unsafe fn c_bytes<'a>(p: *const u8) -> &'a [u8] {
    unsafe { std::ffi::CStr::from_ptr(p as *const std::ffi::c_char) }.to_bytes()
}

/// A string's bytes (Latin-1). Not one of ours: read as a zero-terminated
/// string at that address. `None` for nil.
pub(crate) unsafe fn nsstring_utf8_bytes(s: *mut c_void) -> Option<Vec<u8>> {
    if s.is_null() {
        return None;
    }
    match hdr(s) {
        Some(h) => {
            let n = unsafe { (*h).len } as usize;
            Some(unsafe { std::slice::from_raw_parts(s as *const u8, n) }.to_vec())
        }
        None => Some(unsafe { c_bytes(s as *const u8) }.to_vec()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring(text: *const u8) -> *mut c_void {
    if text.is_null() {
        return std::ptr::null_mut();
    }
    box_string(&latin1_of_utf8(unsafe { c_bytes(text) }))
}

/// A literal (cached by the compiler and never released) or an owned
/// result: either way a new string with one reference.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring_immortal(text: *const u8) -> *mut c_void {
    unsafe { bcpl_objc_nsstring(text) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_nsstring_to_utf8(s: *mut c_void, dest: *mut u8, cap: u64) -> u64 {
    if dest.is_null() || cap == 0 {
        return 0;
    }
    let Some(b) = (unsafe { nsstring_utf8_bytes(s) }) else { return 0 };
    let n = b.len().min(cap as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(b.as_ptr(), dest, n);
        *dest.add(n) = 0;
    }
    n as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_len(s: *mut c_void) -> i64 {
    match hdr(s) {
        Some(h) => unsafe { (*h).len as i64 },
        None if s.is_null() => 0,
        None => unsafe { c_bytes(s as *const u8) }.len() as i64,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_char(s: *mut c_void, idx: i64) -> i64 {
    let n = unsafe { bcpl_str_len(s) };
    if idx < 0 || idx >= n {
        return 0;
    }
    unsafe { *(s as *const u8).add(idx as usize) as i64 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_str_release(s: *mut c_void) {
    unsafe { bcpl_objc_release(s) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_release(obj: *mut c_void) {
    let Some(h) = hdr(obj) else { return };
    COCOA_COUNTERS.releases.fetch_add(1, Ordering::Relaxed);
    unsafe {
        (*h).refs -= 1;
        if (*h).refs == 0 {
            (*h).magic = 0;
            let cap = (*h).cap as usize;
            crate::rosbox::sys_free(h as *mut u8, cap, 16);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_retain(obj: *mut c_void) -> *mut c_void {
    if let Some(h) = hdr(obj) {
        COCOA_COUNTERS.retains.fetch_add(1, Ordering::Relaxed);
        unsafe { (*h).refs += 1 };
    }
    obj
}

// ── Objects: not in the box ──────────────────────────────────────────────

fn no_objects() -> ! {
    crate::rosbox::finish(-5, Some("BCPL: classes and Objective-C are not in the box".into()))
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_box_no_objects() -> i64 {
    no_objects()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_get_class(_name: *const u8) -> *mut c_void {
    std::ptr::null_mut()
}

/// A selector: the name itself, kept for the run.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_sel(name: *const u8) -> *mut c_void {
    name as *mut c_void
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_objc_msgsend_ptr() -> *mut c_void {
    bcpl_box_no_objects as *const () as *mut c_void
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_objc_msgsend_super_ptr() -> *mut c_void {
    bcpl_box_no_objects as *const () as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_new(_name: *const u8) -> *mut c_void {
    no_objects()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_alloc_init(_cls: *mut c_void) -> *mut c_void {
    no_objects()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_allocate_class(_sup: *mut c_void, _name: *const u8) -> *mut c_void {
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_add_ivar(_c: *mut c_void, _n: *const u8, _s: u64, _a: u8, _t: *const u8) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_add_method(_c: *mut c_void, _s: *mut c_void, _i: *mut c_void, _t: *const u8) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_register_class(_cls: *mut c_void) {}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_field_base(_obj: *mut c_void) -> *mut c_void {
    no_objects()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_field_base_for(_obj: *mut c_void, _ivar: *const u8) -> *mut c_void {
    no_objects()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_is_kind_of(_obj: *mut c_void, _name: *const u8) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_objc_make_block(_invoke: *mut c_void) -> *mut c_void {
    no_objects()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_autorelease_pool_push() -> *mut c_void {
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_autorelease_pool_pop(_t: *mut c_void) {}

macro_rules! entry {
    ($f:ident) => {
        (stringify!($f), $f as *const () as usize)
    };
}

pub fn builtin_addresses() -> Vec<(&'static str, usize)> {
    vec![
        entry!(bcpl_objc_get_class),
        entry!(bcpl_objc_sel),
        entry!(bcpl_objc_msgsend_ptr),
        entry!(bcpl_objc_msgsend_super_ptr),
        entry!(bcpl_objc_nsstring),
        entry!(bcpl_objc_nsstring_to_utf8),
        entry!(bcpl_objc_nsstring_immortal),
        entry!(bcpl_str_len),
        entry!(bcpl_str_char),
        entry!(bcpl_str_release),
        entry!(bcpl_objc_new),
        entry!(bcpl_objc_alloc_init),
        entry!(bcpl_objc_release),
        entry!(bcpl_objc_retain),
        entry!(bcpl_autorelease_pool_push),
        entry!(bcpl_autorelease_pool_pop),
        entry!(bcpl_objc_allocate_class),
        entry!(bcpl_objc_add_ivar),
        entry!(bcpl_objc_add_method),
        entry!(bcpl_objc_register_class),
        entry!(bcpl_objc_field_base),
        entry!(bcpl_objc_field_base_for),
        entry!(bcpl_objc_is_kind_of),
        entry!(bcpl_objc_make_block),
    ]
}
