//! NewBCPL LLVM emit + JIT.
//!
//! Lowers `newbcpl-ir::Module` to LLVM IR via Inkwell (LLVM 22),
//! produces both the textual LLVM IR (for `dump-llvm`) and the
//! native assembly (for `dump-asm`). Targets `x86_64-pc-windows-msvc`
//! by default; the JIT entry point arrives in a follow-up.
//!
//! See `emit::emit` for the IR-to-LLVM walker.

pub mod emit;
mod jit_mm;

use std::path::Path;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

use newbcpl_ir::Module as IrModule;

/// Lex / parse / sema / lower the file, emit LLVM IR, and return
/// it as a textual artifact suitable for `dump-llvm`.
pub fn dump_llvm(path: &Path) -> String {
    match build_ir(path) {
        Ok(ir) => {
            let context = Context::create();
            let module = emit::emit(&context, &ir);
            format!(
                "newbcpl-llvm dump\ninput: {}\n\n{}",
                path.display(),
                module.print_to_string().to_string()
            )
        }
        Err(error) => format!(
            "newbcpl-llvm dump\ninput: {}\nerror: {}",
            path.display(),
            error
        ),
    }
}

/// Same pipeline as `dump_llvm`, but runs the LLVM module through a
/// `TargetMachine` to produce native assembly text.
pub fn dump_asm(path: &Path) -> String {
    match build_ir(path) {
        Ok(ir) => {
            let context = Context::create();
            let module = emit::emit(&context, &ir);

            // Initialise the AArch64 target backend — the MacBCPL port
            // targets aarch64-apple-darwin. `get_default_triple()` below
            // resolves to the host arm64 triple.
            Target::initialize_aarch64(&InitializationConfig::default());

            let triple = TargetMachine::get_default_triple();
            module.set_triple(&triple);

            let target = match Target::from_triple(&triple) {
                Ok(t) => t,
                Err(e) => {
                    return format!(
                        "newbcpl-llvm asm\ninput: {}\nfrom_triple error: {}",
                        path.display(),
                        e.to_string()
                    );
                }
            };

            let target_machine = match target.create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            ) {
                Some(tm) => tm,
                None => {
                    return format!(
                        "newbcpl-llvm asm\ninput: {}\ncreate_target_machine failed",
                        path.display()
                    );
                }
            };

            let buf = match target_machine
                .write_to_memory_buffer(&module, FileType::Assembly)
            {
                Ok(b) => b,
                Err(e) => {
                    return format!(
                        "newbcpl-llvm asm\ninput: {}\nwrite_to_memory_buffer error: {}",
                        path.display(),
                        e.to_string()
                    );
                }
            };

            let asm = String::from_utf8_lossy(buf.as_slice()).to_string();
            format!(
                "newbcpl-llvm asm\ninput: {}\ntarget: {}\n\n{}",
                path.display(),
                triple.as_str().to_string_lossy(),
                asm
            )
        }
        Err(error) => format!(
            "newbcpl-llvm asm\ninput: {}\nerror: {}",
            path.display(),
            error
        ),
    }
}

/// Build a JIT execution engine for the program at `path` and call
/// its top-level `START` routine. Builtin addresses (WRITES, WRITEN,
/// WRITEC, NEWLINE) are registered up-front so the JIT'd code can
/// reach them.
///
/// Returns the value `START` produced — typically 0 by BCPL
/// convention. Errors during compilation, linking, or execution
/// surface as `Err(String)` so the driver can print them.
///
/// Equivalent to `run_with_active_folder(path, None)` — no modules
/// are pre-loaded.
pub fn run(path: &Path) -> Result<i64, String> {
    run_with_active_folder(path, None)
}

