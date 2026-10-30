//! The BOX: RISC OS from BCPL (design/BOX_PORT.md, sections 5 and 7).
//!
//! SWIs on BCPL words: `SYS(n, r)` and `XSYS(n, r)` by number,
//! `SYSN(name, r)` and `XSYSN(name, r)` by name, with r!0..r!9 in and out.
//! A word in is taken as 32 bits (from -2^31 to 2^32-1, else the program
//! ends with an error); a word out is the register, zero-extended. `SYS`
//! ends the program with a SWI's error; `XSYS` returns the error block's
//! address (0 if none) and `SYSFLAGS()` gives N Z C V.
//!
//! Streams, as classic BCPL's: FINDINPUT, FINDOUTPUT, FINDAPPEND,
//! SELECTINPUT, SELECTOUTPUT, INPUT, OUTPUT, ENDREAD, ENDWRITE, ENDSTREAM,
//! WRCH; and STOP(code).

use crate::rosbox;
use std::sync::atomic::{AtomicU32, Ordering};

static FLAGS: AtomicU32 = AtomicU32::new(0);

fn fail(text: String) -> ! {
    rosbox::finish(-5, Some(text))
}

fn regs_in(r: *const i64) -> [u32; 10] {
    let mut out = [0u32; 10];
    if r.is_null() {
        return out;
    }
    for (i, o) in out.iter_mut().enumerate() {
        let w = unsafe { *r.add(i) };
        if !(-(1i64 << 31)..(1i64 << 32)).contains(&w) {
            fail(format!("SYS: register {i} ({w}) is not a 32-bit value (an address outside RISC OS's memory?)"));
        }
        *o = w as u32;
    }
    out
}

fn regs_out(r: *mut i64, v: &[u32; 10]) {
    if r.is_null() {
        return;
    }
    for (i, &x) in v.iter().enumerate() {
        unsafe { *r.add(i) = x as i64 };
    }
}

/// The SWI; Ok, or Err(error block address).
fn call(n: u32, r: *mut i64) -> Result<(), u32> {
    let mut v = regs_in(r);
    let res = rosbox::swi(n, &mut v);
    regs_out(r, &v);
    match res {
        Ok(f) => {
            FLAGS.store(f, Ordering::Relaxed);
            Ok(())
        }
        Err(e) => {
            FLAGS.store(1, Ordering::Relaxed);
            Err(e)
        }
    }
}

fn raise(e: u32) -> ! {
    fail(format!("{} (error &{:X})", rosbox::error_text(e), rosbox::error_number(e)))
}

/// The bytes of a BCPL string (or a zero-terminated string at that address).
fn bytes_of(s: i64) -> Vec<u8> {
    unsafe { crate::objc::nsstring_utf8_bytes(s as usize as *mut std::ffi::c_void) }.unwrap_or_default()
}

fn number_of(name: i64) -> u32 {
    let n = String::from_utf8_lossy(&bytes_of(name)).into_owned();
    match rosbox::swi_number(&n) {
        Some(x) => x,
        None => fail(format!("SYSN: no SWI is called {n}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SYS(n: i64, r: *mut i64) -> i64 {
    if let Err(e) = call(n as u32, r) {
        raise(e);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn XSYS(n: i64, r: *mut i64) -> i64 {
    match call(n as u32, r) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SYSN(name: i64, r: *mut i64) -> i64 {
    SYS(number_of(name) as i64, r)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn XSYSN(name: i64, r: *mut i64) -> i64 {
    XSYS(number_of(name) as i64, r)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SWINUMBER(name: i64) -> i64 {
    let n = String::from_utf8_lossy(&bytes_of(name)).into_owned();
    rosbox::swi_number(&n).map(|x| x as i64).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SYSFLAGS() -> i64 {
    FLAGS.load(Ordering::Relaxed) as i64
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn ERRNUM(e: i64) -> i64 {
    if e <= 0 || e > u32::MAX as i64 { 0 } else { rosbox::error_number(e as u32) as i64 }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn ERRTEXT(e: i64) -> i64 {
    let t: Vec<u8> = if e <= 0 || e > u32::MAX as i64 {
        Vec::new()
    } else {
        rosbox::error_text(e as u32).chars().map(|c| c as u32 as u8).collect()
    };
    crate::objc::box_string(&t) as i64
}

/// A string from a control-terminated string at RISC OS address `a`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn CSTR(a: i64) -> i64 {
    if a <= 0 || a > u32::MAX as i64 {
        return crate::objc::box_string(b"") as i64;
    }
    let p = a as usize as *const u8;
    let mut n = 0usize;
    while n < 65536 && unsafe { *p.add(n) } >= 32 {
        n += 1;
    }
    crate::objc::box_string(unsafe { std::slice::from_raw_parts(p, n) }) as i64
}

/// `p` checked to be a RISC OS address.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn ADDR(p: i64) -> i64 {
    if !(0..(1i64 << 32)).contains(&p) {
        fail(format!("ADDR: &{p:X} is not a RISC OS address (a local's address? use a VEC or GETVEC)"));
    }
    p
}

// ── Streams ──────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C-unwind" fn FINDINPUT(name: i64) -> i64 {
    rosbox::open(&bytes_of(name), b'r')
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn FINDOUTPUT(name: i64) -> i64 {
    rosbox::open(&bytes_of(name), b'w')
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn FINDAPPEND(name: i64) -> i64 {
    rosbox::open(&bytes_of(name), b'a')
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SELECTINPUT(s: i64) -> i64 {
    rosbox::select_input(s)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn SELECTOUTPUT(s: i64) -> i64 {
    rosbox::select_output(s)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn INPUT() -> i64 {
    rosbox::input()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn OUTPUT() -> i64 {
    rosbox::output()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn ENDREAD() -> i64 {
    rosbox::end_stream(rosbox::input());
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn ENDWRITE() -> i64 {
    rosbox::end_stream(rosbox::output());
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn ENDSTREAM(s: i64) -> i64 {
    rosbox::end_stream(s);
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn WRCH(c: i64) -> i64 {
    rosbox::write_bytes(&[if (0..256).contains(&c) { c as u8 } else { b'?' }]);
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn STOP(code: i64) -> i64 {
    rosbox::finish(code, None)
}

macro_rules! entry {
    ($f:ident) => {
        crate::builtins::Builtin { name: stringify!($f), address: $f as *const () as usize }
    };
}

pub fn builtin_addresses() -> Vec<crate::builtins::Builtin> {
    vec![
        entry!(SYS),
        entry!(XSYS),
        entry!(SYSN),
        entry!(XSYSN),
        entry!(SWINUMBER),
        entry!(SYSFLAGS),
        entry!(ERRNUM),
        entry!(ERRTEXT),
        entry!(CSTR),
        entry!(ADDR),
        entry!(FINDINPUT),
        entry!(FINDOUTPUT),
        entry!(FINDAPPEND),
        entry!(SELECTINPUT),
        entry!(SELECTOUTPUT),
        entry!(INPUT),
        entry!(OUTPUT),
        entry!(ENDREAD),
        entry!(ENDWRITE),
        entry!(ENDSTREAM),
        entry!(WRCH),
        entry!(STOP),
    ]
}
