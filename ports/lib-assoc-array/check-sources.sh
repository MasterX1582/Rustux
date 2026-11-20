#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
check() {
  if diff -q "$1" "$L/$2" > /dev/null 2>&1; then
    echo "$2 identical"
  else
    echo "$2 DIFFERS"
    fail=1
  fi
}
check "assoc_array.c"              "lib/assoc_array.c"
check "linux/assoc_array.h"        "include/linux/assoc_array.h"
check "linux/assoc_array_priv.h"   "include/linux/assoc_array_priv.h"
exit $fail
