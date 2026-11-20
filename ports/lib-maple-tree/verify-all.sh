#!/bin/bash
set -euo pipefail
source ~/.cargo/env 2>/dev/null || export PATH=$HOME/.cargo/bin:$PATH
cd ""/usr/bin"
./check-sources.sh
gcc -O2 -fPIC -shared -I. -o rust/wrappers-rs/.rust-anything/original.so wrappers.c maple_tree.c
gcc -O2 -I. -o /tmp/mt_selftest selftest.c maple_tree.c && /tmp/mt_selftest
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