/// Same as `run`, but first scans `modules_dir` (if `Some`) for
/// `.bcl` files and *links* each into the program's LLVM module
/// before creating the JIT engine. Module top-level functions are
/// renamed `<stem>_<name>` post-emit; after linking, every
/// cross-module call (program→module, module→module, mutual-recursive)
/// is just a normal LLVM call resolved by LLVM's linker. No
/// address-threading, no MCJIT `add_global_mapping` for module
/// functions — only for the host-process built-ins.
///
/// A missing or empty `modules_dir` is fine — no modules are loaded.
/// A single module's compile or link failure aborts the whole run
/// with a clear error.
/// Whether to wrap each top-level program run in an Objective-C autorelease
/// pool (default ON). The pool gives +0 / convenience-constructor Cocoa
/// objects a defined lifetime — valid for the run, drained at its end —
/// instead of leaking (no pool) or dangling across run-loop turns. +1 owned
/// objects (alloc/new/copy/init and BCPL `NEW`) are released
/// deterministically at their scope and never touch the pool. Turned off by
/// `newbcpl-driver --no-autorelease-pool`.
static AUTORELEASE_POOL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Set whether top-level runs are wrapped in an autorelease pool (default
/// `true`). Call before [`run_with_active_folder`] / [`run_program_ir`].
pub fn set_autorelease_pool(on: bool) {
    AUTORELEASE_POOL.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn autorelease_pool_enabled() -> bool {
    AUTORELEASE_POOL.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn run_with_active_folder(
    path: &Path,
    modules_dir: Option<&Path>,
) -> Result<i64, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("io: {e}"))?;
    let module_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let ir = build_ir_from_source_with_base(
        &source,
        module_name,
        base_dir.as_deref(),
        modules_dir,
    )?;
    run_program_ir(&ir, modules_dir)
}

/// Same as [`run_with_active_folder`] but the program's source is
/// passed in as a string instead of a file path. The active-modules
/// folder is still scanned as files. Used by the GUI driver to JIT
/// the current bedit buffer without round-tripping through disk —
/// the user's unsaved edits run immediately on Ctrl+R / Program ▸ Run.
///
/// `module_name` is the name embedded in the IR (visible in dump-ir,
/// dump-llvm output). The driver passes the launch-time file stem so
/// the IR looks the same as a file-based run.
pub fn run_source_with_active_folder(
    source: &str,
    module_name: &str,
    modules_dir: Option<&Path>,
) -> Result<i64, String> {
    // No base_dir — the source isn't backed by a file the GUI knows
    // about. `GET "name"` still works as long as `name` lives in
    // modules-active.
    let ir = build_ir_from_source_with_base(source, module_name, None, modules_dir)?;
    run_program_ir(&ir, modules_dir)
}

/// Heavy work shared by [`run_with_active_folder`] and
/// [`run_source_with_active_folder`]: spin up an LLVM Context, emit
/// the program IR, link every module file's IR into it, JIT, and
/// invoke `START`.
/// Obj-C method type encoding `"<ret>@:<args>"` for a JIT'd BCPL method
/// IMP whose LLVM signature is `(self, _cmd, args…)`. Every BCPL method
/// param is an i64 word (`"q"`); the return may be `f64` (`"d"`), a
/// pointer (`"@"`), void (`"v"`), or i64 (`"q"` — including routines,
/// which physically `ret i64 0`). `@:` are the implicit self/_cmd.
fn objc_method_type_encoding(fv: inkwell::values::FunctionValue) -> String {
    use inkwell::types::BasicTypeEnum;
    let ret = match fv.get_type().get_return_type() {
        None => "v".to_string(),
        Some(BasicTypeEnum::FloatType(_)) => "d".to_string(),
        Some(BasicTypeEnum::PointerType(_)) => "@".to_string(),
        Some(_) => "q".to_string(),
    };
    let nargs = (fv.count_params() as usize).saturating_sub(2); // minus self, _cmd
    let mut enc = ret;
    enc.push_str("@:");
    for _ in 0..nargs {
        enc.push('q');
    }
    enc
}

fn run_program_ir(ir: &IrModule, modules_dir: Option<&Path>) -> Result<i64, String> {
    let context = Context::create();

    // Install the signal-safe crash handler (idempotent). On a fatal
    // signal — including the SIGABRT from a Rust panic in a runtime
    // helper that can't unwind through a JIT frame — it writes an
    // annotated backtrace (BCPL routine names via the JIT registry) to
    // stderr, then re-raises so the OS still produces its crash report.
    // This is the macOS replacement for the upstream Windows SEH path.
    newbcpl_runtime::crash::bcpl_install_crash_handler();

    // Begin a new JIT run: installs a unique per-run class-name prefix
    // so the Obj-C classes this run registers (never disposed — the
    // engine is leaked) don't collide with classes from a prior run of
    // a different program in the same process. emit_new bakes the same
    // prefix into its `bcpl_objc_new` calls, so they agree.
    emit::begin_run();

    // ─── Program emit (first, so we have the host module to link into) ─
    let module = emit::emit(&context, ir);

    // ─── Module phase ─────────────────────────────────────────────
    // For every *.bcl in modules_dir (alphabetical), parse → sema →
    // IR → LLVM emit into a fresh Module<'ctx>, rename top-level
    // functions with the module prefix, then link into the program
    // module. After linking, the program module contains every
    // exported function as a real definition.
    //
    // Accumulate all the modules' IR layouts (class vtable
    // descriptions) into `all_layouts` so the vtable-patch loop
    // below can fix up vtables emitted by modules too. Modules in
    // v0 are class-free in practice, but the wiring is here so the
    // first class-shipping module doesn't break things silently.
    let mut all_layouts = ir.layouts.clone();
    if let Some(dir) = modules_dir {
        if dir.is_dir() {
            let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
                .map_err(|e| format!("io: read_dir {}: {e}", dir.display()))?
                .filter_map(|r| r.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "bcl"))
                .collect();
            paths.sort();
            for mpath in &paths {
                let stem = mpath
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| {
                        format!("module path has no stem: {}", mpath.display())
                    })?;
                let mod_ir = build_ir(mpath)?;
                let mod_llvm = emit::emit(&context, &mod_ir);

                // Rename top-level user functions in this module
                // to `<stem>_<name>` before linking. LLVM updates
                // intra-module call sites automatically (calls hold
                // a FunctionValue reference, not a name string).
                use inkwell::llvm_sys::core::LLVMSetValueName2;
                use inkwell::values::AsValueRef;
                for ir_fn in &mod_ir.functions {
                    if let Some(fv) = mod_llvm.get_function(&ir_fn.name) {
                        let new_name = format!("{}_{}", stem, ir_fn.name);
                        unsafe {
                            LLVMSetValueName2(
                                fv.as_value_ref(),
                                new_name.as_ptr() as *const i8,
                                new_name.len(),
                            );
                        }
                    }
                }

                // Link the module into the program. After this the
                // mod_llvm is consumed; its functions live in the
                // program module's symbol table.
                module
                    .link_in_module(mod_llvm)
                    .map_err(|e| format!("link module {stem}: {}", e.to_string()))?;

                all_layouts.extend(mod_ir.layouts.iter().cloned());
                eprintln!(
                    "[loader] module {stem}: {} functions linked",
                    mod_ir.functions.len()
                );
            }
        }
    }

    // ─── JIT setup ────────────────────────────────────────────────
    //
    // Catch unbound externs *before* code emission. Any function the
    // module declares without a body (linkage = external, no entry
    // basic block) and that isn't a known runtime builtin would
    // otherwise be called at address 0 and segfault. Surface it as a
    // clean diagnostic instead. With module linking, cross-module
    // references now have bodies and pass the check.
    let mut missing: Vec<String> = Vec::new();
    let mut fopt = module.get_first_function();
    while let Some(f) = fopt {
        if f.count_basic_blocks() == 0 {
            let name = f.get_name().to_string_lossy().into_owned();
            // Skip LLVM intrinsics (`llvm.memset.*` etc.) — those
            // are resolved by LLVM itself, not by our table.
            if !name.starts_with("llvm.")
                && !newbcpl_runtime::builtins::is_builtin(&name)
                && !ir.is_asm_proc(&name)
            {
                missing.push(name);
            }
        }
        fopt = f.get_next_function();
    }
    if !missing.is_empty() {
        return Err(format!("missing builtin: {}", missing.join(", ")));
    }

    // Build the MCJIT engine through llvm-sys so we can pass a
    // custom memory manager that captures `.pdata`/`.xdata`/`.text`
    // and registers Windows SEH unwind tables on finalize. inkwell's
    // `create_jit_execution_engine` doesn't expose the `MCJMM` slot
    // on `LLVMMCJITCompilerOptions`, so we drop one layer down and
    // hold a raw `LLVMExecutionEngineRef`.
    //
    // Trade-off: we lose the `inkwell::ExecutionEngine` wrapper and
    // do builtin binding / function-pointer lookup via the LLVM C
    // API directly (`LLVMAddGlobalMapping`, `LLVMGetFunctionAddress`,
    // `LLVMGetPointerToGlobal`). Same operations, lower-level surface.
    //
    // Without this manager, JIT'd code carries unwind tables LLVM
    // emitted (uwtable=2 on every function in emit.rs) but the OS
    // unwinder can't see them — a panic from a runtime helper
    // escapes through MSVC SEH 0xE06D7363 and aborts the process.
    use inkwell::llvm_sys::execution_engine::{
        LLVMAddGlobalMapping, LLVMCreateMCJITCompilerForModule,
        LLVMExecutionEngineRef, LLVMGetFunctionAddress, LLVMGetGlobalValueAddress,
        LLVMGetPointerToGlobal, LLVMInitializeMCJITCompilerOptions,
        LLVMLinkInMCJIT, LLVMMCJITCompilerOptions,
    };
    use inkwell::values::AsValueRef;
    use std::ffi::CString;

    // First-time MCJIT setup. Calling these more than once is a
    // documented no-op. inkwell's `create_jit_execution_engine` does
    // these internally; doing it ourselves means we have to do these.
    use std::sync::Once;
    static MCJIT_INIT: Once = Once::new();
    MCJIT_INIT.call_once(|| {
        unsafe {
            LLVMLinkInMCJIT();
        }
        Target::initialize_native(&InitializationConfig::default())
            .expect("Target::initialize_native");
    });

    let mut opts: LLVMMCJITCompilerOptions = unsafe { std::mem::zeroed() };
    unsafe {
        LLVMInitializeMCJITCompilerOptions(
            &mut opts,
            std::mem::size_of::<LLVMMCJITCompilerOptions>(),
        );
    }
    opts.OptLevel = OptimizationLevel::Default as u32;
    // Force frame pointers in the MCJIT target machine. Without this,
    // MCJIT elides them for JIT'd routines (it ignores the per-function
    // "frame-pointer"="all" attribute), leaving the arm64 x29 chain
    // broken — so the BRK / crash-handler fp-walk can't see BCPL frames.
    // With it, every JIT'd routine sets `mov x29,sp` and is a proper
    // link in the chain.
    opts.NoFramePointerElim = 1;
    // On Windows we install our custom MCJIT memory manager so JIT'd
    // code's `.pdata`/`.xdata` get registered with the OS SEH unwinder
    // (`RtlAddFunctionTable`). On macOS arm64 we leave `MCJMM` null so
    // MCJIT uses its default memory manager, which registers DWARF
    // `.eh_frame` for the JIT'd code — that's what lets a Rust panic in
    // a runtime helper unwind cleanly back through the JIT frames.
    // (Same split as MacModula2's newm2-llvm.)
    #[cfg(windows)]
    {
        opts.MCJMM = unsafe { jit_mm::make_mm() };
    }

    let mut engine: LLVMExecutionEngineRef = std::ptr::null_mut();
    let mut err_msg: *mut std::ffi::c_char = std::ptr::null_mut();
    // `LLVMCreateMCJITCompilerForModule` consumes the module; we
    // can't keep using the inkwell `Module` after this. inkwell's
    // wrapper is non-owning so dropping it is fine — the engine
    // owns the underlying module from here on.
    let rc = unsafe {
        LLVMCreateMCJITCompilerForModule(
            &mut engine,
            module.as_mut_ptr(),
            &mut opts,
            std::mem::size_of::<LLVMMCJITCompilerOptions>(),
            &mut err_msg,
        )
    };
    if rc != 0 || engine.is_null() {
        let msg = if err_msg.is_null() {
            "LLVMCreateMCJITCompilerForModule failed with no message".to_string()
        } else {
            let s = unsafe { std::ffi::CStr::from_ptr(err_msg) }
                .to_string_lossy()
                .into_owned();
            unsafe { inkwell::llvm_sys::core::LLVMDisposeMessage(err_msg) };
            s
        };
        return Err(format!("LLVMCreateMCJITCompilerForModule: {msg}"));
    }

    // Register every builtin's host-process address with the JIT by
    // symbol name. We can't rely on the dynamic linker finding them —
    // this binary is the JIT host, so we hand the addresses over
    // directly via `LLVMAddGlobalMapping`.
    for builtin in newbcpl_runtime::builtins::builtin_addresses() {
        if let Some(fv) = module.get_function(builtin.name) {
            unsafe {
                LLVMAddGlobalMapping(
                    engine,
                    fv.as_value_ref(),
                    builtin.address as *mut std::ffi::c_void,
                );
            }
        }
    }

    // ─── Objective-C class registrar ────────────────────────────────
    //
    // Each BCPL class becomes a REAL Obj-C class (the MacModula2 object
    // model). For every class layout, at JIT finalize and before START:
    //   objc_allocateClassPair(super, "<prefix><Class>")
    //   class_addIvar("__bcpl_<Class>", own-fields-size)   [per-class]
    //   class_addMethod(sel("<m>"), <JIT'd Class_m IMP>, <type-enc>)
    //   objc_registerClassPair
    // Method IMP addresses resolve via LLVMGetPointerToGlobal here — the
    // exact mechanism the old vtable patch loop relied on. NEW Class
    // (emit_new) then does [[Class alloc] init]; dispatch is
    // objc_msgSend; fields live in the per-class ivars.
    {
        use core::ffi::c_void;
        use inkwell::values::AsValueRef;
        use newbcpl_runtime::objc as rt;
        use std::collections::HashMap;

        let cstr = |s: &str| CString::new(s).expect("class/selector name has interior NUL");

        // Register base-before-subclass: objc_allocateClassPair needs a
        // registered superclass. all_layouts is alphabetical, so sort by
        // inheritance depth (0 = no BCPL parent).
        let by_name: HashMap<&str, &newbcpl_sema::ClassLayout> = all_layouts
            .iter()
            .map(|l| (l.class_name.as_str(), l))
            .collect();
        fn depth(
            name: &str,
            by_name: &HashMap<&str, &newbcpl_sema::ClassLayout>,
            guard: usize,
        ) -> usize {
            if guard > 1024 {
                return guard; // cyclic EXTENDS — bail
            }
            match by_name.get(name).and_then(|l| l.extends.as_deref()) {
                Some(parent) if by_name.contains_key(parent) => {
                    1 + depth(parent, by_name, guard + 1)
                }
                _ => 0,
            }
        }
        let mut order: Vec<&newbcpl_sema::ClassLayout> = all_layouts.iter().collect();
        order.sort_by_key(|l| depth(&l.class_name, &by_name, 0));

        let prefix = emit::class_prefix();
        let nsobject = cstr("NSObject");

        for layout in order {
            let objc_name = format!("{prefix}{}", layout.class_name);
            let cname = cstr(&objc_name);

            // Re-entry guard: START may re-run within this engine and
            // re-enter the registrar; skip an already-registered class.
            let existing =
                unsafe { rt::bcpl_objc_get_class(cname.as_ptr() as *const u8) };
            if !existing.is_null() {
                continue;
            }

            // Superclass: a BCPL parent (prefixed) if present, else a
            // real Cocoa class of that name, else NSObject.
            let super_cls: *mut c_void = match &layout.extends {
                Some(base) => {
                    let pb = cstr(&format!("{prefix}{base}"));
                    let s = unsafe { rt::bcpl_objc_get_class(pb.as_ptr() as *const u8) };
                    if !s.is_null() {
                        s
                    } else {
                        let rb = cstr(base);
                        let s2 =
                            unsafe { rt::bcpl_objc_get_class(rb.as_ptr() as *const u8) };
                        if !s2.is_null() {
                            s2
                        } else {
                            unsafe {
                                rt::bcpl_objc_get_class(nsobject.as_ptr() as *const u8)
                            }
                        }
                    }
                }
                None => unsafe {
                    rt::bcpl_objc_get_class(nsobject.as_ptr() as *const u8)
                },
            };

            let cls = unsafe {
                rt::bcpl_objc_allocate_class(super_cls, cname.as_ptr() as *const u8)
            };
            if cls.is_null() {
                continue; // allocation failed (name collision, etc.)
            }

            // One ivar for this class's OWN fields (skip if none —
            // class_addIvar rejects size 0).
            if layout.own_fields_size > 0 {
                let ivar = cstr(&format!("__bcpl_{}", layout.class_name));
                let enc = cstr(&format!("[{}c]", layout.own_fields_size));
                unsafe {
                    rt::bcpl_objc_add_ivar(
                        cls,
                        ivar.as_ptr() as *const u8,
                        layout.own_fields_size as u64,
                        3, // 2^3 = 8-byte alignment
                        enc.as_ptr() as *const u8,
                    );
                }
            }

            // Methods DEFINED BY this class become Obj-C methods. (An
            // inherited method's defining_class != this class — it was
            // installed when its owner was registered, and Obj-C
            // dispatch finds it up the chain.)
            for entry in &layout.vtable {
                let Some(owner) = entry.defining_class.as_deref() else {
                    continue;
                };
                if owner != layout.class_name {
                    continue;
                }
                let method_symbol = format!("{owner}_{}", entry.method_name);
                let Some(fv) = module.get_function(&method_symbol) else {
                    continue;
                };
                let imp =
                    unsafe { LLVMGetPointerToGlobal(engine, fv.as_value_ref()) }
                        as *mut c_void;
                if imp.is_null() {
                    continue;
                }
                // Mangled selector (bcpl_<method>) — must match the
                // dispatch site in emit_objc_dispatch.
                let sel_name = cstr(&emit::objc_selector(&entry.method_name));
                let sel = unsafe { rt::bcpl_objc_sel(sel_name.as_ptr() as *const u8) };
                let enc = cstr(&objc_method_type_encoding(fv));
                unsafe {
                    rt::bcpl_objc_add_method(
                        cls,
                        sel,
                        imp,
                        enc.as_ptr() as *const u8,
                    );
                }
            }

            // Lifecycle no-ops: a root BCPL class with no user CREATE /
            // RELEASE still needs to respond to those selectors (e.g.
            // an explicit `obj.RELEASE()`), since the mangled selector
            // isn't an NSObject method. Bind undefined CREATE/RELEASE
            // slots to the runtime no-op (returns 0). Subclasses inherit
            // these; a subclass that defines its own overrides above.
            if layout.extends.is_none() {
                let noop = newbcpl_runtime::builtins::__newbcpl_default_method
                    as *const () as *mut c_void;
                for m in ["CREATE", "RELEASE"] {
                    let undefined = layout
                        .vtable
                        .iter()
                        .find(|v| v.method_name == m)
                        .map(|v| v.defining_class.is_none())
                        .unwrap_or(true);
                    if undefined {
                        let sel_name = cstr(&emit::objc_selector(m));
                        let sel =
                            unsafe { rt::bcpl_objc_sel(sel_name.as_ptr() as *const u8) };
                        let enc = cstr("q@:");
                        unsafe {
                            rt::bcpl_objc_add_method(
                                cls,
                                sel,
                                noop,
                                enc.as_ptr() as *const u8,
                            );
                        }
                    }
                }
            }

            unsafe { rt::bcpl_objc_register_class(cls) };
        }
    }

    // Locate START. Every BCPL program declares one; if it isn't
    // there, the program is malformed for execution purposes.
    let _start_fn = module
        .get_function("START")
        .ok_or_else(|| "no START function declared".to_string())?;

    // Resolve START's compiled address via `LLVMGetFunctionAddress`
    // — this is what actually triggers code emission and finalize on
    // the engine, which in turn fires the custom memory manager's
    // `finalize_memory` callback and therefore the SEH registration.
    // Without calling this, the JIT'd code would never become
    // executable.
    let start_cname = CString::new("START").expect("no NUL in START");
    let start_addr = unsafe { LLVMGetFunctionAddress(engine, start_cname.as_ptr()) };
    if start_addr == 0 {
        return Err("LLVMGetFunctionAddress(START) returned 0".to_string());
    }

    // Walk every function in the module and register its compiled
    // address with the runtime's JIT-symbol registry. BRK's stack
    // walk reads this to map RIPs back to BCPL routine names.
    //
    // We do this *after* resolving START because START's address
    // resolution is what triggers MCJIT finalize — at that point
    // every function in the module has a stable code-section
    // address. Functions emitted with no body (extern declarations
    // for runtime builtins) return address 0 from
    // `LLVMGetFunctionAddress` and are skipped.
    let mut fopt = module.get_first_function();
    while let Some(f) = fopt {
        let name = f.get_name().to_string_lossy().into_owned();
        if !name.is_empty() && f.get_first_basic_block().is_some() {
            let cname = CString::new(name.as_str()).ok();
            if let Some(cname) = cname {
                let addr = unsafe { LLVMGetFunctionAddress(engine, cname.as_ptr()) };
                if addr != 0 {
                    newbcpl_runtime::brk::register_jit_symbol(addr, &name);
                }
            }
        }
        fopt = f.get_next_function();
    }
    // Freeze + publish the JIT symbol table so the signal handler's
    // backtrace can name BCPL routines with a lock-free lookup.
    newbcpl_runtime::crash::bcpl_finalize_jit_symbols();

    // (The old name-keyed (vtable, method_names) registration loop is
    // gone: dispatch is objc_msgSend now, so __newbcpl_lookup_method
    // and the @Class.vtable / @Class.method_names globals are unused.)

    // Leak the engine. Drop would call LLVMDisposeExecutionEngine,
    // tearing down JIT'd code the host keeps forever — leaking is the
    // right contract; module-retirement would pair this with explicit
    // teardown, but we don't have retirement yet.
    let engine_box = Box::new(engine);
    let _ = Box::leak(engine_box);

    // Safety: START takes no args and returns i64 by the BCPL-routine
    // ABI convention. Marked `extern "C-unwind"` so a Rust panic
    // raised in a runtime helper inside this call propagates through
    // the JIT frame back to the host's `catch_unwind` boundary (the
    // unwind info LLVM emitted via `uwtable=2` is registered with the
    // OS by our custom memory manager's `finalize_memory`).
    type StartFn = unsafe extern "C-unwind" fn() -> i64;
    let start: StartFn = unsafe { std::mem::transmute(start_addr as *const ()) };
    // Wrap in `catch_unwind` so a panic from inside the JIT frame is
    // turned into a clean Err for the host — `newbcpl-driver run`
    // turns this into a non-zero exit + diagnostic, `gui` lets the
    // editor keep running. `AssertUnwindSafe` because a function
    // pointer carries no `UnwindSafe` evidence on its own; we're
    // promising the JIT'd code doesn't leave shared state in a
    // logically-broken state (it can't — START is the root call).
    // Stack state unwinds cleanly thanks to the SEH machinery the
    // memory manager registers.
    //
    // While the catch is in flight we swap in a no-op panic hook so
    // the default Rust hook doesn't dump a "thread panicked at..."
    // line to stderr for an *expected* unwind. The original hook is
    // restored on the way out. Set NEWBCPL_LOG_JIT_PANICS=1 to keep
    // the default hook for debugging.
    // Open an autorelease pool around the run (default on) so +0 /
    // convenience-constructor Cocoa objects are released when it drains at
    // the end of the run, rather than leaking with no pool in place. A GUI
    // program's `[app run]` loop installs its own per-turn pools; this is the
    // outer backstop. Disabled by `--no-autorelease-pool` (a null token from
    // a disabled pool or unresolved symbol makes `pop` a no-op).
    let pool = if autorelease_pool_enabled() {
        newbcpl_runtime::objc::autorelease_pool_push()
    } else {
        std::ptr::null_mut()
    };
    let outcome = run_with_quiet_panic_hook(|| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { start() }))
    });
    // Drain the pool whether START returned or unwound cleanly. (A hard abort
    // — a runtime-helper panic that can't unwind the JIT frame — exits the
    // process before here, and the OS reclaims everything.)
    newbcpl_runtime::objc::autorelease_pool_pop(pool);
    match outcome {
        Ok(value) => Ok(value),
        Err(payload) => Err(format!("JIT panic: {}", panic_payload_to_string(payload))),
    }
}

