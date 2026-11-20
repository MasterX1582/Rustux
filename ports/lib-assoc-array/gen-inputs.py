import json

# wrap_assoc_seq(seed, nops_i, seed2) drives the whole assoc_array API:
#   seed  -> deterministic op stream (insert/cluster/burst/delete/find/
#            iterate/gc/clear/destroy/misc, incl. cancel_edit and
#            insert_set_object via flag bits on the op word)
#   nops_i -> op count mod 190 (negative exercised via iabs)
#   seed2 -> periodic full-state-dump cadence (|s2| % 7) and the
#            alloc-failure mask (fail_mask = |s2| * phi64; s2 % 5 == 1
#            fails every allocation -> all -ENOMEM paths)
#
# The grid below pairs a seed spread with nops sizes and seed2 masks so
# every API entry point, every edit-script branch (empty tree, in-node,
# node split, shortcut insert, mid-shortcut, delete collapse, clear,
# gc pack/fold/excise) and the failure-injection paths are hit.
FN = "wrap_assoc_seq"

SEEDS = [0, 1, 2, 3, 7, 13, 42, 100, 255, 1000, 4096, 65535,
         0xDEADBEEF, 0x123456789, 0xCAFEBABE, 0xFFFFFFFF,
         0x100000000, 0x7FFFFFFFFFFFFFFF, 0x8000000000000000,
         0xFFFFFFFFFFFFFFFF]

NOPS = [0, 1, 3, 17, 64, 149, 189, 190, 250, -1, -64]

# seed2 values: 0 (no failures, dump every 5), the selftest's own mask
# set, values that light up dense/sparse fail bits, the %5==1 fail-all
# class, negatives, and values that shift the dump cadence 5..11.
S2 = [0, 1, 2, 3, 4, 6, 7, 11, 16, 21, 26, 42, 101, 255, 1024,
      -7, -1, -21, 0xFFFF, 0x100000, 0x7FFFFFFFFFFFFFFF,
      -0x8000000000000000]

out = []

# Full seed x nops sweep at the two most useful masks: clean runs
# (s2=0) and partial failure injection (s2=3).
for s in SEEDS:
    for n in NOPS:
        out.append({"fn": FN, "args": [s, n, 0]})
        out.append({"fn": FN, "args": [s, n, 3]})

# Seed x seed2 sweep at a mid-size op count: exercises every dump
# cadence and every fail_mask class.
for s in SEEDS:
    for s2 in S2:
        out.append({"fn": FN, "args": [s, 149, s2]})

# Selftest grid verbatim (keeps the C aggregate meaningful).
for s in [1, 3, 42, 255, 1000, 65535, 0xDEADBEEF, 0x123456789]:
    for s2 in [0, 1, 2, 3, 6, 11, 21, 255, -7]:
        out.append({"fn": FN, "args": [s, 149, s2]})

# Long runs on a few seeds at several masks: deep trees, pool
# exhaustion, gc pressure, shortcut chains.
for s in [42, 1000, 0xDEADBEEF, 0x123456789, 0xCAFEBABE]:
    for s2 in S2:
        out.append({"fn": FN, "args": [s, 189, s2]})

with open("rust/wrappers-rs/extra-inputs.jsonl", "w", newline="\n") as f:
    for c in out:
        f.write(json.dumps(c) + "\n")
print("wrote", len(out), "extra cases")
