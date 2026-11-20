#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd ""/usr/bin"
./check-sources.sh
for d in crc4-rs wrappers-rs; do
  echo "== $d =="
  (cd "rust/$d" && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
done
