#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd ""/usr/bin"
./check-sources.sh
gcc -O2 -fPIC -shared -I. -o rust/wrappers-rs/.rust-anything/original.so wrappers.c xarray.c
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
