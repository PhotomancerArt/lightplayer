#!/usr/bin/env python3
"""spike: name guest addresses from an ELF's symbol table.  symof.py <elf> <hex-addr>..."""
import bisect
import subprocess
import sys

elf = sys.argv[1]
out = subprocess.run(["nm", "-n", "-C", elf], capture_output=True, text=True).stdout
syms = []
for line in out.splitlines():
    p = line.split(None, 2)
    if len(p) == 3 and p[1] in "tTwW":
        syms.append((int(p[0], 16), p[2]))
addrs = [s[0] for s in syms]
for arg in sys.argv[2:]:
    a = int(arg, 16)
    i = bisect.bisect_right(addrs, a) - 1
    if i >= 0 and a - syms[i][0] < 0x4000:
        print(f"{a:#x} = {syms[i][1]}+{a - syms[i][0]:#x}")
    else:
        print(f"{a:#x} = ? (nearest below: {syms[i][1] if i >= 0 else None})")
