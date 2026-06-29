#!/bin/bash
# Build the "MacBCPL IDE.app" launcher stub — a tiny Mach-O that exec's the
# JIT driver on examples/bcpl-ide.bcl (a shell-script bundle executable
# won't double-click on modern macOS). Run after `cargo build -p
# newbcpl-driver`. The bundle is relocatable: the stub finds the repo
# relative to itself, so the .app works wherever the repo lives.
set -e
cd "$(dirname "$0")"
cc -O2 -o "MacBCPL IDE.app/Contents/MacOS/MacBCPL-IDE" launcher.c
chmod +x "MacBCPL IDE.app/Contents/MacOS/MacBCPL-IDE"
echo "Built 'dist/MacBCPL IDE.app'.  Launch it with:  open 'dist/MacBCPL IDE.app'"
