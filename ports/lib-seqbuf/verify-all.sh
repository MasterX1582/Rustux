#!/bin/bash
# End-to-end: source identity, C harness build + selftest, record,
# Rust build, differential replay, gate.
set -euo pipefail
source ~/.cargo/env
cd "$(dirname "$0")"

./check-sources.sh

# C side sanity: everything compiles and the selftest passes.
cc -O1 -w -I. -Ilinux -o /tmp/lib-seqbuf-selftest selftest.c seq_buf.c
/tmp/lib-seqbuf-selftest | tail -1

cd rust/wrappers-rs
rust-anything record . 2>&1 | head -1
rust-anything build . 2>&1 | tail -1
rust-anything diff . 2>&1 | tail -2
rust-anything gate . 2>&1 | tail -3
