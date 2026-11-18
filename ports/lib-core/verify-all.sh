#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports/lib-core
for d in bcd-rs core-wrappers-rs; do
  echo "== $d =="
  (cd "rust/$d" && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
done
