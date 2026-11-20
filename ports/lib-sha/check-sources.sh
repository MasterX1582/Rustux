#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"
for pair in \
  "sha1.c lib/crypto/sha1.c" \
  "sha256.c lib/crypto/sha256.c" \
  "sha512.c lib/crypto/sha512.c" \
  "fips-sha.h lib/crypto/fips-sha.h"; do
  set -- $pair
  if diff -q "$1" "$L/$2" > /dev/null; then
    echo "$2 identical"
  else
    echo "$2 DIFFERS"
    fail=1
  fi
done
for h in sha1.h sha2.h hmac.h; do
  if diff -q "crypto/$h" "$L/include/crypto/$h" > /dev/null; then
    echo "include/crypto/$h identical"
  else
    echo "include/crypto/$h DIFFERS"
    fail=1
  fi
done
exit $fail
