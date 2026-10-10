#!/usr/bin/env python3
"""Lifetime census of an emulated C6's heap, and the segregation counterfactual.

Reads one `lp-cli emu run --alloc-trace` trace (format:
`lp-emu/esp/lp-emu-esp32c6/src/alloc_trace.rs`) and:

1. **Classifies** every allocation of one boot into a lifetime class, by its
   birth and death against the boot's exact log-record markers (`L` lines):
   `platform`, `session`, `project`, `frame`, `transient` (the big block and
   the job windows: load, compile, unload, boot), `discardable`; the radio's
   C blocks (`caps != 0`) are `radio` and stay in their own region in every
   replay. The rule is in [`classify`]; every flag it raises on an
   ambiguous block is counted and its call sites listed.
2. **Finds the mixers**: long-lived blocks born inside a transient window, and
   the live blocks that bound the largest holes of today's heap at each point.
3. **Replays** the boot on today's layout (esp-alloc's regions, tried in
   order by capability, `linked_list_allocator` first fit at RV32 geometry:
   no header, 8 B minimum block, 4 B granule) and checks the model against the
   trace's own addresses — each allocation's alignment is not in the trace, so
   it is inferred as the smallest one that reproduces the recorded address.
   Then replays it once per **grouping** (a partition of the classes into
   regions), each group first fit in a region of its own, and prices it: each
   group's first-fit footprint (the region size it needed to never fail), and
   at every marker the largest block a new request of each group could get,
   against today's.

Probe traffic is dropped first: every allocation with a frame through
`largest_free_block` (the gates' binary-search probes) is not the program's.

Usage:
    lifetime-census.py TRACE --elf p2.elf [--boot N] [--out DIR]
        [--grouping NAME=cls+cls/cls/…] [--areas 186848,65536] [--frame-ms 20]

Standard library only; `rust-nm` must be on PATH (cargo-binutils).
"""

from __future__ import annotations

import argparse
import bisect
import importlib.util
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "alloc_trace_report", Path(__file__).with_name("alloc-trace-report.py"))
atr = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(atr)

CLASSES = ("platform", "session", "project", "frame", "transient", "discardable")
CYCLES_PER_US = 160

# Today's C6 heap (fw-esp32c6 `board/esp32c6/init.rs`), in registration order:
# (name, size, capability tags). esp-alloc tries every region whose tags are a
# superset of the request's, in this order.
INTERNAL, EXTERNAL = 1, 2
C6_REGIONS = (("main", 186_848, INTERNAL), ("dram2", 65_536, 0), ("radio", 49_152, INTERNAL | EXTERNAL))

# A block that a call site marks as rebuildable: the resolver's payload tables
# (its structural and frame value copies, gated by `resolver-payload-cache`)
# and the palette bake cache. Matched on any frame of the allocation's stack.
DISCARDABLE_SITES = re.compile(
    r"ResolverCache|resolver_cache|palette_bake_cache|lpc_wire::slot::access_sync|ToLpValue")
# Per-connection state: one JSON Pack learned table per packed link, the
# lp-link session, a BLE connection, a LAN client or relay route.
SESSION_SITES = re.compile(
    r"pack_learned|PackedLink|packed_link|lp_link::session|LinkSession|ble_connection|"
    r"lan_client|relay_route|RelayRoute")
PROBE_SITE = "largest_free_block"