/// Run `body` (the `catch_unwind` boundary), keeping the default panic
/// hook so a runtime-helper panic's message reaches stderr.
///
/// On macOS arm64 a Rust panic raised in a runtime helper called from
/// JIT'd code cannot unwind through the JIT frame (MCJIT doesn't
/// register usable DWARF unwind info), so `catch_unwind` never recovers
/// — the panic runtime aborts (SIGABRT). That's intentional on this
/// fork: the default hook prints "thread … panicked at … <message>",
/// then the signal-safe crash handler catches the abort and writes an
/// annotated backtrace. So we do NOT silence the hook (unlike the
/// upstream Windows path, which could unwind cleanly and recover).
fn run_with_quiet_panic_hook<F, R>(body: F) -> R
where
    F: FnOnce() -> R,
{
    body()
}

/// Best-effort conversion of a `catch_unwind` payload to a human
/// message. `panic!("…")` and `panic_any(String)` are the common
/// cases. Other payload types fall through to a generic marker.
fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

fn build_ir(path: &Path) -> Result<IrModule, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("io: {e}"))?;
    let module_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");
    let base_dir = path.parent().map(|p| p.to_path_buf());
    build_ir_from_source_with_base(&source, module_name, base_dir.as_deref(), None)
}

/// Ahead-of-time compile the BCPL source at `path` to a relocatable object at
/// `out_obj`. The object holds the program's code plus a C `main` that installs
/// the crash handler, wraps the run in an autorelease pool, and calls `START`.
/// The driver links it against `libnewbcpl_runtime.a` + system frameworks to
/// produce a standalone Mach-O executable.
///
/// Increment 1: single-file console programs (no `modules-active` linking, no
/// Cocoa class registrar). Those follow.
pub fn emit_aot_object(path: &Path, out_obj: &Path) -> Result<(), String> {
    let ir = build_ir(path)?;
    let context = Context::create();
    let module = emit::emit(&context, &ir);
    emit_aot_main(&context, &module);
    write_object(&module, out_obj)
}

