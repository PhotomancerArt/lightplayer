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
  python3 scripts/zook-dome/convert.py            # per-channel form: 10 path objects, format 1
  python3 scripts/zook-dome/convert.py --repeat   # 1 gapped sector x repeat 5, format 2 (the shipped form)

Both modes write examples/zook-dome/fixture.map2d.json and
scripts/zook-dome/validation.svg. `--repeat` authors channel 1 as a single
gapped path wrapped in a 5-count rotational repeat, and runs a fidelity
check against the per-channel form: per-lamp deviation instance-by-instance,
plus how far each hand-drawn sector sits from an exact 72-degree copy of
sector 1 (the number the G2 gate needs for rotational-vs-hand-drawn).
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

REPEAT_CENTER = (581.5, 573.9)  # the apex hub = the dome's rotational axis

# The sketch's sectors run COUNTERCLOCKWISE in document (= sketch channel)
# order, while a map2d repeat turns clockwise, so repeat instance k lands on
# physical channel INSTANCE_CHANNELS[k]. examples/zook-dome/output.json lists
# its pins in instance order so every physical sector keeps the pin the
# per-channel form assigned it (ch1..ch5 = IO18/IO16/IO14/IO2/IO13).
INSTANCE_CHANNELS = (1, 5, 4, 3, 2)

# Fidelity gate: hand-drawing slop is single-digit units and the
# stretch-boundary knife edge (see fidelity_check) moves at most a lamp or
# two per instance, so anything past these bounds means structural mismatch
# (wrong instance order, wrong rotation direction, drifted sketch).
FIDELITY_MEAN_TOL = 10.0
FIDELITY_VERTEX_TOL = 25.0
FIDELITY_OUTLIER_TOL = 25.0
FIDELITY_MAX_OUTLIERS = 2


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


def stretches_from_trail(trail_edges):
    """Merge consecutive solid runs into polylines; jumpers break them."""
    stretches = []
    current = None
    for frm, to, inert in trail_edges:
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


def write_validation_svg(view_box, structure, channel_runs):
    """channel_runs: (color, runs) per channel, each run a list of lamp
    positions; the first and last lamp of a run draw larger."""
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
    for color, runs in channel_runs:
        for pts in runs:
            for k, (x, y) in enumerate(pts):
                r = 6 if 0 < k < len(pts) - 1 else 11
                parts.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" '
                             f'fill="{color}"/>')
    parts.append("</svg>")
    OUT_VALIDATION.write_text("\n".join(parts) + "\n")


def channel_trails(groups, hubs, apex):
    """Per channel in document (= wiring) order: (color, trail edges),
    where trail edges are (from_hub, to_hub, inert) in data order."""
    channels = []
    for gid, solid, dashed, arrow, color in SECTORS:
        edges = parse_sector_runs(groups[gid], solid, dashed, arrow, hubs)
        trails = eulerian_trail(edges, apex)
        if len(trails) != 1:
            sys.exit(f"ERROR: {gid} has {len(trails)} complete trails from "
                     f"the apex (want exactly 1). Edges: {edges}")
        channels.append((color, [edges[i] for i in trails[0]]))
    return channels


def rounded(points):
    return [[round(x, 1), round(y, 1)] for x, y in points]


def doc_skeleton(view_box, fmt, objects):
    return {
        "format": fmt,
        "sample_diameter": 2.0,
        "canvas": [view_box[0], view_box[1], round(view_box[2], 1),
                   round(view_box[3], 1)],
        "objects": objects,
    }


def write_doc(doc, objects_desc):
    OUT_DOC.parent.mkdir(parents=True, exist_ok=True)
    OUT_DOC.write_text(json.dumps(doc, indent=1) + "\n")
    print(f"wrote {OUT_DOC.relative_to(REPO)} ({objects_desc})")


def channel_lamps_per_channel_form(trail_edges):
    """A channel's 300 lamp positions as the per-channel (P1) doc resolves
    them: rounded stretch polylines, largest-remainder split, each stretch
    sampled endpoint-inclusive."""
    stretches = [rounded(s) for s in stretches_from_trail(trail_edges)]
    lengths = [polyline_length(s) for s in stretches]
    counts = split_lamps(lengths, LAMPS_PER_CHANNEL)
    lamps = []
    for pts, n in zip(stretches, counts):
        lamps.extend(sample_polyline(pts, n))
    return lamps


