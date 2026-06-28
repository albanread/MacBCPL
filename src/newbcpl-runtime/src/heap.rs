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

// `tag` is a provenance marker so FREEVEC is safe regardless of which
// tier a pointer came from. Escape analysis is conservative, so an
// arena-allocated value can legitimately reach a FREEVEC; the heap free
// only acts on its own blocks and treats anything else as a no-op.
const HEAP_MAGIC: usize = 0xBCB1_C0DE_0000_0001; // manual-heap block
pub(crate) const ARENA_MAGIC: usize = 0xBCB1_C0DE_0000_0002; // arena block (never individually freed)

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
        (*hdr).tag = HEAP_MAGIC;
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
    let (tag, total) = unsafe {
        let h = block as *const Hdr;
        ((*h).tag, (*h).block_size)
    };
    // Only free our own manual-heap blocks. An arena block (ARENA_MAGIC),
    // a string literal, a stack pointer, or an already-freed block has a
    // different/zeroed tag — leave it alone (no-op) rather than corrupt.
    // This is what makes a conservatively-routed FREEVEC always safe.
    if tag != HEAP_MAGIC || total < HEADER_SIZE + BLOCK_ALIGN || total > (1usize << 40) {
        return;
    }
    // Poison the tag so a double free is caught by the check above.
    unsafe {
        (*(block as *mut Hdr)).tag = 0;
    }
    let layout = Layout::from_size_align(total, BLOCK_ALIGN)
        .expect("bcpl_heap_free: invalid layout");
    unsafe { dealloc(block, layout) };
    bump_counters_free(total);
}

// ─────────────────────────────────────────────────────────────────────
// Tier 1 — per-scope ARENA (the "heap objects with stack-scope lifetime"
// mechanism). A thread-local stack of bump-allocated regions: each
// lexical scope that owns an arena does `arena_enter` on entry and
// `arena_free` on every exit edge (woven into the same innermost-first
// exit walk that fires USING RELEASE). Allocations from the innermost
// arena are reclaimed wholesale at scope exit — no collector, no
// per-object free. Values that escape their scope are routed to the
// manual heap instead (by the compiler's escape analysis), or promoted
// with `bcpl_promote`.
// ─────────────────────────────────────────────────────────────────────

use std::cell::RefCell;

const DEFAULT_CHUNK: usize = 64 * 1024;

struct Chunk {
    base: *mut u8,
    cap: usize,
    used: usize,
}

impl Drop for Chunk {
    fn drop(&mut self) {
        if !self.base.is_null() {
            let layout = Layout::from_size_align(self.cap, BLOCK_ALIGN).unwrap();
            unsafe { dealloc(self.base, layout) };
            let live = HEAP_COUNTERS.live_bytes.load(Ordering::Relaxed);
            HEAP_COUNTERS
                .live_bytes
                .fetch_sub((self.used as u64).min(live), Ordering::Relaxed);
        }
    }
}

struct Arena {
    chunks: Vec<Chunk>,
}

impl Arena {
    fn new() -> Self {
        Arena { chunks: Vec::new() }
    }

    /// Bump-allocate `total` (header+payload) bytes, growing by a new
    /// chunk when the current one is full. Returns the block start.
    fn alloc_block(&mut self, total: usize) -> *mut u8 {
        if let Some(c) = self.chunks.last_mut() {
            let off = round_up(c.used, BLOCK_ALIGN);
            if off + total <= c.cap {
                let p = unsafe { c.base.add(off) };
                c.used = off + total;
                return p;
            }
        }
        let cap = total.max(DEFAULT_CHUNK);
        let layout = Layout::from_size_align(cap, BLOCK_ALIGN)
            .expect("arena chunk layout");
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            handle_alloc_error(layout);
        }
        HEAP_COUNTERS
            .live_bytes
            .fetch_add(total as u64, Ordering::Relaxed);
        let live = HEAP_COUNTERS.live_bytes.load(Ordering::Relaxed);
        let peak = HEAP_COUNTERS.peak_live_bytes.load(Ordering::Relaxed);
        if live > peak {
            HEAP_COUNTERS.peak_live_bytes.store(live, Ordering::Relaxed);
        }
        self.chunks.push(Chunk { base, cap, used: total });
        base
    }

    /// Drop all but the first chunk and rewind it — for per-iteration
    /// reuse (loop back-edges) without tearing the arena down.
    fn reset(&mut self) {
        self.chunks.truncate(1);
        if let Some(c) = self.chunks.first_mut() {
            c.used = 0;
        }
    }
}

thread_local! {
    static ARENA_STACK: RefCell<Vec<Arena>> = const { RefCell::new(Vec::new()) };
}

/// Push a new arena and make it current; returns its handle (stack
/// index). The lowerer pairs this with `bcpl_arena_free(handle)` on
/// every scope-exit edge.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_arena_enter() -> i64 {
    ARENA_STACK.with(|s| {
        let mut st = s.borrow_mut();
        st.push(Arena::new());
        (st.len() - 1) as i64
    })
}

