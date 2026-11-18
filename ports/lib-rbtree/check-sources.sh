#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=/mnt/c/Users/dev/Documents/GitHub/linux
cd "$(dirname "$0")"
if diff -q "rbtree.c" "$L/lib/rbtree.c" > /dev/null; then
  echo "lib/rbtree.c identical"
else
  echo "lib/rbtree.c DIFFERS"
  fail=1
fi
for h in rbtree.h rbtree_types.h rbtree_augmented.h; do
  if diff -q "linux/$h" "$L/include/linux/$h" > /dev/null; then
    echo "include/linux/$h identical"
  else
    echo "include/linux/$h DIFFERS"
    fail=1
  fi
done
exit $fail