/// Synthesize `int main()`: `bcpl_install_crash_handler();` then the run wrapped
/// in an autorelease pool around `START()`, mirroring the JIT `run_program_ir`
/// startup. The pool token is a Word (pointer ≡ i64 in a register), like the
/// arena handle.
fn emit_aot_main<'ctx>(context: &'ctx Context, module: &Module<'ctx>) {
    let i32t = context.i32_type();
    let i64t = context.i64_type();
    let voidt = context.void_type();

    let mut ext = |name: &str, ty| {
        module
            .get_function(name)
            .unwrap_or_else(|| module.add_function(name, ty, Some(Linkage::External)))
    };
    let install = ext("bcpl_install_crash_handler", voidt.fn_type(&[], false));
    let pool_push = ext("bcpl_autorelease_pool_push", i64t.fn_type(&[], false));
    let pool_pop = ext("bcpl_autorelease_pool_pop", voidt.fn_type(&[i64t.into()], false));
    // START is defined by the program's codegen (a BCPL routine returning WORD).
    let start = ext("START", i64t.fn_type(&[], false));

    let main_fn = module.add_function("main", i32t.fn_type(&[], false), None);
    let builder = context.create_builder();
    let bb = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(bb);
    builder.build_call(install, &[], "").expect("call crash handler");
    let tok = match builder
        .build_call(pool_push, &[], "pool")
        .expect("call pool push")
        .try_as_basic_value()
    {
        inkwell::values::ValueKind::Basic(v) => v,
        inkwell::values::ValueKind::Instruction(_) => {
            unreachable!("bcpl_autorelease_pool_push returns a value")
        }
    };
    builder.build_call(start, &[], "").expect("call START");
    builder
        .build_call(pool_pop, &[tok.into()], "")
        .expect("call pool pop");
    builder
        .build_return(Some(&i32t.const_zero()))
        .expect("return from main");
}

