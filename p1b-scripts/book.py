import sys, collections
sys.path.insert(0, '/private/tmp/claude-502/-Users-yona-dev-photomancer-lp2025--claude-worktrees-sleepy-moore-868468/e37cf271-ceb4-4a4d-aac7-a869e80072ec/scratchpad')
from bucket import run, ROOT

# The per-slot bookkeeping actions, by (file, line) of the innermost workspace frame.
ACT = {}
for l in (797, 798):
    ACT[('mod.rs', l)] = 'budget compare (per slot, !whole only)'
ACT[('mod.rs', 804)] = 'set_issuing'
for l in (824,):
    ACT[('mod.rs', l)] = 'instruction_count += 1'
for l in (825, 1157, 1158, 1159):
    ACT[('mod.rs', l)] = 'charge (cycles_for + add)'
for l in (826, 827, 828):
    ACT[('mod.rs', l)] = 'pc = new_pc.unwrap_or(pc+size)'
ACT[('mod.rs', 829)] = 'ran += 1'
for l in (833, 834, 835, 836, 837, 838, 839, 840, 841):
    ACT[('mod.rs', l)] = 'side-band / yield test'
for l in (842, 1168, 1169, 1170, 1171, 1172, 1173):
    ACT[('mod.rs', l)] = 'charge_memory'
for l in (851, 852, 853, 854, 855):
    ACT[('mod.rs', l)] = 'straight-on check + pc advance'
ACT[('mod.rs', 819)] = 'debug_assert (nothing in release)'
ACT[('mod.rs', 0)] = 'mach/mod.rs unattributed line'
ACT[('cycle_model.rs', 0)] = 'charge (cycles_for + add)'
for l in range(139, 175):
    ACT[('cycle_model.rs', l)] = 'charge (cycles_for + add)'
for l in (1676, 1677, 1678, 1679, 1680):
    ACT[('bus.rs', l)] = 'set_issuing'
for l in (1681, 1682, 1683, 1684, 1685, 1686, 1687, 1688):
    ACT[('bus.rs', l)] = 'side-band / yield test'
for l in (1689, 1690, 1691, 1692):
    ACT[('bus.rs', l)] = 'charge_memory'
for l in (1724, 1725, 1726, 1727, 1728, 1729, 1730, 1731, 1732, 1733, 1734):
    ACT[('bus.rs', l)] = 'note_cached_execute (strict only)'

for path in sys.argv[1:]:
    tot, b, lines = run(path)
    print('###', path, ' bookkeeping =', f'{100*b["bookkeeping"]/tot:.1f}%')
    acts = collections.Counter()
    for (k, rel, line, fn), v in lines.items():
        if k != 'bookkeeping':
            continue
        base = rel.split('/')[-1]
        acts[ACT.get((base, line), f'?? {base}:{line} {fn}')] += v
    for a, v in acts.most_common():
        print(f'  {100*v/tot:5.2f}  {a}')
