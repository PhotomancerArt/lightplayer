#!/usr/bin/env python3
"""Per-section sizes and digests of an ELF, for comparing two builds.

    scripts/emu/elf-section-digest.py <elf>

Prints one line per section — index, name, file size, and the first 16 hex
digits of the sha256 of its bytes — then the file's own size and sha256. Two
builds that disagree can then be compared by reading two logs, which is the
only way to compare a Mac's build with a GitHub runner's without shipping a
9 MB artefact between them.

Why a script rather than `readelf -S`: sizes alone do not say which section
moved when two builds are the same size, `readelf` is not on a stock macOS,
and a digest per section turns "the shas differ" into "`.text` differs and
`.rodata` does not", which is a different investigation.

Pure stdlib, no dependencies, and it reads the section header table directly
(32-bit little-endian ELF, which every image this repository builds for the
C6 is).
"""

import hashlib
import struct
import sys


def sections(blob: bytes):
    if blob[:4] != b"\x7fELF":
        raise SystemExit("not an ELF")
    if blob[4] != 1 or blob[5] != 1:
        raise SystemExit("only 32-bit little-endian ELF is supported here")
    e_shoff, = struct.unpack_from("<I", blob, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", blob, 0x2E)
    heads = []
    for i in range(e_shnum):
        off = e_shoff + i * e_shentsize
        name, typ, flags, addr, offset, size = struct.unpack_from("<IIIIII", blob, off)
        heads.append((name, typ, flags, addr, offset, size))
    str_off = heads[e_shstrndx][4]

    def name_at(idx: int) -> str:
        end = blob.index(b"\0", str_off + idx)
        return blob[str_off + idx : end].decode("utf-8", "replace")

    for i, (name, typ, flags, addr, offset, size) in enumerate(heads):
        # SHT_NOBITS (8) occupies no file bytes; everything else does.
        body = b"" if typ == 8 else blob[offset : offset + size]
        yield i, name_at(name), addr, size, hashlib.sha256(body).hexdigest()[:16]


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    path = sys.argv[1]
    with open(path, "rb") as f:
        blob = f.read()
    print(f"elf-section-digest: {path}")
    for i, name, addr, size, digest in sections(blob):
        print(f"  [{i:2}] {name:<22} addr 0x{addr:08x} size {size:>9} {digest}")
    print(f"  file size {len(blob)} sha256 {hashlib.sha256(blob).hexdigest()}")


if __name__ == "__main__":
    main()
