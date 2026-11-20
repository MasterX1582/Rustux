#!/bin/bash
# End-to-end verification for the lib/assoc_array.c port:
#   1. upstream source copies are byte-identical
#   2. C selftest grid (8 seeds x 9 seed2 masks, every op family and
#      the alloc-failure injection) still matches the recorded aggregate
#   3. the recorded 1383 cases still replay identically under Rust
#   4. the rust-anything gate is green
set -euo pipefail
source ~/.cargo/env
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports/lib-assoc-array
./check-sources.sh
gcc -O2 -w -I. -o /tmp/selftest-assoc-array selftest.c wrappers.c assoc_array.c
/tmp/selftest-assoc-array | tail -1
gcc -O2 -fPIC -shared -w -I. -o rust/wrappers-rs/.rust-anything/original.so \
    wrappers.c assoc_array.c
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 \
    && rust-anything gate . 2>&1 | tail -13)
