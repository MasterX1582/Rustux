#!/usr/bin/env python3
"""Confirm the recorded ramp cases equal the upstream SipHash test vectors.

wrap_{,h}siphash_ramp(K0, K1, off=0, len=i) hashes the bytes 0..i-1 with
the official test key, so its recorded `ok` must equal
test_vectors_{siphash,hsiphash}[i]. The tables are parsed out of
lib/crypto/tests/siphash_kunit.c (BITS_PER_LONG == 64 branch), so this
double-checks the recorded corpus against the upstream vectors without
re-typing them.
"""
import json
import re
import sys

KUNIT = "/mnt/c/Users/dev/Documents/GitHub/linux/lib/crypto/tests/siphash_kunit.c"
K0, K1 = 0x0706050403020100, 0x0F0E0D0C0B0A0908


def table(src, name):
    m = re.search(name + r"\[64\] = \{(.*?)\};", src, re.S)
    vals = re.findall(r"0x[0-9a-fA-F]+(?:ULL|UL|U|LL|L)?", m.group(1))
    return [int(re.sub(r"[UL]+$", "", x), 16) for x in vals]


src = open(KUNIT).read()
# The siphash table precedes the BITS_PER_LONG split; the 64-bit hsiphash
# table is the first branch.
lp64 = src.split("#if BITS_PER_LONG == 64")[1].split("#else")[0]
sip = table(src, "test_vectors_siphash")
hsip = table(lp64, "test_vectors_hsiphash")
assert len(sip) == 64 and len(hsip) == 64

seen = {}
for line in open("rust/wrappers-rs/cases.jsonl"):
    c = json.loads(line)
    if c["fn"] not in ("wrap_siphash_ramp", "wrap_hsiphash_ramp"):
        continue
    k0, k1, off, length = c["args"]
    if (k0, k1, off) == (K0, K1, 0) and length < 64:
        seen[(c["fn"], length)] = c["ok"]

fails = 0
for i, want in enumerate(sip):
    got = seen.get(("wrap_siphash_ramp", i))
    if got != want:
        print(f"siphash[{i}]: recorded {got:#x} != vector {want:#x}")
        fails += 1
for i, want in enumerate(hsip):
    got = seen.get(("wrap_hsiphash_ramp", i))
    if got != want:
        print(f"hsiphash[{i}]: recorded {got:#x} != vector {want:#x}")
        fails += 1
print(f"vector check: {128 - fails}/128 recorded ramp cases match upstream vectors")
sys.exit(1 if fails else 0)