def emit_per_channel(view_box, structure, channels):
    """The P1 form: one path object per stretch, 10 objects, format 1."""
    objects = []
    validation_runs = []
    print(f"{'ch':<4}{'runs':>5}{'jumpers':>9}{'stretches':>11}"
          f"{'length':>9}{'lamps':>7}")
    for ch_index, (color, trail_edges) in enumerate(channels):
        stretches = stretches_from_trail(trail_edges)
        lengths = [polyline_length(s) for s in stretches]
        counts = split_lamps(lengths, LAMPS_PER_CHANNEL)
        n_jumpers = sum(1 for _, _, inert in trail_edges if inert)
        print(f"ch{ch_index + 1:<3}{len(trail_edges):>5}{n_jumpers:>9}"
              f"{len(stretches):>11}{sum(lengths):>9.0f}"
              f"{sum(counts):>7}")
        for s_index, (pts, count) in enumerate(zip(stretches, counts)):
            objects.append({
                "name": f"ch{ch_index + 1}-{chr(ord('a') + s_index)}",
                "shape": {"path": {
                    "points": rounded(pts),
                    "count": count,
                }},
            })
        validation_runs.append(
            (color, [sample_polyline(pts, n)
                     for pts, n in zip(stretches, counts)]))

    total = sum(o["shape"]["path"]["count"] for o in objects)
    assert total == LAMPS_PER_CHANNEL * len(SECTORS), total
    for o in objects:
        assert len(o["shape"]["path"]["points"]) >= 2

    write_doc(doc_skeleton(view_box, 1, objects),
              f"{len(objects)} objects, {total} lamps")
    write_validation_svg(view_box, structure, validation_runs)
    print(f"wrote {OUT_VALIDATION.relative_to(REPO)}")


def sample_gapped(points, inert, count):
    """Mirror lpc-mapping resolve_path with gaps: `count` lamps evenly by
    ACTIVE arc length; inert segments move the walk without consuming
    distance, so no lamp lands on one."""
    seg = [dist(points[i], points[i + 1]) for i in range(len(points) - 1)]
    total_active = sum(l for l, g in zip(seg, inert) if not g)

    def at_active(target):
        remaining = target
        last_active_end = None
        for i, l in enumerate(seg):
            if inert[i] or l <= 0.0:
                continue
            if remaining <= l:
                t = remaining / l
                a, b = points[i], points[i + 1]
                return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
            remaining -= l
            last_active_end = points[i + 1]
        return last_active_end if last_active_end else tuple(points[0])

    if count == 1:
        return [at_active(0.0)]
    return [at_active(total_active * k / (count - 1)) for k in range(count)]


def rotate_about(p, center, degrees):
    """Mirror lpc-mapping Rotation2d: screen coordinates, y-down, positive
    turns clockwise."""
    r = math.radians(degrees)
    s, c = math.sin(r), math.cos(r)
    dx, dy = p[0] - center[0], p[1] - center[1]
    return (center[0] + dx * c - dy * s, center[1] + dx * s + dy * c)


def best_fit_center(channels, step_deg):
    """Least-squares rotation center: the point c minimizing, over every
    instance k>0 and vertex pair (x from sector 1, y from its physical
    channel), the error of (I - R_k) c = y - R_k x  where R_k rotates by
    k*step_deg."""
    m00 = m01 = m11 = b0 = b1 = 0.0
    verts0 = channel_vertices(channels[0][1])
    for k in range(1, len(INSTANCE_CHANNELS)):
        verts = channel_vertices(channels[INSTANCE_CHANNELS[k] - 1][1])
        if len(verts) != len(verts0):
            continue
        r = math.radians(k * step_deg)
        s, c = math.sin(r), math.cos(r)
        # rows of (I - R): [1-c, s], [-s, 1-c]
        for x, y in zip(verts0, verts):
            rx = y[0] - (x[0] * c - x[1] * s)
            ry = y[1] - (x[0] * s + x[1] * c)
            m00 += (1 - c) * (1 - c) + s * s
            m01 += (1 - c) * s + (-s) * (1 - c)  # symmetric cross term
            m11 += s * s + (1 - c) * (1 - c)
            b0 += (1 - c) * rx + (-s) * ry
            b1 += s * rx + (1 - c) * ry
    det = m00 * m11 - m01 * m01
    if abs(det) < 1e-9:
        return None
    return ((m11 * b0 - m01 * b1) / det, (m00 * b1 - m01 * b0) / det)


def channel_vertices(trail_edges):
    return [trail_edges[0][0]] + [e[1] for e in trail_edges]


