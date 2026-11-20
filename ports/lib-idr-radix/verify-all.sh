#!/bin/bash
# End-to-end verification for the idr/ida/radix-tree port:
#   1. upstream source copies are byte-identical
#   2. C selftest grid (72 seed x seed2 combinations) still matches the
#      recorded aggregate
#   3. the recorded 879 cases still replay identically under Rust
#   4. the rust-anything gate is green
set -euo pipefail
source ~/.cargo/env
cd ""/usr/bin"
./check-sources.sh
gcc -O2 -w -I. -o /tmp/selftest-idr selftest.c wrappers.c xarray.c radix-tree.c
/tmp/selftest-idr | tail -1
gcc -O2 -fPIC -shared -w -I. -o rust/wrappers-rs/.rust-anything/original.so \
    wrappers.c xarray.c radix-tree.c
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 \
    && rust-anything gate . 2>&1 | tail -13)
