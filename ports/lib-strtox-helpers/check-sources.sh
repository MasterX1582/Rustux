#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
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
check "kstrtox.c"                 "lib/kstrtox.c"
check "kstrtox.h"                 "lib/kstrtox.h"
check "hexdump.c"                 "lib/hexdump.c"
check "string_helpers.c"          "lib/string_helpers.c"
check "ctype.c"                   "lib/ctype.c"
check "linux/kstrtox.h"           "include/linux/kstrtox.h"
check "linux/string_helpers.h"    "include/linux/string_helpers.h"
check "linux/string_choices.h"    "include/linux/string_choices.h"
check "linux/hex.h"               "include/linux/hex.h"
check "linux/args.h"              "include/linux/args.h"
exit $fail
