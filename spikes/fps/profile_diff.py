#!/usr/bin/env python3
"""SPIKE (fps spike): per-function ms/frame from a silicon PC profile, and the
difference between two profiles. Usage:
  profile_diff.py <elf> <rom.elf> <console_a> <tick_ms_a> [<console_b> <tick_ms_b> <divisor>]
Prints ms per frame per function for A (or (A-B)/divisor)."""
import bisect, collections, os, re, subprocess, sys, glob
NM = glob.glob(os.path.expanduser("~/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm"))[0]
def syms(elf):
    rows = []
    for line in subprocess.run([NM, "-C", "-n", elf], capture_output=True, text=True).stdout.splitlines():
        p = line.split(" ", 2)
        if len(p) == 3 and p[0] and p[1] in "tTwW":
            rows.append((int(p[0], 16), p[2]))
    return sorted(rows)
elf, rom = sys.argv[1], sys.argv[2]
table = syms(elf) + syms(rom); table.sort(); addrs = [a for a, _ in table]
def name(pc):
    if 0x42800000 <= pc < 0x43000000: return "JIT (PSRAM)"
    i = bisect.bisect_right(addrs, pc) - 1
    return table[i][1] if i >= 0 and pc - addrs[i] < 0x8000 else f"?{pc:#x}"
def short(n):
    n = re.sub(r"<([^<>]*?) as [^<>]*?>", r"\1", n)
    return n[:110]
def prof(console, tick):
    c = collections.Counter()
    for line in open(console, errors="replace"):
        m = re.search(r"\[pcs\] \d+\.\d+ ((?:[0-9a-f]{8} ?)+)", line)
        if m:
            for h in m.group(1).split(): c[short(name(int(h, 16)))] += 1
    tot = sum(c.values())
    return {k: v / tot * tick for k, v in c.items()}, tot
a, na = prof(sys.argv[3], float(sys.argv[4]))
if len(sys.argv) > 5:
    b, nb = prof(sys.argv[5], float(sys.argv[6])); div = float(sys.argv[7])
    keys = set(a) | set(b)
    d = {k: (a.get(k, 0) - b.get(k, 0)) / div for k in keys}
    print(f"samples A={na} B={nb}; ms/frame per unit = (A-B)/{div}; total {sum(d.values()):.2f}")
else:
    d = a; print(f"samples {na}; total {sum(d.values()):.2f} ms/frame")
for k, v in sorted(d.items(), key=lambda kv: -kv[1])[:45]:
    print(f"{v:7.3f}  {k}")
