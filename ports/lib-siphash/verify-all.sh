#!/bin/bash
# End-to-end verification for the lib/crypto/siphash.c port:
#   1. upstream source copies are byte-identical
#   2. C selftest (272 checks: all 64+64 upstream vectors, aligned and
#      unaligned, every tail path 0..64, all scalar variants) passes
#   3. the recorded 4815 cases still replay identically under Rust
#   4. the rust-anything gate is green
set -euo pipefail
source ~/.cargo/env
cd ""/usr/bin"
./check-sources.sh
gcc -O2 -w -I. -o /tmp/selftest-siphash selftest.c siphash.c
/tmp/selftest-siphash | tail -1
gcc -O2 -fPIC -shared -w -I. -o rust/wrappers-rs/.rust-anything/original.so \
    wrappers.c siphash.c
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 \
    && rust-anything gate . 2>&1 | tail -13)
