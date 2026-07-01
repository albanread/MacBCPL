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

    newbcpl_llvm::emit_aot_object(&src, &obj).expect("emit_aot_object should succeed");

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
