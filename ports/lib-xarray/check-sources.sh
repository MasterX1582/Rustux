#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=/mnt/c/Users/dev/Documents/GitHub/linux
cd "$(dirname "$0")"
if diff -q "xarray.c" "$L/lib/xarray.c" > /dev/null; then
  echo "lib/xarray.c identical"
else
  echo "lib/xarray.c DIFFERS"
  fail=1
fi
if diff -q "radix-tree.h" "$L/lib/radix-tree.h" > /dev/null; then
  echo "lib/radix-tree.h identical"
else
  echo "lib/radix-tree.h DIFFERS"
  fail=1
fi
if diff -q "linux/xarray.h" "$L/include/linux/xarray.h" > /dev/null; then
  echo "include/linux/xarray.h identical"
else
  echo "include/linux/xarray.h DIFFERS"
  fail=1
fi
exit $fail
