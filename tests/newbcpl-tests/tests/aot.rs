//! AOT object-emission probe. Verifies `emit_aot_object` lowers a program to a
//! valid Mach-O relocatable object (the front half of `newbcpl-driver build`).
//! The full build→link→run pipeline is exercised by the driver's `build`
//! command; this pins the object emission without needing a linker.

#[test]
fn aot_emits_a_macho64_object() {
    let dir = std::env::temp_dir();
    let src = dir.join("newbcpl_aot_probe.bcl");
    std::fs::write(&src, "LET START() BE $(\n  WRITES(\"hi\")\n$)\n").unwrap();
    let obj = dir.join("newbcpl_aot_probe.o");

    newbcpl_llvm::emit_aot_object(&src, &obj, None).expect("emit_aot_object should succeed");

    let bytes = std::fs::read(&obj).expect("object file readable");
    assert!(bytes.len() > 64, "object should be non-trivial ({} bytes)", bytes.len());
    // Mach-O 64-bit magic 0xFEEDFACF, little-endian on disk.
    assert_eq!(
        &bytes[0..4],
        &[0xCF, 0xFA, 0xED, 0xFE],
        "should start with the Mach-O 64 magic"
    );

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&obj);
}

/// A program with a user `CLASS` emits a Mach-O object that also contains the
/// generated `__bcpl_register_classes` (the Obj-C registrar `main` runs before
/// `START`). Emission must succeed and produce a valid object.
#[test]
fn aot_emits_class_program_with_registrar() {
    let dir = std::env::temp_dir();
    let src = dir.join("newbcpl_aot_class_probe.bcl");
    std::fs::write(
        &src,
        "CLASS C $(\n  DECL x\n  ROUTINE CREATE(v) BE SELF.x := v\n  FUNCTION get() = SELF.x\n$)\nLET START() BE $(\n  LET o = NEW C(42)\n  WRITEN(o.get())\n$)\n",
    )
    .unwrap();
    let obj = dir.join("newbcpl_aot_class_probe.o");

    newbcpl_llvm::emit_aot_object(&src, &obj, None).expect("emit_aot_object should succeed for a class");

    let bytes = std::fs::read(&obj).expect("object file readable");
    assert_eq!(&bytes[0..4], &[0xCF, 0xFA, 0xED, 0xFE], "Mach-O 64 object");

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&obj);
}
