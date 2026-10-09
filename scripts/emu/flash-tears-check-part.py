#!/usr/bin/env python3
"""Check that every boot of a `flash-tears` transcript read the stated flash part.

    scripts/emu/flash-tears-check-part.py <transcript.txt> 0x464016

The soak driver (`flash-tears-soak.sh`) states the part's JEDEC id in each
batch's sidecar `note` before the batch runs — a silicon configuration
cannot read it, the board's own `ft-boot` records can. This runs after the
batch: exit 0 when every `ft-boot` record carries `expected`, exit 4 (and
say which ids it found) when any does not, so a different part stops the
sitting instead of filing tears under the wrong name. Reads the file; never
edits it.
"""

import json
import sys

TAG = "[fw-check-json] "


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    path, expected = sys.argv[1], sys.argv[2].lower()
    ids: dict[str, int] = {}
    with open(path, "rb") as f:
        for raw in f:
            line = raw.decode("utf-8", errors="replace")
            if TAG not in line:
                continue
            rec = json.loads(line.split(TAG, 1)[1])
            if rec.get("kind") == "ft-boot":
                fid = str(rec.get("flash_id", "?")).lower()
                ids[fid] = ids.get(fid, 0) + 1
    if not ids:
        print(f"STOPPED: {path} has no ft-boot record", file=sys.stderr)
        return 4
    if set(ids) != {expected}:
        print(
            f"STOPPED: {path} reports flash id(s) {ids}; its sidecar says {expected}. "
            "A different part: fix FLASH_JEDEC in flash-tears-soak.sh and say so in the report",
            file=sys.stderr,
        )
        return 4
    print(f"flash id {expected} on all {ids[expected]} boots of {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
