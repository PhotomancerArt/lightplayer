#!/usr/bin/env python3
"""tapstat: size a wire tap recorded by `lp-cli emu serve`.

Record a tap, then read it:

    LP_EMU_WIRE_TAP=/tmp/tap just studio-dev-emu     # use Studio, then stop
    just wire-tap-stat /tmp/tap/c6-a.tap             # per-message-kind sizes
    just wire-tap-stat /tmp/tap/c6-a.tap --ledger    # lens replies, by JSON path

The tap format (lp-cli/src/commands/emu/serve/wire_tap.rs) is one record per
chunk the byte pump carried: `<unix_us> <'>'|'<'> <len>\\n<len bytes>\\n`,
`>` host -> board and `<` board -> host. This script reassembles lines per
direction and classifies each `M!{json}` line by message kind.

A board that was asked to pack (JSON Pack) writes packed frames
(`\\n 0x00 'P' COBS 0x00`) into the `<` chunks, and the tap annotates each
one with a `P` record carrying the `M!{json}` line it stands for and the
frame's size on the wire (`E` for one that did not decode). This script
strips the frames out of `<` (they are 0x00-delimited) and reads the `P`
records in their place, so a packed message is classified by its JSON and
sized by its packed bytes: every size below is WIRE bytes, and the summary
adds a `json` column with what the same messages take as `M!` lines. The
ledger slices the JSON, so its paths are JSON bytes either way.

Project reads are labelled by their request's shape, so the two that
interleave on a live link (the lens and the device card) are kept apart:

  [lens]  Studio's lens: queries AND probes (shapes/nodes/resources/runtime,
          control product, output frame, binding graph);
  [card]  the device card: an `output_frame` probe and nothing else;
  [sync]  Studio's initial sync pages: queries, no probes;
  [other] anything else (e.g. the initial sync's one-probe reads).

A reply takes its request's label through the message id.

Options:

  --skip-seconds N   drop lines in the first N seconds (the push, warm-up)
  --timeline         print bytes per second, both directions
  --ledger [KIND]    for each reply ("unit") of KIND, bytes by JSON path.
                     KIND is `lens` (default), `card`, `sync`, `other`,
                     `heartbeat`, or any kind the summary prints (its first
                     dotted part is enough). Board -> host lines only. A project read's
                     unit is every frame sharing its id; other kinds' unit is
                     a line.
  --depth N          ledger path depth, in keys (default 10)
  --min-bytes N      ledger rows below this median are hidden (default 24)
  --sort size        ledger rows by median size instead of tree order

The ledger's columns, per JSON path (arrays collapse to `[]`, and a path
seen several times in one unit is summed):

  median    the median bytes that path took per unit (exact: sliced from
            the wire bytes, not re-encoded);
  distinct  how many distinct values it took across the units;
  units     how many units it appeared in.

`distinct / units` is the column that finds structure resent unchanged: a
large path with distinct 1 of 500 is the same bytes every read.

Sizes are exact. Times and rates are the host's wall clock over an emulated
board, so they are the emulator's, never a gate. Python 3 stdlib only.
"""

import argparse
import collections
import json
import statistics
import sys


def main():
    args = parse_args()
    records = read_records(args.tap)
    if not records:
        sys.exit("empty tap")
    t0 = records[0][0]
    all_lines = reassemble_lines(records)
    # Labels come from every request, so a reply just past the skip keeps
    # the label its request (just before it) gave it.
    labels = read_labels(all_lines)
    lines = [line for line in all_lines if line[0] - t0 >= args.skip_seconds * 1e6]
    if args.ledger is not None:
        print_ledger(lines, labels, args)
    else:
        print_summary(records, lines, labels, args)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Size a wire tap recorded by `lp-cli emu serve` (LP_EMU_WIRE_TAP).",
        epilog="See the module docstring (head of this file) for the columns.",
    )
    parser.add_argument("tap", help="a <board>.tap file")
    parser.add_argument("--skip-seconds", type=float, default=0.0)
    parser.add_argument("--timeline", action="store_true")
    parser.add_argument("--ledger", nargs="?", const="lens", default=None, metavar="KIND")
    parser.add_argument("--depth", type=int, default=10)
    parser.add_argument("--min-bytes", type=int, default=24)
    parser.add_argument("--sort", choices=["tree", "size"], default="tree")
    return parser.parse_args()


# ---------------------------------------------------------------------------
# Reading the tap.
# ---------------------------------------------------------------------------


