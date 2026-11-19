#!/bin/bash
# lib-percpu-printk verification: source identity, C selftest,
# differential replay and the rust-anything gate.
set -u
source ~/.cargo/env
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports/lib-percpu-printk
fail=0

echo "=== 1/5 source identity ==="
if ! ./check-sources.sh; then fail=1; fi

echo "=== 2/5 C selftest ==="
if gcc -O1 -w -I. -o /tmp/lpp-selftest selftest.c percpu_counter.c printk_ringbuffer.c \
    && /tmp/lpp-selftest; then
  echo "selftest OK"
else
  echo "selftest FAILED"; fail=1
fi

echo "=== 3/5 build shared C library ==="
if gcc -O2 -fPIC -shared -I. -o rust/wrappers-rs/.rust-anything/original.so \
    wrappers.c percpu_counter.c printk_ringbuffer.c; then
  echo "original.so OK"
else
  echo "original.so FAILED"; fail=1
fi

echo "=== 4/5 differential replay ==="
if ! (cd rust/wrappers-rs && rust-anything diff . 2>&1 | tail -2); then fail=1; fi

echo "=== 5/5 rust-anything gate ==="
if ! (cd rust/wrappers-rs && rust-anything gate . 2>&1 | tail -12); then fail=1; fi

echo
if [ "$fail" -eq 0 ]; then
  echo "verify-all: PASS"
else
  echo "verify-all: FAIL"
fi
exit $fail
