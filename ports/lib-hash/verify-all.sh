#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports/lib-hash
./check-sources.sh
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
