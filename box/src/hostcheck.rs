//! box/hostcheck.sh's compiler: a BCPL program and the library modules it
//! names, compiled by the box's front end and code generator for another
//! triple (the Intel box's, on an Arm Mac), as the JIT would.
//!
//!   bcpl-hostcheck <file.bcl> <library dir> <triple> <out.o>

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 5 {
        eprintln!("usage: bcpl-hostcheck <file.bcl> <library dir> <triple> <out.o>");
        std::process::exit(2);
    }
    if let Err(e) = newbcpl_box::host_emit(a[1].as_ref(), a[2].as_ref(), &a[3], a[4].as_ref()) {
        eprintln!("{}: {e}", a[1]);
        std::process::exit(1);
    }
}