/// Free every arena from `handle` up to the top of the stack (so a
/// non-local exit that skips inner frees still reclaims them), then
/// truncate the stack to `handle`. Wholesale reclamation of the scope's
/// heap objects.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_arena_free(handle: i64) {
    if handle < 0 {
        return;
    }
    ARENA_STACK.with(|s| {
        let mut st = s.borrow_mut();
        let h = handle as usize;
        if h < st.len() {
            st.truncate(h); // drops arenas (and their chunks) at index >= h
        }
    });
}

/// Rewind the arena at `handle` to a single empty chunk — per-iteration
/// reuse for loop bodies.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bcpl_arena_reset(handle: i64) {
    if handle < 0 {
        return;
    }
    ARENA_STACK.with(|s| {
        let mut st = s.borrow_mut();
        let h = handle as usize;
        if let Some(a) = st.get_mut(h) {
            a.reset();
        }
    });
}

/// Arena counterpart of `__newbcpl_alloc_rec`: allocate `payload` bytes
/// in the innermost arena (stack-scope lifetime). If no arena is active
/// (shouldn't happen once lowering brackets every function), falls back
/// to the manual heap so the result is always valid. Returns the payload
/// pointer; same 16-byte header / `__newbcpl_len` contract as the heap.
///
/// # Safety
/// The result lives only until the owning scope exits — the compiler
/// only routes proven-non-escaping allocations here.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn __newbcpl_alloc_rec_arena(size: i64) -> *mut u8 {
    let payload = round_up((size.max(0) as usize).max(1), BLOCK_ALIGN);
    let total = HEADER_SIZE + payload;
    let block = ARENA_STACK.with(|s| {
        let mut st = s.borrow_mut();
        st.last_mut().map(|a| a.alloc_block(total))
    });
    match block {
        Some(block) => unsafe {
            let hdr = block as *mut Hdr;
            (*hdr).tag = ARENA_MAGIC;
            (*hdr).block_size = total;
            block.add(HEADER_SIZE)
        },
        // No active arena → safe fallback to the manual heap.
        None => unsafe { bcpl_heap_alloc(payload) },
    }
}

/// Copy an allocation into the manual heap and return the heap pointer —
/// used to promote a value that escapes its arena (RETAIN, or a return /
/// store the escape analysis couldn't prove local). `payload_words` is
/// the number of payload bytes to copy. Idempotent-ish: a value already
/// on the manual heap is returned unchanged.
///
/// # Safety
/// `ptr` must be a valid payload pointer with at least `payload_bytes`
/// readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bcpl_promote(ptr: *mut u8, payload_bytes: i64) -> *mut u8 {
    if ptr.is_null() {
        return ptr;
    }
    let block = unsafe { ptr.sub(HEADER_SIZE) };
    let tag = unsafe { (*(block as *const Hdr)).tag };
    if tag == HEAP_MAGIC {
        return ptr; // already heap-resident; nothing to do
    }
    let n = payload_bytes.max(0) as usize;
    let dst = unsafe { bcpl_heap_alloc(n) };
    unsafe { std::ptr::copy_nonoverlapping(ptr, dst, n) };
    dst
}

/// `(symbol, address)` table for the arena/heap entry points the JIT
/// must resolve.
pub fn builtin_addresses() -> Vec<(&'static str, usize)> {
    vec![
        ("bcpl_arena_enter", bcpl_arena_enter as *const () as usize),
        ("bcpl_arena_free", bcpl_arena_free as *const () as usize),
        ("bcpl_arena_reset", bcpl_arena_reset as *const () as usize),
        (
            "__newbcpl_alloc_rec_arena",
            __newbcpl_alloc_rec_arena as *const () as usize,
        ),
        ("bcpl_promote", bcpl_promote as *const () as usize),
    ]
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

    #[test]
    fn arena_alloc_and_free() {
        unsafe {
            let h = bcpl_arena_enter();
            let a = __newbcpl_alloc_rec_arena(32) as *mut i64;
            let b = __newbcpl_alloc_rec_arena(4096) as *mut i64; // forces a 2nd chunk path
            *a = 11;
            *b = 22;
            *b.add(100) = 33;
            assert_eq!(*a, 11);
            assert_eq!(*b, 22);
            assert_eq!(*b.add(100), 33);
            // FREEVEC on an arena pointer must be a safe no-op (not a crash).
            bcpl_heap_free(a as *mut u8);
            assert_eq!(*a, 11, "arena block must survive a stray FREEVEC");
            bcpl_arena_free(h); // wholesale reclaim
        }
    }

    #[test]
    fn promote_copies_arena_to_heap() {
        unsafe {
            let h = bcpl_arena_enter();
            let a = __newbcpl_alloc_rec_arena(16) as *mut i64;
            *a = 0x7777;
            *a.add(1) = 0x8888;
            let p = bcpl_promote(a as *mut u8, 16) as *mut i64;
            assert_ne!(p as usize, a as usize, "promote must move off the arena");
            assert_eq!(*p, 0x7777);
            assert_eq!(*p.add(1), 0x8888);
            bcpl_arena_free(h); // a is gone; p (heap) still valid
            assert_eq!(*p, 0x7777, "promoted copy survives arena free");
            bcpl_heap_free(p as *mut u8);
        }
    }
}
