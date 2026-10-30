//! The BOX's JIT (design/BOX_PORT.md, section 3): MCJIT, as on the Mac, but
//! with the box's memory manager: code in mmap'd pages just above 4 GB
//! (AArch64 MCJIT code reaches its data by ADRP, +-4 GB), data (the
//! program's statics) in the run's dynamic area so that `@` of a static is
//! a RISC OS address. The large code model, so that calls reach the runtime
//! in `/init` from the code wherever it lies; every runtime builtin bound by
//! address.
//!
//! LLVM is /init's (the NCL set, linked once for every language), and so
//! are `dlopen` and `dlsym` (rosgd runtime/llvm/llvm_dl.c): the check for
//! names the JIT cannot bind asks that same `dlsym`.
//!
//! The engine lives until the run ends (`box_end_run`), when it is disposed
//! of: that deregisters the code's `.eh_frame` and unmaps the code.

use super::*;
use inkwell::llvm_sys::execution_engine::{
    LLVMAddGlobalMapping, LLVMCreateMCJITCompilerForModule, LLVMCreateSimpleMCJITMemoryManager,
    LLVMDisposeExecutionEngine, LLVMExecutionEngineRef, LLVMGetFunctionAddress,
    LLVMInitializeMCJITCompilerOptions, LLVMLinkInMCJIT, LLVMMCJITCompilerOptions,
};
use inkwell::values::AsValueRef;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Mutex;

/// The run's engine (an LLVMExecutionEngineRef) and context, as integers so
/// that the static is Send; 0 when no run holds one.
static ENGINE: Mutex<(usize, usize)> = Mutex::new((0, 0));

/// `*BCPL -d`: the JIT's sections and entry point are written out.
pub static BOX_DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn debug(s: String) {
    if BOX_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
        newbcpl_runtime::rosbox::write_str(&s);
    }
}

fn section_name(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

unsafe extern "C" {
    fn mmap(a: *mut c_void, n: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
    fn munmap(a: *mut c_void, n: usize) -> i32;
    fn mprotect(a: *mut c_void, n: usize, prot: i32) -> i32;
    fn __clear_cache(start: *mut c_char, end: *mut c_char);
    /// /init's (rosgd runtime/llvm/llvm_dl.c): what MCJIT will find
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const PROT_EXEC: i32 = 4;
const MAP_PRIVATE: i32 = 2;
const MAP_ANONYMOUS: i32 = 0x20;

// ── The memory manager ───────────────────────────────────────────────────

struct Mm {
    /// mmap'd code: (address, length)
    code: Vec<(usize, usize)>,
}

const NEAR: usize = 0x1_0000_0000;
const FAR: usize = 0x1_7000_0000;
static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(NEAR);

/// Pages for code just above 4 GB, where the kernel gives the address asked
/// for if it is free.
fn code_pages(len: usize) -> *mut c_void {
    use std::sync::atomic::Ordering::Relaxed;
    let mut at = NEXT.load(Relaxed);
    while at + len < FAR {
        let p = unsafe { mmap(at as *mut c_void, len, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0) };
        if p as isize == -1 {
            break;
        }
        if p as usize == at {
            NEXT.store((at + len + 0xFFFF) & !0xFFFF, Relaxed);
            return p;
        }
        unsafe { munmap(p, len) };
        at += 0x100_0000;
    }
    if newbcpl_runtime::rosbox::arena_on() {
        return std::ptr::null_mut();
    }
    let p = unsafe { mmap(std::ptr::null_mut(), len, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0) };
    if p as isize == -1 { std::ptr::null_mut() } else { p }
}

extern "C" fn alloc_code(opaque: *mut c_void, size: usize, align: u32, _id: u32, name: *const c_char) -> *mut u8 {
    let mm = unsafe { &mut *(opaque as *mut Mm) };
    let len = (size.max(1) + align.max(16) as usize + 0xFFF) & !0xFFF;
    let p = code_pages(len);
    if p.is_null() {
        return std::ptr::null_mut();
    }
    mm.code.push((p as usize, len));
    debug(format!("JIT code {} at &{:X} ({} bytes)\n", section_name(name), p as usize, size));
    p as *mut u8
}

extern "C" fn alloc_data(_opaque: *mut c_void, size: usize, align: u32, _id: u32, name: *const c_char, _ro: i32) -> *mut u8 {
    let align = align.max(16) as usize;
    if newbcpl_runtime::rosbox::arena_on() {
        let p = newbcpl_runtime::rosbox::arena_alloc(size.max(1), align);
        if !p.is_null() {
            unsafe { std::ptr::write_bytes(p, 0, size) };
        }
        debug(format!("JIT data {} at &{:X} ({} bytes)\n", section_name(name), p as usize, size));
        return p;
    }
    let layout = std::alloc::Layout::from_size_align(size.max(1), align).unwrap();
    unsafe { std::alloc::alloc_zeroed(layout) }
}

extern "C" fn finalize(opaque: *mut c_void, _err: *mut *mut c_char) -> i32 {
    let mm = unsafe { &mut *(opaque as *mut Mm) };
    for &(p, n) in &mm.code {
        unsafe {
            __clear_cache(p as *mut c_char, (p + n) as *mut c_char);
            mprotect(p as *mut c_void, n, PROT_READ | PROT_EXEC);
        }
    }
    0
}

extern "C" fn destroy(opaque: *mut c_void) {
    let mm = unsafe { Box::from_raw(opaque as *mut Mm) };
    for &(p, n) in &mm.code {
        unsafe { munmap(p as *mut c_void, n) };
    }
}

// ── Building the module ──────────────────────────────────────────────────

fn opt_level(n: u32) -> OptimizationLevel {
    match n {
        0 => OptimizationLevel::None,
        1 => OptimizationLevel::Less,
        2 => OptimizationLevel::Default,
        _ => OptimizationLevel::Aggressive,
    }
}

fn init_llvm() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        unsafe { LLVMLinkInMCJIT() };
        Target::initialize_native(&InitializationConfig::default()).expect("Target::initialize_native");
    });
}

