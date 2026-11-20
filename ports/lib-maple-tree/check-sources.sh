#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
for pair in "maple_tree.c:$L/lib/maple_tree.c" \
            "linux/maple_tree.h:$L/include/linux/maple_tree.h" \
            "linux/xarray.h:$L/include/linux/xarray.h"; do
  a=${pair%%:*}; b=${pair##*:}
  if diff -q "$a" "$b" > /dev/null; then
    echo "$a identical"
  else
    echo "$a DIFFERS"
    fail=1
  fi
done
exit $fail
