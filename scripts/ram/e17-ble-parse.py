#!/usr/bin/env python3
"""E17: split a sitting's USB console (usb.txt) by its marks.tsv and print,
per phase, the heartbeat `memory` (free / used / largest block), and every
`[ble]`, `[radio-heap]`, `[stack]`, `[*stack]` and fault line in order.

usage: e17-ble-parse.py <run-dir> [--lines]
"""

import json
import re
import sys
from pathlib import Path

HB = re.compile(r'^M!(\{.*"heartbeat".*)$')
KEEP = re.compile(
    r"\[ble\] link\d+: (connected|notifications on|disconnected)|\[radio-heap\]|\[stack\]|stack\] high-water"
    r"|\[e17|Guru|panic|rst:0x|\[RECOVERY\]|alloc .* failed|overflow"
)


def phases(run):
    marks = []
    for ln in (run / "marks.tsv").read_text().splitlines():
        _, secs, n, label = ln.split("\t")
        marks.append((int(n), label))
    return marks


def phase_of(i, marks):
    cur = "pre"
    for n, label in marks:
        if i >= n:
            cur = label
    return cur


def heartbeat(line):
    m = HB.match(line)
    if not m:
        return None
    try:
        msg = json.loads(m.group(1))["msg"]["heartbeat"]
    except (json.JSONDecodeError, KeyError):
        # A truncated console line: pull the memory object out by regex.
        mm = re.search(r'"uptime_ms":(\d+).*"memory":\{"freeBytes":(\d+),"usedBytes":(\d+),"totalBytes":\d+,"largestFreeBlock":(\d+)', line)
        if not mm:
            return None
        return tuple(int(x) for x in mm.groups())
    mem = msg.get("memory") or {}
    return (msg.get("uptime_ms"), mem.get("freeBytes"), mem.get("usedBytes"), mem.get("largestFreeBlock"))


def main():
    run = Path(sys.argv[1])
    show_lines = "--lines" in sys.argv
    marks = phases(run)
    # The driver's --usb-respawn writes one file per capture after a reset.
    lines = []
    for f in [run / "usb.txt"] + sorted(run.glob("usb-r*.txt")):
        if f.exists():
            lines += f.read_bytes().decode("utf-8", "replace").splitlines()
    by_phase = {}
    order = []
    events = []
    for i, ln in enumerate(lines):
        ph = phase_of(i, marks)
        if ph not in by_phase:
            by_phase[ph] = []
            order.append(ph)
        hb = heartbeat(ln)
        if hb:
            by_phase[ph].append(hb)
        elif KEEP.search(ln):
            events.append((ph, i, ln.split(": ", 1)[-1] if "fw_esp32" in ln else ln))
    print(f"# {run}")
    print("| phase (after mark) | heartbeats | uptime s | free B (min–max) | free median | used B (min–max) | largest block B (min–max) | largest median |")
    print("|---|---:|---|---|---:|---|---|---:|")
    for ph in order:
        hbs = by_phase[ph]
        if not hbs:
            print(f"| {ph} | 0 | | | | | | |")
            continue
        up = [h[0] / 1000 for h in hbs if h[0] is not None]
        col = lambda k: f"{min(h[k] for h in hbs)}–{max(h[k] for h in hbs)}"
        med = lambda k: sorted(h[k] for h in hbs)[len(hbs) // 2]
        print(f"| {ph} | {len(hbs)} | {min(up):.0f}–{max(up):.0f} | {col(1)} | {med(1)} | {col(2)} | {col(3)} | {med(3)} |")
    print()
    print("## events")
    for ph, i, ln in events:
        print(f"- [{ph} @ line {i}] {ln[:240]}")
    if show_lines:
        print("\n## every heartbeat")
        for ph in order:
            for h in by_phase[ph]:
                print(f"{ph}\t{h[0]}\t{h[1]}\t{h[2]}\t{h[3]}")


if __name__ == "__main__":
    main()
