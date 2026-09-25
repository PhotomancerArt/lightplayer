#!/usr/bin/env python3
"""Generate a map2d document and its numbered mapping SVG from a PCB's exports.

A fixture whose lamps are soldered to one PCB already has its mapping in the
design files: the PCB knows where every LED sits and the netlist knows the
order the data line visits them. This script reads both and writes the two
files a catalog entry carries:

  <out-dir>/<map2d>   the map2d document (format 1)
  <out-dir>/<svg>     every lamp numbered in wire order and labelled with its
                      PCB designator, one group per mapping object

Inputs, all given on the command line:

  --exports   The design folder (or a copy of it): the newest `Altium_*.zip`
              and `Netlist_*.tel` in it are read (EasyEDA dates their names).
              Or name the two files directly:
  --pcb       EasyEDA's "Altium" export: the zip it downloads, or the
              `*.pcbdoc` inside it (Altium's ASCII PcbDoc). Every
              `|RECORD=Component|` whose SOURCEDESIGNATOR carries the table's
              prefix is a lamp, at X/Y in mil, y-up. The FIRST record, which
              must be the Board record, carries the outline (VXn/VYn).
  --netlist   EasyEDA's Telesis netlist (`Netlist_*.tel`). The chain is
              walked from the one DIN pin whose net has no DOUT on it (the
              data-in pad), DOUT -> DIN, one net at a time.
  --strokes   A JSON strokes table: a path, or the bare name of a table in
              scripts/pcb-export-strokes/ (`playful-choker`). It says which
              designators form each mapping object, in wire order. The
              concatenation of every stroke MUST be the netlist chain — the
              script refuses to write otherwise — so the table can only say
              where the pen lifts, never reorder the wire.
  --out-dir   Where the two files go. Defaults to the table's `out_dir`,
              relative to the repo root (a catalog project directory).

One strokes table per board; `just pcb-map2d <table> <design-folder>` is the
front door.

Doc space is millimetres, y-down, origin at the board outline's top-left, and
the outline's bounding box is the canvas. Each object is a `path` shape whose
points are its pads and whose `count` is its lamp count; the resolver spreads
`count` lamps evenly along the polyline, so the summary line reports the worst
distance between an even sample and the pad it stands for.

  scripts/pcb-export-to-map2d.py --strokes playful-choker --exports <design-folder>
  scripts/pcb-export-to-map2d.py --strokes playful-choker --exports <design-folder> --check

`--check` writes nothing: it regenerates into a temporary directory and fails
unless both files are byte-identical to the ones in --out-dir. `--self-test`
runs the whole pipeline on a synthetic three-lamp board built in memory, so
the parser has a proof that needs no design files, and checks that every
checked-in strokes table loads and names a folder holding both of its outputs
(`just lint-pcb-export`).

The exports themselves are design files and are never committed here.
"""
import argparse
import difflib
import glob
import io
import json
import math
import os
import re
import sys
import tempfile
import zipfile

MIL_TO_MM = 0.0254
SCRIPTS_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(SCRIPTS_DIR)
TABLES_DIR = os.path.join(SCRIPTS_DIR, "pcb-export-strokes")


class ExportError(Exception):
    """The exports or the strokes table do not describe one clean chain."""


