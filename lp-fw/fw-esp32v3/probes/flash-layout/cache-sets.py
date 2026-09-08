#!/usr/bin/env python3
"""Map an ESP32 image's flash-resident symbols onto the classic's flash cache.

ESP32 TRM 1.3.4: each CPU has a 32 KB, two-way set-associative cache with
32-byte blocks.  512 sets x 2 ways; way size 16 KB; set = (addr >> 5) & 0x1FF.
Both IROM (.text at 0x400D_xxxx) and DROM (.rodata at 0x3F40_xxxx) go through
the same per-core cache, and the MMU constraint paddr % 64K == vaddr % 64K
means the set index is the same whether the cache indexes virtual or physical
addresses.

Usage:
  cache-sets.py ELF --hot PATTERN [--hot PATTERN ...] [--nm NM]
  cache-sets.py ELF_A ELF_B --hot ...      # side by side

A "hot" symbol is any flash-resident (T/t/R/r) symbol whose demangled name
matches one of the regexes.  For every set the script counts hot lines and
reports sets holding more than the two ways, naming the symbols that collide.
"""
import argparse, re, subprocess, sys
from collections import defaultdict

LINE = 32
SETS = 512
WAYS = 2
SET_SHIFT = 5
SET_MASK = SETS - 1

IROM = (0x400D0000, 0x40400000)
DROM = (0x3F400000, 0x3F800000)

def in_flash(addr):
    return IROM[0] <= addr < IROM[1] or DROM[0] <= addr < DROM[1]

def load(nm, elf):
    out = subprocess.run([nm, '-S', '-C', '--defined-only', elf], capture_output=True, text=True, check=True).stdout
    syms = []
    for line in out.splitlines():
        parts = line.split(None, 3)
        if len(parts) != 4:
            continue
        addr, size, kind, name = parts
        try:
            addr = int(addr, 16); size = int(size, 16)
        except ValueError:
            continue
        if kind.lower() not in 'tr' or size == 0:
            continue
        if not in_flash(addr):
            continue
        syms.append((addr, size, kind, name))
    syms.sort()
    return syms

def lines_of(addr, size):
    first = addr >> SET_SHIFT
    last = (addr + size - 1) >> SET_SHIFT
    return range(first, last + 1)

def literal_refs(objdump, elf, hot):
    """Flash addresses each hot function's `l32r` loads read (its literal pool
    entries), plus the flash callees those literals point at when they name a
    function.  Returned as pseudo-symbols (addr, 4, 'L', 'lit:<owner>')."""
    refs = []
    for addr, size, kind, name in hot:
        out = subprocess.run([objdump, '-d', f'--start-address={addr:#x}', f'--stop-address={addr+size:#x}', elf],
                             capture_output=True, text=True).stdout
        for m in re.finditer(r'l32r\s+a\d+,\s*([0-9a-f]{8})', out):
            lit = int(m.group(1), 16)
            if in_flash(lit):
                refs.append((lit, 4, 'L', 'lit:' + short(name, 40)))
    return refs

def gamma_table(objdump, elf, syms):
    """GAMMA16 is an anonymous 513 x u32 rodata constant; find it through the
    DROM literal `encode_fixture_channel` loads."""
    for addr, size, kind, name in syms:
        if name.endswith('fixture_node::encode_fixture_channel'):
            out = subprocess.run([objdump, '-d', f'--start-address={addr:#x}', f'--stop-address={addr+size:#x}', elf],
                                 capture_output=True, text=True).stdout
            for m in re.finditer(r'l32r\s+a\d+,\s*([0-9a-f]{8})', out):
                lit = int(m.group(1), 16)
                dump = subprocess.run([objdump, '-s', f'--start-address={lit:#x}', f'--stop-address={lit+4:#x}', elf],
                                      capture_output=True, text=True).stdout.strip().splitlines()[-1]
                word = dump.split()[1]
                val = int.from_bytes(bytes.fromhex(word), 'little')
                if DROM[0] <= val < DROM[1]:
                    return (val, 513 * 4, 'GAMMA16 (rodata)')
    return None

def analyse(elf, nm, hot_res, objdump=None, extra=(), ranges=()):
    syms = load(nm, elf)
    if objdump:
        g = gamma_table(objdump, elf, syms)
        if g:
            ranges = list(ranges) + [g]
    hot = [s for s in syms if any(r.search(s[3]) for r in hot_res)]
    hot += [s for s in syms if s[3] in extra and s not in hot]
    hot += [(a, n, 'D', name) for a, n, name in ranges]
    if objdump:
        seen = set()
        for r in literal_refs(objdump, elf, hot):
            if r[0] not in seen:
                seen.add(r[0]); hot.append(r)
    # distinct hot LINES, each attributed to the first symbol that touches it
    line_owner = {}
    for addr, size, kind, name in hot:
        for ln in lines_of(addr, size):
            line_owner.setdefault(ln, name)
    occ = defaultdict(list)  # set -> [(line_addr, name)]
    for ln, name in sorted(line_owner.items()):
        occ[ln & SET_MASK].append((ln << SET_SHIFT, name))
    return syms, hot, occ

def short(name, n=70):
    return name if len(name) <= n else name[:n-1] + '…'

def report(elf, syms, hot, occ):
    total_lines = sum(len(v) for v in occ.values())
    funcs = [h for h in hot if h[2] != 'L']; lits = [h for h in hot if h[2] == 'L']
    print(f'== {elf}')
    print(f'   flash symbols: {len(syms)}   hot functions: {len(funcs)}   literal lines: {len({l[0] >> SET_SHIFT for l in lits})}   hot lines: {total_lines}   sets touched: {len(occ)}/{SETS}')
    for addr, size, kind, name in funcs:
        first = addr >> SET_SHIFT; last = (addr + size - 1) >> SET_SHIFT
        print(f'   {addr:08x} +{size:6d} {kind} sets {first & SET_MASK:3d}..{last & SET_MASK:3d} ({last-first+1:3d} lines) {short(name)}')
    over = {s: v for s, v in occ.items() if len(v) > WAYS}
    print(f'   sets with > {WAYS} hot lines: {len(over)}  (excess lines: {sum(len(v)-WAYS for v in over.values())})')
    for s in sorted(over):
        names = defaultdict(int)
        for _, n in occ[s]:
            names[n] += 1
        print(f'     set {s:3d}: {len(occ[s])} lines — ' + '; '.join(f'{short(n, 45)} x{c}' for n, c in names.items()))
    return over

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('elf', nargs='+')
    ap.add_argument('--hot', action='append', default=[])
    ap.add_argument('--nm', default='xtensa-esp32-elf-nm')
    ap.add_argument('--objdump', default=None, help='follow l32r literal loads of hot functions')
    ap.add_argument('--hot-file', default=None, help='file of exact demangled symbol names, one per line')
    ap.add_argument('--hot-range', action='append', default=[], help='ADDR:SIZE:NAME explicit flash data range (anonymous rodata such as GAMMA16)')
    args = ap.parse_args()
    hot_res = [re.compile(p) for p in args.hot]
    extra = set()
    if args.hot_file:
        extra = {l.strip() for l in open(args.hot_file) if l.strip() and not l.startswith('#')}
    for elf in args.elf:
        ranges = []
        for r in args.hot_range:
            a, n, name = r.split(':', 2)
            ranges.append((int(a, 16), int(n, 0), name))
        syms, hot, occ = analyse(elf, args.nm, hot_res, args.objdump, extra, ranges)
        report(elf, syms, hot, occ)

if __name__ == '__main__':
    main()
