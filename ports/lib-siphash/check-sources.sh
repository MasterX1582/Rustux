#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
# Upstream moved lib/siphash.c to lib/crypto/siphash.c; this tree carries it
# there (lib/siphash.c does not exist).
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
check "siphash.c" "lib/crypto/siphash.c"
check "linux/siphash.h" "include/linux/siphash.h"
exit $fail
