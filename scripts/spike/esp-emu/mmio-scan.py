"""Static MMIO scan of an rv32 disassembly (rust-objdump -d output).

Finds every `lui rd, IMM` whose upper-20 lands a peripheral window
(0x2000_0000 CLINT/PLIC, 0x6000_0000..0x600F_FFFF HP/LP peripherals) and
follows the next few instructions in the same basic block for uses of rd:
lw/sw/lh/sh/lb/sb/amo* with an offset, or an addi that refines the base.
Emits (peripheral, absolute register address, access kind, function) rows.
"""
import re, sys, bisect
from collections import defaultdict

PAC = {}
for line in open(sys.argv[2]):
    a, n = line.split()
    PAC[int(a, 16)] = n
bases = sorted(PAC)
def periph(addr):
    i = bisect.bisect_right(bases, addr) - 1
    if i < 0: return None
    b = bases[i]
    # windows: HP/LP peripherals are 4 KiB or 1 KiB apart; assume up to next base
    nxt = bases[i+1] if i+1 < len(bases) else b + 0x10000
    if addr >= nxt: return None
    if addr - b > 0x10000: return None
    return PAC[b]

func_re = re.compile(r'^([0-9a-f]+) <(.+)>:$')
ins_re = re.compile(r'^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$')
lui_re = re.compile(r'^(\w+), (0x[0-9a-f]+|\d+)$')
mem_re = re.compile(r'^(\w+), (-?\d+|-?0x[0-9a-f]+)\((\w+)\)$')
addi_re = re.compile(r'^(\w+), (\w+), (-?\d+|-?0x[0-9a-f]+)$')
BR = ('j','jal','jalr','jr','ret','beq','bne','blt','bge','bltu','bgeu','beqz','bnez','blez','bgez','bltz','bgtz','c.j','c.jal','c.jr','c.jalr','c.beqz','c.bnez','ecall','mret','wfi')

rows = defaultdict(lambda: defaultdict(int))  # (periph, addr, kind) -> {func: count}
cur = '?'
regs = {}  # reg -> base value
for line in open(sys.argv[1]):
    m = func_re.match(line)
    if m:
        cur = m.group(2); regs = {}; continue
    m = ins_re.match(line)
    if not m: continue
    op, args = m.group(2), m.group(3).strip()
    if op in BR or op.startswith('b') and op not in ('bclr','bext','binv','bset'):
        # branch/jump ends tracking; be conservative (keep regs across fallthrough labels though)
        if op in ('ret','jr','j','mret','ecall','wfi','c.j','c.jr'): regs = {}
        continue
    if op == 'lui':
        mm = lui_re.match(args)
        if mm:
            v = int(mm.group(2), 0) << 12
            if 0x2000_0000 <= v < 0x2001_0000 or 0x6000_0000 <= v < 0x6010_0000:
                regs[mm.group(1)] = v
            else:
                regs.pop(mm.group(1), None)
        continue
    if op in ('addi','c.addi','addiw'):
        mm = addi_re.match(args)
        if mm and mm.group(2) in regs and mm.group(1) != 'zero':
            regs[mm.group(1)] = regs[mm.group(2)] + int(mm.group(3), 0)
            continue
        elif mm:
            regs.pop(mm.group(1), None)
        continue
    if op in ('lw','sw','lh','sh','lb','sb','lhu','lbu','c.lw','c.sw') or op.startswith('amo') or op.startswith('lr.') or op.startswith('sc.'):
        mm = mem_re.match(args)
        if mm and mm.group(3) in regs:
            addr = regs[mm.group(3)] + int(mm.group(2), 0)
            p = periph(addr)
            if p:
                kind = 'R' if op[0] in ('l',) or op.startswith('lr') or op.startswith('c.l') else ('RW' if op.startswith('amo') else 'W')
                rows[(p, addr, kind)][cur] += 1
            if mm.group(1) in regs and op[0] == 'l': regs.pop(mm.group(1), None)
        elif mm and op[0] == 'l':
            regs.pop(mm.group(1), None)
        continue
    # any other instruction writing a tracked reg invalidates it
    dst = args.split(',')[0].strip() if args else ''
    if dst in regs: regs.pop(dst, None)

import json
out = []
for (p, addr, kind), funcs in sorted(rows.items(), key=lambda kv: (kv[0][1], kv[0][2])):
    out.append({'periph': p, 'addr': f'0x{addr:08x}', 'kind': kind, 'sites': sum(funcs.values()), 'funcs': sorted(funcs, key=lambda f: -funcs[f])[:6]})
json.dump(out, open(sys.argv[3], 'w'), indent=1)
by_p = defaultdict(lambda: [0, set()])
for r in out:
    by_p[r['periph']][0] += r['sites']; by_p[r['periph']][1].add(r['addr'])
print(f"{'periph':16} {'regs':>5} {'sites':>6}")
for p, (n, regs_) in sorted(by_p.items(), key=lambda kv: -kv[1][0]):
    print(f"{p:16} {len(regs_):5d} {n:6d}")
