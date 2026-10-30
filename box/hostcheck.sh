#!/bin/sh
# hostcheck.sh -- the Intel box's code, checked on an Arm Mac (no x86 box
# may run here): box/tests/*.bcl compiled by the box's own front end and
# code generator (newbcpl-box's bcpl-hostcheck, with LLVM's X86 backend) for
# x86_64-unknown-linux-musl, as the box's MCJIT makes code (the large code
# model), with the library modules each names, then checked:
#   - each object is ELF x86-64;
#   - no call or reference reaches outside the object by a 32-bit
#     PC-relative relocation (the JIT's code may be anywhere: the large
#     model's 64-bit absolute R_X86_64_64 only, or the GOT);
#   - every name an object needs is one the box's JIT binds (the runtime's
#     builtin tables: builtins.rs, objc_box.rs, riscos.rs) or one /init's
#     dlsym gives (A7232ToolChain rosgd/runtime/llvm/llvm_dl.c).
#
#   sh box/hostcheck.sh
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
box_root=$(cd "$here/../.." && pwd)
HB=/opt/homebrew/opt/llvm@22/bin
out=${BCPL_HOSTCHECK:-$box_root/caches/bcpl/hostcheck}
llvm_dl=$(ls "$box_root"/src/A7232ToolChain-bcpl/rosgd/runtime/llvm/llvm_dl.c "$box_root"/src/A7232ToolChain/rosgd/runtime/llvm/llvm_dl.c 2>/dev/null | head -1)
rm -rf "$out"
mkdir -p "$out"
cd "$here"
export LLVM_SYS_221_PREFIX=${LLVM_SYS_221_PREFIX:-/opt/homebrew/opt/llvm}
CARGO_TARGET_DIR=$box_root/caches/bcpl/target-host cargo build -q --release -p newbcpl-box --features x86-check --bin bcpl-hostcheck
# the library as the ROM has it (box/build.sh)
mkdir -p "$out/lib"
for m in libhdr maths geom; do cp "modules-active/$m.bcl" "$out/lib/"; done
cp box/lib/*.bcl "$out/lib/"
bad=0
for f in box/tests/*.bcl; do
    n=$(basename "$f" .bcl)
    [ "$n" = bad ] && continue                  # the compile error's test
    if ! "$box_root/caches/bcpl/target-host/release/bcpl-hostcheck" "$f" "$out/lib" x86_64-unknown-linux-musl "$out/$n.o"; then
        echo "FAIL $f did not compile"; bad=1
    fi
done
# the names the JIT binds, and /init's dlsym's
grep -ohE '(builtin|entry)!\([A-Za-z0-9_]+\)' src/newbcpl-runtime/src/builtins.rs src/newbcpl-runtime/src/objc_box.rs \
    src/newbcpl-runtime/src/riscos.rs | sed 's/.*(//;s/)//' > "$out/bound.txt"
sed -n '/pub fn builtin_addresses/,/^}/p' src/newbcpl-runtime/src/heap.rs | grep -oE '"[A-Za-z0-9_]+"' | tr -d '"' >> "$out/bound.txt"
if [ -n "$llvm_dl" ]; then
    sed -n '/^static const struct/,/^};/p' "$llvm_dl" | grep -oE 'E\([A-Za-z0-9_]+\)|"[A-Za-z0-9_]+"' | tr -d '"E()' >> "$out/bound.txt"
fi
for o in "$out"/*.o; do
    fmt=$("$HB/llvm-objdump" -f "$o" | grep -o 'file format .*')
    case $fmt in *elf64-x86-64*) ;; *) echo "FAIL $o: $fmt"; bad=1 ;; esac
    pc=$("$HB/llvm-objdump" -r "$o" | grep -E 'R_X86_64_(PC32|PLT32|GOTPCREL)' | awk '{print $3}' | grep -v '^\.' || true)
    for s in $pc; do
        # a PC-relative reference to a name defined in the object is fine
        if ! "$HB/llvm-nm" --defined-only "$o" | awk '{print $3}' | grep -qxF "${s%%[-+]*}"; then
            echo "FAIL $o: a 32-bit PC-relative reference to $s, outside the object"; bad=1
        fi
    done
    for u in $("$HB/llvm-nm" -u "$o" | awk '{print $2}'); do
        grep -qxF "$u" "$out/bound.txt" && continue
        case $u in llvm.*|_GLOBAL_OFFSET_TABLE_) continue ;; esac
        echo "FAIL $o: $u is not bound by the JIT"; bad=1
    done
    printf '%s: %s relocations, %s undefined names\n' "$(basename "$o")" \
        "$("$HB/llvm-objdump" -r "$o" | grep -c R_X86_64 || true)" "$("$HB/llvm-nm" -u "$o" | wc -l | tr -d ' ')"
done
[ $bad = 0 ] && echo "hostcheck: all objects pass"
exit $bad
