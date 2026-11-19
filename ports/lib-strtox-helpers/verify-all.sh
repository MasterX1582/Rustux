#!/bin/bash
# End-to-end: source identity, C harness build + selftest, recorded-case
# replay under Rust, gate.
set -euo pipefail
source ~/.cargo/env
cd "$(dirname "$0")"

./check-sources.sh

# C side sanity: everything compiles and the selftest passes.
gcc -O1 -w -I. -o /tmp/lib-strtox-helpers-selftest \
    selftest.c wrappers.c kstrtox.c hexdump.c string_helpers.c ctype.c
/tmp/lib-strtox-helpers-selftest | tail -1

# Rebuild the recorded original and replay.
gcc -O2 -fPIC -shared -w -I. -o rust/wrappers-rs/.rust-anything/original.so \
    wrappers.c kstrtox.c hexdump.c string_helpers.c ctype.c
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 \
    && rust-anything gate . 2>&1 | tail -13)
