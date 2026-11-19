import json

# Official upstream test key from lib/crypto/tests/siphash_kunit.c.
K0 = 0x0706050403020100
K1 = 0x0F0E0D0C0B0A0908

out = []

# Dense tail coverage: every length 0..96 at every byte offset 0..7.
# len % 513 == len here, off & 7 == off; off=0 aligned, off!=0 unaligned.
for fn in ("wrap_siphash", "wrap_hsiphash"):
    for length in range(0, 97):
        for off in range(0, 8):
            out.append({"fn": fn, "args": [K0, K1, off, 0x0123456789ABCDEF, length]})

# Ramp input buf[i]=i reproduces the upstream SipHash test vectors:
# len 0..64 (64 = beyond the 64-entry vector table, still deterministic),
# every alignment.
for fn in ("wrap_siphash_ramp", "wrap_hsiphash_ramp"):
    for length in range(0, 65):
        for off in range(0, 8):
            out.append({"fn": fn, "args": [K0, K1, off, length]})

# Scalar variants on the upstream self-test arguments (match the KUnit
# vector table indices) plus integer edge values, including > i64::MAX.
EDGE = [0, 1, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF, 0x100000000,
        0x7FFFFFFFFFFFFFFF, 0x8000000000000000, 0xFFFFFFFFFFFFFFFF]
vec = [
    ("wrap_siphash_1u64", [0x0706050403020100]),
    ("wrap_siphash_2u64", [0x0706050403020100, 0x0F0E0D0C0B0A0908]),
    ("wrap_siphash_3u64", [0x0706050403020100, 0x0F0E0D0C0B0A0908, 0x1716151413121110]),
    ("wrap_siphash_4u64", [0x0706050403020100, 0x0F0E0D0C0B0A0908, 0x1716151413121110, 0x1F1E1D1C1B1A1918]),
    ("wrap_siphash_1u32", [0x03020100]),
    ("wrap_siphash_2u32", [0x03020100, 0x07060504]),
    ("wrap_siphash_3u32", [0x03020100, 0x07060504, 0x0B0A0908]),
    ("wrap_siphash_4u32", [0x03020100, 0x07060504, 0x0B0A0908, 0x0F0E0D0C]),
    ("wrap_hsiphash_1u32", [0x03020100]),
    ("wrap_hsiphash_2u32", [0x03020100, 0x07060504]),
    ("wrap_hsiphash_3u32", [0x03020100, 0x07060504, 0x0B0A0908]),
    ("wrap_hsiphash_4u32", [0x03020100, 0x07060504, 0x0B0A0908, 0x0F0E0D0C]),
]
for fn, args in vec:
    out.append({"fn": fn, "args": args + [K0, K1]})

# A few all-edge scalar sweeps.
for fn, nargs in (("wrap_siphash_1u64", 1), ("wrap_siphash_4u64", 4),
                  ("wrap_siphash_1u32", 1), ("wrap_siphash_4u32", 4),
                  ("wrap_hsiphash_1u32", 1), ("wrap_hsiphash_4u32", 4)):
    for v in EDGE:
        out.append({"fn": fn, "args": [v] * nargs + [v, K1]})

with open("rust/wrappers-rs/extra-inputs.jsonl", "w", newline="\n") as f:
    for c in out:
        f.write(json.dumps(c) + "\n")
print("wrote", len(out), "extra cases")
