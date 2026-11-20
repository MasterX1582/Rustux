#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
for pair in "siphash.c lib/crypto/siphash.c" "xxhash.c lib/xxhash.c"; do
  set -- $pair
  if diff -q "$1" "$L/$2" > /dev/null; then
    echo "$2 identical"
  else
    echo "$2 DIFFERS"
    fail=1
  fi
done
for h in siphash.h xxhash.h jhash.h; do
  if diff -q "linux/$h" "$L/include/linux/$h" > /dev/null; then
    echo "include/linux/$h identical"
  else
    echo "include/linux/$h DIFFERS"
    fail=1
  fi
done
exit $fail
