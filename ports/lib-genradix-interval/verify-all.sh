#!/bin/bash
set -euo pipefail
source ~/.cargo/env
cd ""/usr/bin"
./check-sources.sh
gcc -O2 -w -I. -o /tmp/selftest-genradix-interval selftest.c \
  generic-radix-tree.c interval_tree.c rbtree.c
/tmp/selftest-genradix-interval
(cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2 && rust-anything gate . 2>&1 | tail -2)
