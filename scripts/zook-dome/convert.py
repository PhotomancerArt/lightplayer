#!/usr/bin/env python3
"""Convert Zook's dome wiring sketch (mapping-attempt.svg) into the
examples/zook-dome fixture.map2d.json.

One-off tooling for one specific Illustrator export — NOT the product SVG
importer. The sketch encodes:

  - <g id="Structure">: gray struts + hub circles (not LED data; used to
    build the canonical hub list and the validation plot).
  - Five sector groups (Sector_1, Sector_11..14), one per output channel,
    in document order red/yellow/green/blue/purple. Every channel starts
    at the apex hub, where the controller lives.
  - Inside a sector, each child <g> is one hub-to-hub run: <line>
    elements (solid = LEDs, dashed = jumper wire with no LEDs) plus one
    filled arrowhead <path> whose initial M point sits on the run's far
    hub, giving the data direction.

Reconstruction: snap run endpoints to clustered hub centers, orient each
run by its arrowhead, then find the Eulerian trail from the apex that
uses every run exactly once (plain greedy chaining is ambiguous at hubs
the channel visits twice — e.g. the out-and-back around each jumper).
Consecutive solid runs merge into one polyline ("stretch"); a jumper ends
the current stretch. Each channel's 300 lamps split across its stretches
proportionally to arc length (largest remainder), matching fixed-pitch
strip cut at the hubs.

Usage:
  python3 scripts/zook-dome/convert.py

Writes examples/zook-dome/fixture.map2d.json and
scripts/zook-dome/validation.svg, and prints a per-channel table.
"""

import json
import math
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
SVG_PATH = HERE / "mapping-attempt.svg"
OUT_DOC = REPO / "examples" / "zook-dome" / "fixture.map2d.json"
OUT_VALIDATION = HERE / "validation.svg"

LAMPS_PER_CHANNEL = 300

# Style classes per sector, transcribed from the SVG's <style> block.
# (channel name, solid line classes, dashed jumper classes, arrowhead fill
# class, display color) in document order = channel order.
SECTORS = [
    ("Sector_1", {"st7"}, {"st8"}, "st14", "#da1f26"),   # ch1 red
    ("Sector_11", {"st6"}, {"st4"}, "st13", "#d7b928"),  # ch2 yellow
    ("Sector_12", {"st10"}, {"st2"}, "st12", "#4bb749"), # ch3 green
    ("Sector_13", {"st9"}, {"st3"}, "st15", "#3550a3"),  # ch4 blue
    ("Sector_14", {"st1"}, {"st5"}, "st17", "#8e459a"),  # ch5 purple
]

STRUCTURE_LINE_CLASS = "st11"
STRUCTURE_CIRCLE_CLASS = "st16"

HUB_CLUSTER_TOL = 20.0   # sloppy hand-drawn hub positions within this merge
                         # (true inter-hub spacing is >100 units)
ENDPOINT_MATCH_TOL = 3.0 # line endpoints within this are "the same point"
HUB_SNAP_TOL = 40.0      # run ends stop up to ~25 units short of hub centers
ARROW_HUB_TOL = 25.0     # arrowhead M point must sit this close to a hub

APEX_HINT = (580.7, 571.2)


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def strip_ns(tag):
    return tag.split("}")[-1]


def parse_svg():
    tree = ET.parse(SVG_PATH)
    root = tree.getroot()
    view_box = [float(v) for v in root.get("viewBox").split()]
    groups = {}
    for child in root:
        if strip_ns(child.tag) == "g":
            groups[child.get("id")] = child
    return view_box, groups


def collect_hubs(structure):
    """Canonical hub centers: cluster circle centers + strut endpoints."""
    points = []
    for el in structure:
        tag = strip_ns(el.tag)
        if tag == "circle" and el.get("class") == STRUCTURE_CIRCLE_CLASS:
            points.append((float(el.get("cx")), float(el.get("cy"))))
        elif tag == "line" and el.get("class") == STRUCTURE_LINE_CLASS:
            points.append((float(el.get("x1")), float(el.get("y1"))))
            points.append((float(el.get("x2")), float(el.get("y2"))))
    clusters = []  # list of [sum_x, sum_y, n]
    for p in points:
        for c in clusters:
            if dist(p, (c[0] / c[2], c[1] / c[2])) <= HUB_CLUSTER_TOL:
                c[0] += p[0]
                c[1] += p[1]
                c[2] += 1
                break
        else:
            clusters.append([p[0], p[1], 1])
    hubs = [(round(c[0] / c[2], 1), round(c[1] / c[2], 1)) for c in clusters]
    return hubs


