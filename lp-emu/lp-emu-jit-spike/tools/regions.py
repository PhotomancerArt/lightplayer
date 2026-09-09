#!/usr/bin/env python3
"""spike: group the block census (LP_EMU_BLOCKPROF) into hot regions.

    regions.py <census.txt> [--min-execs N] [--top K] [--dump-region PC]

A region is a connected component, over the hot and translatable blocks only,
of the static control-flow edges: fall-through, branch target, direct call
target, and "return-to" (the fall-through after a call). Indirect jumps carry
no static edge. Shares are of executed slots (= retired instructions through
the cache) over the whole run.
"""
import sys
from collections import defaultdict


def sext(v, bits):
    return v - (1 << bits) if v & (1 << (bits - 1)) else v


def imm_b(w):
    imm = ((w >> 31) & 1) << 12 | ((w >> 7) & 1) << 11 | ((w >> 25) & 0x3F) << 5 | ((w >> 8) & 0xF) << 1
    return sext(imm, 13)


def imm_j(w):
    imm = ((w >> 31) & 1) << 20 | ((w >> 12) & 0xFF) << 12 | ((w >> 20) & 1) << 11 | ((w >> 21) & 0x3FF) << 1
    return sext(imm, 21)


def cj_off(i):
    imm = ((i >> 12) & 1) << 11 | ((i >> 8) & 1) << 10 | ((i >> 9) & 3) << 8 | ((i >> 6) & 1) << 7 \
        | ((i >> 7) & 1) << 6 | ((i >> 2) & 1) << 5 | ((i >> 11) & 1) << 4 | ((i >> 3) & 7) << 1
    return sext(imm, 12)


def cb_off(i):
    imm = ((i >> 12) & 1) << 8 | ((i >> 5) & 3) << 6 | ((i >> 2) & 1) << 5 | ((i >> 10) & 3) << 3 | ((i >> 3) & 3) << 1
    return sext(imm, 9)


def translatable(width, w):
    """The RV32IMC subset the spike translator covers."""
    if width == 2:
        q, f3 = w & 3, (w >> 13) & 7
        if q == 0:
            return f3 in (0, 2, 6)
        if q == 1:
            if f3 == 4:
                f2 = (w >> 10) & 3
                if f2 == 3:
                    return ((w >> 10) & 0x3F) == 0b100011
                return True
            return True
        if q == 2:
            return f3 in (0, 2, 4, 6)
        return False
    op = w & 0x7F
    f3 = (w >> 12) & 7
    f7 = (w >> 25) & 0x7F
    if op == 0x33:
        if f7 == 0:
            return True
        if f7 == 0x20:
            return f3 in (0, 5)
        if f7 == 1:
            return True
        return False
    if op == 0x13:
        if f3 == 1:
            return f7 == 0
        if f3 == 5:
            return f7 in (0, 0x20)
        return True
    if op in (0x03, 0x23, 0x37, 0x17, 0x63, 0x6F, 0x67):
        if op == 0x03:
            return f3 in (0, 1, 2, 4, 5)
        if op == 0x23:
            return f3 in (0, 1, 2)
        if op == 0x63:
            return f3 in (0, 1, 4, 5, 6, 7)
        if op == 0x67:
            return f3 == 0
        return True
    return False


def successors(pc, blen, words):
    """(kind, [targets]) from the terminator. kind in fall/branch/jump/call/callind/indirect/cap."""
    width, w = words[-1]
    last_pc = pc + blen - width
    fall = pc + blen
    if width == 4:
        op = w & 0x7F
        rd = (w >> 7) & 0x1F
        if op == 0x63:
            return "branch", [fall, (last_pc + imm_b(w)) & 0xFFFFFFFF]
        if op == 0x6F:
            t = (last_pc + imm_j(w)) & 0xFFFFFFFF
            return ("call", [t, fall]) if rd else ("jump", [t])
        if op == 0x67:
            return ("callind", [fall]) if rd else ("indirect", [])
        return "cap", [fall]
    q, f3 = w & 3, (w >> 13) & 7
    if q == 1 and f3 == 5:
        return "jump", [(last_pc + cj_off(w)) & 0xFFFFFFFF]
    if q == 1 and f3 == 1:
        return "call", [(last_pc + cj_off(w)) & 0xFFFFFFFF, fall]
    if q == 1 and f3 in (6, 7):
        return "branch", [fall, (last_pc + cb_off(w)) & 0xFFFFFFFF]
    if q == 2 and f3 == 4 and ((w >> 2) & 0x1F) == 0:
        return ("callind", [fall]) if (w >> 12) & 1 else ("indirect", [])
    return "cap", [fall]


