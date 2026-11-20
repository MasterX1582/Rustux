#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
if diff -q "llist.c" "$L/lib/llist.c" > /dev/null; then
  echo "lib/llist.c identical"
else
  echo "lib/llist.c DIFFERS"
  fail=1
fi
if diff -q "linux/llist.h" "$L/include/linux/llist.h" > /dev/null; then
  echo "include/linux/llist.h identical"
else
  echo "include/linux/llist.h DIFFERS"
  fail=1
fi
exit $fail
