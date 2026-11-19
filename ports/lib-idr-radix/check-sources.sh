#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
# upstream/idr.inc is lib/idr.c verbatim; it is textually included by
# wrappers.c so the recursive *.c harness glob never compiles it twice.
set -u
fail=0
L=/mnt/c/Users/dev/Documents/GitHub/linux
cd "$(dirname "$0")"
check() {
  if diff -q "$1" "$L/$2" > /dev/null 2>&1; then
    echo "$2 identical"
  else
    echo "$2 DIFFERS"
    fail=1
  fi
}
check "upstream/idr.inc" "lib/idr.c"
check "radix-tree.c" "lib/radix-tree.c"
check "xarray.c" "lib/xarray.c"
check "radix-tree.h" "lib/radix-tree.h"
check "linux/idr.h" "include/linux/idr.h"
check "linux/radix-tree.h" "include/linux/radix-tree.h"
check "linux/xarray.h" "include/linux/xarray.h"
exit $fail
