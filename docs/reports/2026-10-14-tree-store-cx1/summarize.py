#!/usr/bin/env python3
"""Summarize the CX1 captures in this directory (the 2026-10-14 tree-store sitting).

    python3 docs/reports/2026-10-14-tree-store-cx1/summarize.py

`*-format-*.txt`, `*-mount-*.txt`: one boot each (scripts/emu/reset-and-capture.py),
read for the `[perf] fs mount <us> us` line (`heap_peak_diag`'s mount timer,
which also covers a first-boot format).

`*-rest-*.txt`: `lp-cli link capture --request reboot` runs. Only the boot after
the reboot is read (the lines after the last `[link] reset (PeerRestarted)`):
its own `[perf] fs mount` line, and the `[heap] … largest=…` lines that come
after the board's first `[wifi] connected` heartbeat line, skipping the first
of those (its window still holds the boot's own load), as the emulated walk
skips the first line after a reboot.
"""

import glob
import os
import re
import statistics

HERE = os.path.dirname(os.path.abspath(__file__))
MOUNT = re.compile(rb"\[perf\] fs mount (\d+) us")
HEAP = re.compile(rb"\[heap\] peak=(\d+) live=(\d+) max_usage=(\d+) largest=(\d+)")


def spread(xs):
    return f"median {statistics.median(xs):,.0f} (range {min(xs):,}–{max(xs):,}, n={len(xs)})"


def boot_lines(path):
    data = open(path, "rb").read()
    cut = data.rfind(b"[link] reset (PeerRestarted)")
    return data[cut:] if cut >= 0 else data


def main():
    groups = {}
    for path in sorted(glob.glob(os.path.join(HERE, "*-*-*.txt"))):
        name = os.path.basename(path)[:-4]
        image, kind = name.split("-", 1)
        kind = kind.rsplit("-", 1)[0]
        groups.setdefault((image, kind), []).append(path)

    for (image, kind), paths in sorted(groups.items()):
        if kind in ("rest", "late"):
            mounts, largest, live, max_usage, joined = [], [], [], [], 0
            for p in paths:
                boot = boot_lines(p)
                m = MOUNT.search(boot)
                if m:
                    mounts.append(int(m.group(1)))
                at = boot.find(b"[wifi] connected")
                if at < 0:
                    continue
                joined += 1
                lines = [tuple(map(int, h.groups())) for h in HEAP.finditer(boot, at)][1:]
                run_largest = [l[3] for l in lines]
                print(f"  {os.path.basename(p)}: {len(lines)} lines, largest {sorted(set(run_largest))}, live {sorted(set(l[1] for l in lines))}")
                largest.append(statistics.median(run_largest))
                live.append(statistics.median(l[1] for l in lines))
                max_usage.append(lines[-1][2])
            print(f"{image} {kind}: joined {joined}/{len(paths)} boots")
            print(f"  largest free block (per-boot median): {spread([int(x) for x in largest])}")
            print(f"  live: {spread([int(x) for x in live])}")
            print(f"  max_usage: {spread(max_usage)}")
            if mounts:
                print(f"  fs mount µs on those boots: {spread(mounts)}")
        else:
            us = []
            for p in paths:
                m = MOUNT.search(open(p, "rb").read())
                us.append(int(m.group(1)) if m else None)
            got = [u for u in us if u is not None]
            print(f"{image} {kind}: {spread(got)}; each {us}")

    # (a) mount with c40 stored, every boot of every protocol: the chip resets
    # (`mount-c40`, `lateboot`) and the software reboots (`rest`).
    for image in sorted({i for i, _ in groups}):
        us = []
        for kind in ("mount-c40", "lateboot", "rest"):
            for p in groups.get((image, kind), []):
                m = MOUNT.search(boot_lines(p))
                if m:
                    us.append(int(m.group(1)))
        print(f"{image} mount with c40, all boots: {spread(us)}")


if __name__ == "__main__":
    main()