fn machine(triple: &inkwell::targets::TargetTriple, opt: u32, reloc: RelocMode) -> Result<TargetMachine, String> {
    let target = Target::from_triple(triple).map_err(|e| format!("Target::from_triple: {e}"))?;
    target
        .create_target_machine(triple, "generic", "", opt_level(opt), reloc, CodeModel::Large)
        .ok_or_else(|| "create_target_machine failed".to_string())
}

/// The program and the library modules it uses, emitted and linked into
/// one LLVM module (each module's functions named `<stem>_<name>`), set for
/// `tm` and optimised.
fn linked_module<'ctx>(
    ctx: &'ctx Context,
    prog: &IrModule,
    mods: &[(String, IrModule)],
    tm: &TargetMachine,
    opt: u32,
) -> Result<Module<'ctx>, String> {
    emit::begin_run();
    let module = emit::emit(ctx, prog);
    for (stem, mod_ir) in mods {
        let m = emit::emit(ctx, mod_ir);
        for f in &mod_ir.functions {
            if let Some(fv) = m.get_function(&f.name) {
                let new_name = format!("{stem}_{}", f.name);
                unsafe {
                    inkwell::llvm_sys::core::LLVMSetValueName2(fv.as_value_ref(), new_name.as_ptr() as *const c_char, new_name.len());
                }
            }
        }
        module.link_in_module(m).map_err(|e| format!("module {stem}: {}", e.to_string()))?;
    }
    module.set_triple(&tm.get_triple());
    module.set_data_layout(&tm.get_target_data().get_data_layout());
    module.verify().map_err(|e| format!("LLVM verify: {}", e.to_string()))?;
    if opt > 0 {
        use inkwell::passes::PassBuilderOptions;
        let n = opt.min(3);
        module
            .run_passes(&format!("default<O{n}>"), tm, PassBuilderOptions::create())
            .map_err(|e| format!("run_passes O{n}: {}", e.to_string()))?;
    }
    Ok(module)
}

/// The names the module declares, uses and does not define, that neither
/// the runtime's builtins nor `dlsym` can give.
fn unbound(module: &Module<'_>, asm_proc: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut missing = Vec::new();
    let mut f = module.get_first_function();
    while let Some(func) = f {
        let name = func.get_name().to_string_lossy().into_owned();
        if func.count_basic_blocks() == 0
            && !name.starts_with("llvm.")
            && !unsafe { inkwell::llvm_sys::core::LLVMGetFirstUse(func.as_value_ref()) }.is_null()
            && !newbcpl_runtime::builtins::is_builtin(&name)
        {
            let c = CString::new(name.as_str()).unwrap_or_default();
            if asm_proc(&name) || unsafe { dlsym(std::ptr::null_mut(), c.as_ptr()) }.is_null() {
                missing.push(name);
            }
        }
        f = func.get_next_function();
    }
    missing
}

/// `*BCPL -c`: the whole back end, no run.
pub fn compile_box(prog: &IrModule, mods: &[(String, IrModule)], opt: u32) -> Result<(), String> {
    init_llvm();
    let ctx = Context::create();
    let tm = machine(&TargetMachine::get_default_triple(), opt, RelocMode::Static)?;
    let module = linked_module(&ctx, prog, mods, &tm, opt)?;
    let missing = unbound(&module, &|n| prog.is_asm_proc(n));
    if !missing.is_empty() {
        return Err(format!("no such routine: {}", missing.join(", ")));
    }
    tm.write_to_memory_buffer(&module, FileType::Object)
        .map_err(|e| format!("code generation: {}", e.to_string()))?;
    Ok(())
}

