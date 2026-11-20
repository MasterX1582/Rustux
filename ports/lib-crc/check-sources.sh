#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
for f in crc-ccitt.c crc-itu-t.c crc16.c crc32-main.c crc4.c crc64-main.c \
         crc7.c crc8.c crc-t10dif-main.c; do
  if diff -q "$f" "$L/lib/crc/$f" > /dev/null; then
    echo "$f identical"
  else
    echo "$f DIFFERS"
    fail=1
  fi
done
for h in crc32poly.h crc-ccitt.h crc-itu-t.h; do
  if diff -q "linux/$h" "$L/include/linux/$h" > /dev/null; then
    echo "linux/$h identical"
  else
    echo "linux/$h DIFFERS"
    fail=1
  fi
done
exit $fail
