#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=/mnt/c/Users/dev/Documents/GitHub/linux
cd "$(dirname "$0")"
for f in generic-radix-tree.c interval_tree.c rbtree.c; do
  if diff -q "$f" "$L/lib/$f" > /dev/null; then
    echo "lib/$f identical"
  else
    echo "lib/$f DIFFERS"
    fail=1
  fi
done
for h in generic-radix-tree.h interval_tree.h interval_tree_generic.h \
         rbtree.h rbtree_types.h rbtree_augmented.h; do
  if diff -q "linux/$h" "$L/include/linux/$h" > /dev/null; then
    echo "include/linux/$h identical"
  else
    echo "include/linux/$h DIFFERS"
    fail=1
  fi
done
exit $fail