def main():
    args = sys.argv[1:]
    path = args[0]
    min_execs = 10_000
    top = 12
    dump = None
    emit = None
    i = 1
    while i < len(args):
        if args[i] == "--min-execs":
            min_execs = int(args[i + 1]); i += 2
        elif args[i] == "--top":
            top = int(args[i + 1]); i += 2
        elif args[i] == "--dump-region":
            dump = int(args[i + 1], 0); i += 2
        elif args[i] == "--emit":
            emit = args[i + 1]; i += 2
        else:
            raise SystemExit(f"unknown arg {args[i]}")

    blocks = {}
    dyn = {}
    total_slots = 0
    for line in open(path):
        if line.startswith("#"):
            continue
        f = line.split()
        if f[0] == "edge":
            dyn[(int(f[1], 16), int(f[2], 16))] = int(f[3])
            continue
        pc, blen, nbytes, execs, slots = int(f[0], 16), int(f[1]), int(f[2]), int(f[4]), int(f[5])
        words = [(int(t.split(":")[0]), int(t.split(":")[1], 16)) for t in f[6:]]
        blocks[pc] = dict(pc=pc, len=blen, bytes=nbytes, execs=execs, slots=slots, words=words)
        total_slots += slots

    hot = {pc: b for pc, b in blocks.items() if b["execs"] >= min_execs}
    hot_slots = sum(b["slots"] for b in hot.values())
    untrans = {pc for pc, b in hot.items() if not all(translatable(w, x) for w, x in b["words"])}
    print(f"blocks {len(blocks)}, executed slots {total_slots:,}")
    print(f"hot (execs >= {min_execs}): {len(hot)} blocks, {hot_slots / total_slots:.2%} of slots; "
          f"{len(untrans)} of them untranslatable "
          f"({sum(hot[p]['slots'] for p in untrans) / total_slots:.2%} of slots)")

    parent = {pc: pc for pc in hot if pc not in untrans}

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb

    edges = defaultdict(list)
    for pc, b in hot.items():
        if pc in untrans:
            continue
        kind, targets = successors(pc, b["bytes"], b["words"])
        b["kind"] = kind
    # Dynamic edges decide membership: an edge is hot when it was taken at
    # least min_execs / 10 times, and both ends are hot translatable blocks.
    min_edge = max(1, min_execs // 10)
    entries_from_outside = defaultdict(int)
    for (a, t), n in dyn.items():
        if n < min_edge:
            continue
        if a in parent and t in parent:
            edges[a].append(t)
            union(a, t)
        elif t in parent:
            entries_from_outside[t] += n

    regions = defaultdict(list)
    for pc in parent:
        regions[find(pc)].append(pc)
    ranked = sorted(regions.values(), key=lambda r: -sum(hot[p]["slots"] for p in r))

    print(f"\n{'#':>3} {'share':>7} {'cum':>7} {'blocks':>6} {'insts':>6} {'entries':>8} {'lowest pc':>10} {'highest pc':>10}  kinds")
    cum = 0.0
    for n, r in enumerate(ranked[:top], 1):
        s = sum(hot[p]["slots"] for p in r) / total_slots
        cum += s
        insts = sum(hot[p]["len"] for p in r)
        execs = sum(hot[p]["execs"] for p in r)
        kinds = defaultdict(int)
        for p in r:
            kinds[hot[p]["kind"]] += 1
        print(f"{n:>3} {s:7.2%} {cum:7.2%} {len(r):>6} {insts:>6} {execs:>8} {min(r):#010x} {max(r):#010x}  "
              + " ".join(f"{k}={v}" for k, v in sorted(kinds.items())))
    for k in (1, 3, 10):
        print(f"top {k:>2}: {sum(sum(hot[p]['slots'] for p in r) for r in ranked[:k]) / total_slots:.2%}")

    if emit is not None:
        # `--emit out.txt`: the top `top` regions as a region file the
        # emulator's `--jit-region` reads: `region <n> <share>` then one block
        # start per line.
        with open(emit, "w") as f:
            for n, r in enumerate(ranked[:top], 1):
                s = sum(hot[p]["slots"] for p in r) / total_slots
                f.write(f"region r{n} {s:.4f}\n")
                for p in sorted(r):
                    f.write(f"{p:#010x}\n")
                # Observed indirect-jump targets inside the region, hottest
                # first: the translator dispatches these internally.
                rs = set(r)
                tcount = defaultdict(int)
                for p in r:
                    if hot[p]["kind"] in ("indirect", "callind"):
                        for t in edges[p]:
                            if t in rs:
                                tcount[t] += dyn.get((p, t), 0)
                for t, c in sorted(tcount.items(), key=lambda kv: -kv[1]):
                    f.write(f"target {t:#010x}  # {c}\n")
        print(f"\nwrote {min(top, len(ranked))} region(s) to {emit}")

    if dump is not None:
        r = next(r for r in ranked if dump in r)
        print(f"\nregion containing {dump:#010x}: {len(r)} blocks, "
              f"entered from outside at " + " ".join(f"{p:#x}({entries_from_outside[p]})" for p in sorted(r) if entries_from_outside[p]))
        for p in sorted(r):
            b = hot[p]
            outs = " ".join(f"{t:#x}({dyn.get((p, t), 0)})" for t in sorted(set(edges[p])))
            print(f"  {p:#010x} len={b['len']:>2} bytes={b['bytes']:>3} execs={b['execs']:>9} slots={b['slots']:>10} "
                  f"{b['kind']:<8} -> {outs}")


if __name__ == "__main__":
    main()
