#!/usr/bin/env python3
"""Custom verification for `iter_div_u64_rem` (div64.c).

The function's `uint64_t *remainder` out-param keeps it outside rust-anything's
ctypes bridge, so this script diffs the original C shared object against the
Rust port directly: same signature, quotient return + remainder out-param.

Faithful edge cases skipped on purpose: `divisor == 0` makes the C (and the
port) loop forever — `while (dividend >= divisor)` can never terminate.

Usage: check-iter-div.py <original.so> <port.so>
"""

import ctypes
import signal
import sys

# Watchdog: a port regression that makes the subtract loop non-terminating
# must fail the check, not hang it (the whole suite runs in well under a
# second when correct).
signal.alarm(60)

CASE_DIVISORS = [1, 2, 3, 7, 255, 0x10000, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF]


def cases():
    for d in CASE_DIVISORS:
        # quotient stays small on purpose: the C loop is O(quotient). Large
        # dividends are unreachable here — divisor is u32, so a big dividend
        # implies a huge quotient and the C code would run ~forever.
        for n in (0, 1, d - 1, d, d + 1, 2 * d + 3, 100 * d + 9, 999 * d + 5):
            yield (n & 0xFFFFFFFFFFFFFFFF, d)


def bind(path):
    lib = ctypes.CDLL(path)
    fn = lib.iter_div_u64_rem
    fn.restype = ctypes.c_uint32
    fn.argtypes = [ctypes.c_uint64, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint64)]
    return fn


def main():
    orig = bind(sys.argv[1])
    port = bind(sys.argv[2])
    total = bad = 0
    for dividend, divisor in cases():
        rem_o = ctypes.c_uint64(0xDEADBEEF)
        rem_p = ctypes.c_uint64(0xDEADBEEF)
        q_o = orig(dividend, divisor, ctypes.byref(rem_o))
        q_p = port(dividend, divisor, ctypes.byref(rem_p))
        total += 1
        if q_o != q_p or rem_o.value != rem_p.value:
            bad += 1
            print(
                f"MISMATCH iter_div_u64_rem({dividend:#x}, {divisor:#x}): "
                f"C=({q_o}, {rem_o.value:#x}) Rust=({q_p}, {rem_p.value:#x})"
            )
    print(f"{total - bad}/{total} cases match")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
