#!/bin/bash
# Verify the harness copies are byte-identical to the Linux reference tree.
set -u
fail=0
L=${LINUX_REF:-$HOME/linux-ref}
cd "$(dirname "$0")"

check() {
  if diff -q "$1" "$2" > /dev/null; then
    echo "$3 identical"
  else
    echo "$3 DIFFERS"
    fail=1
  fi
}

check "percpu_counter.c"      "$L/lib/percpu_counter.c"             "lib/percpu_counter.c"
check "linux/percpu_counter.h" "$L/include/linux/percpu_counter.h"  "include/linux/percpu_counter.h"
check "printk_ringbuffer.c"   "$L/kernel/printk/printk_ringbuffer.c" "kernel/printk/printk_ringbuffer.c"
check "printk_ringbuffer.h"   "$L/kernel/printk/printk_ringbuffer.h" "kernel/printk/printk_ringbuffer.h"

exit $fail