def nearest_hub(hubs, p, tol, what):
    best = min(hubs, key=lambda h: dist(h, p))
    d = dist(best, p)
    if d > tol:
        sys.exit(f"ERROR: {what} at {p} is {d:.1f} units from nearest hub "
                 f"{best} (tol {tol}) — sketch drifted from expectations")
    return best


def parse_sector_runs(sector_el, solid_cls, dashed_cls, arrow_cls, hubs):
    """Each child <g> with lines + one arrowhead becomes a directed edge
    (from_hub, to_hub, inert)."""
    edges = []
    for child in sector_el:
        if strip_ns(child.tag) != "g":
            continue  # stray empty st0 paths at sector level
        lines = []
        arrow_m = None
        for el in child:
            tag = strip_ns(el.tag)
            cls = el.get("class", "")
            if tag == "line":
                if cls in solid_cls:
                    lines.append((False, (float(el.get("x1")), float(el.get("y1"))),
                                  (float(el.get("x2")), float(el.get("y2")))))
                elif cls in dashed_cls:
                    lines.append((True, (float(el.get("x1")), float(el.get("y1"))),
                                  (float(el.get("x2")), float(el.get("y2")))))
                else:
                    sys.exit(f"ERROR: unexpected line class {cls!r} in "
                             f"{sector_el.get('id')}")
            elif tag == "path" and cls == arrow_cls:
                m = re.match(r"[Mm]\s*(-?[\d.]+)[,\s](-?[\d.]+)", el.get("d"))
                if not m:
                    sys.exit(f"ERROR: arrowhead path without leading M in "
                             f"{sector_el.get('id')}")
                arrow_m = (float(m.group(1)), float(m.group(2)))
        if not lines and arrow_m is None:
            continue
        if not lines or arrow_m is None:
            sys.exit(f"ERROR: run group in {sector_el.get('id')} has "
                     f"{len(lines)} lines and arrow={arrow_m}")
        # Extremities: each group is a single straight hub-to-hub run
        # (possibly drawn as stub + dashed middle + stub, with visual gaps
        # between the pieces), so the extremities are simply the
        # farthest-apart pair of line endpoints.
        endpoints = []
        for _, a, b in lines:
            endpoints.append(a)
            endpoints.append(b)
        e0, e1 = max(
            ((p, q) for i, p in enumerate(endpoints)
             for q in endpoints[i + 1:]),
            key=lambda pq: dist(pq[0], pq[1]))
        h0 = nearest_hub(hubs, e0, HUB_SNAP_TOL, "run endpoint")
        h1 = nearest_hub(hubs, e1, HUB_SNAP_TOL, "run endpoint")
        far = nearest_hub(hubs, arrow_m, ARROW_HUB_TOL, "arrowhead")
        if far == h0:
            frm, to = h1, h0
        elif far == h1:
            frm, to = h0, h1
        else:
            sys.exit(f"ERROR: arrowhead {arrow_m} snaps to {far}, which is "
                     f"neither run end ({h0}, {h1}) in {sector_el.get('id')}")
        inert = any(d for d, _, _ in lines)
        edges.append((frm, to, inert))
    return edges


def eulerian_trail(edges, start):
    """All orderings of `edges` forming a connected trail from `start`.
    Backtracking; edge counts are ~12 per channel."""
    trails = []

    def step(at, used, acc):
        if len(acc) == len(edges):
            trails.append(list(acc))
            return
        for i, (frm, to, inert) in enumerate(edges):
            if i in used or dist(frm, at) > 0.01:
                continue
            used.add(i)
            acc.append(i)
            step(to, used, acc)
            acc.pop()
            used.remove(i)

    step(start, set(), [])
    return trails


def stretches_from_trail(edges, trail):
    """Merge consecutive solid runs into polylines; jumpers break them."""
    stretches = []
    current = None
    for i in trail:
        frm, to, inert = edges[i]
        if inert:
            current = None
            continue
        if current is None:
            current = [frm, to]
            stretches.append(current)
        else:
            current.append(to)
    return stretches


def polyline_length(points):
    return sum(dist(points[i], points[i + 1]) for i in range(len(points) - 1))


def split_lamps(lengths, total):
    """Largest-remainder split of `total` lamps proportional to lengths."""
    whole = sum(lengths)
    raw = [total * l / whole for l in lengths]
    counts = [int(r) for r in raw]
    for i in sorted(range(len(raw)), key=lambda i: raw[i] - counts[i],
                    reverse=True)[: total - sum(counts)]:
        counts[i] += 1
    assert sum(counts) == total
    return counts