# ---- Entry point -------------------------------------------------------------


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__.split("\n\n", 1)[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--exports", help="design folder: newest Altium_*.zip and Netlist_*.tel in it")
    ap.add_argument("--pcb", help="EasyEDA Altium export: the .zip, or the .pcbdoc inside it")
    ap.add_argument("--netlist", help="EasyEDA Telesis netlist (.tel)")
    ap.add_argument("--strokes", help="strokes table: a JSON path, or a name in scripts/pcb-export-strokes/")
    ap.add_argument("--out-dir", help="where the map2d and SVG go (default: the table's out_dir)")
    ap.add_argument("--check", action="store_true",
                    help="write nothing; fail unless regenerating reproduces --out-dir byte for byte")
    ap.add_argument("--self-test", action="store_true",
                    help="run the pipeline on a synthetic board; needs no exports")
    args = ap.parse_args()

    if args.self_test:
        return self_test()
    if args.strokes is None:
        ap.error("missing --strokes")
    if args.exports is None and (args.pcb is None or args.netlist is None):
        ap.error("give --exports <design-folder>, or both --pcb and --netlist")

    try:
        table_path = resolve_table(args.strokes)
        table = load_strokes(table_path)
        pcb, netlist = args.pcb, args.netlist
        if args.exports is not None:
            pcb = pcb or newest(args.exports, "Altium_*.zip")
            netlist = netlist or newest(args.exports, "Netlist_*.tel")
        out_dir = args.out_dir or os.path.join(REPO_ROOT, table["out_dir"])
        print(f"strokes: {table_path}\npcb:     {pcb}\nnetlist: {netlist}")
        outputs, summary = generate(table, read_pcbdoc(pcb), read_text(netlist))
    except ExportError as e:
        print(f"pcb-export-to-map2d: {e}", file=sys.stderr)
        return 1
    print(summary)

    if args.check:
        return check_outputs(outputs, out_dir)
    for name, text in outputs.items():
        with open(os.path.join(out_dir, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"wrote {os.path.join(out_dir, name)}")
    return 0


def generate(table: dict, pcb_text: str, netlist_text: str) -> tuple[dict, str]:
    """Return ({file name: contents}, summary line) for one strokes table."""
    prefix = table["designator_prefix"]
    canvas, pos = read_board(pcb_text, prefix)
    chain = read_chain(netlist_text, prefix, table["din_pin"], table["dout_pin"])

    if sorted(pos) != sorted(chain):
        on_pcb, on_chain = set(pos), set(chain)
        raise ExportError(
            f"the PCB and the netlist disagree on the lamps: "
            f"only on the PCB {sorted(on_pcb - on_chain)}, only on the chain {sorted(on_chain - on_pcb)}")

    runs = [(s["name"], [f"{prefix}{n}" for n in s["leds"]]) for s in table["strokes"]]
    flat = [d for _, run in runs for d in run]
    if flat != chain:
        at = next((i for i, (a, b) in enumerate(zip(flat, chain)) if a != b), min(len(flat), len(chain)))
        raise ExportError(
            f"the strokes are not the netlist chain in order: they diverge at wire index {at} "
            f"(strokes {flat[at:at + 3]}, chain {chain[at:at + 3]}; "
            f"{len(flat)} designators in the strokes, {len(chain)} on the chain)")

    map2d = render_map2d(table, canvas, pos, runs)
    svg = render_svg(table, canvas, pos, runs)
    summary = (f"{len(chain)} lamps in {len(runs)} objects, chain {chain[0]}..{chain[-1]}; "
               f"canvas {canvas[2]}x{canvas[3]} mm; "
               f"worst even-sampling offset {worst_sampling_offset(pos, runs):.2f} mm")
    return {table["map2d"]: map2d, table["svg"]: svg}, summary


# ---- Inputs ------------------------------------------------------------------


def resolve_table(name_or_path: str) -> str:
    """A strokes table path; a bare name means scripts/pcb-export-strokes/<name>.json."""
    if os.path.exists(name_or_path):
        return name_or_path
    named = os.path.join(TABLES_DIR, f"{name_or_path}.json")
    if os.sep not in name_or_path and os.path.exists(named):
        return named
    known = sorted(os.path.splitext(n)[0] for n in os.listdir(TABLES_DIR) if n.endswith(".json"))
    raise ExportError(f"no strokes table {name_or_path!r}; the checked-in tables are {known}")


def newest(folder: str, pattern: str) -> str:
    """The newest export of one kind: EasyEDA dates the names, so the last by name."""
    found = sorted(glob.glob(os.path.join(glob.escape(folder), pattern)))
    if not found:
        raise ExportError(f"no {pattern} in {folder}")
    return found[-1]


def read_pcbdoc(path: str) -> str:
    """The ASCII PcbDoc's text, from the export zip or the bare file."""
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as z:
            docs = [n for n in z.namelist() if n.lower().endswith(".pcbdoc")]
            if len(docs) != 1:
                raise ExportError(f"{path}: expected one .pcbdoc in the zip, found {docs}")
            return z.read(docs[0]).decode("latin-1")
    with open(path, encoding="latin-1") as f:
        return f.read()


def read_text(path: str) -> str:
    with open(path, encoding="latin-1") as f:
        return f.read()


def load_strokes(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        table = json.load(f)
    for key in ("out_dir", "map2d", "svg", "svg_comment", "sample_diameter", "designator_prefix",
                "din_pin", "dout_pin", "strokes"):
        if key not in table:
            raise ExportError(f"{path}: strokes table has no {key!r}")
    return table


def read_board(pcb_text: str, prefix: str) -> tuple[list, dict]:
    """(canvas, {designator: (x_mm, y_mm)}) in doc space: y-down, outline top-left = 0."""
    records = pcb_text.split("|RECORD=")
    if len(records) < 2 or not records[1].startswith("Board|"):
        raise ExportError("the PcbDoc's first record is not the Board record")
    board = records[1]
    vx = [float(v) * MIL_TO_MM for v in re.findall(r"\|VX\d+=([-\d.]+)mil", board)]
    vy = [float(v) * MIL_TO_MM for v in re.findall(r"\|VY\d+=([-\d.]+)mil", board)]
    if not vx or not vy:
        raise ExportError("the Board record has no outline vertices")
    minx, maxy = min(vx), max(vy)
    canvas = [0.0, 0.0, round(max(vx) - minx, 2), round(maxy - min(vy), 2)]

    pos = {}
    for r in records:
        # Component records only: the pads and tracks that follow carry X/Y too.
        if not r.startswith("Component|"):
            continue
        fields = dict(kv.split("=", 1) for kv in r.rstrip("\r\n").split("|") if "=" in kv)
        des = fields.get("SOURCEDESIGNATOR", "")
        if not re.fullmatch(re.escape(prefix) + r"\d+", des):
            continue
        if des in pos:
            raise ExportError(f"{des} is placed twice on the PCB")
        pos[des] = (round(mil(fields["X"]) * MIL_TO_MM - minx, 3),
                    round(maxy - mil(fields["Y"]) * MIL_TO_MM, 3))
    if not pos:
        raise ExportError(f"no {prefix}n components on the PCB")
    return canvas, pos


def mil(value: str) -> float:
    if not value.endswith("mil"):
        raise ExportError(f"expected a mil coordinate, got {value!r}")
    return float(value[:-3])


def read_chain(netlist_text: str, prefix: str, din_pin: int, dout_pin: int) -> list:
    """The lamps in wire order, from the data-in pad's net onward."""
    if "$NETS" not in netlist_text:
        raise ExportError("the netlist has no $NETS section")
    nets = netlist_text.split("$NETS", 1)[1].split("$SCHEDULE", 1)[0]
    nets = re.sub(r",\s*\n\s*", " ", nets)  # a trailing comma continues the net
    pin_re = re.compile(re.escape(prefix) + r"(\d+)\.(\d+)")
    dout_to_din, head = {}, None
    for line in nets.splitlines():
        if ";" not in line:
            continue
        ins, outs = [], []
        for p in line.split(";", 1)[1].split():
            m = pin_re.fullmatch(p)
            if m and int(m.group(2)) == din_pin:
                ins.append(f"{prefix}{m.group(1)}")
            elif m and int(m.group(2)) == dout_pin:
                outs.append(f"{prefix}{m.group(1)}")
        if len(ins) == 1 and len(outs) == 1:
            dout_to_din[outs[0]] = ins[0]
        elif len(ins) == 1 and not outs:
            if head is not None:
                raise ExportError(f"two chain heads: {head} and {ins[0]} both take data from off the chain")
            head = ins[0]
        elif len(ins) > 1:
            raise ExportError(f"one net drives several DINs: {ins} (a split chain is not one wire)")
    if head is None:
        raise ExportError("no chain head: no DIN pin whose net has no DOUT on it")
    chain = [head]
    while chain[-1] in dout_to_din:
        nxt = dout_to_din[chain[-1]]
        if nxt in chain:
            raise ExportError(f"the chain loops back to {nxt}")
        chain.append(nxt)
    return chain


# ---- Outputs -----------------------------------------------------------------


def render_map2d(table: dict, canvas: list, pos: dict, runs: list) -> str:
    objects = [{"name": name, "shape": {"path": {
        "points": [list(pos[d]) for d in run], "count": len(run), "reversed": False}}}
        for name, run in runs]
    doc = {"format": 1, "sample_diameter": table["sample_diameter"], "canvas": canvas,
           "objects": objects}
    return json.dumps(doc, indent=2) + "\n"


def render_svg(table: dict, canvas: list, pos: dict, runs: list) -> str:
    w, h = canvas[2], canvas[3]
    comment = table["svg_comment"]
    lines = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">']
    lines += [("  <!-- " if i == 0 else "       ") + text + (" -->" if i == len(comment) - 1 else "")
              for i, text in enumerate(comment)]
    lines.append(f'  <rect x="0" y="0" width="{w}" height="{h}" fill="#111" stroke="#6bbd45" stroke-width="0.2"/>')
    idx = 0
    for name, run in runs:
        lines.append(f'  <g id="{name}">')
        lines.append('    <polyline fill="none" stroke="#6bbd45" stroke-width="0.15" points="'
                     + " ".join(f"{pos[d][0]} {pos[d][1]}" for d in run) + '"/>')
        for d in run:
            idx += 1
            x, y = pos[d]
            lines.append(f'    <rect x="{x-1:.2f}" y="{y-1:.2f}" width="2" height="2" '
                         f'fill="#fff" stroke="#000" stroke-width="0.1"/>')
            lines.append(f'    <text x="{x:.2f}" y="{y+0.45:.2f}" font-family="sans-serif" '
                         f'font-size="1.2" text-anchor="middle" fill="#000">{idx}</text>')
            lines.append(f'    <text x="{x:.2f}" y="{y+2.2:.2f}" font-family="sans-serif" '
                         f'font-size="0.8" text-anchor="middle" fill="#6bbd45">{d}</text>')
        lines.append("  </g>")
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def worst_sampling_offset(pos: dict, runs: list) -> float:
    """Worst distance from an evenly spaced sample along a stroke to its pad."""
    worst = 0.0
    for _, run in runs:
        pts = [pos[d] for d in run]
        segs = [math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]
        total, n = sum(segs), len(pts)
        for k in range(1, n - 1):
            d, i = total * k / (n - 1), 0
            while i < len(segs) - 1 and d > segs[i]:
                d -= segs[i]
                i += 1
            t = d / segs[i]
            s = (pts[i][0] + (pts[i + 1][0] - pts[i][0]) * t,
                 pts[i][1] + (pts[i + 1][1] - pts[i][1]) * t)
            worst = max(worst, math.dist(s, pts[k]))
    return worst


def check_outputs(outputs: dict, out_dir: str) -> int:
    failed = 0
    for name, text in outputs.items():
        path = os.path.join(out_dir, name)
        try:
            with open(path, "rb") as f:
                have = f.read()
        except FileNotFoundError:
            print(f"MISSING {path}")
            failed += 1
            continue
        if have == text.encode("utf-8"):
            print(f"ok      {path}")
            continue
        failed += 1
        print(f"DIFFERS {path}")
        diff = difflib.unified_diff(have.decode("utf-8", "replace").splitlines(), text.splitlines(),
                                    f"{path} (committed)", f"{path} (regenerated)", lineterm="")
        for line in list(diff)[:40]:
            print(f"  {line}")
    if failed:
        print(f"{failed} file(s) do not match the exports; regenerate without --check "
              f"and review the diff", file=sys.stderr)
    return 1 if failed else 0


# ---- Self-test: the whole pipeline on a synthetic board, no exports needed ---


def self_test() -> int:
    checks = 0

    def expect(cond: bool, what: str) -> None:
        nonlocal checks
        if not cond:
            raise AssertionError(what)
        checks += 1

    # A 100 x 50 mil board (2.54 x 1.27 mm) at an arbitrary origin, three lamps
    # wired LED7 -> LED3 -> LED9, a capacitor, and a pad record carrying X/Y and
    # a designator that must NOT be read as a lamp.
    pcb = "".join([
        "|RECORD=Board|KIND=Protel_Advanced_PCB|VX0=1000mil|VY0=2050mil|VX1=1100mil|VY1=2050mil"
        "|VX2=1100mil|VY2=2000mil|VX3=1000mil|VY3=2000mil|MAINCONTOURVERTEXCOUNT=4\n",
        "|RECORD=Component|ID=1|X=1010mil|Y=2040mil|SOURCEDESIGNATOR=LED7|SOURCEUNIQUEID=a\n",
        "|RECORD=Component|ID=2|X=1050mil|Y=2040mil|SOURCEDESIGNATOR=LED3|SOURCEUNIQUEID=b\n",
        "|RECORD=Component|ID=3|X=1090mil|Y=2010mil|SOURCEDESIGNATOR=LED9|SOURCEUNIQUEID=c\n",
        "|RECORD=Component|ID=4|X=1020mil|Y=2020mil|SOURCEDESIGNATOR=C1|SOURCEUNIQUEID=d\n",
        "|RECORD=Pad|X=1000mil|Y=2000mil|SOURCEDESIGNATOR=LED99\n",
    ])
    netlist = "\n".join([
        "$PACKAGES",
        "C0402 ! C0402 ! 100nF ; C1",
        "$NETS",
        "'+5V' ; C1.2 LED7.4 LED3.4 ,",
        "        LED9.4",
        "'$1N1' ; CN1.1 LED7.3",
        "'$1N2' ; LED7.1 LED3.3",
        "'$1N3' ; LED3.1 ,",
        "        LED9.3",
        "'$1N4' ; LED9.1",
        "$SCHEDULE",
        "$END",
    ])
    table = {
        "map2d": "t.map2d.json", "svg": "t-mapping.svg",
        "svg_comment": ["first line", "last line"],
        "sample_diameter": 1.5, "designator_prefix": "LED", "din_pin": 3, "dout_pin": 1,
        "strokes": [{"name": "one", "leds": [7, 3]}, {"name": "two", "leds": [9]}],
    }

    canvas, pos = read_board(pcb, "LED")
    expect(canvas == [0.0, 0.0, 2.54, 1.27], f"canvas {canvas}")
    expect(sorted(pos) == ["LED3", "LED7", "LED9"], f"lamps {sorted(pos)} (C1 and the pad are not lamps)")
    expect(pos["LED7"] == (0.254, 0.254), f"LED7 at {pos['LED7']} (y flips about the outline's top)")
    expect(pos["LED9"] == (2.286, 1.016), f"LED9 at {pos['LED9']}")
    expect(read_chain(netlist, "LED", 3, 1) == ["LED7", "LED3", "LED9"], "chain walks DOUT -> DIN")

    outputs, summary = generate(table, pcb, netlist)
    doc = json.loads(outputs["t.map2d.json"])
    expect([o["name"] for o in doc["objects"]] == ["one", "two"], "one object per stroke, in order")
    expect(doc["objects"][0]["shape"]["path"] == {
        "points": [[0.254, 0.254], [1.27, 0.254]], "count": 2, "reversed": False}, "path shape")
    svg = outputs["t-mapping.svg"]
    expect("  <!-- first line\n       last line -->\n" in svg, "comment block")
    expect(">3</text>" in svg and ">LED9</text>" in svg, "each lamp carries its wire index and designator")
    expect(svg.index(">LED7<") < svg.index(">LED3<") < svg.index(">LED9<"), "lamps drawn in wire order")
    expect("3 lamps in 2 objects, chain LED7..LED9" in summary, summary)

    # The zip form reads the same PcbDoc.
    with tempfile.TemporaryDirectory() as tmp:
        zpath = os.path.join(tmp, "Altium_test.zip")
        with zipfile.ZipFile(zpath, "w") as z:
            z.writestr("proj/Board1/PCB1.pcbdoc", pcb.encode("latin-1"))
            z.writestr("proj/Board1/Schematic1/P1.schdoc", "not a pcb")
        expect(read_pcbdoc(zpath) == pcb, "zip export yields its one .pcbdoc")

        # --check: identical passes, a one-byte change fails.
        for name, text in outputs.items():
            with open(os.path.join(tmp, name), "w", encoding="utf-8", newline="\n") as f:
                f.write(text)
        quiet = io.StringIO()
        saved = sys.stdout, sys.stderr
        try:
            sys.stdout = sys.stderr = quiet
            ok = check_outputs(outputs, tmp)
            with open(os.path.join(tmp, "t-mapping.svg"), "a", encoding="utf-8") as f:
                f.write(" ")
            drifted = check_outputs(outputs, tmp)
        finally:
            sys.stdout, sys.stderr = saved
        expect(ok == 0, "--check passes on identical files")
        expect(drifted == 1, "--check fails on a one-byte drift")

    # Refusals: every way the table or the exports can disagree.
    def refuses(fragment: str, fn) -> None:
        try:
            fn()
        except ExportError as e:
            expect(fragment in str(e), f"refusal said {e!s}, wanted {fragment!r}")
            return
        raise AssertionError(f"accepted what should be refused ({fragment})")

    reordered = dict(table, strokes=[{"name": "one", "leds": [3, 7]}, {"name": "two", "leds": [9]}])
    refuses("diverge at wire index 0", lambda: generate(reordered, pcb, netlist))
    short = dict(table, strokes=[{"name": "one", "leds": [7, 3]}])
    refuses("diverge at wire index 2", lambda: generate(short, pcb, netlist))
    refuses("two chain heads", lambda: read_chain(netlist.replace("LED7.1 LED3.3", "LED3.3"), "LED", 3, 1))
    refuses("loops back", lambda: read_chain(netlist.replace("'$1N4' ; LED9.1", "'$1N4' ; LED9.1 LED3.3"),
                                             "LED", 3, 1))
    refuses("disagree on the lamps", lambda: generate(table, pcb.replace("LED9|", "LED8|"), netlist))
    refuses("not the Board record", lambda: read_board(pcb.split("\n", 1)[1], "LED"))

    # --exports picks the newest dated export of each kind.
    with tempfile.TemporaryDirectory() as tmp:
        for name in ("Altium_a_2026-01-02.zip", "Altium_a_2026-09-22.zip", "Netlist_s_2026-09-22.tel"):
            open(os.path.join(tmp, name), "w").close()
        expect(newest(tmp, "Altium_*.zip").endswith("Altium_a_2026-09-22.zip"), "newest zip by dated name")
        refuses("no Netlist_*.tel", lambda: newest(os.path.join(tmp, "nope"), "Netlist_*.tel"))
    refuses("the checked-in tables are", lambda: resolve_table("no-such-board"))

    # Every checked-in table is well-formed and points at a folder holding its outputs.
    tables = sorted(glob.glob(os.path.join(TABLES_DIR, "*.json")))
    expect(bool(tables), "at least one checked-in strokes table")
    for path in tables:
        t = load_strokes(path)
        name = os.path.splitext(os.path.basename(path))[0]
        expect(resolve_table(name) == path, f"{name} resolves by name")
        for out in (t["map2d"], t["svg"]):
            expect(os.path.isfile(os.path.join(REPO_ROOT, t["out_dir"], out)),
                   f"{name}: {t['out_dir']}/{out} exists")
        leds = [n for s in t["strokes"] for n in s["leds"]]
        expect(len(leds) == len(set(leds)), f"{name}: no designator in two strokes")

    print(f"self-test: {checks} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
