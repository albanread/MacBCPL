//! `BRK` statement runtime — process-state dump (macOS arm64 fork).
//!
//! Classical BCPL leaves `BRK` as a hint that an attached debugger
//! should halt. We synthesise the inspection instead: when a JIT
//! program executes `BRK`, control transfers to
//! `__newbcpl_brk(routine_name, line)`, which writes a structured
//! snapshot to stderr and returns — execution continues after the BRK.
//!
//! The dump (cheapest, most-likely-to-succeed sections first):
//!   1. Banner — the BRK site (routine name + source line).
//!   2. Heap summary — from `gc::HEAP_COUNTERS` (process globals).
//!   3. Context — arm64 pc / sp / fp at the BRK site.
//!   4. Stack walk — `libc::backtrace`, each frame named via `dladdr`
//!      then the shared JIT-symbol registry (so BCPL routines appear
//!      as `in <routine>`).
//!
//! This replaces the upstream Windows implementation (RtlCaptureContext
//! + RtlVirtualUnwind over SEH tables) — MacBCPL is a fork with no
//! Windows / SEH support.

use crate::crash;
use core::ffi::c_void;

/// Register a JIT-emitted routine for BRK / crash stack-trace
/// resolution. Forwards to the shared crash-handler JIT symbol registry
/// (single source of truth). Called from the LLVM crate once each
/// routine's stable address is known.
pub fn register_jit_symbol(start_addr: u64, name: &str) {
    crash::bcpl_register_jit_symbol(start_addr as usize, name.as_ptr(), name.len());
}

/// `BRK` handler. `routine_name` is the NUL-terminated mangled name of
/// the routine the BRK fired inside; `line` is its source line. Both
/// best-effort (the handler tolerates null / 0).
///
/// # Safety
/// `routine_name` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn __newbcpl_brk(routine_name: *const u8, line: i64) {
    // ── 1. Banner ──────────────────────────────────────────────────
    let mut l = crash::Line::new();
    l.push_str("\n=== BRK in routine `");
    if routine_name.is_null() {
        l.push_str("<null>");
    } else {
        let mut n = 0usize;
        while n < 256 && unsafe { *routine_name.add(n) } != 0 {
            n += 1;
        }
        let slice = unsafe { core::slice::from_raw_parts(routine_name, n) };
        l.push(slice);
    }
    if line > 0 {
        l.push_str("` at line ");
        l.push_dec(line as usize);
        l.push_str(" ===\n");
    } else {
        l.push_str("` ===\n");
    }
    l.flush();

    // ── 2. Heap summary ────────────────────────────────────────────
    {
        use crate::gc::HEAP_COUNTERS;
        use core::sync::atomic::Ordering;
        let bytes = HEAP_COUNTERS.live_bytes.load(Ordering::Relaxed);
        let blocks = HEAP_COUNTERS.live_blocks.load(Ordering::Relaxed);
        let peak = HEAP_COUNTERS.peak_live_bytes.load(Ordering::Relaxed);
        let mut h = crash::Line::new();
        h.push_str("heap:    live=");
        h.push_dec(bytes as usize);
        h.push_str(" bytes  blocks=");
        h.push_dec(blocks as usize);
        h.push_str("  peak=");
        h.push_dec(peak as usize);
        h.push_str(" bytes\n");
        h.flush();
    }

    // ── 3. Context (arm64 pc / sp / fp at the BRK site) ─────────────
    {
        let sp: usize;
        let fp: usize;
        // x29 = frame pointer, sp = stack pointer. The BRK site's pc is
        // our return address, x30 (lr) on entry — captured as the link
        // register before it's clobbered.
        let lr: usize;
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) sp);
            core::arch::asm!("mov {}, x29", out(reg) fp);
            core::arch::asm!("mov {}, x30", out(reg) lr);
        }
        let mut c = crash::Line::new();
        c.push_str("context: pc=0x");
        c.push_hex(lr);
        c.push_str("  sp=0x");
        c.push_hex(sp);
        c.push_str("  fp=0x");
        c.push_hex(fp);
        c.push_str("\n");
        c.flush();
    }

    // ── 4. Stack walk ──────────────────────────────────────────────
    //
    // We walk the arm64 frame-pointer (x29) chain by hand rather than
    // via `libc::backtrace`: macOS libunwind does NOT reliably traverse
    // JIT-compiled frames (they carry no registered unwind info), so a
    // backtrace would skip BCPL routines. Every frame on macOS arm64
    // preserves the fp chain — frame record = [saved_fp, return_addr] at
    // [fp], [fp+8] — so an fp-walk reaches every BCPL routine. Each
    // return address is named via the JIT registry (`in <routine>`) then
    // dladdr (host / runtime / OS).
    {
        let mut hdr = crash::Line::new();
        hdr.push_str("stack:\n");
        hdr.flush();

        let mut fp: usize;
        unsafe {
            core::arch::asm!("mov {}, x29", out(reg) fp);
        }
        let mut shown = 0usize;
        let mut guard = 0usize;
        while fp != 0 && (fp & 0xf) == 0 && guard < 128 {
            guard += 1;
            // [fp] = caller's fp, [fp+8] = return address into caller.
            let next_fp = unsafe { *(fp as *const usize) };
            let ret = unsafe { *((fp + 8) as *const usize) };
            if ret == 0 {
                break;
            }
            let mut ln = crash::Line::new();
            ln.push_str("  #");
            ln.push_dec(shown);
            ln.push_str("  0x");
            ln.push_hex(ret);
            if let Some((nm, off)) = crash::resolve_jit_symbol(ret) {
                ln.push_str("  in ");
                ln.push_str(nm);
                ln.push_str("+0x");
                ln.push_hex(off);
            } else {
                let mut di: libc::Dl_info = unsafe { core::mem::zeroed() };
                if unsafe { libc::dladdr(ret as *const c_void, &mut di) } != 0
                    && !di.dli_sname.is_null()
                {
                    let mut len = 0usize;
                    while len < 256 && unsafe { *di.dli_sname.add(len) } != 0 {
                        len += 1;
                    }
                    let name =
                        unsafe { core::slice::from_raw_parts(di.dli_sname as *const u8, len) };
                    ln.push_str("  in ");
                    ln.push(name);
                }
            }
            ln.push_str("\n");
            ln.flush();
            shown += 1;
            // Frame records climb to higher addresses; stop if the
            // chain isn't strictly ascending (corrupt / end of stack).
            if next_fp <= fp {
                break;
            }
            fp = next_fp;
        }
    }

    let mut end = crash::Line::new();
    end.push_str("=== END BRK ===\n\n");
    end.flush();
}
