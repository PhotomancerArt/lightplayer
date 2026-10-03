#!/usr/bin/env python3
"""The web-built actions ratchet (docs/adr/2026-10-01-agentic-control-offers-in-core.md).

Every verb the user can press is meant to be built in core and published
on the view model, where the app agent can see it. Studio's web layer
still builds some actions itself. This script counts them per file and
fails when a file builds MORE than its recorded count, or a new file
builds any. Fewer is welcome: lock the drop in with `--bless`.

Counted, in `lp-app/lpa-studio-web/src` outside stories, tests and
fixtures (and above a file's `#[cfg(test)]` module), with line comments
stripped:

- `UiAction::from_op(` — a controller op made into an action in the view;
- `<Something>Op::action_for(`, `<something>_action_for(`,
  `DevicesOp::new(` / `DevicesOp::on_sim(` — core constructors called
  with arguments the view chose, i.e. the view deciding what is offered;
- `<Something>Op { … }.into_action()` (a struct literal, over any number
  of lines) and `<op variable>.into_action()` — the same op built field by
  field in the view;
- core's device-action helpers (`pending_escape_action(`,
  `blocked_erase_action(`, `device_escape_action(`, `.blocked_action(`,
  `.update_action(`), which build a verb's action for the caller.

Not counted: the PLUMBING ops below. They are dispatched like actions but
are not verbs anyone presses — a card's mount lease for its live picture is
the web reporting what is on screen — so they are not offers and never go
in the offer tree.

    scripts/check-web-actions.py            # check against the record
    scripts/check-web-actions.py --bless    # rewrite the record
"""

import collections
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
WEB_SRC = os.path.join(ROOT, "lp-app", "lpa-studio-web", "src")
RECORD = os.path.join(ROOT, "scripts", "web-actions-ratchet.txt")

PATTERN = re.compile(
    r"UiAction::from_op\(|\b(?P<op>\w+Op)::action_for\(|\b\w+_action_for\(|\bDevicesOp::(?:new|on_sim)\("
    # Core's device-action helpers, which hand back an action for a verb
    # the caller names: called from the web, they are the view deciding
    # what is offered (M3 moved every one of them into the offer tree).
    r"|\b(?:pending_escape_action|blocked_erase_action|device_escape_action)\("
    r"|\.(?:blocked_action|update_action)\("
)

# `.into_action()`, whatever its receiver; the receiver is read backwards.
INTO_ACTION = re.compile(r"\.\s*into_action\s*\(\s*\)")

# A receiver that names an op: `SomeOp` (a struct literal's type, possibly
# path-qualified) or a variable called `op` / `…_op`.
OP_TYPE = re.compile(r"(?:\w+::)*(?P<op>\w+Op)$")
OP_VARIABLE = re.compile(r"(?:^|_)op$")

# Ops the web dispatches that are plumbing, not verbs: never offers, so
# never counted (Q5, M3). Add one only with the reason beside it.
PLUMBING = {
    # The device card's mount/unmount lease on its board's live picture
    # (`ActionClass::Passive`): the web saying what is on screen.
    "DeviceFeedOp",
}


def skipped(name):
    return (
        "stories" in name
        or name.endswith("tests.rs")
        or "story_fixtures" in name
        or "fixtures" in name
    )


def count():
    counts = collections.Counter()
    for root, _, files in os.walk(WEB_SRC):
        for name in files:
            if not name.endswith(".rs") or skipped(name):
                continue
            path = os.path.join(root, name)
            with open(path, encoding="utf-8") as handle:
                source = handle.read()
            cut = source.find("#[cfg(test)]")
            if cut >= 0:
                source = source[:cut]
            source = "\n".join(line.split("//")[0] for line in source.splitlines())
            hits = sum(
                1 for match in PATTERN.finditer(source) if match.group("op") not in PLUMBING
            )
            hits += sum(
                1 for match in INTO_ACTION.finditer(source) if op_receiver(source, match.start())
            )
            if hits:
                counts[os.path.relpath(path, ROOT)] = hits
    return counts


def op_receiver(source, dot):
    """Whether the receiver of the `.into_action()` at `dot` is an op that
    counts: a `SomeOp { … }` literal (its braces matched backwards, across
    lines) or a variable named like an op, and not a PLUMBING op."""
    at = dot
    while at > 0 and source[at - 1].isspace():
        at -= 1
    if at > 0 and source[at - 1] == "}":
        depth = 0
        at -= 1
        while at >= 0:
            if source[at] == "}":
                depth += 1
            elif source[at] == "{":
                depth -= 1
                if depth == 0:
                    break
            at -= 1
        while at > 0 and source[at - 1].isspace():
            at -= 1
        end = at
        while at > 0 and (source[at - 1].isalnum() or source[at - 1] in "_:"):
            at -= 1
        named = OP_TYPE.search(source[at:end])
        return bool(named) and named.group("op") not in PLUMBING
    end = at
    while at > 0 and (source[at - 1].isalnum() or source[at - 1] == "_"):
        at -= 1
    return bool(OP_VARIABLE.search(source[at:end]))


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
            "# Web-built UiActions per file — a ratchet: counts may only go down.\n"
            "# Written by `scripts/check-web-actions.py --bless`; see that script and\n"
            "# docs/adr/2026-10-01-agentic-control-offers-in-core.md.\n"
        )
        for path in sorted(counts):
            handle.write(f"{counts[path]} {path}\n")


def main():
    counts = count()
    if "--bless" in sys.argv[1:]:
        write_record(counts)
        print(f"web-actions: recorded {sum(counts.values())} in {len(counts)} files")
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
        print("web-actions: fewer web-built actions — lock it in with `just lint-web-actions --bless`:")
        print("\n".join(shrank))
    if grew:
        print(
            "web-actions: the web layer builds MORE actions than recorded. Build the action in core\n"
            "and publish it on the view model (the app agent must see every verb the user has):"
        )
        print("\n".join(grew))
        return 1
    print(f"web-actions: ok ({sum(counts.values())} web-built actions in {len(counts)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
