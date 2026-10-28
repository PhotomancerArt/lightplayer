#!/usr/bin/env python3
"""Generate the `basic-{2n,4n,2n-half}` node-count siblings from `projects/test/basic`.

These three projects exist to measure what *one fixture+output pair* costs the
engine, by taking a slope over node count instead of over lamp count (that is
`per_lamp_memory_table.rs`'s job). Everything but the node count is held
identical to the parent so the difference between two runs is the pair.

Shapes (see `README.md`):

| dir              | pairs | lamps/pair | total |
|------------------|------:|-----------:|------:|
| `basic-2n`       |     2 |        241 |   482 |
| `basic-4n`       |     4 |        241 |   964 |
| `basic-2n-half`  |     2 |        121 |   242 |

`basic-2n-half` has the same node count as `basic-2n` and half the lamps, so
subtracting it isolates the per-lamp part of the per-pair slope.

Run from the workspace root, Python 3 stdlib only, deterministic:

    python3 projects/test/basic-2n/generate.py

⚠️ Node-def JSON is read with `kind` as a leading header, so key order matters:
every dict here is built (or parsed) in file order and dumped without
`sort_keys`. Do not switch to a sorting serializer.
"""

from __future__ import annotations

import json
import shutil
from collections import OrderedDict
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[3]
PARENT = WORKSPACE / "projects" / "test" / "basic"
OUT_ROOT = WORKSPACE / "projects" / "test"

# Files copied verbatim from the parent into every generated project.
SHARED = ("clock.json", "shader.json", "shader.glsl")

# One board pin per pair, in `projects/test/quad-strips`'s order and starting on
# the parent's own D10 — these are the WS281x endpoints the emulator's
# permissive manifest (the XIAO ESP32-C6 board profile) actually resolves. An
# invented label like `P0` loads fine and then fails to *open*, which silently
# removes the per-port buffers from the measurement.
PINS = ("D10", "D9", "D8", "D7")

POINTER = (
    "# {title}\n"
    "\n"
    "Generated from `projects/test/basic` by `projects/test/basic-2n/generate.py`;\n"
    "see [`../basic-2n/README.md`](../basic-2n/README.md) for the rules, the\n"
    "regeneration command, and the report that uses these projects.\n"
)


def load_ordered(path: Path):
    """Parse JSON preserving key order (so `kind` stays the leading header)."""
    return json.loads(path.read_text(), object_pairs_hook=OrderedDict)


def dump_ordered(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def half_counts(counts: list[int]) -> list[int]:
    """Halve each ring's count, keeping every ring non-empty."""
    return [max(1, c // 2) for c in counts]


def write_map2d(dest: Path, index: int, half: bool) -> int:
    """Write pair `index`'s map2d (format 6, like the parent's): the parent's
    parts, its `start` on output `index`; returns the lamp count it resolves to."""
    doc = load_ordered(PARENT / "fixture.map2d.json")
    assert doc["format"] == 6, "the parent's map2d is format 6"
    doc["start"]["output"]["set"] = str(index)
    lamps = 1  # the "center" 1×1 grid part
    rings = next(p for p in doc["parts"] if p["id"] == "disc")["kind"]["circle"]["rings"]
    if half:
        for ring, count in zip(rings, half_counts([r["count"] for r in rings])):
            ring["count"] = count
    lamps += sum(r["count"] for r in rings)
    dump_ordered(dest, doc)
    return lamps


def write_fixture(dest: Path, index: int) -> None:
    """Pair `index`'s fixture: the parent's, re-pointed at its own map and bus."""
    doc = load_ordered(PARENT / "fixture.json")
    doc["mapping"]["source"] = f"fixture{index}.map2d.json"
    doc["bindings"]["output"]["target"] = f"bus:control.out/ch{index}"
    dump_ordered(dest, doc)


def write_output(dest: Path, index: int) -> None:
    """Pair `index`'s output: the parent's, named `index` (the name its map2d's
    `start` sets), on its own pin and bus channel."""
    parent = load_ordered(PARENT / "output.json")
    doc = OrderedDict()
    for key, value in parent.items():
        if key != "name":
            doc[key] = value
        if key == "kind":
            doc["name"] = str(index)
    doc["ports"]["0"]["endpoint"] = f"ws281x:local:{PINS[index - 1]}"
    doc["bindings"]["input"]["source"] = f"bus:control.out/ch{index}"
    dump_ordered(dest, doc)


def generate(name: str, title: str, pairs: int, half: bool) -> int:
    assert pairs <= len(PINS), f"{name}: only {len(PINS)} board pins are wired up"
    out = OUT_ROOT / name
    out.mkdir(parents=True, exist_ok=True)
    for shared in SHARED:
        shutil.copyfile(PARENT / shared, out / shared)

    nodes = OrderedDict()
    nodes["clock"] = OrderedDict(ref="./clock.json")
    nodes["shader"] = OrderedDict(ref="./shader.json")
    lamps = 0
    for index in range(1, pairs + 1):
        lamps += write_map2d(out / f"fixture{index}.map2d.json", index, half)
        write_fixture(out / f"fixture{index}.json", index)
        write_output(out / f"output{index}.json", index)
        nodes[f"fixture{index}"] = OrderedDict(ref=f"./fixture{index}.json")
        nodes[f"output{index}"] = OrderedDict(ref=f"./output{index}.json")

    # The canonical module writer (`NodeDef::write_json`) emits the node map
    # sorted by name, and `lp-cli/tests/examples_valid.rs` holds every rig to
    # that byte order — so sort here rather than emitting in build order.
    nodes = OrderedDict(sorted(nodes.items()))
    dump_ordered(out / "module.json", OrderedDict(kind="Module", nodes=nodes))
    parent_project = load_ordered(PARENT / "project.json")
    parent_project["name"] = title
    dump_ordered(out / "project.json", parent_project)

    readme = out / "README.md"
    if name != "basic-2n":
        readme.write_text(POINTER.format(title=title))
    return lamps


def main() -> None:
    for name, title, pairs, half in (
        ("basic-2n", "Basic 2n", 2, False),
        ("basic-4n", "Basic 4n", 4, False),
        ("basic-2n-half", "Basic 2n half", 2, True),
    ):
        lamps = generate(name, title, pairs, half)
        print(f"{name}: {pairs} pairs, {lamps} lamps")


if __name__ == "__main__":
    main()
