#!/usr/bin/env python3
"""Lay a split-link image out for the app partition (written at 0x10000).

    assemble.py --page 0x8000 loader.bin core.bin engine.bin app.bin

Mirrors lp-base/lp-bootctl/src/split_layout.rs and boot_record.rs: the
loader at 0x10000, an initial boot record (seq 1, not on trial) in the first
record sector, the core at the region's low end (0x18000), the engine at the
first page after the core. A first flash is a proven boot; only an update
writes a trial record.
"""

import argparse
import struct
import sys
import zlib

LOADER_OFFSET = 0x1_0000
LOADER_MAX_LEN = 0x6000
BOOT_RECORD_SECTORS = (0x1_6000, 0x1_7000)
REGION_START = 0x1_8000
REGION_END = 0x31_0000
MAGIC = int.from_bytes(b"LPBR", "little")
VERSION = 1


def record(seq, core_off, core_len, build, trial=False):
    body = struct.pack("<IHHIIII", MAGIC, VERSION, 1 if trial else 0, seq, core_off, core_len, build)
    return body + struct.pack("<I", zlib.crc32(body) & 0xFFFF_FFFF)


def build_hash(engine):
    """lp_bootctl::build_hash of the build id in the engine's header."""
    if engine[:8] != b"LPENGIN1":
        sys.exit("engine.bin has no LPENGIN1 header")
    return zlib.crc32(engine[8:56]) & 0xFFFF_FFFF


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--page", type=lambda s: int(s, 0), required=True)
    ap.add_argument("loader")
    ap.add_argument("core")
    ap.add_argument("engine")
    ap.add_argument("out")
    a = ap.parse_args()
    loader, core, engine = (open(p, "rb").read() for p in (a.loader, a.core, a.engine))

    if len(loader) > LOADER_MAX_LEN:
        sys.exit(f"loader is {len(loader)} B; the boot records start {LOADER_MAX_LEN} B in")
    engine_at = -(-(REGION_START + len(core)) // a.page) * a.page
    if engine_at + len(engine) > REGION_END:
        sys.exit(f"core {len(core)} B + engine {len(engine)} B do not fit the region "
                 f"({REGION_END - REGION_START} B at page {a.page:#x})")

    img = bytearray(b"\xff" * (engine_at + len(engine) - LOADER_OFFSET))
    def put(at, data):
        img[at - LOADER_OFFSET:at - LOADER_OFFSET + len(data)] = data
    put(LOADER_OFFSET, loader)
    put(BOOT_RECORD_SECTORS[0], record(1, REGION_START, len(core), build_hash(engine)))
    put(REGION_START, core)
    put(engine_at, engine)
    open(a.out, "wb").write(img)

    free = REGION_END - engine_at - len(engine)
    print(f"loader {len(loader)} B · core {len(core)} B @{REGION_START:#x} · "
          f"engine {len(engine)} B @{engine_at:#x} · app.bin {len(img)} B · "
          f"room left {free} B ({free // 1024} KiB)")


if __name__ == "__main__":
    main()
