#!/bin/bash
# verify C copies are byte-identical to the reference tree
L=${LINUX_REF:-$HOME/linux-ref}
D=$(dirname "$0")
fail=0
for f in lib/fonts/font_8x16.c lib/fonts/font.h; do
  if cmp -s "$D/$(basename $f)" "$L/$f"; then echo "$f identical"
  else echo "$f DIFFERS"; fail=1; fi
done
exit $fail
