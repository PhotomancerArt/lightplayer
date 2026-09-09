import subprocess, sys, json
pcs, binp, outp = sys.argv[1], sys.argv[2], sys.argv[3]
SENT = '0x1fffffff0'
addrs = []; counts = []; total = 0
for line in open(pcs):
    if line.startswith('#'):
        if line.startswith('# total'):
            total = int(line.split()[2])
        continue
    p = line.split()
    addrs.append(hex(int(p[1]))); counts.append(int(p[2]))
args = []
for a in addrs:
    args += [a, SENT]
res = subprocess.run(['atos', '-o', binp, '-l', '0x100000000', '-i', '--fullPath'] + args,
                     capture_output=True, text=True).stdout.split('\n')
res = [l for l in res if l.strip() != '']
groups = []; cur = []
for l in res:
    if l.strip() == SENT:
        groups.append(cur); cur = []
    else:
        cur.append(l)
assert len(groups) == len(addrs), (len(groups), len(addrs))
json.dump({'total': total,
           'entries': [{'addr': a, 'count': c, 'frames': g}
                       for a, c, g in zip(addrs, counts, groups)]},
          open(outp, 'w'))
unres = sum(c for c, g in zip(counts, groups) if len(g) == 1 and g[0].startswith('0x'))
print(pcs, 'total', total, 'sites', len(addrs), 'unresolved_samples', unres)
