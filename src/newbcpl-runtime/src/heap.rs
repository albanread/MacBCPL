//! Manual free-list heap — Tier 2 of the MacBCPL no-GC memory model.
//!
//! MacBCPL replaces NewBCPL's tracing GC with a non-collector model
//! (see docs / project memory): a per-scope ARENA for transients with
//! stack-scope lifetimes (Tier 1), this MANUAL heap for explicit
//! `GETVEC`/`FREEVEC` and for anything that escapes its scope (Tier 2),
//! and Cocoa objects for `NEW Class` (Tier 3).
//!
//! This tier is intentionally simple and correct: each block is a
//! system allocation carrying the SAME 16-byte header layout the old GC
//! `BlockHeader` used (`tag`, `block_size`), so the payload pointer math
//! is unchanged — payload = block + 16, and `__newbcpl_len` still reads
//! the length word the caller stamps at payload[0] via `p[-1]`. The OS
//! allocator handles coalescing/fragmentation; "manual" here means BCPL
//! controls alloc/free explicitly (no collector). A bump/free-list
//! upgrade can replace the backing later without changing the ABI.

use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};
use std::sync::atomic::Ordering;

use crate::gc::HEAP_COUNTERS;

/// 16-byte block header preceding every payload. Mirrors the old GC
/// `BlockHeader` so the payload offset (+16) and size recovery are
/// identical. `tag` is unused by the manual heap (no tracing); kept so
/// the layout and any header-relative debugging stays consistent.
#[repr(C)]
struct Hdr {
    tag: usize,
    block_size: usize,
}

const HEADER_SIZE: usize = std::mem::size_of::<Hdr>(); // 16
const BLOCK_ALIGN: usize = 16;

#[inline]
fn round_up(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

fn bump_counters_alloc(total: usize) {
    HEAP_COUNTERS.alloc_blocks_lifetime.fetch_add(1, Ordering::Relaxed);
    HEAP_COUNTERS
        .alloc_bytes_lifetime
        .fetch_add(total as u64, Ordering::Relaxed);
    HEAP_COUNTERS.live_blocks.fetch_add(1, Ordering::Relaxed);
    let live = HEAP_COUNTERS
        .live_bytes
        .fetch_add(total as u64, Ordering::Relaxed)
        + total as u64;
    // Monotonic peak (best-effort; single language thread).
    let peak = HEAP_COUNTERS.peak_live_bytes.load(Ordering::Relaxed);
    if live > peak {
        HEAP_COUNTERS.peak_live_bytes.store(live, Ordering::Relaxed);
    }
}

fn bump_counters_free(total: usize) {
    HEAP_COUNTERS.free_blocks_lifetime.fetch_add(1, Ordering::Relaxed);
    HEAP_COUNTERS
        .free_bytes_lifetime
        .fetch_add(total as u64, Ordering::Relaxed);
    // saturating: never underflow if a foreign/double free slips through.
    let prev_blocks = HEAP_COUNTERS.live_blocks.load(Ordering::Relaxed);
    if prev_blocks > 0 {
        HEAP_COUNTERS.live_blocks.fetch_sub(1, Ordering::Relaxed);
    }
    let prev_bytes = HEAP_COUNTERS.live_bytes.load(Ordering::Relaxed);
    let dec = (total as u64).min(prev_bytes);
    HEAP_COUNTERS.live_bytes.fetch_sub(dec, Ordering::Relaxed);
}

/// Allocate `payload` zeroed bytes on the manual heap. Returns a pointer
/// to the payload (the 16-byte header sits just below it). The caller
/// uses the payload exactly as it did under the GC.
///
/// # Safety
/// The returned block must be released with [`bcpl_heap_free`] (or an
/// equivalent `FREEVEC`), never with the system allocator directly.
pub unsafe fn bcpl_heap_alloc(payload: usize) -> *mut u8 {
    let payload = round_up(payload.max(1), BLOCK_ALIGN);
    let total = HEADER_SIZE + payload;
    let layout = Layout::from_size_align(total, BLOCK_ALIGN)
        .expect("bcpl_heap_alloc: invalid layout");
    let block = unsafe { alloc_zeroed(layout) };
    if block.is_null() {
        handle_alloc_error(layout);
    }
    unsafe {
        let hdr = block as *mut Hdr;
        (*hdr).tag = 0;
        (*hdr).block_size = total;
    }
    bump_counters_alloc(total);
    unsafe { block.add(HEADER_SIZE) }
}

/// Free a block previously returned by [`bcpl_heap_alloc`]. Null and
/// already-zeroed-header pointers are ignored (best-effort guard against
/// double / foreign frees — pointers into an arena or a string literal
/// must never be passed here, but a stray FREEVEC won't corrupt).
///
/// # Safety
/// `payload` must be null or a pointer previously returned by
/// `bcpl_heap_alloc`, not yet freed.
pub unsafe fn bcpl_heap_free(payload: *mut u8) {
    if payload.is_null() {
        return;
    }
    let block = unsafe { payload.sub(HEADER_SIZE) };
    let total = unsafe { (*(block as *const Hdr)).block_size };
    if total < HEADER_SIZE + BLOCK_ALIGN || total > (1usize << 40) {
        // Implausible size — not one of ours (arena ptr, literal, double
        // free with a clobbered header). Leak rather than corrupt.
        return;
    }
    // Poison the header so a double free is caught by the guard above.
    unsafe {
        (*(block as *mut Hdr)).block_size = 0;
    }
    let layout = Layout::from_size_align(total, BLOCK_ALIGN)
        .expect("bcpl_heap_free: invalid layout");
    unsafe { dealloc(block, layout) };
    bump_counters_free(total);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_roundtrip() {
        unsafe {
            let p = bcpl_heap_alloc(64) as *mut i64;
            // payload is writable and zeroed
            assert_eq!(*p, 0);
            *p = 0x1234;
            *p.add(7) = 0x5678;
            assert_eq!(*p, 0x1234);
            assert_eq!(*p.add(7), 0x5678);
            bcpl_heap_free(p as *mut u8);
            // double free is a no-op (poisoned header), not a crash
            bcpl_heap_free(p as *mut u8);
        }
    }

    #[test]
    fn null_free_is_noop() {
        unsafe { bcpl_heap_free(std::ptr::null_mut()) };
    }
}