def sample_polyline(points, count):
    """Mirror lpc-mapping resolve_path: count lamps evenly by arc length,
    endpoints inclusive. Used only for the validation plot."""
    if count == 1:
        return [points[0]]
    total = polyline_length(points)
    out = []
    for k in range(count):
        target = total * k / (count - 1)
        acc = 0.0
        pos = points[-1]
        for i in range(len(points) - 1):
            seg = dist(points[i], points[i + 1])
            if seg <= 0.0:
                continue
            if acc + seg >= target or i == len(points) - 2:
                t = min(max((target - acc) / seg, 0.0), 1.0)
                pos = (points[i][0] + (points[i + 1][0] - points[i][0]) * t,
                       points[i][1] + (points[i + 1][1] - points[i][1]) * t)
                break
            acc += seg
        out.append(pos)
    return out


def write_validation_svg(view_box, structure, channels):
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'viewBox="{view_box[0]} {view_box[1]} {view_box[2]} {view_box[3]}">',
        '<rect x="-50" y="-50" width="1300" height="1300" fill="#111"/>',
    ]
    for el in structure:
        if strip_ns(el.tag) == "line":
            parts.append(
                f'<line x1="{el.get("x1")}" y1="{el.get("y1")}" '
                f'x2="{el.get("x2")}" y2="{el.get("y2")}" '
                f'stroke="#444" stroke-width="14"/>')
    for color, stretches, counts in channels:
        for pts, n in zip(stretches, counts):
            for k, (x, y) in enumerate(sample_polyline(pts, n)):
                r = 6 if 0 < k < n - 1 else 11
                parts.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" '
                             f'fill="{color}"/>')
    parts.append("</svg>")
    OUT_VALIDATION.write_text("\n".join(parts) + "\n")


def main():
    view_box, groups = parse_svg()
    structure = groups["Structure"]
    hubs = collect_hubs(structure)
    apex = nearest_hub(hubs, APEX_HINT, HUB_CLUSTER_TOL, "apex hint")

    objects = []
    validation_channels = []
    print(f"{len(hubs)} hubs, apex at {apex}")
    print(f"{'ch':<4}{'runs':>5}{'jumpers':>9}{'stretches':>11}"
          f"{'length':>9}{'lamps':>7}")
    for ch_index, (gid, solid, dashed, arrow, color) in enumerate(SECTORS):
        edges = parse_sector_runs(groups[gid], solid, dashed, arrow, hubs)
        trails = eulerian_trail(edges, apex)
        if len(trails) != 1:
            sys.exit(f"ERROR: {gid} has {len(trails)} complete trails from "
                     f"the apex (want exactly 1). Edges: {edges}")
        stretches = stretches_from_trail(edges, trails[0])
        lengths = [polyline_length(s) for s in stretches]
        counts = split_lamps(lengths, LAMPS_PER_CHANNEL)
        n_jumpers = sum(1 for _, _, inert in edges if inert)
        print(f"ch{ch_index + 1:<3}{len(edges):>5}{n_jumpers:>9}"
              f"{len(stretches):>11}{sum(lengths):>9.0f}"
              f"{sum(counts):>7}")
        for s_index, (pts, count) in enumerate(zip(stretches, counts)):
            objects.append({
                "name": f"ch{ch_index + 1}-{chr(ord('a') + s_index)}",
                "shape": {"path": {
                    "points": [[round(x, 1), round(y, 1)] for x, y in pts],
                    "count": count,
                }},
            })
        validation_channels.append((color, stretches, counts))

    total = sum(o["shape"]["path"]["count"] for o in objects)
    assert total == LAMPS_PER_CHANNEL * len(SECTORS), total
    for o in objects:
        assert len(o["shape"]["path"]["points"]) >= 2

    doc = {
        "format": 1,
        "sample_diameter": 2.0,
        "canvas": [view_box[0], view_box[1], round(view_box[2], 1),
                   round(view_box[3], 1)],
        "objects": objects,
    }
    OUT_DOC.parent.mkdir(parents=True, exist_ok=True)
    OUT_DOC.write_text(json.dumps(doc, indent=1) + "\n")
    write_validation_svg(view_box, structure, validation_channels)
    print(f"wrote {OUT_DOC.relative_to(REPO)} "
          f"({len(objects)} objects, {total} lamps)")
    print(f"wrote {OUT_VALIDATION.relative_to(REPO)}")


if __name__ == "__main__":
    main()
