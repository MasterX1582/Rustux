#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
for pair in "rhashtable.c:lib/rhashtable.c" \
            "bucket_locks.c:lib/bucket_locks.c" \
            "linux/rhashtable.h:include/linux/rhashtable.h" \
            "linux/rhashtable-types.h:include/linux/rhashtable-types.h" \
            "linux/jhash.h:include/linux/jhash.h" \
            "linux/list_nulls.h:include/linux/list_nulls.h"; do
  a=${pair%%:*}; b=${pair##*:}
  if diff -q "$a" "$L/$b" > /dev/null; then
    echo "$b identical"
  else
    echo "$b DIFFERS"
    fail=1
  fi
done
exit $fail