/// Emit `module` as a native relocatable object at `out_obj` for
/// aarch64-apple-darwin, PIC (so it links into a PIE executable).
fn write_object(module: &Module<'_>, out_obj: &Path) -> Result<(), String> {
    Target::initialize_aarch64(&InitializationConfig::default());
    let triple = TargetMachine::get_default_triple();
    module.set_triple(&triple);
    let target = Target::from_triple(&triple).map_err(|e| format!("from_triple: {}", e.to_string()))?;
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| "create_target_machine failed".to_string())?;
    tm.write_to_file(module, FileType::Object, out_obj)
        .map_err(|e| format!("write object {}: {}", out_obj.display(), e.to_string()))
}

fn build_ir_from_source(source: &str, module_name: &str) -> Result<IrModule, String> {
    build_ir_from_source_with_base(source, module_name, None, None)
}

/// As `build_ir_from_source`, plus the base directory `GET` directives
/// should resolve relative to. `modules_dir` is the secondary search
/// location — `GET "geom"` first looks for a sibling file, then falls
/// back to `modules_dir/geom.bcl` so a module file pulls double duty
/// as both a runtime symbol target and a compile-time header.
fn build_ir_from_source_with_base(
    source: &str,
    module_name: &str,
    base_dir: Option<&Path>,
    modules_dir: Option<&Path>,
) -> Result<IrModule, String> {
    let program = newbcpl_parser::parse_source(source)
        .map_err(|e| format!("parse: {}", e.render()))?;
    let expanded = expand_gets(program, base_dir, modules_dir)?;
    let sema = newbcpl_sema::analyze(&expanded);
    if !sema.errors.is_empty() {
        // Sema errors are hard — visibility violations, etc. Don't
        // proceed to IR/codegen with broken access control. Report
        // the first; render the rest in a follow-on summary so users
        // who want full context can see it.
        let mut msg = format!("sema: {}", sema.errors[0].render());
        if sema.errors.len() > 1 {
            msg.push_str(&format!(
                "\nsema: ({} more error{})",
                sema.errors.len() - 1,
                if sema.errors.len() == 2 { "" } else { "s" }
            ));
        }
        return Err(msg);
    }
    Ok(newbcpl_ir::lower(&expanded, &sema, module_name))
}