class Region:
    """One `linked_list_allocator` region at RV32 geometry (8 B minimum block,
    4 B granule, no header), first fit over an address-ordered hole list.
    Mirrors `lp-emu-core`'s `FirstFitHeap` (`frag/first_fit_heap.rs`, checked
    against the real crate there)."""

    MIN = 8
    GRAN = 4

    def __init__(self, name: str, base: int, size: int):
        self.name, self.base = name, base
        bottom = (base + 3) & ~3
        usable = (size - (bottom - base)) & ~3
        self.bottom, self.top = bottom, bottom + usable
        self.st = [bottom] if usable >= self.MIN else []
        self.sz = [usable] if usable >= self.MIN else []
        self._max = None

    @staticmethod
    def footprint(size: int) -> int:
        return (max(size, 8) + 3) & ~3

    def largest(self) -> int:
        if self._max is None:
            self._max = max(self.sz) if self.sz else 0
        return self._max

    def free(self) -> int:
        return sum(self.sz)

    def find(self, size: int, align: int):
        """(index, addr, front, back) of the first fit, or None."""
        req = self.footprint(size)
        if req > self.largest():
            return None
        st, sz = self.st, self.sz
        for i in range(len(sz)):
            s = sz[i]
            if s < req:
                continue
            start = st[i]
            if start % align == 0:
                addr, front = start, None
            else:
                addr = -(-(start + 8) // align) * align
                front = (start, addr - start)
            end = addr + req
            hend = start + s
            if end > hend:
                continue
            back = None
            if end != hend:
                bstart = (end + 3) & ~3
                if bstart + 8 > hend:
                    continue
                back = (bstart, hend - end)
            return i, addr, front, back
        return None

    def commit(self, hit):
        i, addr, front, back = hit
        del self.st[i]
        del self.sz[i]
        if back:
            self.st.insert(i, back[0])
            self.sz.insert(i, back[1])
        if front:
            self.st.insert(i, front[0])
            self.sz.insert(i, front[1])
        self._max = None
        return addr

    def carve(self, addr: int, size: int) -> bool:
        """Force a block at `addr` (the trace's address, when no alignment
        reproduces it): split the hole that holds it. False if none does."""
        req = self.footprint(size)
        i = bisect.bisect_right(self.st, addr) - 1
        if i < 0 or self.st[i] + self.sz[i] < addr + req:
            return False
        start, s = self.st[i], self.sz[i]
        del self.st[i]
        del self.sz[i]
        tail = start + s - (addr + req)
        if tail > 0:
            self.st.insert(i, addr + req)
            self.sz.insert(i, tail)
        if addr > start:
            self.st.insert(i, start)
            self.sz.insert(i, addr - start)
        self._max = None
        return True

    def release(self, addr: int, size: int):
        req = self.footprint(size)
        st, sz = self.st, self.sz
        i = bisect.bisect_left(st, addr)
        st.insert(i, addr)
        sz.insert(i, req)
        if i + 1 < len(st) and st[i] + sz[i] == st[i + 1]:
            sz[i] += sz[i + 1]
            del st[i + 1]
            del sz[i + 1]
        if i > 0 and st[i - 1] + sz[i - 1] == st[i]:
            sz[i - 1] += sz[i]
            del st[i]
            del sz[i]
        self._max = None

    def contains(self, addr: int) -> bool:
        return self.bottom <= addr < self.top


# --------------------------------------------------------------------------
# Parsing


class Trace:
    def __init__(self):
        self.size: list[int] = []
        self.caps: list[int] = []
        self.ptr: list[int] = []
        self.born: list[int] = []
        self.died: list[int | None] = []
        self.frames: list[int] = []  # interned frames-string id
        self.events: list[int] = []  # rid*2 (alloc) / rid*2+1 (free)
        self.marks: list[tuple[int, int, str]] = []  # (event index, cycle, text)
        self.heartbeats: list[tuple[int, int]] = []  # (event index, largestFreeBlock)
        self.frame_strings: list[str] = []
        self.dropped_probe = 0
        self.dropped_failed = 0
        self.unmatched_frees = 0
        self.end_cycle = 0


def parse(path: str, boot_wanted: int, sym) -> Trace:
    t = Trace()
    intern: dict[str, int] = {}
    probe_of: dict[int, bool] = {}
    live: dict[int, int] = {}
    boot = 0
    with open(path) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                if boot != boot_wanted:
                    continue
                _, cyc, ptr, size, caps, frames = line.split(" ", 5)
                frames = frames.rstrip("\n")
                fid = intern.get(frames)
                if fid is None:
                    fid = len(t.frame_strings)
                    intern[frames] = fid
                    t.frame_strings.append(frames)
                    probe_of[fid] = any(
                        PROBE_SITE in sym(int(x, 16)) for x in frames.split(",") if x)
                p = int(ptr, 16)
                if p == 0:
                    t.dropped_failed += 1
                    continue
                if probe_of[fid]:
                    t.dropped_probe += 1
                    live[p] = -1
                    continue
                rid = len(t.size)
                t.size.append(int(size))
                t.caps.append(int(caps))
                t.ptr.append(p)
                t.born.append(int(cyc))
                t.died.append(None)
                t.frames.append(fid)
                t.events.append(rid * 2)
                live[p] = rid
                t.end_cycle = int(cyc)
            elif tag == "F":
                if boot != boot_wanted:
                    continue
                _, cyc, ptr, _size = line.split(" ", 3)
                rid = live.pop(int(ptr, 16), None)
                if rid is None:
                    t.unmatched_frees += 1
                    continue
                if rid < 0:
                    continue
                t.died[rid] = int(cyc)
                t.events.append(rid * 2 + 1)
                t.end_cycle = int(cyc)
            elif tag in "LM":
                _, cyc, text = line.rstrip("\n").split(" ", 2)
                if text.startswith("@reboot"):
                    boot += 1
                    live = {}
                    continue
                if tag == "L" and boot == boot_wanted:
                    t.marks.append((len(t.events), int(cyc), text))
                elif tag == "M" and boot == boot_wanted and "largestFreeBlock" in text:
                    m = re.search(r'"largestFreeBlock":(\d+)', text)
                    if m:
                        t.heartbeats.append((len(t.events), int(m.group(1))))
    return t


# --------------------------------------------------------------------------
# Windows and classification


WINDOWS = (
    # (kind, start substring, end substrings)
    ("load", ("Loading project:", "[mem] boot auto_load before"),
     ("[mem] load_project after", "[mem] boot auto_load after")),
    ("unload", ("[mem] load_project unload existing before", "[mem] stop_all_projects before"),
     ("[mem] load_project unload existing after", "[mem] stop_all_projects after")),
    ("compile", ("compilation starting",), ("compilation succeeded", "compilation failed")),
)


def find_windows(t: Trace):
    """Job windows `(kind, start cycle, end cycle)`, the boot window, and the
    project epochs `(start cycle, end cycle)`: a project lives from its load's
    start to the end of the unload that removes it (or the trace's end)."""
    wins = []
    open_: dict[str, int] = {}
    for _, cyc, text in t.marks:
        for kind, starts, ends in WINDOWS:
            if kind not in open_ and any(s in text for s in starts):
                open_[kind] = cyc
            elif kind in open_ and any(e in text for e in ends):
                wins.append((kind, open_.pop(kind), cyc))
    first_load = min((s for k, s, _ in wins if k == "load"), default=None)
    boot_done = next((c for _, c, x in t.marks if "boot complete" in x), None)
    boot_end = min(c for c in (first_load, boot_done, t.end_cycle) if c is not None)
    wins.append(("boot", 0, boot_end))
    epochs = []
    loads = sorted((s, e) for k, s, e in wins if k == "load")
    unloads = sorted((s, e) for k, s, e in wins if k == "unload")
    for ls, le in loads:
        # A switch's load starts before it unloads the old project (the
        # unload runs inside the new load), so a project's epoch ends at the
        # first unload that starts after its OWN load has finished.
        nxt = next(((us, ue) for us, ue in unloads if us > le), None)
        epochs.append((ls, nxt[1] if nxt else None, nxt[0] if nxt else None))
    return sorted(wins, key=lambda w: w[1]), epochs


def classify(t: Trace, wins, epochs, sym, frame_cycles: int, big: int):
    """Assign each record a class and a set of flags. The rule, in order:

    - `radio`: a C block tagged for the radio's region (`c_heap::RADIO`) —
      the radio blobs' heap, kept as is. A block asking for `Internal` alone
      (the C fallback, esp-radio's own internal-memory asks) lands in main
      today and is classified by its life like any other.
    - short-lived (dies within `frame_cycles` of its birth, or inside the job
      window it was born in): `transient` if it was born inside a job window
      (boot, load, compile, unload) or is at least `big` bytes, else `frame`.
    - otherwise, by its life against the project epochs:
      - born in a project epoch and dead by the end of that epoch's unload
        (or still live at the trace's end while that project is loaded:
        flag `no-unload-seen`): `project`; dead well before the unload:
        `project`, flag `replaced`.
      - born outside every project epoch, or born in one and still live
        after it ended: `platform` (flag `born-in-project` for the latter —
        a mixer by construction). Platform blocks that die before the end:
        flag `platform-churn`.
    - then the call-site overrides, for blocks that are not short-lived:
      `session` (SESSION_SITES) and `discardable` (DISCARDABLE_SITES).
    """
    n = len(t.size)
    cls = [None] * n
    flags: list[tuple[str, ...]] = [()] * n
    job = sorted((s, e, k) for k, s, e in wins)
    job_starts = [s for s, _, _ in job]
    ep_sorted = sorted(epochs)
    ep_starts = [s for s, _, _ in ep_sorted]
    end = t.end_cycle
    site_cache: dict[int, tuple[bool, bool]] = {}

    def sites(fid):
        hit = site_cache.get(fid)
        if hit is None:
            names = [sym(int(x, 16)) for x in t.frame_strings[fid].split(",") if x]
            hit = (any(SESSION_SITES.search(nm) for nm in names),
                   any(DISCARDABLE_SITES.search(nm) for nm in names))
            site_cache[fid] = hit
        return hit

    def window_of(c):
        i = bisect.bisect_right(job_starts, c) - 1
        best = None
        while i >= 0:
            s, e, k = job[i]
            if s <= c <= e:
                # innermost (latest-started) window that holds c
                best = (s, e, k)
                break
            i -= 1
        return best

    def epoch_of(c):
        i = bisect.bisect_right(ep_starts, c) - 1
        if i < 0:
            return None
        s, e, us = ep_sorted[i]
        if e is None or c <= e:
            return ep_sorted[i]
        return None

    for r in range(n):
        if t.caps[r] & EXTERNAL:
            cls[r] = "radio"
            continue
        b, d, sz = t.born[r], t.died[r], t.size[r]
        w = window_of(b)
        short = d is not None and (d - b <= frame_cycles or (w is not None and d <= w[1]))
        if short:
            cls[r] = "transient" if (w is not None or sz >= big) else "frame"
            continue
        f: list[str] = []
        ep = epoch_of(b)
        if ep is not None:
            es, ee, us = ep
            if d is None:
                if ee is None:
                    c = "project"
                    f.append("no-unload-seen")
                else:
                    c = "platform"
                    f.append("born-in-project")
            elif ee is not None and d > ee:
                c = "platform"
                f.append("born-in-project")
            elif us is not None and d >= us:
                c = "project"
            else:
                c = "project"
                f.append("replaced")
        else:
            c = "platform"
            if d is not None:
                f.append("platform-churn")
        is_session, is_disc = sites(t.frames[r])
        if is_session:
            c = "session"
        elif is_disc and c == "project":
            c = "discardable"
        cls[r] = c
        flags[r] = tuple(f)
    return cls, flags


# --------------------------------------------------------------------------
# Replays


def replay_today(t: Trace, regions_spec):
    """Today's layout, each allocation placed as esp-alloc would and checked
    against the trace's address. Returns (regions, align per record, stats,
    snapshots per mark index)."""
    base_of = {"main": 0x40827028, "dram2": 0x4086E610, "radio": 0x4081B028}
    regions = [Region(nm, base_of.get(nm, 0x1000_0000 * (i + 1)), sz)
               for i, (nm, sz, _) in enumerate(regions_spec)]
    tags = [tg for _, _, tg in regions_spec]
    align = [4] * len(t.size)
    where = [None] * len(t.size)  # region index
    stats = defaultdict(int)
    marks_at = defaultdict(list)
    for mi, (ei, _, _) in enumerate(t.marks):
        marks_at[ei].append(mi)
    snaps = {}

    def snapshot(mi):
        snaps[mi] = [(rg.largest(), rg.free(), len(rg.sz)) for rg in regions]

    for ei, ev in enumerate(t.events):
        for mi in marks_at.get(ei, ()):
            snapshot(mi)
        r, is_free = ev >> 1, ev & 1
        if is_free:
            ri = where[r]
            if ri is not None:
                regions[ri].release(t.ptr[r], t.size[r])
            continue
        caps = t.caps[r]
        want = t.ptr[r]
        order = [i for i, tg in enumerate(tags) if tg & caps == caps]
        placed = False
        model_region = None
        for i in order:
            hit = regions[i].find(t.size[r], 4)
            if hit is None:
                continue
            model_region = i
            if hit[1] == want:
                regions[i].commit(hit)
                where[r] = i
                placed = True
                stats["reproduced"] += 1
            break
        if placed:
            continue
        ri = next((i for i, rg in enumerate(regions) if rg.contains(want)), None)
        if ri is None:
            stats["outside-every-region"] += 1
            continue
        if model_region is not None and model_region != ri:
            stats["model-chose-another-region"] += 1
        for al in (8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096):
            hit = regions[ri].find(t.size[r], al)
            if hit is not None and hit[1] == want:
                # Earlier regions in the order must have refused it too.
                regions[ri].commit(hit)
                where[r] = ri
                align[r] = al
                placed = True
                stats[f"reproduced-align-{al}"] += 1
                break
        if not placed:
            if regions[ri].carve(want, t.size[r]):
                where[r] = ri
                stats["forced"] += 1
            else:
                stats["collision"] += 1
    for mi in marks_at.get(len(t.events), ()):
        snapshot(mi)
    return regions, align, stats, snaps, where


def bounding_at_marks(t: Trace, where, marks: set[int], top: int = 3):
    """Walk today's layout from the trace's own addresses and, at each mark in
    `marks`, name the live blocks on either side of each region's `top`
    largest holes: the blocks that hold those holes apart. Returns
    {mark: [(region, hole start, hole size, below rid, above rid)]}."""
    rg2 = [Region(nm, b, sz) for (nm, sz, _), b in
           zip(C6_REGIONS, (0x40827028, 0x4086E610, 0x4081B028))]
    live: dict[int, int] = {}
    ev_marks = defaultdict(list)
    for mi in marks:
        ev_marks[t.marks[mi][0]].append(mi)
    out = {}

    def probe(mi):
        starts = sorted(live)
        rows = []
        for ri, rg in enumerate(rg2):
            holes = sorted(zip(rg.sz, rg.st), reverse=True)[:top]
            for size, start in holes:
                i = bisect.bisect_left(starts, start) - 1
                below = live[starts[i]] if i >= 0 and rg.contains(starts[i]) else None
                above = live.get(start + size)
                rows.append((rg.name, start, size, below, above))
        out[mi] = rows

    for ei, ev in enumerate(t.events):
        for mi in ev_marks.get(ei, ()):
            probe(mi)
        r, is_free = ev >> 1, ev & 1
        ri = where[r]
        if ri is None:
            continue
        if is_free:
            rg2[ri].release(t.ptr[r], t.size[r])
            live.pop(t.ptr[r], None)
        else:
            rg2[ri].carve(t.ptr[r], t.size[r])
            live[t.ptr[r]] = r
    for mi in ev_marks.get(len(t.events), ()):
        probe(mi)
    return out


class GroupReplay:
    """One group's region list: zero or more bounded regions (a group split
    across physical areas), then an unbounded tail. First fit does not depend
    on a region's size until it fails, so the tail's footprint — the highest
    address it ever used — is the size it needed to never fail, and at each
    mark its interior holes and its live top give its largest free block for
    any size at or above that footprint."""

    def __init__(self, name, bounded=()):
        self.name = name
        self.bounded = [Region(f"{name}#{i}", 0x8000_0000 + i * 0x1000_0000, sz)
                        for i, sz in enumerate(bounded)]
        self.rg = Region(name, 0, 1 << 30)
        self.high = 0
        self.live = 0
        self.peak_live = 0

    def alloc(self, size, align):
        fpt = Region.footprint(size)
        self.live += fpt
        if self.live > self.peak_live:
            self.peak_live = self.live
        for i, rg in enumerate(self.bounded):
            hit = rg.find(size, align)
            if hit is not None:
                return rg.commit(hit), i
        hit = self.rg.find(size, align)
        addr = self.rg.commit(hit)
        if addr + fpt > self.high:
            self.high = addr + fpt
        return addr, -1

    def free(self, addr, size, sub):
        (self.bounded[sub] if sub >= 0 else self.rg).release(addr, size)
        self.live -= Region.footprint(size)

    def shape(self):
        """(largest hole of the bounded regions, largest interior hole of the
        tail, the tail's live top): the tail's last hole starts at its live
        top; every other hole is interior."""
        st, sz = self.rg.st, self.rg.sz
        top = st[-1] if st else 0
        interior = max(sz[:-1]) if len(sz) > 1 else 0
        bounded = max((rg.largest() for rg in self.bounded), default=0)
        return bounded, interior, top


def largest_at(shape, size):
    """A group's largest free block at a mark when its tail region is `size`."""
    bounded, interior, top = shape
    return max(bounded, interior, size - top)


def replay_grouping(t: Trace, cls, align, groups, bounded=None):
    """`groups`: list of class tuples; `bounded[g]`: group g's bounded regions
    ahead of its tail. The radio-tagged C blocks get their own region as today
    (49,152 B, first fit); one that does not fit falls back to the platform
    group, as `c_heap` falls back to main. Returns the replays, each mark's
    shapes, and the radio fallbacks."""
    gof = {}
    for gi, g in enumerate(groups):
        for c in g:
            gof[c] = gi
    bounded = bounded or [()] * len(groups)
    reps = [GroupReplay("+".join(g), bounded[gi]) for gi, g in enumerate(groups)]
    radio = Region("radio", 0x4081B028, 49_152)
    radio_over = 0
    addr = [0] * len(t.size)
    gidx = [-1] * len(t.size)
    gsub = [-1] * len(t.size)
    marks_at = defaultdict(list)
    for mi, (ei, _, _) in enumerate(t.marks):
        marks_at[ei].append(mi)
    shapes = {}
    for ei, ev in enumerate(t.events):
        for mi in marks_at.get(ei, ()):
            shapes[mi] = [rp.shape() for rp in reps]
        r, is_free = ev >> 1, ev & 1
        if is_free:
            g = gidx[r]
            if g == -2:
                radio.release(addr[r], t.size[r])
            elif g >= 0:
                reps[g].free(addr[r], t.size[r], gsub[r])
            continue
        c = cls[r]
        if c == "radio":
            hit = radio.find(t.size[r], 4)
            if hit is not None:
                addr[r] = radio.commit(hit)
                gidx[r] = -2
                continue
            radio_over += 1
            c = "platform"
        g = gof[c]
        addr[r], gsub[r] = reps[g].alloc(t.size[r], align[r])
        gidx[r] = g
    for mi in marks_at.get(len(t.events), ()):
        shapes[mi] = [rp.shape() for rp in reps]
    return reps, shapes, radio_over


def plan_spill(sizes: list[int], areas: list[int], absorber: int):
    """When no group-per-area assignment fits whole: fill the first area with
    the non-absorber groups, largest first, and split the one that crosses its
    end across both areas (esp-alloc's two regions, first fit in order); the
    rest and the absorber go in the second area. Returns (split group, its
    part in area 0, groups whole in area 1) or None if they cannot fit."""
    if len(areas) != 2:
        return None
    order = sorted((i for i in range(len(sizes)) if i != absorber), key=lambda i: -sizes[i])
    rem0 = areas[0]
    split = None
    in1 = []
    for i in order:
        if split is None and sizes[i] <= rem0:
            rem0 -= sizes[i]
        elif split is None:
            split = i
        else:
            in1.append(i)
    if split is None:
        return None
    return split, rem0, in1


def pack(sizes: list[int], areas: list[int], absorber: int):
    """Place each group's region (its footprint) whole in one physical area;
    the absorber group (the big-block one) takes its area's leftover. Brute
    force over every assignment; the best is the one whose absorber ends up
    largest. Returns (assignment, absorber size, leftover per area) or None."""
    best = None
    n = len(sizes)
    k = len(areas)

    def rec(i, assign, used):
        nonlocal best
        if i == n:
            a = assign[absorber]
            left = [areas[j] - used[j] for j in range(k)]
            size = sizes[absorber] + left[a]
            if best is None or size > best[1]:
                best = (list(assign), size, left)
            return
        for j in range(k):
            if used[j] + sizes[i] <= areas[j]:
                assign.append(j)
                used[j] += sizes[i]
                rec(i + 1, assign, used)
                used[j] -= sizes[i]
                assign.pop()

    rec(0, [], [0] * k)
    return best


# --------------------------------------------------------------------------
# Report


def fmt(n):
    return f"{n:,}"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--boot", type=int, default=0, help="which boot to analyze (0 = power-on)")
    ap.add_argument("--label", default=None)
    ap.add_argument("--out", default=None, help="directory for the per-mark TSVs and JSON")
    ap.add_argument("--frame-ms", type=float, default=100.0,
                    help="a block that dies within this many emulated ms is short-lived")
    ap.add_argument("--big", type=int, default=1024,
                    help="a short-lived block this large is transient (the big block) wherever born")
    ap.add_argument("--areas", default="186848,65536",
                    help="the physical areas the segregated regions must fit in")
    ap.add_argument("--grouping", action="append", default=[],
                    help="NAME=cls+cls/cls/… (groups separated by '/'; every class once)")
    ap.add_argument("--top", type=int, default=8)
    args = ap.parse_args()

    sym = atr.Symbolizer(args.elf)
    t = parse(args.trace, args.boot, sym)
    wins, epochs = find_windows(t)
    frame_cycles = int(args.frame_ms * 1000 * CYCLES_PER_US)
    cls, flags = classify(t, wins, epochs, sym, frame_cycles, args.big)
    label = args.label or Path(args.trace).name
    n = len(t.size)

    print(f"# Lifetime census: {label}")
    print()
    print(f"Trace `{args.trace}`, boot {args.boot}; {fmt(n)} allocations kept, "
          f"{fmt(t.dropped_probe)} probe allocations dropped (`{PROBE_SITE}`), "
          f"{fmt(t.dropped_failed)} failed (null) dropped, {t.unmatched_frees} unmatched frees. "
          f"Short-lived = dies within {args.frame_ms} ms emulated or inside its birth window; "
          f"big = {args.big} B.")
    print()
    print("Windows: " + ", ".join(
        f"{k} ×{sum(1 for w in wins if w[0] == k)}" for k in ("boot", "load", "compile", "unload")
    ) + f"; project epochs: {len(epochs)} "
        f"({sum(1 for e in epochs if e[1] is None)} without an unload in the trace).")
    print()

    # ---- live-set walk: per-class live bytes and peaks, live at marks
    fp = [Region.footprint(s) for s in t.size]
    live_by = defaultdict(int)
    peak_by = defaultdict(int)
    tot_live = 0
    tot_peak = 0
    rust_live = 0
    rust_peak = 0
    rust_peak_mark = None
    alloc_bytes = defaultdict(int)
    alloc_count = defaultdict(int)
    marks_at = defaultdict(list)
    for mi, (ei, _, _) in enumerate(t.marks):
        marks_at[ei].append(mi)
    live_at_mark = {}
    for ei, ev in enumerate(t.events):
        for mi in marks_at.get(ei, ()):
            live_at_mark[mi] = (dict(live_by), rust_live)
        r, is_free = ev >> 1, ev & 1
        c = cls[r]
        if is_free:
            live_by[c] -= fp[r]
            if c != "radio":
                rust_live -= fp[r]
            continue
        alloc_bytes[c] += t.size[r]
        alloc_count[c] += 1
        live_by[c] += fp[r]
        if live_by[c] > peak_by[c]:
            peak_by[c] = live_by[c]
        if c != "radio":
            rust_live += fp[r]
            if rust_live > rust_peak:
                rust_peak = rust_live
                rust_peak_mark = ei
    for mi in marks_at.get(len(t.events), ()):
        live_at_mark[mi] = (dict(live_by), rust_live)
    sum_of_peaks = sum(peak_by[c] for c in CLASSES)

    print("## Census by class")
    print()
    print("Bytes are footprints (each request rounded as the allocator rounds it). "
          "'peak live' is the class's own high-water; the classes peak at different times, so "
          "their peaks sum to more than the Rust heap's peak live set.")
    print()
    print("| class | allocations | bytes allocated (requested) | peak live | live at end |")
    print("|---|---:|---:|---:|---:|")
    for c in CLASSES + ("radio",):
        print(f"| {c} | {fmt(alloc_count[c])} | {fmt(alloc_bytes[c])} | {fmt(peak_by[c])} | "
              f"{fmt(live_by[c])} |")
    print(f"| **Rust (all but radio)** | {fmt(sum(alloc_count[c] for c in CLASSES))} | "
          f"{fmt(sum(alloc_bytes[c] for c in CLASSES))} | **{fmt(rust_peak)}** (one instant) "
          f"/ {fmt(sum_of_peaks)} (sum of class peaks) | {fmt(rust_live)} |")
    print()

    # flags
    fl = defaultdict(lambda: [0, 0, defaultdict(lambda: [0, 0])])
    owner_cache = {}

    def owner(r):
        fid = t.frames[r]
        hit = owner_cache.get(fid)
        if hit is None:
            rets = [int(x, 16) for x in t.frame_strings[fid].split(",") if x]
            names = [sym(x) for x in rets]
            hit = "?"
            for i, nm in enumerate(names):
                if not atr.is_machinery(nm):
                    hit = " < ".join(names[i:i + 3])
                    break
            owner_cache[fid] = hit
        return hit

    for r in range(n):
        for f in flags[r]:
            e = fl[f]
            e[0] += 1
            e[1] += fp[r]
            o = e[2][(cls[r], owner(r))]
            o[0] += fp[r]
            o[1] += 1
    print("## Ambiguous blocks (flags)")
    print()
    print("| flag | blocks | bytes | meaning |")
    print("|---|---:|---:|---|")
    meaning = {
        "replaced": "born in a project, died before its unload but not short-lived (a project "
                    "object replaced mid-project: a recompile's code, a resized buffer)",
        "born-in-project": "born while a project ran, outlived it — a platform block born "
                           "inside a project (a mixer by construction)",
        "platform-churn": "born outside every project, died before the end, not short-lived",
        "no-unload-seen": "born in a project that the trace never unloads, live at the end",
    }
    for f, (cnt, byt, _) in sorted(fl.items(), key=lambda kv: -kv[1][1]):
        print(f"| {f} | {fmt(cnt)} | {fmt(byt)} | {meaning.get(f, '')} |")
    print()
    for f, (_, _, owners) in sorted(fl.items(), key=lambda kv: -kv[1][1]):
        print(f"Top call sites, `{f}`:")
        print()
        for (c, o), (byt, cnt) in sorted(owners.items(), key=lambda kv: -kv[1][0])[:args.top]:
            print(f"- {fmt(byt)} B in {cnt} block(s), {c}: `{o}`")
        print()

    # top sites per class: by bytes allocated (churn) and by live at the class's peak
    print("## Top call sites per class (bytes allocated)")
    print()
    for c in CLASSES:
        groups = defaultdict(lambda: [0, 0, 0])
        for r in range(n):
            if cls[r] == c:
                g = groups[owner(r)]
                g[0] += t.size[r]
                g[1] += 1
                g[2] = max(g[2], t.size[r])
        if not groups:
            continue
        tot = sum(v[0] for v in groups.values())
        print(f"**{c}** ({fmt(tot)} B allocated):")
        print()
        for o, (byt, cnt, mx) in sorted(groups.items(), key=lambda kv: -kv[1][0])[:args.top]:
            print(f"- {fmt(byt)} B ({100 * byt / max(tot, 1):.0f} %), {fmt(cnt)} blocks, "
                  f"largest {fmt(mx)}: `{o}`")
        print()

    # ---- mixers: long-lived blocks born inside a transient window
    print("## Mixers: long-lived blocks born inside a job window")
    print()
    job = sorted((s, e, k) for k, s, e in wins)
    mix = defaultdict(lambda: [0, 0])
    mix_sites = defaultdict(lambda: [0, 0])
    for r in range(n):
        c = cls[r]
        if c in ("frame", "transient", "radio"):
            continue
        b = t.born[r]
        for s, e, k in job:
            if s <= b <= e and k in ("compile", "unload"):
                mix[(k, c)][0] += fp[r]
                mix[(k, c)][1] += 1
                mix_sites[(k, c, owner(r))][0] += fp[r]
                mix_sites[(k, c, owner(r))][1] += 1
                break
    print("Load windows are not counted here: a project is born in its load by design. "
          "A compile's or an unload's long-lived births are the blocks that land among that "
          "window's churn.")
    print()
    print("| window | class | blocks | bytes |")
    print("|---|---|---:|---:|")
    for (k, c), (byt, cnt) in sorted(mix.items(), key=lambda kv: -kv[1][0]):
        print(f"| {k} | {c} | {fmt(cnt)} | {fmt(byt)} |")
    print()
    for (k, c, o), (byt, cnt) in sorted(mix_sites.items(), key=lambda kv: -kv[1][0])[:args.top]:
        print(f"- {k}/{c}: {fmt(byt)} B in {cnt}: `{o}`")
    print()

    # ---- today's replay
    regions, align, stats, snaps, where = replay_today(t, C6_REGIONS)
    rep = sum(v for k, v in stats.items() if k.startswith("reproduced"))
    print("## Today's layout: the model against the trace")
    print()
    print(f"{fmt(rep)} of {fmt(n)} placements reproduced at the trace's own address "
          f"({100 * rep / max(n, 1):.3f} %): " +
          ", ".join(f"{k} {fmt(v)}" for k, v in sorted(stats.items())) +
          ". (`reproduced-align-N`: the address needs alignment N, which the trace does not "
          "carry; the counterfactuals replay those blocks at that alignment. `forced`: no "
          "alignment reproduces it, so the block was put where the trace says.)")
    print()
    if t.heartbeats:
        # Today's largest free block at each heartbeat line's arrival, from the
        # trace's own addresses.
        want = {ei for ei, _ in t.heartbeats}
        got = {}
        rg2 = [Region(nm, b, sz) for (nm, sz, _), b in
               zip(C6_REGIONS, (0x40827028, 0x4086E610, 0x4081B028))]
        for ei, ev in enumerate(t.events):
            if ei in want:
                got[ei] = max(r_.largest() for r_ in rg2)
            r, is_free = ev >> 1, ev & 1
            ri = where[r]
            if ri is None:
                continue
            if is_free:
                rg2[ri].release(t.ptr[r], t.size[r])
            else:
                rg2[ri].carve(t.ptr[r], t.size[r])
        same = sum(1 for ei, lfb in t.heartbeats if got.get(ei) == lfb)
        print(f"Cross-check: the firmware's heartbeat `largestFreeBlock` against the trace's own "
              f"layout where the console line arrived: {same} of {len(t.heartbeats)} equal (the "
              f"line lags the guest by up to a slice). Firmware/trace: " +
              ", ".join(f"{lfb:,}/{got.get(ei, 0):,}" for ei, lfb in t.heartbeats[:8]) + ".")
        print()

    # ---- mixers, part 2: who bounds today's largest holes
    probe_marks = {mi for mi, (_, cyc, x) in enumerate(t.marks)
                   if cyc >= min((s_ for k, s_, _ in wins if k == "load"), default=0)
                   and any(k_ in x for k_ in ("compilation starting", "[perf] frame="))}
    bnd = bounding_at_marks(t, where, probe_marks)
    by_cls = defaultdict(lambda: [0, 0])
    by_site = defaultdict(lambda: [0, set()])
    sides = 0
    for mi, rows in bnd.items():
        for region, start, size, below, above in rows:
            for side in (below, above):
                if side is None:
                    continue
                sides += 1
                by_cls[(region, cls[side])][0] += 1
                by_site[(region, cls[side], owner(side))][0] += 1
                by_site[(region, cls[side], owner(side))][1].add(side)
    print("### Who holds today's largest holes apart")
    print()
    print(f"At {len(bnd)} markers from the first load on (every compile start and every `[perf]` "
          f"line), the live blocks on either side of each region's three largest holes, from the "
          f"trace's own addresses ({sides} sides in all; a hole at a region's edge has one).")
    print()
    print("| region | class of the bounding block | sides |")
    print("|---|---|---:|")
    for (region, c), (cnt, _) in sorted(by_cls.items(), key=lambda kv: (kv[0][0], -kv[1][0])):
        print(f"| {region} | {c} | {cnt} |")
    print()
    for (region, c, o), (cnt, rids) in sorted(by_site.items(), key=lambda kv: -kv[1][0])[:args.top + 4]:
        sz = ", ".join(sorted({fmt(t.size[x]) for x in rids})[:4])
        print(f"- {region}, {c}: {cnt} sides, {len(rids)} distinct block(s) ({sz} B): `{o}`")
    print()

    # ---- groupings
    groupings = []
    specs = args.grouping or [
        "all-six=platform/session/project/frame/transient/discardable",
        "three=platform+session/project+discardable/frame+transient",
        "big-block-only=platform+session+project+discardable+frame/transient",
        "big-block+frame=platform+session+project+discardable/frame+transient",
    ]
    for spec in specs:
        name, rest = spec.split("=", 1)
        groups = [tuple(g.split("+")) for g in rest.split("/")]
        seen = [c for g in groups for c in g]
        assert sorted(seen) == sorted(CLASSES), f"{spec}: every class exactly once"
        groupings.append((name, groups))

    areas = [int(a) for a in args.areas.split(",")]
    budget = sum(areas)
    first_load = min((s_ for k, s_, _ in wins if k == "load"), default=0)
    key_marks = [mi for mi, (_, _, x) in enumerate(t.marks)
                 if any(k_ in x for k_ in ("compilation starting", "[perf] frame=",
                                           "[mem] load_project after", "[mem] boot auto_load after",
                                           "[mem] load_project unload existing before",
                                           "[mem] stop_all_projects before"))]

    def today_largest(mi):
        return max(x[0] for x in snaps[mi])

    out = {"label": label, "trace": args.trace, "boot": args.boot, "rust_peak": rust_peak,
           "sum_of_class_peaks": sum_of_peaks, "peak_by_class": dict(peak_by),
           "live_at_end_by_class": dict(live_by), "alloc_bytes": dict(alloc_bytes),
           "alloc_count": dict(alloc_count), "replay_stats": dict(stats),
           "flags": {f: [v[0], v[1]] for f, v in fl.items()}, "groupings": {}}

    print("## Segregation counterfactual")
    print()
    print(f"Each group replays first fit in a region of its own; its **footprint** is the highest "
          f"address it used (the region size at which it never fails: zero slack). The group "
          f"holding `transient` is the **absorber**, the one big block: it takes whatever the "
          f"physical areas have left. Areas: {', '.join(fmt(a) for a in areas)} B (today's main "
          f"and dram2_seg, {fmt(budget)} B); the radio's region stays as it is, holding the radio's "
          f"C blocks. A grouping **packs** when every non-absorber group fits whole in one area; "
          f"else it **spills**: the largest groups fill the first area and the one crossing its end "
          f"is split across both (two regions tried in order, as esp-alloc does), replayed again "
          f"that way. Today's peak live set (everything but the radio's region): "
          f"**{fmt(rust_peak)} B** (footprints).")
    print()
    print("| grouping | group | footprint | its own peak live | region as placed |")
    print("|---|---|---:|---:|---:|")
    for name, groups in groupings:
        reps, shapes, radio_over = replay_grouping(t, cls, align, groups)
        absorber = next(i for i, g in enumerate(groups) if "transient" in g)
        sizes = [rp.high for rp in reps]
        total = sum(sizes)
        packed = pack(sizes, areas, absorber)
        mode = "packs"
        region_size = list(sizes)
        bounded_note = ""
        split = None
        rem0 = 0
        if packed:
            region_size[absorber] = packed[1]
        else:
            plan = plan_spill(sizes, areas, absorber)
            ok = False
            if plan:
                split, rem0, in1 = plan
                bnd = [()] * len(groups)
                bnd[split] = (rem0,)
                reps2, shapes2, radio_over2 = replay_grouping(t, cls, align, groups, bnd)
                f2 = reps2[split].high
                left1 = areas[1] - sum(sizes[i] for i in in1) - f2
                if left1 >= sizes[absorber]:
                    ok = True
                    mode = "spills"
                    reps, shapes, radio_over = reps2, shapes2, radio_over2
                    region_size[split] = f2
                    region_size[absorber] = left1
                    total = sum(sizes) - sizes[split] + rem0 + f2
                    bounded_note = (f"; {reps[split].name} split {fmt(rem0)} B (area 0) + "
                                    f"{fmt(f2)} B (area 1)")
            if not ok:
                mode = "does not fit"
                split = None
                region_size[absorber] = sizes[absorber] + max(0, budget - total)
        for gi, rp in enumerate(reps):
            shown = (f"{fmt(rem0)} + {fmt(region_size[gi])}" if gi == split
                     else fmt(region_size[gi]))
            print(f"| {name} | {rp.name}{' (absorber)' if gi == absorber else ''} | "
                  f"{fmt(sizes[gi])} | {fmt(rp.peak_live)} | {shown} |")
        print(f"| {name} | **total reservation** (Σ footprints) | **{fmt(total)}** "
              f"({100 * total / max(rust_peak, 1):.1f} % of today's peak live) | | "
              f"{mode}{bounded_note}; radio fallbacks {radio_over} |")
        g_out = {"groups": [rp.name for rp in reps], "footprint": sizes,
                 "peak_live": [rp.peak_live for rp in reps], "absorber": absorber,
                 "total": total, "mode": mode, "region_size": region_size,
                 "split": split, "split_area0": rem0,
                 "radio_fallbacks": radio_over, "marks": []}
        for mi in range(len(t.marks)):
            sh = shapes.get(mi)
            if sh is None or mi not in snaps:
                continue
            per = [largest_at(sh[gi], region_size[gi]) for gi in range(len(groups))]
            g_out["marks"].append({"mark": mi, "cycle": t.marks[mi][1],
                                   "text": t.marks[mi][2][:120], "today": today_largest(mi),
                                   "today_regions": [x[0] for x in snaps[mi]],
                                   "largest_per_group": per})
        out["groupings"][name] = g_out
    print()

    print("### Largest block a big-block request could get, at the key points")
    print()
    print("'today' = the largest free block any Rust request can reach (main, dram2_seg, radio), "
          "from the replay (see the cross-check). Each grouping's column is its absorber's "
          "largest free block, as placed.")
    print()
    print("| point | today | " + " | ".join(nm for nm, _ in groupings) + " |")
    print("|---|---:|" + "---:|" * len(groupings))
    for mi in key_marks:
        if mi not in snaps:
            continue
        cells = []
        for nm, _ in groupings:
            g = out["groupings"][nm]
            m = next(x for x in g["marks"] if x["mark"] == mi)
            v = m["largest_per_group"][g["absorber"]]
            cells.append(fmt(v) if g["mode"] != "does not fit" else "—")
        text = t.marks[mi][2].split(": ", 1)[-1][:56]
        print(f"| {t.marks[mi][1] / CYCLES_PER_US / 1e6:.2f} s {text} | {fmt(today_largest(mi))} | "
              + " | ".join(cells) + " |")
    print()
    print("— = the grouping's regions, each at its footprint, need more than the areas hold: "
          "there is no absorber to measure.")
    print()

    print("### Summary over every marker from the first project load on")
    print()
    print("Ratio = the absorber's largest free block ÷ today's, at the same marker. "
          "'at compiles' = only the markers where a shader compile starts (the big ask).")
    print()
    print("| grouping | total reservation | % of today's peak live | fits | min today | "
          "min absorber | min ratio | median ratio | median ratio at compiles |")
    print("|---|---:|---:|---|---:|---:|---:|---:|---:|")
    for nm, _ in groupings:
        g = out["groupings"][nm]
        ms = [m for m in g["marks"] if m["cycle"] >= first_load]
        ratios = sorted(m["largest_per_group"][g["absorber"]] / max(m["today"], 1) for m in ms)
        comp = sorted(m["largest_per_group"][g["absorber"]] / max(m["today"], 1) for m in ms
                      if "compilation starting" in m["text"])
        med = ratios[len(ratios) // 2] if ratios else 0
        cmed = comp[len(comp) // 2] if comp else 0
        mt = min((m["today"] for m in ms), default=0)
        ma = min((m["largest_per_group"][g["absorber"]] for m in ms), default=0)
        if g["mode"] == "does not fit":
            row = {"grouping": nm, "total": g["total"],
                   "pct_of_peak": 100 * g["total"] / max(rust_peak, 1), "mode": g["mode"],
                   "min_today": mt, "min_absorber": None, "min_ratio": None,
                   "median_ratio": None, "median_ratio_compiles": None,
                   "over_budget": g["total"] - budget}
            g["summary"] = row
            print(f"| {nm} | {fmt(g['total'])} | {row['pct_of_peak']:.1f} | does not fit "
                  f"(+{fmt(g['total'] - budget)} B over {fmt(budget)}) | {fmt(mt)} | — | — | — | — |")
            continue
        row = {"grouping": nm, "total": g["total"],
               "pct_of_peak": 100 * g["total"] / max(rust_peak, 1),
               "mode": g["mode"], "min_today": mt, "min_absorber": ma,
               "min_ratio": ratios[0] if ratios else 0, "median_ratio": med,
               "median_ratio_compiles": cmed}
        g["summary"] = row
        print(f"| {nm} | {fmt(g['total'])} | {row['pct_of_peak']:.1f} | {g['mode']} | {fmt(mt)} | "
              f"{fmt(ma)} | {row['min_ratio']:.2f} | {med:.2f} | {cmed:.2f} |")
    print()

    print("### Every group's largest free block (min / median over the same markers)")
    print()
    for nm, _ in groupings:
        g = out["groupings"][nm]
        ms = [m for m in g["marks"] if m["cycle"] >= first_load]
        cells = []
        for gi, gname in enumerate(g["groups"]):
            vals = sorted(m["largest_per_group"][gi] for m in ms)
            if vals:
                cells.append(f"{gname} {fmt(vals[0])} / {fmt(vals[len(vals) // 2])}")
        print(f"- {nm}: " + "; ".join(cells))
    ms = [mi for mi in snaps if t.marks[mi][1] >= first_load]
    tv = sorted(today_largest(mi) for mi in ms)
    if tv:
        print(f"- today (one shared heap), any request: {fmt(tv[0])} / {fmt(tv[len(tv) // 2])}")
    print()

    if args.out:
        os.makedirs(args.out, exist_ok=True)
        with open(os.path.join(args.out, f"{label}.json"), "w") as fo:
            json.dump(out, fo, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
