//! Signal-safe crash handler for MacBCPL (macOS arm64).
//!
//! Ported from MacModula2's `newm2-runtime/src/crash.rs` (POSIX path).
//! MacBCPL is a fork with no Windows support, so there is no SEH /
//! vectored-exception path here — only the POSIX `sigaction` handler.
//!
//! On a fatal signal (SIGSEGV / SIGBUS / SIGILL / SIGFPE / SIGABRT /
//! SIGTRAP) it writes an annotated backtrace to stderr and then restores
//! the default disposition and re-raises, so the OS still produces its
//! normal crash report. The handler is **async-signal-safe**: no heap
//! allocation and no locking — output goes through a fixed stack buffer
//! and raw `write(2)`. Frames are named via `dladdr` (for real linked
//! symbols and runtime helpers) with a fallback to the nearest
//! JIT-registered BCPL routine (a lock-free binary search over a frozen,
//! sorted symbol table published via atomics).
//!
//! The same JIT symbol registry is reused by the `BRK` statement
//! (`brk.rs`) so its stack walk can name BCPL routines too.

use core::ffi::c_void;

// ── JIT symbol registry (shared with BRK) ─────────────────────────────
//
// `dladdr` names statically-linked symbols (runtime helpers, libSystem,
// frameworks). JIT-compiled BCPL routines have no dladdr entry, so the
// JIT registers each routine's (address, name) here at finalize. The
// table is built once, then frozen and published via atomics so the
// signal handler can binary-search it without locking.
mod jit_syms {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

    pub struct Sym {
        pub addr: usize,
        pub name: String,
    }

    static PENDING: Mutex<Vec<Sym>> = Mutex::new(Vec::new());
    static FROZEN_PTR: AtomicPtr<Sym> = AtomicPtr::new(core::ptr::null_mut());
    static FROZEN_LEN: AtomicUsize = AtomicUsize::new(0);

    pub fn register(addr: usize, name: *const u8, len: usize) {
        if addr == 0 || name.is_null() || len == 0 {
            return;
        }
        let bytes = unsafe { core::slice::from_raw_parts(name, len) };
        let name = String::from_utf8_lossy(bytes).into_owned();
        if let Ok(mut v) = PENDING.lock() {
            v.push(Sym { addr, name });
        }
    }

    pub fn finalize() {
        let Ok(mut v) = PENDING.lock() else { return };
        let mut syms = core::mem::take(&mut *v);
        syms.sort_by_key(|s| s.addr);
        let boxed: Box<[Sym]> = syms.into_boxed_slice();
        let len = boxed.len();
        let ptr = Box::leak(boxed).as_mut_ptr();
        FROZEN_LEN.store(len, Ordering::Release);
        FROZEN_PTR.store(ptr, Ordering::Release);
    }

    /// Nearest registered symbol with `addr <= pc`, plus byte offset.
    /// Lock-free — safe to call from a signal handler.
    pub fn resolve(pc: usize) -> Option<(&'static str, usize)> {
        let ptr = FROZEN_PTR.load(Ordering::Acquire);
        let len = FROZEN_LEN.load(Ordering::Acquire);
        if ptr.is_null() || len == 0 {
            return None;
        }
        let syms = unsafe { core::slice::from_raw_parts(ptr, len) };
        let (mut lo, mut hi) = (0usize, len);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if syms[mid].addr <= pc {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == 0 {
            return None;
        }
        let s = &syms[lo - 1];
        let offset = pc - s.addr;
        // Don't attribute an address that's far past a routine's start to
        // that routine — it's probably in unregistered code.
        if offset > (1 << 20) {
            return None;
        }
        Some((s.name.as_str(), offset))
    }
}

pub use jit_syms::resolve as resolve_jit_symbol;

// ── Async-signal-safe output ──────────────────────────────────────────

fn write_all(bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let r = unsafe {
            libc::write(2, bytes[off..].as_ptr() as *const c_void, bytes.len() - off)
        };
        if r <= 0 {
            break;
        }
        off += r as usize;
    }
}

/// Fixed-capacity, no-alloc line buffer (signal-safe).
pub(crate) struct Line {
    buf: [u8; 512],
    len: usize,
}
impl Line {
    pub(crate) fn new() -> Self {
        Line { buf: [0u8; 512], len: 0 }
    }
    pub(crate) fn push(&mut self, s: &[u8]) {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s[..n]);
        self.len += n;
    }
    pub(crate) fn push_str(&mut self, s: &str) {
        self.push(s.as_bytes());
    }
    pub(crate) fn push_hex(&mut self, mut v: usize) {
        if v == 0 {
            self.push(b"0");
            return;
        }
        let mut tmp = [0u8; 16];
        let mut i = tmp.len();
        while v != 0 {
            i -= 1;
            let d = (v & 0xf) as u8;
            tmp[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
            v >>= 4;
        }
        self.push(&tmp[i..]);
    }
    pub(crate) fn push_dec(&mut self, mut v: usize) {
        if v == 0 {
            self.push(b"0");
            return;
        }
        let mut tmp = [0u8; 20];
        let mut i = tmp.len();
        while v != 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        self.push(&tmp[i..]);
    }
    pub(crate) fn flush(&mut self) {
        write_all(&self.buf[..self.len]);
        self.len = 0;
    }
}