def read_records(path):
    """[(unix_us, dir, bytes, wire_len)] in tap order. `dir` is `>`, `<`, or
    an annotation: `P` (a packed frame's `M!` line; `wire_len` is the
    frame's own size) or `E` (a packed frame that did not decode)."""
    data = open(path, "rb").read()
    records = []
    i = 0
    while i < len(data):
        nl = data.index(b"\n", i)
        fields = data[i:nl].split(b" ")
        us, direction, length = fields[:3]
        wire_len = int(fields[3]) if len(fields) > 3 else None
        length = int(length)
        records.append(
            (int(us), chr(direction[0]), data[nl + 1 : nl + 1 + length], wire_len)
        )
        i = nl + 1 + length + 1
    return records


def reassemble_lines(records):
    """[(unix_us, dir, line_bytes_with_newline, wire_bytes)]: a line is
    stamped with the chunk that completed it. A packed frame's line comes
    from its `P` record and is sized by the frame; every other line's wire
    size is its length."""
    pending = {">": b"", "<": b""}
    in_frame = False
    lines = []
    for us, direction, chunk, wire_len in records:
        if direction == "P":
            lines.append((us, "<", chunk, wire_len))
            continue
        if direction == "E":
            continue
        if direction == "<":
            # Every 0x00 opens or closes a packed frame; keep what is outside.
            parts = chunk.split(b"\x00")
            for n, part in enumerate(parts):
                if not in_frame:
                    pending["<"] += part
                if n < len(parts) - 1:
                    in_frame = not in_frame
        else:
            pending[direction] += chunk
        while b"\n" in pending[direction]:
            line, pending[direction] = pending[direction].split(b"\n", 1)
            lines.append((us, direction, line + b"\n", len(line) + 1))
    return lines


def parse_message(line):
    """The `M!` line's JSON as (object, raw_json_bytes), or (None, None)."""
    if not line.startswith(b"M!"):
        return None, None
    raw = line[2:].rstrip(b"\r\n")
    try:
        return json.loads(raw), raw
    except ValueError:
        return None, None


def read_labels(lines):
    """{request id: label} from the host's projectRead requests."""
    labels = {}
    for _, direction, line, _wire in lines:
        if direction != ">":
            continue
        message, _ = parse_message(line)
        if not isinstance(message, dict):
            continue
        body = message.get("msg")
        if isinstance(body, dict) and "projectRead" in body:
            request = body["projectRead"].get("request", {})
            labels[message.get("id")] = read_shape(request)
    return labels


def read_shape(request):
    probes = request.get("probes") or []
    kinds = [next(iter(p)) for p in probes if isinstance(p, dict) and p]
    queries = bool(request.get("queries"))
    if not queries and kinds == ["output_frame"]:
        return "card"
    if queries and kinds:
        return "lens"
    if queries:
        return "sync"
    return "other"


def kind_of(line, labels):
    message, _ = parse_message(line)
    if message is None:
        return "console" if not line.startswith(b"M!") else "M!(unparsed)"
    body = message.get("msg", message)
    if isinstance(body, str):
        return body
    if not isinstance(body, dict) or not body:
        return "M!(empty)"
    key = next(iter(body))
    value = body[key]
    kind = key
    if isinstance(value, dict) and value:
        sub = next(iter(value))
        if key == "projectCommand" and isinstance(value.get("command"), dict):
            sub = "command." + next(iter(value["command"]))
        kind = f"{key}.{sub}"
    if key == "projectRead":
        kind += f" [{labels.get(message.get('id'), 'other')}]"
    return kind


# ---------------------------------------------------------------------------
# The summary.
# ---------------------------------------------------------------------------


