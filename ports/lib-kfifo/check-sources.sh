#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
if diff -q "kfifo.c" "$L/lib/kfifo.c" > /dev/null; then
  echo "lib/kfifo.c identical"
else
  echo "lib/kfifo.c DIFFERS"
  fail=1
fi
if diff -q "linux/kfifo.h" "$L/include/linux/kfifo.h" > /dev/null; then
  echo "include/linux/kfifo.h identical"
else
  echo "include/linux/kfifo.h DIFFERS"
  fail=1
fi
exit $fail
