#!/bin/bash
# Verify the harness copy is byte-identical to the Linux reference tree.
set -u
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
if diff -q string.c "$L/lib/string.c" > /dev/null; then
  echo "lib/string.c identical"
  exit 0
else
  echo "lib/string.c DIFFERS"
  exit 1
fi
