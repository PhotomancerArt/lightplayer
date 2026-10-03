#!/usr/bin/env python3
"""The core test-ops ratchet (docs/adr/2026-10-01-agentic-control-offers-in-core.md).

Core's tests are the third consumer of the offer tree, after the web and
the app agent: a test that presses `project/save` by path fails the moment
the UI stops offering Save, and a test that builds `ProjectOp::SaveOverlay`
itself never notices. Tests press offers through the test API in
`lp-app/lpa-studio-core/src/app/studio/offer_press_test_api.rs`. This script
counts the sites in core's test code that still build a user-verb action
directly, per file, and fails when a file has MORE than its recorded count,
or a new file has any. Fewer is welcome: lock the drop in with `--bless`.

Test code, in `lp-app/lpa-studio-core/src`:

- every `*_tests.rs` file;
- every file of a module declared behind `#[cfg(test)]` (`#[cfg(test)]
  mod evals;` makes all of `evals/` test code), found by reading the
  declarations, not from a list;
- every inline `#[cfg(test)] mod … { … }` block of any other file.

Counted there, with comments and the insides of string literals blanked:

- `UiAction::from_op(` — an op made into an action by hand;
- `<Something>Op::action_for(` and `<something>_action_for(` — core's
  constructors for a verb's action, called with arguments the test chose;
- `project_action(` — the e2e tests' helper over `UiAction::from_op`;
- `DevicesOp::` — the device roster's op, however it is built;
- `<Something>Op { … }.into_action()` — an op built field by field (the
  braces are matched backwards, across lines).

A helper's own definition (`fn project_action(`) is not a site; the
`UiAction::from_op(` inside it is.

Not counted, each for its reason (EXCLUDED_DIRS, EXCLUDED_FILES,
OP_DEFINITION, PLUMBING):

- `core/action/` and `core/offer/`: the action and offer types' own unit
  tests. They test the machinery a press goes through, so they build
  actions to feed it; there is no tree to press yet. The same goes for the
  press test API's own unit tests (`app/studio/offer_press_test_api.rs`),
  which press a stand-in tree.
- the inline test modules of a file that defines an op (`pub enum FooOp` /
  `pub struct FooOp`): an op's own unit test builds the op it defines.
- the PLUMBING ops below: dispatched like actions but not verbs anyone
  presses, so never offers (the same set as `check-web-actions.py`).

    scripts/check-core-test-ops.py            # check against the record
    scripts/check-core-test-ops.py --bless    # rewrite the record
"""

import collections
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
CORE_SRC = os.path.join(ROOT, "lp-app", "lpa-studio-core", "src")
RECORD = os.path.join(ROOT, "scripts", "core-test-ops-ratchet.txt")

# The action and offer types' own unit tests (see the module docs).
EXCLUDED_DIRS = (
    os.path.join(CORE_SRC, "core", "action"),
    os.path.join(CORE_SRC, "core", "offer"),
)
# The press test API's own unit tests build a stand-in tree to press.
EXCLUDED_FILES = (os.path.join(CORE_SRC, "app", "studio", "offer_press_test_api.rs"),)

# Ops core dispatches that are plumbing, not verbs: never offers, so never
# counted. Add one only with the reason beside it.
PLUMBING = {
    # The device card's mount/unmount lease on its board's live picture
    # (`ActionClass::Passive`): the web saying what is on screen.
    "DeviceFeedOp",
}

PATTERN = re.compile(
    r"\bUiAction::from_op\("
    r"|\b(?P<op>\w+Op)::action_for\("
    r"|(?P<helper>\b\w+_action_for\(|\bproject_action\()"
    r"|\bDevicesOp::"
)
INTO_ACTION = re.compile(r"\.\s*into_action\s*\(\s*\)")
OP_TYPE = re.compile(r"(?:\w+::)*(?P<op>\w+Op)$")
# A file that defines an op: its own inline tests build it (see above).
OP_DEFINITION = re.compile(r"\bpub(?:\([^)]*\))?\s+(?:enum|struct)\s+\w+Op\b")
# `#[cfg(test)]`, any further attributes, then `mod name;` or `mod name {`.
TEST_MOD = re.compile(
    r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<name>\w+)\s*(?P<end>[;{])"
)
RAW_STRING = re.compile(r'b?r(#*)"')
CHAR_LITERAL = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'")


