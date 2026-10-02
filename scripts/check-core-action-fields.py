#!/usr/bin/env python3
"""The core action-fields ratchet (docs/adr/2026-10-01-agentic-control-offers-in-core.md).

Every verb the user can press is meant to be published once, into the
offer tree beside the view (`UiStudioView::offers`), where the web renders
it and the app agent reads and presses it by path. Core's view types still
carry many actions on their own DTO fields instead. This script counts
those fields per file and fails when a file carries MORE than its recorded
count, or a new file carries any. Fewer is welcome: lock the drop in with
`--bless`.

Counted, in `lp-app/lpa-studio-core/src` (above a file's `#[cfg(test)]`
module, with line comments stripped): every `pub` (or `pub(...)`) field of
a braced struct whose type mentions `UiAction`, `UiPaneAction` or
`UiActions` — bare, or inside `Vec<…>`, `Option<…>` and the like.

Not counted: `core/action/` and `core/offer/`, which hold the action and
offer types themselves (`UiOffer.action` is the one place an action
belongs).

    scripts/check-core-action-fields.py            # check against the record
    scripts/check-core-action-fields.py --bless    # rewrite the record
"""

import collections
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
CORE_SRC = os.path.join(ROOT, "lp-app", "lpa-studio-core", "src")
RECORD = os.path.join(ROOT, "scripts", "core-action-fields-ratchet.txt")

EXCLUDED_DIRS = (
    os.path.join(CORE_SRC, "core", "action"),
    os.path.join(CORE_SRC, "core", "offer"),
)

STRUCT_OPEN = re.compile(r"\bstruct\s+\w+\s*(?:<[^{;()]*>)?\s*(?:where[^{;]*)?\{")
PUB_FIELD = re.compile(r"^pub(?:\([^)]*\))?\s+(?:r#)?\w+\s*:(.*)$", re.DOTALL)
ACTION_TYPE = re.compile(r"\b(?:UiAction|UiPaneAction|UiActions)\b")


def excluded(path):
    return any(path.startswith(directory + os.sep) for directory in EXCLUDED_DIRS)


def strip(source):
    cut = source.find("#[cfg(test)]")
    if cut >= 0:
        source = source[:cut]
    return "\n".join(line.split("//")[0] for line in source.splitlines())


def struct_bodies(source):
    """The text between each braced struct's `{` and its matching `}`."""
    for match in STRUCT_OPEN.finditer(source):
        depth = 1
        start = match.end()
        at = start
        while at < len(source) and depth:
            if source[at] == "{":
                depth += 1
            elif source[at] == "}":
                depth -= 1
            at += 1
        yield source[start : at - 1]


def fields(body):
    """Split a struct body at its top-level commas."""
    depth = 0
    field = []
    previous = ""
    for char in body:
        if char in "<([{":
            depth += 1
        elif char in ">)]}" and not (char == ">" and previous == "-"):
            depth -= 1
        previous = char
        if char == "," and depth == 0:
            yield "".join(field)
            field = []
        else:
            field.append(char)
    if "".join(field).strip():
        yield "".join(field)


def without_attributes(field):
    lines = [line for line in field.strip().splitlines() if not line.strip().startswith("#[")]
    return " ".join(line.strip() for line in lines)


def count():
    counts = collections.Counter()
    for root, _, files in os.walk(CORE_SRC):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(root, name)
            if excluded(path):
                continue
            with open(path, encoding="utf-8") as handle:
                source = strip(handle.read())
            hits = 0
            for body in struct_bodies(source):
                for field in fields(body):
                    match = PUB_FIELD.match(without_attributes(field))
                    if match and ACTION_TYPE.search(match.group(1)):
                        hits += 1
            if hits:
                counts[os.path.relpath(path, ROOT)] = hits
    return counts


def read_record():
    record = {}
    if not os.path.exists(RECORD):
        return record
    with open(RECORD, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            hits, path = line.split(None, 1)
            record[path] = int(hits)
    return record


def write_record(counts):
    with open(RECORD, "w", encoding="utf-8") as handle:
        handle.write(
            "# Action-carrying pub fields on core view types, per file — a ratchet:\n"
            "# counts may only go down. Written by `scripts/check-core-action-fields.py\n"
            "# --bless`; see that script and docs/adr/2026-10-01-agentic-control-offers-in-core.md.\n"
        )
        for path in sorted(counts):
            handle.write(f"{counts[path]} {path}\n")


def main():
    counts = count()
    if "--bless" in sys.argv[1:]:
        write_record(counts)
        print(f"core-action-fields: recorded {sum(counts.values())} in {len(counts)} files")
        return 0
    record = read_record()
    grew = []
    shrank = []
    for path, hits in sorted(counts.items()):
        was = record.get(path, 0)
        if hits > was:
            grew.append(f"  {path}: {was} -> {hits}")
        elif hits < was:
            shrank.append(f"  {path}: {was} -> {hits}")
    for path, was in sorted(record.items()):
        if path not in counts:
            shrank.append(f"  {path}: {was} -> 0")
    if shrank:
        print(
            "core-action-fields: fewer action fields on core view types — lock it in with\n"
            "`just lint-core-action-fields --bless`:"
        )
        print("\n".join(shrank))
    if grew:
        print(
            "core-action-fields: core view types carry MORE action fields than recorded. Publish\n"
            "the verb into the offer tree (`UiOfferTree`, at a path) instead of a DTO field, so the\n"
            "web and the app agent read it from one place:"
        )
        print("\n".join(grew))
        return 1
    print(
        f"core-action-fields: ok ({sum(counts.values())} action fields in {len(counts)} files)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
