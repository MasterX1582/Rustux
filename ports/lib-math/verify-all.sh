#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports/lib-math
for d in wrappers-rs div64-rs int-log-rs gcd-rs lcm-rs int-pow-rs; do
  echo "== $d =="
  (cd "rust/$d" && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
done
echo "== iter_div_u64_rem custom check =="
gcc -shared -fPIC -O2 -I. -o /tmp/libmath-orig.so *.c
python3 check-iter-div.py /tmp/libmath-orig.so rust/div64-rs/target/release/libdiv64_rs.so
