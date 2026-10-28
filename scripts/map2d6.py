"""Map2d format 6, spelled the way the Rust writer spells it.

The one Python writer behind the generators that author `*.map2d.json`
(`scripts/zook-dome/convert.py`, `scripts/perf/gen_lanes_project.py`,
`scripts/pcb-export-to-map2d.py`). Pure stdlib. The canonical form is
`lpc_mapping::map2d6::write` (lp-core/lpc-mapping/src/map2d6/write.rs), and the
project upgrade writes the same bytes (lp-app/lpa-upgrade/src/v11/
freeze_document.rs): two-space-indented JSON ending in a newline, fields in
the writer's order, defaults left out, every number the shortest text that
reads back to the same `f32` (`1.0`, `17.666666`), and a path's `d` as SVG
path data with numbers in Rust's `Display` form (`M 942 831.2 L …`).

One file holds a fixture's layout and its patching: `start` names the output
the fixture's first lamp goes to (`"1"`), and that output's node file carries
the same `"name"` (`named_output`).

    doc = document(sample_diameter=2.0, canvas=[0, 0, 100, 100], parts=[...],
                   output="1", next_ids=ids.next)
    text = dumps(doc)
"""

import json
import struct
from decimal import Decimal

FORMAT = 6

# A part id's kind words, in the reader's order (`PART_KIND_WORDS`).
KIND_WORDS = ("path", "polygon", "circle", "grid", "filled", "points", "group", "repeat")

ID_MAX_LEN = 24


def f32(x):
    """`x` rounded to the `f32` the reader holds, as the shortest Python float
    that reads back to it, so `json.dumps` spells it as `serde_json` does."""
    want = struct.pack("<f", x)
    v = struct.unpack("<f", want)[0]
    for digits in range(1, 10):
        short = float(f"{v:.{digits}g}")
        if struct.pack("<f", short) == want:
            break
    # Python and serde_json (ryu) both write a plain decimal in this range;
    # outside it the exponent forms differ ("1e-05" against "0.00001").
    if short != 0.0 and not 1e-4 <= abs(short) < 1e16:
        raise ValueError(f"{x}: outside the range this writer spells like serde_json")
    return short


def f32s(values):
    return [f32(v) for v in values]


def f32_text(x):
    """An `f32` as Rust's `Display` writes it: shortest digits, no exponent,
    no trailing `.0` (`942`, `831.2`)."""
    text = format(Decimal(repr(f32(x))), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    return text


def path_d(points, gaps=()):
    """A polyline as path data: a move to the first point, a line to each next
    one, and a move instead of the line that ends a jumper (`gaps` holds
    segment indexes: segment i joins point i and point i + 1)."""
    gaps = set(gaps)
    words = []
    for index, (x, y) in enumerate(points):
        letter = "M" if index == 0 or (index - 1) in gaps else "L"
        words.append(f"{letter} {f32_text(x)} {f32_text(y)}")
    return " ".join(words)


def slug(name):
    """A name as an id, as the project upgrade mints one (`freeze_ids::slug`):
    lower-cased, anything outside `[a-z0-9_-]` a dash, runs of dashes one,
    at most 24 bytes; None when that is no id (it must start with a letter)."""
    out = ""
    for c in name:
        c = c.lower() if c.isascii() else c
        if not (("a" <= c <= "z") or ("0" <= c <= "9") or c in "_-"):
            c = "-"
        if c == "-" and (not out or out.endswith("-")):
            continue
        out += c
    out = out[:ID_MAX_LEN].rstrip("-")
    if out and "a" <= out[0] <= "z":
        return out
    return None


def minted_number(part_id):
    """(`word`, n) for an id that reads as minted (`path3`), else None."""
    for word in KIND_WORDS:
        digits = part_id[len(word):] if part_id.startswith(word) else ""
        if digits.isdigit() and digits.isascii():
            return word, int(digits)
    return None


class PartIds:
    """Every object's ids and the document's `next` counters."""

    def __init__(self, objects, next_ids):
        self.objects = objects  # [(repeat ids outermost first, held id)]
        self.next = next_ids    # {word: n}


def part_ids(objects):
    """Ids for a fixture's objects (`freeze_ids::mint_part_ids`).

    `objects` is [(name, kind word, repeats)]. An object's id is its name's
    slug when that is unique among the objects, else its kind and a number
    (`path1`, `path2`, …); each repeat wrapping it takes `repeat<n>`."""
    slugs = [slug(name) for name, _, _ in objects]
    used = set()
    held = [None] * len(objects)
    for index, candidate in enumerate(slugs):
        if candidate is None:
            continue
        if slugs.count(candidate) == 1 and candidate not in used:
            used.add(candidate)
            held[index] = candidate

    counters = {}

    def mint(word):
        n = counters.get(word, 1)
        while f"{word}{n}" in used:
            n += 1
        counters[word] = n + 1
        used.add(f"{word}{n}")
        return f"{word}{n}"

    out = []
    for (_, word, repeats), mine in zip(objects, held):
        repeat_ids = [mint("repeat") for _ in range(repeats)]
        out.append((repeat_ids, mine if mine is not None else mint(word)))
    for part_id in used:
        minted = minted_number(part_id)
        if minted:
            word, n = minted
            counters[word] = max(counters.get(word, 1), n + 1)
    return PartIds(out, counters)


def part(part_id, name, kind_word, body):
    """A part: its id, its name (left out when empty), its kind."""
    out = {"id": part_id}
    if name:
        out["name"] = name
    out["kind"] = {kind_word: body}
    return out


def path(d, count, reversed_=False):
    body = {"d": d, "count": count}
    if reversed_:
        body["reversed"] = True
    return body


def grid(origin, cols, rows, pitch, routing="snake", start_corner="tl"):
    return {"origin": f32s(origin), "cols": cols, "rows": rows, "pitch": f32(pitch),
            "routing": routing, "start_corner": start_corner}


def repeat(repeat_id, center, count, held):
    return {"id": repeat_id,
            "kind": {"repeat": {"center": f32s(center), "count": count, "part": held}}}


def start(output, port=0, lamp=0):
    """The fixture's first lamp: this output, this port, this lamp."""
    return {"output": {"set": output}, "port": {"set": port},
            "channel": {"set": {"lamps": lamp}}}


def document(sample_diameter, parts, output, canvas=None, next_ids=None):
    """A whole format-6 document, in the writer's field order."""
    doc = {"format": FORMAT, "sample_diameter": f32(sample_diameter)}
    if canvas is not None:
        doc["canvas"] = f32s(canvas)
    if next_ids:
        doc["next"] = dict(sorted(next_ids.items()))
    doc["start"] = start(output)
    doc["parts"] = parts
    return doc


def dumps(doc):
    """The document's text: two-space indent, a trailing newline."""
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def named_output(output_doc, name):
    """An output node with `"name"` set, placed right after `"kind"`."""
    out = {}
    for key, value in output_doc.items():
        if key == "name":
            continue
        out[key] = value
        if key == "kind":
            out["name"] = name
    if "name" not in out:
        out = {"name": name, **out}
    return out
