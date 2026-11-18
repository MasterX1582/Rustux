#!/bin/bash
# End-to-end: source identity, C harness build + selftest, Rust build,
# differential replay, gate.
set -euo pipefail
source ~/.cargo/env
cd "$(dirname "$0")"
export LC_ALL=C

./check-sources.sh

# C side sanity: all three units compile, and the selftest passes.
cc -O1 -w -I. -Ilinux -o /tmp/lib-sha-selftest selftest.c sha1.c sha256.c sha512.c
/tmp/lib-sha-selftest | tail -1

cd rust/wrappers-rs
rust-anything build . 2>&1 | tail -1
rust-anything diff . 2>&1 | tail -2
rust-anything gate . 2>&1 | tail -12