/// Walk a `Program` and replace each `Decl::Get { path, .. }` with
/// the declarations of the file it names, recursing through nested
/// GETs. Three resolution rules, tried in order:
///
///   1. **Absolute path** — used verbatim.
///   2. **Sibling file** — relative to `base_dir` (the source file's
///      directory). With the `.bcl` extension added if absent.
///   3. **Modules-active fallback** — `modules_dir/<name>.bcl`. This
///      is the bridge that lets a module file pull double duty as a
///      header. `GET "geom"` from anywhere imports the declarations
///      of `modules-active/geom.bcl`; `geom`'s runtime functions are
///      still linked separately by the module loader.
///
/// Cycle detection via a depth limit (`MAX_GET_DEPTH`); a circular
/// include errors with a clear diagnostic rather than recursing
/// forever.
fn expand_gets(
    program: newbcpl_parser::Program,
    base_dir: Option<&Path>,
    modules_dir: Option<&Path>,
) -> Result<newbcpl_parser::Program, String> {
    fn go(
        prog: newbcpl_parser::Program,
        base_dir: Option<&Path>,
        modules_dir: Option<&Path>,
        depth: u32,
    ) -> Result<newbcpl_parser::Program, String> {
        const MAX_GET_DEPTH: u32 = 32;
        if depth > MAX_GET_DEPTH {
            return Err(format!(
                "GET nesting exceeded {MAX_GET_DEPTH} levels — likely a cyclic include"
            ));
        }
        let mut out_items: Vec<newbcpl_parser::Decl> = Vec::with_capacity(prog.items.len());
        for item in prog.items {
            if let newbcpl_parser::Decl::Get(get) = &item {
                let resolved = resolve_get(&get.path, base_dir, modules_dir)?;
                let included_source = std::fs::read_to_string(&resolved).map_err(|e| {
                    format!(
                        "GET {:?}: io reading {}: {e}",
                        get.path,
                        resolved.display()
                    )
                })?;
                let included = newbcpl_parser::parse_source(&included_source).map_err(|e| {
                    format!(
                        "GET {:?}: parse {}: {}",
                        get.path,
                        resolved.display(),
                        e.render()
                    )
                })?;
                // Recurse with the new file's directory as the next base.
                let nested_base = resolved.parent().map(|p| p.to_path_buf());
                let expanded = go(included, nested_base.as_deref(), modules_dir, depth + 1)?;
                out_items.extend(expanded.items);
                continue;
            }
            out_items.push(item);
        }
        Ok(newbcpl_parser::Program {
            items: out_items,
            ..prog
        })
    }
    go(program, base_dir, modules_dir, 0)
}