def fidelity_check(channels, sector_points, inert, center):
    """Compare the repeat form against the per-channel form, instance k vs
    physical channel INSTANCE_CHANNELS[k].

    Two deviation sources, reported separately:

    - **vertex drift**: how far the hand-drawn sector sits from an exact
      rotation of sector 1 — pure drawing slop, single-digit units.
    - **lamp deviation**: adds the sampling-model difference. The
      per-channel form split 300 lamps across two stretches by largest
      remainder and sampled each endpoint-inclusive; the gapped form keeps
      one uniform pitch over the whole active length. Channels whose split
      sat on the largest-remainder knife edge (raw share ~171.5) flip one
      boundary lamp to the far side of the jumper, so a single lamp can
      deviate by the whole jumper length while every other lamp agrees to
      single digits. Hence the outlier-count bound instead of a max bound —
      the gapped form is the physically faithful one (fixed-pitch strip,
      cut at the hubs and jumpered)."""
    step = 360.0 / len(channels)
    sector_lamps = sample_gapped(sector_points, inert, LAMPS_PER_CHANNEL)

    print("\nfidelity: repeat form vs per-channel form "
          f"(center {center[0]:.1f},{center[1]:.1f})")
    print(f"{'instance':<10}{'vs ch':>6}{'max lamp':>10}{'mean lamp':>11}"
          f"{'lamps>' + format(FIDELITY_OUTLIER_TOL, '.0f'):>9}"
          f"{'vertex drift':>14}")
    failures = []
    verts0 = channel_vertices(channels[0][1])
    for k, ch in enumerate(INSTANCE_CHANNELS):
        color, trail_edges = channels[ch - 1]
        instance = [rotate_about(p, center, k * step) for p in sector_lamps]
        p1 = channel_lamps_per_channel_form(trail_edges)
        devs = [dist(a, b) for a, b in zip(instance, p1)]
        max_dev = max(devs)
        mean_dev = sum(devs) / len(devs)
        outliers = sum(1 for d in devs if d > FIDELITY_OUTLIER_TOL)
        verts = channel_vertices(trail_edges)
        if len(verts) == len(verts0):
            drift = max(
                dist(rotate_about(v, center, -k * step), v0)
                for v, v0 in zip(verts, verts0))
            drift_s = f"{drift:>14.1f}"
        else:
            drift = None
            drift_s = f"{'n/a':>14}"
        print(f"{k:<10}{ch:>6}{max_dev:>10.1f}{mean_dev:>11.1f}"
              f"{outliers:>9}{drift_s}")
        if mean_dev > FIDELITY_MEAN_TOL:
            failures.append(f"instance {k}: mean lamp deviation {mean_dev:.1f}")
        if outliers > FIDELITY_MAX_OUTLIERS:
            failures.append(f"instance {k}: {outliers} lamps deviate past "
                            f"{FIDELITY_OUTLIER_TOL}")
        if drift is None or drift > FIDELITY_VERTEX_TOL:
            failures.append(f"instance {k}: vertex drift {drift}")

    fit = best_fit_center(channels, step)
    if fit:
        print(f"best-fit rotation center: {fit[0]:.1f},{fit[1]:.1f} "
              f"(authored center {center[0]:.1f},{center[1]:.1f}, "
              f"{dist(fit, center):.1f} apart)")
    if failures:
        sys.exit("ERROR: fidelity check failed — structural mismatch "
                 "against the per-channel form:\n  " + "\n  ".join(failures))
    print("fidelity OK: sectors are true rotations within drawing slop; "
          "lamp deviations are the documented sampling-model difference")


def emit_repeat(view_box, structure, channels):
    """The shipped form: channel 1 as one gapped path, wrapped in a
    5-count rotational repeat about the apex. Instance k = physical
    channel k+1."""
    _, trail_edges = channels[0]
    points = rounded(channel_vertices(trail_edges))
    gaps = [i for i, e in enumerate(trail_edges) if e[2]]
    inert = [i in set(gaps) for i in range(len(points) - 1)]
    print(f"sector: {len(trail_edges)} runs, {len(gaps)} jumpers, "
          f"gaps at {gaps}")

    doc = doc_skeleton(view_box, 2, [{
        "name": "sector",
        "shape": {"repeat": {
            "shape": {"path": {
                "points": points,
                "count": LAMPS_PER_CHANNEL,
                "gaps": gaps,
            }},
            "center": [REPEAT_CENTER[0], REPEAT_CENTER[1]],
            "count": len(SECTORS),
        }},
    }])
    write_doc(doc, f"1 repeat object, {len(SECTORS)}x{LAMPS_PER_CHANNEL} lamps")

    fidelity_check(channels, points, inert, REPEAT_CENTER)

    step = 360.0 / len(SECTORS)
    sector_lamps = sample_gapped(points, inert, LAMPS_PER_CHANNEL)
    write_validation_svg(view_box, structure, [
        (channels[ch - 1][0], [[rotate_about(p, REPEAT_CENTER, k * step)
                                for p in sector_lamps]])
        for k, ch in enumerate(INSTANCE_CHANNELS)])
    print(f"wrote {OUT_VALIDATION.relative_to(REPO)}")


def main():
    repeat_mode = "--repeat" in sys.argv[1:]
    view_box, groups = parse_svg()
    structure = groups["Structure"]
    hubs = collect_hubs(structure)
    apex = nearest_hub(hubs, APEX_HINT, HUB_CLUSTER_TOL, "apex hint")
    print(f"{len(hubs)} hubs, apex at {apex}")
    channels = channel_trails(groups, hubs, apex)
    if repeat_mode:
        emit_repeat(view_box, structure, channels)
    else:
        emit_per_channel(view_box, structure, channels)


if __name__ == "__main__":
    main()