/// Walk the call stack with `libc::backtrace`, naming each frame via
/// `dladdr` then the JIT registry, writing to stderr. Shared by the
/// signal handler and the `BRK` statement. `skip` drops the first N
/// frames (the dumper's own frames). Signal-safe.
pub(crate) fn write_backtrace(skip: usize) {
    let mut frames: [*mut c_void; 64] = [core::ptr::null_mut(); 64];
    let n = unsafe { libc::backtrace(frames.as_mut_ptr(), frames.len() as i32) } as usize;
    let mut shown = 0usize;
    for (i, &f) in frames.iter().take(n).enumerate() {
        if i < skip {
            continue;
        }
        let pc = f as usize;
        if pc == 0 {
            break;
        }
        let mut ln = Line::new();
        ln.push_str("  #");
        ln.push_dec(shown);
        ln.push_str("  0x");
        ln.push_hex(pc);
        let mut named = false;
        let mut di: libc::Dl_info = unsafe { core::mem::zeroed() };
        if unsafe { libc::dladdr(f as *const c_void, &mut di) } != 0 && !di.dli_sname.is_null() {
            let mut len = 0usize;
            while len < 512 && unsafe { *di.dli_sname.add(len) } != 0 {
                len += 1;
            }
            let name = unsafe { core::slice::from_raw_parts(di.dli_sname as *const u8, len) };
            ln.push_str("  ");
            ln.push(name);
            if !di.dli_saddr.is_null() {
                ln.push_str("+0x");
                ln.push_hex(pc - di.dli_saddr as usize);
            }
            named = true;
        }
        if !named {
            if let Some((nm, off)) = resolve_jit_symbol(pc) {
                ln.push_str("  BCPL ");
                ln.push_str(nm);
                ln.push_str("+0x");
                ln.push_hex(off);
            } else {
                ln.push_str("  <unknown>");
            }
        }
        ln.push_str("\n");
        ln.flush();
        shown += 1;
    }
}

// ── The signal handler ────────────────────────────────────────────────

fn sig_name(sig: i32) -> &'static str {
    match sig {
        libc::SIGSEGV => "SIGSEGV (segmentation fault)",
        libc::SIGBUS => "SIGBUS (bus error)",
        libc::SIGILL => "SIGILL (illegal instruction)",
        libc::SIGFPE => "SIGFPE (arithmetic fault)",
        libc::SIGABRT => "SIGABRT (abort)",
        libc::SIGTRAP => "SIGTRAP (trap)",
        _ => "fatal signal",
    }
}

static IN_HANDLER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe extern "C" fn handler(sig: i32, info: *mut libc::siginfo_t, _ctx: *mut c_void) {
    use std::sync::atomic::Ordering;
    // Re-entrancy (a fault while dumping): go straight to default.
    if IN_HANDLER.swap(true, Ordering::SeqCst) {
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
        return;
    }

    let fault = if info.is_null() {
        0usize
    } else {
        unsafe { (*info).si_addr as usize }
    };

    let mut l = Line::new();
    l.push_str("\n=== MacBCPL fatal signal: ");
    l.push_str(sig_name(sig));
    l.push_str(" — faulting address 0x");
    l.push_hex(fault);
    l.push_str(" ===\n");
    l.flush();

    write_backtrace(0);

    let mut tail = Line::new();
    tail.push_str("=== end backtrace (OS crash report follows) ===\n");
    tail.flush();

    // Restore default disposition and re-raise so the OS still produces
    // its normal crash report; the process terminates as it otherwise
    // would.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

static INSTALL: std::sync::Once = std::sync::Once::new();

/// Install the signal-safe crash handler (idempotent). Call once at
/// startup, before running JIT'd code.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_install_crash_handler() {
    INSTALL.call_once(|| unsafe {
        let mut sa: libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = handler as libc::sighandler_t;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut sa.sa_mask);
        for &s in &[
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGABRT,
            libc::SIGTRAP,
        ] {
            libc::sigaction(s, &sa, core::ptr::null_mut());
        }
    });
}

/// Register a JIT-compiled routine's (address, name) so crash / BRK
/// backtraces can name it. Call per routine at JIT finalize.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_register_jit_symbol(addr: usize, name: *const u8, len: usize) {
    jit_syms::register(addr, name, len);
}

/// Freeze and publish the JIT symbol table for lock-free signal-handler
/// lookup. Call once after all routines are registered, before running.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_finalize_jit_symbols() {
    jit_syms::finalize();
}