def print_summary(records, lines, labels, args):
    if not lines:
        sys.exit("no lines after --skip-seconds")
    t0 = lines[0][0]
    t1 = lines[-1][0]
    span = max((t1 - t0) / 1e6, 1e-6)
    print(
        f"span {span:.1f} s (after skipping {args.skip_seconds:g} s), "
        f"{sum(1 for r in records if r[1] in '<>')} chunks in the whole tap"
    )
    sizes = collections.defaultdict(list)
    json_sizes = collections.defaultdict(int)
    for us, direction, line, wire in lines:
        key = (direction, kind_of(line, labels))
        sizes[key].append(wire)
        json_sizes[key] += len(line)
    packed = [r for r in records if r[1] == "P"]
    errors = [r for r in records if r[1] == "E"]
    totals = {">": 0, "<": 0}
    json_column = f" {'json':>9}" if packed else ""
    print(
        f"{'dir':3} {'kind':46} {'n':>5} {'min':>6} {'med':>6} {'max':>6} {'total':>9}"
        + json_column
    )
    for (direction, kind), values in sorted(sizes.items(), key=lambda kv: -sum(kv[1])):
        values.sort()
        totals[direction] += sum(values)
        json_cell = f" {json_sizes[(direction, kind)]:9}" if packed else ""
        print(
            f"{direction:3} {kind:46} {len(values):5} {values[0]:6} "
            f"{values[len(values) // 2]:6} {values[-1]:6} {sum(values):9}" + json_cell
        )
    print(
        f"host->board {totals['>']} B = {totals['>'] / span:.0f} B/s ; "
        f"board->host {totals['<']} B = {totals['<'] / span:.0f} B/s over {span:.1f} s"
    )
    if packed:
        wire = sum(r[3] for r in packed)
        json = sum(len(r[2]) for r in packed)
        print(
            f"{len(packed)} packed frames in the whole tap: {wire} B on the wire, "
            f"{json} B as M! lines ({wire / max(json, 1):.1%})"
        )
    if errors:
        print(f"{len(errors)} packed frames did not decode (E records)")
    print_read_units(lines, labels)
    if args.timeline:
        per_second = collections.defaultdict(lambda: [0, 0])
        for us, direction, line, wire in lines:
            per_second[int((us - t0) / 1e6)][0 if direction == ">" else 1] += wire
        print("second  host->board  board->host")
        for second in sorted(per_second):
            up, down = per_second[second]
            print(f"{second:6} {up:12} {down:12}")


def print_read_units(lines, labels):
    """A project read's reply is every frame with its id: size those whole."""
    by_label = collections.defaultdict(list)
    for (label, _), (size, frames) in read_units(lines, labels).items():
        by_label[label].append((size, frames))
    if not by_label:
        return
    print()
    print(f"{'project read replies (all frames of one id)':46} {'n':>5} {'min':>6} {'med':>6} {'max':>6} {'frames':>6}")
    for label, units in sorted(by_label.items()):
        sizes = sorted(u[0] for u in units)
        frames = statistics.median(u[1] for u in units)
        print(
            f"  [{label}]{'':{44 - len(label) - 2}} {len(sizes):5} {sizes[0]:6} "
            f"{sizes[len(sizes) // 2]:6} {sizes[-1]:6} {frames:6g}"
        )


def read_units(lines, labels):
    """{(label, id): (line bytes, frame count)} over board->host projectRead lines."""
    units = {}
    for _, direction, line, wire in lines:
        if direction != "<":
            continue
        message, _ = parse_message(line)
        if not isinstance(message, dict):
            continue
        body = message.get("msg")
        if not (isinstance(body, dict) and "projectRead" in body):
            continue
        key = (labels.get(message.get("id"), "other"), message.get("id"))
        size, frames = units.get(key, (0, 0))
        units[key] = (size + wire, frames + 1)
    return units


# ---------------------------------------------------------------------------
# The ledger.
# ---------------------------------------------------------------------------


def print_ledger(lines, labels, args):
    units = ledger_units(lines, labels, args.ledger)
    if not units:
        sys.exit(f"no units of kind {args.ledger!r} (see the summary's kinds)")
    per_path = collections.defaultdict(list)  # path -> [(size, value_bytes)]
    unit_sizes = []
    for unit in units:
        unit_sizes.append(sum(len(raw) + 3 for raw in unit))
        seen = collections.defaultdict(lambda: [0, []])
        for raw in unit:
            walk(raw, body_start(raw), "", 0, args.depth, seen)
        for path, (size, parts) in seen.items():
            per_path[path].append((size, b"\x00".join(parts)))
    count = len(units)
    print(
        f"ledger [{args.ledger}]: {count} units, median {statistics.median(unit_sizes):g} B "
        f"per unit (line bytes, M! and \\n included), after skipping "
        f"{args.skip_seconds:g} s"
    )
    rows = []
    for path, entries in per_path.items():
        median = statistics.median(e[0] for e in entries)
        if median < args.min_bytes:
            continue
        distinct = len({e[1] for e in entries})
        rows.append((path, median, distinct, len(entries)))
    if args.sort == "size":
        rows.sort(key=lambda r: -r[1])
    else:
        rows.sort(key=lambda r: r[0])
    print(f"{'median':>8} {'distinct':>8} {'units':>6}  path")
    for path, median, distinct, seen_in in rows:
        depth = path.count(".")
        leaf = path.rsplit(".", 1)[-1] if args.sort == "tree" else path
        indent = "  " * depth if args.sort == "tree" else ""
        leaf = leaf if args.sort == "size" or depth else path
        print(f"{median:8g} {distinct:8} {seen_in:6}  {indent}{leaf}")


