//! `new-asm` — minimal stub for the MacBCPL (arm64) port.
//!
//! NewBCPL's original `new-asm` crate is an x86-64 (Intel-syntax) text
//! assembler used to emit the body of inline `ASM { … }` procedures as
//! an LLVM `module asm` blob. That feature is inherently x86-specific:
//! the assembly text inside a BCPL `ASM { }` block is raw Intel-syntax
//! x86, which cannot run on Apple Silicon.
//!
//! Rather than carry the full assembler, this stub preserves exactly the
//! API surface the IR (`newbcpl-ir`) and codegen (`newbcpl-llvm`) crates
//! consume — the `AsmProc` / `AsmParam` / `AsmType` / `AsmRetType` types
//! and `build_module_asm_string` — so the workspace compiles unchanged.
//! On arm64 the emitted blob is empty: the matching `declare`s still go
//! out (so call sites typecheck), but any program that actually *calls*
//! an `ASM` proc will fail at JIT symbol resolution. No example in the
//! corpus relies on inline x86 asm; if one ever does, it must be ported
//! to a real BCPL routine or to AArch64 (see the MacModula2 `new-asm`
//! AArch64 RASM encoder for the eventual native path).

/// Register class of an `ASM` procedure parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsmType {
    /// 64-bit integer word.
    Word,
    /// `f64` scalar.
    Float,
    /// `<4 x f32>` SIMD quad.
    FQuad,
    /// `<8 x f32>` SIMD oct.
    FOct,
}

/// Return register class of an `ASM` function (`Void` for `BE ASM`
/// routines that yield no value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsmRetType {
    Word,
    Float,
    FQuad,
    FOct,
    Void,
}

/// One parameter of an `ASM` procedure.
#[derive(Clone, Debug)]
pub struct AsmParam {
    pub name: String,
    pub ty: AsmType,
}

/// A lowered `ASM { … }` procedure: its name, typed parameter list,
/// return class, and the raw assembly body text.
#[derive(Clone, Debug)]
pub struct AsmProc {
    pub name: String,
    pub params: Vec<AsmParam>,
    pub return_type: AsmRetType,
    /// Raw assembly text from between the `{` and `}` of the source
    /// `ASM` block. Ignored by this stub (it is x86 Intel syntax).
    pub body: String,
}

/// Build the `module asm` blob for one `ASM` procedure.
///
/// On the arm64 port this is a no-op: the body is x86 Intel-syntax
/// assembly that cannot be emitted for Apple Silicon, so we return an
/// empty string. The caller appends this to the LLVM module's inline
/// assembly; an empty append leaves the `declare`d symbol unresolved,
/// which only matters if the program calls the `ASM` proc.
pub fn build_module_asm_string(_proc: &AsmProc) -> String {
    String::new()
}
