import json, re, sys, collections, os

ROOT = '/Users/yona/dev/photomancer/lp2025/.claude/worktrees/agent-a1676bc71e062fc30/'

FRAME_RE = re.compile(r'\(([^()]*):(\d+)\)\s*$')
FN_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z0-9_]+)')

_fnmap_cache = {}


SHADOW = '/Users/yona/dev/photomancer/lp2025/.claude/worktrees/agent-a1676bc71e062fc30/target/p1b/src/'


def fnmap(path):
    """line number -> enclosing top-level-ish fn name, by source scan.

    Reads the sources as they were at 6d14b7834, not as they are now: the
    profile's line numbers are the merged tree's, and this branch has since
    edited two of the files."""
    if path.startswith(ROOT):
        alt = SHADOW + path[len(ROOT):]
        if os.path.exists(alt):
            path = alt
    if path in _fnmap_cache:
        return _fnmap_cache[path]
    m = {}
    try:
        lines = open(path).read().split('\n')
    except OSError:
        _fnmap_cache[path] = m
        return m
    cur = '?'
    for i, l in enumerate(lines, 1):
        g = FN_RE.match(l)
        if g:
            cur = g.group(1)
        m[i] = cur
    _fnmap_cache[path] = m
    return m


def innermost_workspace(frames):
    for f in frames:
        g = FRAME_RE.search(f)
        if not g:
            continue
        path, line = g.group(1), int(g.group(2))
        if path.startswith(ROOT):
            return path[len(ROOT):], line
    return None, None


def bucket(rel, line):
    fn = fnmap(ROOT + rel).get(line, '?')
    if rel == 'lp-emu/lp-emu-core/src/block.rs':
        return 'block dispatch'
    if rel == 'lp-emu/lp-riscv-emu/src/mach/block.rs':
        return 'block dispatch'
    if rel == 'lp-emu/lp-emu-core/src/cycle_model.rs':
        return 'bookkeeping'
    if rel == 'lp-emu/lp-riscv-emu/src/mach/mod.rs':
        if fn in ('run_blocks', 'run_slice_cached', 'run_slice', 'drain_block_flush'):
            return 'block dispatch'
        if fn == 'run_block':
            if line in (806, 807, 817):
                return 'slot handler call'
            return 'bookkeeping'
        if fn in ('charge', 'charge_memory'):
            return 'bookkeeping'
        if fn in ('step_once', 'step', 'run_slice_stepping'):
            return 'uncached step path'
        if line == 0:
            return 'bookkeeping'   # mach/mod.rs:0 — the run_block body, unattributed
        return 'machine / scheduler / host'
    if rel.startswith('lp-emu/lp-riscv-emu/src/emu/executor/'):
        if fn.startswith('decode_execute'):
            return 'decode + dispatch'
        return 'execute'
    if rel.startswith('lp-riscv/lp-riscv-inst/'):
        return 'decode + dispatch'
    if rel == 'lp-emu/esp/lp-emu-esp-common/src/bus.rs':
        return BUSFN.get(fn, 'bus: MMIO + peripheral models')
    if rel.startswith('lp-emu/esp/'):
        return 'machine / scheduler / host'
    if rel.startswith('lp-emu/lp-emu-core/src/'):
        return 'machine / scheduler / host'
    return 'machine / scheduler / host'


BUSFN = {}


def load_busfn():
    """Classify SocBus methods into RAM / MMIO / region-lookup / bookkeeping."""
    ram = {'read_word', 'write_word', 'read_byte', 'write_byte', 'read_half',
           'write_half', 'read_ram', 'write_ram', 'fetch_instruction',
           'read_region', 'write_region', 'read', 'write',
           'check_watchpoints', 'check_watchpoints_slow',
           'check_store_watchpoint', 'require_mmio_alignment', 'in_mmio_window'}
    lookup = {'region_of', 'region_index', 'region_index_slow',
              'fetch_region_index', 'find_region', 'resource_of',
              'region_for', 'mmio_index', 'mmio_index_slow', 'lookup_region',
              'region_mut'}
    book = {'set_issuing', 'take_sideband', 'take_yield', 'take_memory_cost',
            'note_cached_execute', 'fetch_is_pure', 'note_fence_i'}
    for k in ram:
        BUSFN[k] = 'bus: RAM data access'
    for k in lookup:
        BUSFN[k] = 'bus: region lookup'
    for k in book:
        BUSFN[k] = 'bookkeeping'


load_busfn()

order = ['block dispatch', 'slot handler call', 'decode + dispatch', 'execute',
         'bookkeeping', 'bus: RAM data access', 'bus: region lookup',
         'bus: MMIO + peripheral models', 'bus: other', 'uncached step path',
         'machine / scheduler / host', 'external (malloc, clock_gettime)']


def run(path):
    d = json.load(open(path))
    tot = d['total']
    b = collections.Counter()
    lines = collections.Counter()
    for e in d['entries']:
        rel, line = innermost_workspace(e['frames'])
        if rel is None:
            b['external (malloc, clock_gettime)'] += e['count']
            continue
        k = bucket(rel, line)
        b[k] += e['count']
        lines[(k, rel, line, fnmap(ROOT + rel).get(line, '?'))] += e['count']
    return tot, b, lines


if __name__ == '__main__':
    names = sys.argv[1:]
    res = {n: run(n) for n in names}
    keys = list(order) + [k for n in names for k in res[n][1] if k not in order]
    seen = set(); keys = [k for k in keys if not (k in seen or seen.add(k))]
    print('| bucket | ' + ' | '.join(os.path.basename(n).replace('p1b-', '').replace('.json', '') for n in names) + ' |')
    print('|---|' + '---:|' * len(names))
    for k in keys:
        row = [f'{100*res[n][1][k]/res[n][0]:.1f}' for n in names]
        if all(float(x) == 0 for x in row):
            continue
        print(f'| {k} | ' + ' | '.join(row) + ' |')
    print('| samples | ' + ' | '.join(str(res[n][0]) for n in names) + ' |')
    for n in names:
        print('\n### ' + n)
        tot, b, lines = res[n]
        for (k, rel, line, fn), v in lines.most_common(28):
            print(f'  {100*v/tot:5.2f}  {k:24s}  {rel.split("/")[-1]}:{line}  {fn}')
