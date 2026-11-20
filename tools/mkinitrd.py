#!/usr/bin/env python3
# Emit a newc ("070701") cpio archive to stdout.
# Usage: mkinitrd.py <file> <arcname> [<file> <arcname> ...]
# Format per entry: 110-byte ascii-hex header, NUL-terminated name,
# 4-align, data, 4-align; a TRAILER!!! record ends the archive.
import sys

def entry(name, data, ino, mode=0o100644):
    hdr = b"070701" + b"".join(b"%08x" % v for v in [
        ino, mode, 0, 0, 1, 0, len(data), 0, 1, 0, 0, len(name) + 1, 0])
    out = hdr + name.encode() + b"\0"
    out += b"\0" * ((4 - len(out) % 4) % 4)
    out += data + b"\0" * ((4 - len(data) % 4) % 4)
    return out

out = b""
ino = 1
for path, arc in zip(sys.argv[1::2], sys.argv[2::2]):
    out += entry(arc, open(path, "rb").read(), ino)
    ino += 1
out += entry("TRAILER!!!", b"", ino)
sys.stdout.buffer.write(out)