def ledger_units(lines, labels, kind):
    """[[raw_json_bytes, ...]]: one list per unit of `kind`."""
    if kind in ("lens", "card", "sync", "other"):
        grouped = collections.OrderedDict()
        for _, direction, line, _wire in lines:
            if direction != "<":
                continue
            message, raw = parse_message(line)
            if not isinstance(message, dict):
                continue
            body = message.get("msg")
            if not (isinstance(body, dict) and "projectRead" in body):
                continue
            if labels.get(message.get("id"), "other") != kind:
                continue
            grouped.setdefault(message.get("id"), []).append(raw)
        return list(grouped.values())
    units = []
    for _, direction, line, _wire in lines:
        if direction != "<":
            continue
        this_kind = kind_of(line, labels)
        if this_kind == kind or this_kind.split(".")[0] == kind:
            _, raw = parse_message(line)
            if raw is not None:
                units.append([raw])
    return units


def body_start(raw):
    """Where a message's body starts: inside `msg` and its one kind key, so
    paths read `events[].probe…` rather than `msg.projectRead.events[]…`.
    The envelope (`id`, `seq`, `fin`) is the unit total minus the body."""
    at = skip_ws(raw, 0)
    for key, start in object_fields(raw, at):
        if key == "msg":
            if raw[start : start + 1] == b"{":
                fields = object_fields(raw, start)
                if len(fields) == 1:
                    return fields[0][1]
            return start
    return at


def walk(raw, at, path, depth, max_depth, seen, record=True):
    """Record the value at `raw[at]` under `path`, then its children. An
    array's elements are walked under `path[]` without a row of their own
    (the array's row already carries their bytes)."""
    end = skip_value(raw, at)
    if path and record:
        entry = seen[path]
        entry[0] += end - at
        entry[1].append(raw[at:end])
    if depth >= max_depth:
        return end
    c = raw[at : at + 1]
    if c == b"{":
        for key, start in object_fields(raw, at):
            walk(raw, start, f"{path}.{key}" if path else key, depth + 1, max_depth, seen)
    elif c == b"[":
        for start in array_elements(raw, at):
            walk(raw, start, f"{path}[]", depth, max_depth, seen, record=False)
    return end


def object_fields(raw, at):
    """[(key, value_start)] of the object at `at`."""
    fields = []
    i = skip_ws(raw, at + 1)
    if raw[i : i + 1] == b"}":
        return fields
    while True:
        key_end = skip_value(raw, i)
        key = raw[i + 1 : key_end - 1].decode("utf-8", "replace")
        i = skip_ws(raw, key_end) + 1  # past ':'
        start = skip_ws(raw, i)
        fields.append((key, start))
        i = skip_ws(raw, skip_value(raw, start))
        if raw[i : i + 1] == b",":
            i = skip_ws(raw, i + 1)
        else:
            return fields


def array_elements(raw, at):
    """[value_start] of the array at `at`."""
    elements = []
    i = skip_ws(raw, at + 1)
    if raw[i : i + 1] == b"]":
        return elements
    while True:
        elements.append(i)
        i = skip_ws(raw, skip_value(raw, i))
        if raw[i : i + 1] == b",":
            i = skip_ws(raw, i + 1)
        else:
            return elements


def skip_value(raw, at):
    """The index just past the JSON value starting at `at`."""
    c = raw[at]
    if c == 0x22:  # "
        i = at + 1
        while True:
            b = raw[i]
            if b == 0x5C:  # backslash
                i += 2
            elif b == 0x22:
                return i + 1
            else:
                i += 1
    if c in (0x7B, 0x5B):  # { [
        depth = 0
        i = at
        while True:
            b = raw[i]
            if b == 0x22:
                i = skip_value(raw, i)
                continue
            if b in (0x7B, 0x5B):
                depth += 1
            elif b in (0x7D, 0x5D):
                depth -= 1
                if depth == 0:
                    return i + 1
            i += 1
    i = at
    while i < len(raw) and raw[i] not in b",}] \t\r\n":
        i += 1
    return i


def skip_ws(raw, i):
    while i < len(raw) and raw[i] in b" \t\r\n":
        i += 1
    return i


if __name__ == "__main__":
    main()
