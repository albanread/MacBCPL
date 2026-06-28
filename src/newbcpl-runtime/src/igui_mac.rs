//! BCPL-callable iGui surface — **macOS port.**
//!
//! This module provides the same `iGui_*` C-ABI symbols the Windows
//! `igui_builtins` module exposes (see that file for the canonical
//! signatures and the BCPL ABI rationale), so the JIT can resolve the
//! `igui` standard module on macOS exactly as on Windows.
//!
//! For now these are **no-op stubs**: they satisfy symbol resolution
//! and let console programs that merely *link* the `igui` module run,
//! and let GUI programs execute their non-drawing logic. Phase 3 of the
//! MacBCPL port replaces the bodies with a real Cocoa implementation
//! built on the MacModula2 Objective-C bridge (AppKit window + a
//! CoreGraphics/CoreText custom NSView for the draw batch, and an
//! NSEvent-backed event mailbox for `iGui_NextEvent`). The signatures
//! here are frozen to match `igui_builtins.rs` so that swap is purely
//! internal.

#![cfg(not(windows))]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicI64, Ordering};

/// Hands out monotonically increasing child-window ids so programs
/// that key state by window id behave sanely even against the stub.
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn fresh_id() -> i64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ── window lifecycle ────────────────────────────────────────────────

/// # Safety
/// `out_id` must be null or a valid, writable `*mut i64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_OpenChild(_title: *const u8, out_id: *mut i64) -> i64 {
    if !out_id.is_null() {
        unsafe { *out_id = fresh_id() };
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_CloseChild(_id: i64) -> i64 {
    1
}

/// # Safety
/// `title` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_SetTitle(_id: i64, _title: *const u8) -> i64 {
    1
}

// ── draw batch ──────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_BeginBatch(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_SubmitBatch() -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_Clear(_r: f64, _g: f64, _b: f64, _a: f64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_FillRect(
    _x0: f64,
    _y0: f64,
    _x1: f64,
    _y1: f64,
    _r: f64,
    _g: f64,
    _b: f64,
    _a: f64,
) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_StrokeRect(
    _x0: f64,
    _y0: f64,
    _x1: f64,
    _y1: f64,
    _thickness: f64,
    _r: f64,
    _g: f64,
    _b: f64,
    _a: f64,
) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_FillCircle(
    _cx: f64,
    _cy: f64,
    _radius: f64,
    _r: f64,
    _g: f64,
    _b: f64,
    _a: f64,
) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_DrawLine(
    _x0: f64,
    _y0: f64,
    _x1: f64,
    _y1: f64,
    _thickness: f64,
    _r: f64,
    _g: f64,
    _b: f64,
    _a: f64,
) -> i64 {
    1
}

/// # Safety
/// `text` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_DrawText(
    _text: *const u8,
    _x: f64,
    _y: f64,
    _size: f64,
    _r: f64,
    _g: f64,
    _b: f64,
    _a: f64,
) -> i64 {
    1
}

// ── event mailbox ───────────────────────────────────────────────────
//
// The stub never produces events: `iGui_NextEvent` returns 0 (no event
// available). A program that *blocks* waiting for a Quit event would
// spin; the real Cocoa backend in Phase 3 drives these from the NSEvent
// queue. Console programs and one-shot draw programs don't pump events,
// so the stub is sufficient for them.

/// # Safety
/// Every `out_*` pointer must be null or a valid, writable `*mut i64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_NextEvent(
    _out_kind: *mut i64,
    _out_child: *mut i64,
    _out_time: *mut i64,
    _out_p1: *mut i64,
    _out_p2: *mut i64,
    _out_p3: *mut i64,
    _out_p4: *mut i64,
    _timeout_ms: i64,
) -> i64 {
    0
}

/// # Safety
/// Every `out_*` pointer must be null or a valid, writable `*mut i64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_NextEventFor(
    _target_child: i64,
    _out_kind: *mut i64,
    _out_child: *mut i64,
    _out_time: *mut i64,
    _out_p1: *mut i64,
    _out_p2: *mut i64,
    _out_p3: *mut i64,
    _out_p4: *mut i64,
    _timeout_ms: i64,
) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_Quit() -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_DiscardStashedEvents() -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_FilterOnWindow(_child_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_UnfilterWindow(_child_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_ClearFilter() -> i64 {
    1
}

// ── text pane ───────────────────────────────────────────────────────

/// # Safety
/// `out_id` must be null or a valid, writable `*mut i64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_OpenText(_title: *const u8, out_id: *mut i64) -> i64 {
    if !out_id.is_null() {
        unsafe { *out_id = fresh_id() };
    }
    1
}

/// # Safety
/// `text` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextWriteStr(_id: i64, _text: *const u8) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextWriteChar(_id: i64, _codepoint: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextNewline(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextSetCursor(_id: i64, _row: i64, _col: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextClear(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextClearEol(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextClearEos(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextScrollUp(_id: i64, _n: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextSetPen(_id: i64, _fg: i64, _bg: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextResetPen(_id: i64) -> i64 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iGui_TextShowCaret(_id: i64, _visible: i64) -> i64 {
    1
}