/// Resolve a `GET "name"` to a concrete file path. See `expand_gets`
/// for the resolution order.
fn resolve_get(
    requested: &str,
    base_dir: Option<&Path>,
    modules_dir: Option<&Path>,
) -> Result<std::path::PathBuf, String> {
    let req = Path::new(requested);
    if req.is_absolute() && req.is_file() {
        return Ok(req.to_path_buf());
    }
    // Helper: try `dir/name`, `dir/name.bcl`, and (if `name` had a
    // different extension like `.h`) `dir/<stem>.bcl`. Classic BCPL
    // programs write `GET "libhdr.h"`; our adapter file is
    // `libhdr.bcl`, and the `.h → .bcl` swap lets the corpus
    // resolve to it without rewriting the source.
    let try_in = |dir: &Path| -> Option<std::path::PathBuf> {
        let direct = dir.join(req);
        if direct.is_file() {
            return Some(direct);
        }
        if req.extension().is_none() {
            let with_ext = dir.join(format!("{requested}.bcl"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        } else if let Some(stem) = req.file_stem().and_then(|s| s.to_str()) {
            // Strip any existing extension and try .bcl.
            let swapped = dir.join(format!("{stem}.bcl"));
            if swapped.is_file() {
                return Some(swapped);
            }
        }
        None
    };
    if let Some(base) = base_dir {
        if let Some(p) = try_in(base) {
            return Ok(p);
        }
    }
    if let Some(modules) = modules_dir {
        if let Some(p) = try_in(modules) {
            return Ok(p);
        }
    }
    let where_searched = match (base_dir, modules_dir) {
        (Some(b), Some(m)) => format!("base={} or modules-active={}", b.display(), m.display()),
        (Some(b), None) => format!("base={}", b.display()),
        (None, Some(m)) => format!("modules-active={}", m.display()),
        (None, None) => "no base or modules-active directory configured".to_string(),
    };
    Err(format!(
        "GET {requested:?}: file not found ({where_searched})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    fn emit_text(source: &str) -> String {
        let program = newbcpl_parser::parse_source(source).expect("parse");
        let sema = newbcpl_sema::analyze(&program);
        let ir = newbcpl_ir::lower(&program, &sema, "test");
        let context = Context::create();
        let module = emit::emit(&context, &ir);
        module.print_to_string().to_string()
    }

    #[test]
    fn empty_routine_emits_function_with_ret_zero() {
        let text = emit_text("LET S() BE { }");
        assert!(text.contains("define i64 @S()"));
        assert!(text.contains("ret i64 0"));
    }

    #[test]
    fn function_returning_int_literal() {
        let text = emit_text("LET answer() = 42");
        assert!(text.contains("define i64 @answer()"));
        assert!(text.contains("ret i64 42"));
    }

    #[test]
    fn function_with_int_param_and_arithmetic() {
        let text = emit_text("LET inc(x) = x + 1");
        // Parameter is i64; body is alloca + store + load + add + ret.
        assert!(text.contains("define i64 @inc(i64"));
        assert!(text.contains("add i64"));
    }

    #[test]
    fn float_function_returns_double() {
        let text = emit_text("LET pi() = 3.14");
        assert!(text.contains("define double @pi()"));
    }

    #[test]
    fn extern_writes_declared_with_pointer_arg() {
        let text = emit_text("LET S() BE { WRITES(\"hi*N\") }");
        // WRITES gets declared on first call.
        assert!(text.contains("declare i64 @WRITES("));
        // The `hi*N` literal is cooked to `hi\n` and stored in a
        // global string.
        assert!(text.contains("hi\\0A"));
    }

    #[test]
    fn if_else_emits_three_blocks_and_branch() {
        let text = emit_text("LET S(x) BE { IF x = 0 THEN f() ELSE g() }");
        assert!(text.contains("br i1"));
        assert!(text.contains("if.then"));
        assert!(text.contains("if.else"));
        assert!(text.contains("if.end"));
    }

    #[test]
    fn relational_results_zero_extend_to_word() {
        let text = emit_text("LET cmp(a, b) = a < b");
        assert!(text.contains("icmp slt"));
        assert!(text.contains("zext i1"));
    }

    // ─── loops, switchon, GEP, lane extract ─────────────────────

    #[test]
    fn while_loop_emits_loop_blocks() {
        let text = emit_text("LET S(n) BE { WHILE n < 10 DO n := n + 1 }");
        assert!(text.contains("while.header"));
        assert!(text.contains("while.body"));
        assert!(text.contains("while.end"));
        assert!(text.contains("br i1"));
    }

    #[test]
    fn for_loop_emits_canonical_cfg() {
        let text = emit_text("LET S() BE { FOR i = 1 TO 10 DO f() }");
        assert!(text.contains("for.header"));
        assert!(text.contains("for.body"));
        assert!(text.contains("for.incr"));
        assert!(text.contains("for.end"));
    }

    #[test]
    fn valof_with_resultis_threads_through() {
        let text = emit_text(
            "LET sum(n) = VALOF $(\n LET acc = 0\n FOR i = 1 TO n DO acc := acc + i\n RESULTIS acc\n$)",
        );
        // The function returns the loaded VALOF result.
        assert!(text.contains("valof.result"));
        assert!(text.contains("valof.end"));
        assert!(text.contains("ret i64"));
        // FOR loop bodies are present too.
        assert!(text.contains("for.header"));
    }

    #[test]
    fn switchon_emits_llvm_switch() {
        let text = emit_text(
            "LET S(x) BE { SWITCHON x INTO $( CASE 1: f()\n CASE 2: g()\n DEFAULT: h() $) }",
        );
        assert!(text.contains("switch i64"));
        assert!(text.contains("switch.case0"));
        assert!(text.contains("switch.case1"));
        assert!(text.contains("switch.default"));
    }

    #[test]
    fn vec_subscript_emits_gep_plus_load() {
        let text = emit_text("LET S() BE { LET v = VEC 10\n LET a = v!3 }");
        // GEP with i8 element type carries the byte offset.
        assert!(text.contains("getelementptr"));
        assert!(text.contains("load i64"));
    }

    #[test]
    fn vec_subscript_assign_emits_gep_plus_store() {
        let text = emit_text("LET S() BE { LET v = VEC 10\n v!3 := 42 }");
        assert!(text.contains("getelementptr"));
        assert!(text.contains("store i64 42"));
    }

    #[test]
    fn float_subscript_loads_double() {
        let text = emit_text("LET S() BE { LET fv = FVEC 10\n LET a = fv.%3 }");
        assert!(text.contains("load double"));
    }

    #[test]
    fn prefix_indirection_emits_load_of_word() {
        let text = emit_text("LET S(p) BE { LET a = !p }");
        assert!(text.contains("load i64"));
    }

    #[test]
    fn prefix_indirection_assignment_emits_store() {
        let text = emit_text("LET S(p) BE { !p := 42 }");
        assert!(text.contains("store i64 42"));
    }

    // ─── classes: NEW + field load/store ────────────────────────

    #[test]
    fn new_class_allocates_instance() {
        let text = emit_text(
            "CLASS Point $( DECL x, y $)\nLET S() BE { LET p = NEW Point }",
        );
        // `NEW Class` now creates a real Obj-C object via
        // `bcpl_objc_new("<prefix>Point")` == [[Point alloc] init].
        // No GC heap allocation, no inline vtable store.
        assert!(text.contains("@bcpl_objc_new"));
        assert!(!text.contains("@__newbcpl_alloc_rec"));
    }

    #[test]
    fn field_load_uses_byte_offset_from_layout() {
        let text = emit_text(
            "CLASS Point $( DECL x, y $)\nLET S() BE { LET p = NEW Point\n LET v = p.y }",
        );
        // Field access rebases through the defining class's __bcpl ivar
        // (bcpl_objc_field_base_for), then GEPs the OWN-relative offset.
        // y is Point's second own field — own offset 8 (x at 0).
        assert!(text.contains("@bcpl_objc_field_base_for"));
        assert!(text.contains("getelementptr"));
        assert!(text.contains("i64 8"));
        assert!(text.contains("load i64"));
    }

    #[test]
    fn field_store_emits_gep_plus_store() {
        let text = emit_text(
            "CLASS Point $( DECL x, y $)\nLET S() BE { LET p = NEW Point\n p.x := 99 }",
        );
        // x is Point's first own field — own offset 0, in Point's
        // __bcpl ivar reached via bcpl_objc_field_base_for.
        assert!(text.contains("@bcpl_objc_field_base_for"));
        assert!(text.contains("store i64 99"));
    }

    #[test]
    fn class_with_create_emits_call() {
        let text = emit_text(
            "CLASS Foo $(\n  DECL x\n  ROUTINE CREATE(ix) BE $( SELF.x := ix $)\n$)\nLET S() BE { LET f = NEW Foo(42) }",
        );
        // CREATE is now called via its mangled `{Class}_CREATE`
        // symbol so multiple classes can each have their own.
        // The receiver pointer is the first argument; 42 is the
        // second.
        assert!(text.contains("call i64 @Foo_CREATE"));
        assert!(text.contains("i64 42"));
    }

    #[test]
    fn dump_llvm_smoke() {
        // End-to-end: write a tiny program to a temp file, run
        // dump_llvm, and check the header / body.
        let tmp = std::env::temp_dir().join("newbcpl_llvm_smoke.bcl");
        std::fs::write(&tmp, "LET S() BE { LET y = 1 + 2 }").unwrap();
        let dump = dump_llvm(&tmp);
        assert!(dump.contains("newbcpl-llvm dump"));
        assert!(dump.contains("define i64 @S()"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn every_function_polls_safepoint() {
        // Cooperative-GC plumbing: the IR-emit pass inserts a
        // `__newbcpl_safepoint()` call at the top of every JIT'd
        // function so the collector can pause threads that
        // never allocate. Confirm the call shows up in both the
        // top-level routine and a class method body — START and
        // Foo_CREATE both need to be parkable.
        let text = emit_text(
            "CLASS Foo $(\n  DECL x\n  ROUTINE CREATE(ix) BE $( SELF.x := ix $)\n$)\nLET S() BE { LET f = NEW Foo(42) }",
        );
        let safepoint_calls = text.matches("call void @__newbcpl_safepoint()").count();
        assert!(
            safepoint_calls >= 2,
            "expected at least one safepoint call per function (START and Foo_CREATE), got {safepoint_calls}\n{text}"
        );
        assert!(text.contains("declare void @__newbcpl_safepoint()"));
    }

    #[test]
    fn jit_run_class_program_via_objc() {
        // End-to-end proof that JIT-emitted `NEW Class` flows through
        // the Obj-C runtime: compile a program that creates class
        // instances (each is [[Point alloc] init]), runs their CREATE
        // constructors, and stores into the per-class __bcpl ivar.
        // Success (no crash, clean exit) proves the registrar +
        // bcpl_objc_new + field_base_for path works under the JIT.
        let tmp = std::env::temp_dir().join("newbcpl_jit_objc_class.bcl");
        std::fs::write(
            &tmp,
            "CLASS Point $(\n  DECL x, y\n  ROUTINE CREATE(ix, iy) BE $( SELF.x := ix\n SELF.y := iy $)\n  FUNCTION sum() = SELF.x + SELF.y\n$)\nLET START() BE $(\n LET a = NEW Point(1, 2)\n LET b = NEW Point(3, 4)\n LET c = NEW Point(5, 6)\n LET t = a.sum() + b.sum() + c.sum()\n WRITEN(t)\n$)",
        )
        .unwrap();
        run(&tmp).expect("JIT run of an Obj-C-backed class program should succeed");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn collect_after_jit_run_does_not_crash() {
        // Fix B in `docs/jit_typedesc_lifetime.md`: TypeDescs
        // are interned by `__newbcpl_alloc_rec` on the runtime
        // side, so they survive the JIT engine drop. After
        // `run()` returns we can safely walk the heap with
        // `collect()` and start a fresh JIT run on top of it.
        //
        // The previous incarnation of this test crashed in
        // `collect()` because `BlockHeader.tag` pointed into the
        // JIT module's freed data section. With `__newbcpl_alloc_rec`
        // in place, every tag points into a `Box::leak`'d
        // `RuntimeTypeDesc` that lives for the process lifetime.
        let tmp = std::env::temp_dir().join("newbcpl_jit_collect.bcl");
        std::fs::write(
            &tmp,
            "CLASS Point $(\n  DECL x, y\n  ROUTINE CREATE(ix, iy) BE $( SELF.x := ix\n SELF.y := iy $)\n$)\nLET START() BE $(\n LET a = NEW Point(1, 2)\n LET b = NEW Point(3, 4)\n LET c = NEW Point(5, 6)\n$)",
        )
        .unwrap();
        run(&tmp).expect("first JIT run should succeed");
        // collect() walks every BlockHeader; if any tag pointed
        // into freed JIT memory this would access-violation.
        newbcpl_runtime::gc::collect();
        // Heap must remain usable for subsequent JIT runs.
        run(&tmp).expect("post-collect JIT run should succeed");
        newbcpl_runtime::gc::collect();
        let _ = std::fs::remove_file(&tmp);
    }
}