/// The host check (box/hostcheck.sh): the box's code for `triple` (as the
/// JIT makes it: the large code model) written as an object file.
pub fn emit_object_box(prog: &IrModule, mods: &[(String, IrModule)], triple: &str, out: &Path) -> Result<(), String> {
    Target::initialize_all(&InitializationConfig::default());
    let ctx = Context::create();
    let t = inkwell::targets::TargetTriple::create(triple);
    let tm = machine(&t, 2, RelocMode::Default)?;
    let module = linked_module(&ctx, prog, mods, &tm, 2)?;
    tm.write_to_file(&module, FileType::Object, out).map_err(|e| format!("write {}: {}", out.display(), e.to_string()))
}

/// Compiles the program and runs START: `Ok(Ok(result))`, or `Ok(Err(p))`
/// for the panic that ended it (a FINISH, Escape, an error), or `Err` if it
/// could not be made. `compiled` is told when the code is ready.
pub fn run_box(
    prog: &IrModule,
    mods: &[(String, IrModule)],
    opt: u32,
    compiled: &dyn Fn(),
) -> Result<std::thread::Result<i64>, String> {
    box_end_run();
    init_llvm();
    // The context lives as long as the engine (box_end_run frees it)
    let ctx: &'static Context = Box::leak(Box::new(Context::create()));
    *ENGINE.lock().unwrap() = (0, ctx as *const Context as usize);
    let tm = machine(&TargetMachine::get_default_triple(), opt, RelocMode::Static)?;
    let module = linked_module(ctx, prog, mods, &tm, opt)?;
    let missing = unbound(&module, &|n| prog.is_asm_proc(n));
    if !missing.is_empty() {
        return Err(format!("no such routine: {}", missing.join(", ")));
    }
    if module.get_function("START").is_none() {
        return Err("the program has no START".into());
    }

    let mut jo: LLVMMCJITCompilerOptions = unsafe { std::mem::zeroed() };
    unsafe { LLVMInitializeMCJITCompilerOptions(&mut jo, std::mem::size_of::<LLVMMCJITCompilerOptions>()) };
    jo.OptLevel = opt.min(3);
    jo.CodeModel = inkwell::llvm_sys::target_machine::LLVMCodeModel::LLVMCodeModelLarge;
    jo.NoFramePointerElim = 1;
    let mm = Box::into_raw(Box::new(Mm { code: Vec::new() })) as *mut c_void;
    jo.MCJMM = unsafe { LLVMCreateSimpleMCJITMemoryManager(mm, alloc_code, alloc_data, finalize, Some(destroy)) };

    let mut engine: LLVMExecutionEngineRef = std::ptr::null_mut();
    let mut err: *mut c_char = std::ptr::null_mut();
    let mref = module.as_mut_ptr();
    let rc = unsafe {
        LLVMCreateMCJITCompilerForModule(&mut engine, mref, &mut jo, std::mem::size_of::<LLVMMCJITCompilerOptions>(), &mut err)
    };
    if rc != 0 || engine.is_null() {
        let msg = if err.is_null() {
            "no engine".to_string()
        } else {
            let s = unsafe { CStr::from_ptr(err) }.to_string_lossy().into_owned();
            unsafe { inkwell::llvm_sys::core::LLVMDisposeMessage(err) };
            s
        };
        return Err(format!("the JIT could not be made: {msg}"));
    }
    ENGINE.lock().unwrap().0 = engine as usize;

    for b in newbcpl_runtime::builtins::builtin_addresses() {
        if let Some(fv) = module.get_function(b.name) {
            unsafe { LLVMAddGlobalMapping(engine, fv.as_value_ref(), b.address as *mut c_void) };
        }
    }
    // The engine owns the module now
    std::mem::forget(module);

    let start_addr = unsafe { LLVMGetFunctionAddress(engine, c"START".as_ptr()) };
    if start_addr == 0 {
        return Err("START could not be made".into());
    }
    debug(format!("START &{start_addr:X}\n"));
    compiled();
    type StartFn = unsafe extern "C-unwind" fn() -> i64;
    let start: StartFn = unsafe { std::mem::transmute(start_addr as usize) };
    Ok(std::panic::catch_unwind(|| unsafe { start() }))
}

/// The run is over: the engine (and with it the code and its unwind
/// tables) goes.
pub fn box_end_run() {
    let mut g = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    let (engine, ctx) = *g;
    *g = (0, 0);
    if engine != 0 {
        unsafe { LLVMDisposeExecutionEngine(engine as LLVMExecutionEngineRef) };
    }
    if ctx != 0 {
        drop(unsafe { Box::from_raw(ctx as *mut Context) });
    }
    NEXT.store(NEAR, std::sync::atomic::Ordering::Relaxed);
}

/// After a fault jumped out of a run.
pub fn box_recover() {
    ENGINE.clear_poison();
    box_end_run();
}
