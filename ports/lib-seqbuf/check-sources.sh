#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
if diff -q "seq_buf.c" "$L/lib/seq_buf.c" > /dev/null; then
  echo "lib/seq_buf.c identical"
else
  echo "lib/seq_buf.c DIFFERS"
  fail=1
fi
if diff -q "linux/seq_buf.h" "$L/include/linux/seq_buf.h" > /dev/null; then
  echo "include/linux/seq_buf.h identical"
else
  echo "include/linux/seq_buf.h DIFFERS"
  fail=1
fi
exit $fail