def blank(source):
    """`source` with comments removed and string/char literal contents
    dropped (newlines kept), so patterns and brace matching see only code."""
    out = []
    at = 0
    size = len(source)
    while at < size:
        char = source[at]
        if source.startswith("//", at):
            while at < size and source[at] != "\n":
                at += 1
            continue
        if source.startswith("/*", at):
            depth = 0
            while at < size:
                if source.startswith("/*", at):
                    depth += 1
                    at += 2
                elif source.startswith("*/", at):
                    depth -= 1
                    at += 2
                    if depth == 0:
                        break
                else:
                    if source[at] == "\n":
                        out.append("\n")
                    at += 1
            continue
        if char in "br" and (at == 0 or not (source[at - 1].isalnum() or source[at - 1] == "_")):
            raw = RAW_STRING.match(source, at)
            if raw:
                close = '"' + raw.group(1)
                end = source.find(close, raw.end())
                end = size if end < 0 else end + len(close)
                out.append('""' + "\n" * source.count("\n", at, end))
                at = end
                continue
        if char == '"':
            start = at
            at += 1
            while at < size and source[at] != '"':
                if source[at] == "\\":
                    at += 1
                at += 1
            at += 1
            out.append('""' + "\n" * source.count("\n", start, at))
            continue
        if char == "'":
            literal = CHAR_LITERAL.match(source, at)
            if literal:
                at = literal.end()
                out.append("' '")
                continue
        out.append(char)
        at += 1
    return "".join(out)


def matching_brace(source, open_at):
    """The index just past the `}` that closes the `{` at `open_at`."""
    depth = 0
    for at in range(open_at, len(source)):
        if source[at] == "{":
            depth += 1
        elif source[at] == "}":
            depth -= 1
            if depth == 0:
                return at + 1
    return len(source)


def module_dir(path):
    """The directory a file's child modules live in."""
    base = os.path.basename(path)
    if base in ("mod.rs", "lib.rs", "main.rs"):
        return os.path.dirname(path)
    return path[: -len(".rs")]


def sources():
    for root, _, files in os.walk(CORE_SRC):
        for name in sorted(files):
            if name.endswith(".rs"):
                path = os.path.join(root, name)
                with open(path, encoding="utf-8") as handle:
                    yield path, blank(handle.read())


def test_only_roots(blanked):
    """The modules declared `#[cfg(test)] mod name;`: each one's file, and
    its directory (everything under it)."""
    roots = []
    for path, source in blanked.items():
        for match in TEST_MOD.finditer(source):
            if match.group("end") == ";":
                child = os.path.join(module_dir(path), match.group("name"))
                roots.extend([child + ".rs", child + os.sep])
    return roots


def test_code(path, source, roots):
    """The parts of `source` that are test code (see the module docs)."""
    if path.endswith("_tests.rs") or any(
        path == root or path.startswith(root) for root in roots
    ):
        return [source]
    if OP_DEFINITION.search(source):
        return []
    return [
        source[match.end() - 1 : matching_brace(source, match.end() - 1)]
        for match in TEST_MOD.finditer(source)
        if match.group("end") == "{"
    ]


def sites(code):
    hits = 0
    for match in PATTERN.finditer(code):
        if match.group("op") in PLUMBING:
            continue
        if match.group("helper") and re.search(r"\bfn\s+$", code[: match.start()]):
            continue
        hits += 1
    hits += sum(1 for match in INTO_ACTION.finditer(code) if op_literal(code, match.start()))
    return hits


def op_literal(source, dot):
    """Whether the receiver of the `.into_action()` at `dot` is a
    `SomeOp { … }` literal of an op that counts."""
    at = dot
    while at > 0 and source[at - 1].isspace():
        at -= 1
    if at == 0 or source[at - 1] != "}":
        return False
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


def excluded(path):
    return path in EXCLUDED_FILES or any(
        path.startswith(directory + os.sep) for directory in EXCLUDED_DIRS
    )


def count():
    blanked = dict(sources())
    roots = test_only_roots(blanked)
    counts = collections.Counter()
    for path, source in blanked.items():
        if excluded(path):
            continue
        hits = sum(sites(code) for code in test_code(path, source, roots))
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
            "# Core test sites that build a user-verb action directly, per file — a ratchet:\n"
            "# counts may only go down. Written by `scripts/check-core-test-ops.py --bless`;\n"
            "# see that script and docs/adr/2026-10-01-agentic-control-offers-in-core.md.\n"
        )
        for path in sorted(counts):
            handle.write(f"{counts[path]} {path}\n")


def main():
    counts = count()
    if "--bless" in sys.argv[1:]:
        write_record(counts)
        print(f"core-test-ops: recorded {sum(counts.values())} in {len(counts)} files")
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
            "core-test-ops: fewer directly-built actions in core tests — lock it in with\n"
            "`just lint-core-test-ops --bless`:"
        )
        print("\n".join(shrank))
    if grew:
        print(
            "core-test-ops: core tests build MORE actions directly than recorded. Press the\n"
            "offer by its path instead (`OfferPressTestApi::press`, in\n"
            "lp-app/lpa-studio-core/src/app/studio/offer_press_test_api.rs), so the test fails\n"
            "when the UI stops offering the verb:"
        )
        print("\n".join(grew))
        return 1
    print(
        f"core-test-ops: ok ({sum(counts.values())} directly-built actions in {len(counts)} files)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
