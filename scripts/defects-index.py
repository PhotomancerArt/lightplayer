#!/usr/bin/env python3
"""The defect registry's index, built from each entry's own frontmatter.

docs/defects/ keeps no hand-written index. Every PR that files or closes a
defect used to add or edit a row at the top of one table in README.md, so
any two such PRs open at once conflicted on the same lines, and a
conflicted PR gets no CI at all (docs/debt/hand-written-defects-index.md).
Now an entry's frontmatter IS its index row: filing a defect touches only
its own new file, and this script builds the table when someone reads it.

Usage:
  scripts/defects-index.py                  every entry, newest first
  scripts/defects-index.py --open           only the open ones
  scripts/defects-index.py --class fidelity one class (combines with --open)
  scripts/defects-index.py --by-class       classes by count, then each one's entries
  scripts/defects-index.py --check          lint: every entry carries what the index needs
  scripts/defects-index.py --self-test      the lint's own fixture tests

Output is a markdown table; links are relative to the repo root.

`--check` (`just lint-defects`, in `just check-lint`, and CI's "Defect
registry" job) fails when an entry is not named `YYYY-MM-DD-slug.md`, has
no frontmatter, has a `status` other than open/fixed/wontfix, a `found`
that does not start with a date, an empty `area`, a `class` that is not
one kebab-case word, or no `# title` line. It also fails on a hand-written
index row in README.md: a branch cut before the table went away re-adds
its row when it merges main, and the fix is to delete that row.

A class missing from README.md's class vocabulary is not a lint failure;
`--by-class` marks it, so the drift is visible where classes are read.

Offline and stdlib-only, like the repo's other lint scripts. It reads just
enough YAML for flat `key: value  # comment` frontmatter (continuation
lines join the key above; list items are skipped). Anything it cannot read
is a lint error, never a silent pass.
"""

from __future__ import annotations

import argparse
import datetime
import os
import re
import sys
import tempfile
from dataclasses import dataclass

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFECTS_DIR = os.path.join(REPO_ROOT, "docs", "defects")

STATUSES = ("open", "fixed", "wontfix")
ENTRY_NAME = re.compile(r"^(\d{4}-\d{2}-\d{2})-([a-z0-9]+(?:-[a-z0-9]+)*)\.md$")
CLASS_NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
DATE_PREFIX = re.compile(r"^(\d{4}-\d{2}-\d{2})\b")
FM_KEY = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*):(.*)$")
VOCAB_ITEM = re.compile(r"^- \*\*`([a-z0-9-]+)`\*\*")
# A README table row linking to a dated entry in this directory.
README_ROW = re.compile(r"^\|.*\]\((?:\./)?\d{4}-\d{2}-\d{2}-[^)/]+\.md\)")


def main() -> int:
    ap = argparse.ArgumentParser(
        description="The defect registry's index, built from each entry's frontmatter."
    )
    ap.add_argument("--open", action="store_true", help="only open entries")
    ap.add_argument("--class", dest="klass", metavar="CLASS", help="only this class")
    ap.add_argument("--by-class", action="store_true", help="group by class, biggest first")
    ap.add_argument("--check", action="store_true", help="lint every entry's frontmatter")
    ap.add_argument("--self-test", action="store_true", help="run the lint's fixture tests")
    ap.add_argument("--dir", default=DEFECTS_DIR, help=argparse.SUPPRESS)
    args = ap.parse_args()

    if args.self_test:
        return self_test()
    if args.check:
        problems = lint(args.dir)
        for p in problems:
            print(p, file=sys.stderr)
        if problems:
            print(
                f"\n{len(problems)} problem(s) in the defect registry. The entry "
                "template is in docs/defects/README.md; the index is built from "
                "these fields, so nothing else needs editing.",
                file=sys.stderr,
            )
            return 1
        print(f"defect registry ok: {len(entry_files(args.dir))} entries")
        return 0

    entries = [e for e in load_entries(args.dir) if e is not None]
    if args.open:
        entries = [e for e in entries if e.status == "open"]
    if args.klass:
        entries = [e for e in entries if e.klass == args.klass]
    entries.sort(key=lambda e: (e.date, e.file), reverse=True)
    if args.by_class:
        print(render_by_class(entries, read_vocabulary(args.dir)))
    else:
        print(render_table(entries, with_class=True))
    return 0


@dataclass
class Entry:
    file: str
    date: str
    status: str
    found: str
    area: str
    klass: str
    title: str


def entry_files(directory: str) -> list[str]:
    return sorted(
        f for f in os.listdir(directory) if f.endswith(".md") and f != "README.md"
    )


def load_entries(directory: str) -> list[Entry | None]:
    return [load_entry(directory, f)[0] for f in entry_files(directory)]


def load_entry(directory: str, name: str) -> tuple[Entry | None, list[str]]:
    """One entry and the lint problems found reading it."""
    problems: list[str] = []

    def bad(msg: str) -> None:
        problems.append(f"docs/defects/{name}: {msg}")

    m = ENTRY_NAME.match(name)
    date = ""
    if not m:
        bad("not named YYYY-MM-DD-slug.md (dated by when the defect was found)")
    elif not valid_date(m.group(1)):
        bad(f"{m.group(1)} in the file name is not a date")
    else:
        date = m.group(1)

    with open(os.path.join(directory, name), encoding="utf-8") as f:
        text = f.read()
    fields, body = parse_frontmatter(text)
    if fields is None:
        bad("no frontmatter block (a `---` line first, then status/found/area/class)")
        return None, problems

    status = fields.get("status", "")
    if status not in STATUSES:
        shown = f"`{status}`" if status else "missing"
        bad(
            f"status is {shown}; it must be one of {', '.join(STATUSES)} "
            "(put any detail in a `# comment` after it)"
        )
    found = fields.get("found", "")
    fm = DATE_PREFIX.match(found)
    if not fm or not valid_date(fm.group(1)):
        bad("`found:` must start with the date it was found (YYYY-MM-DD)")
    area = fields.get("area", "")
    if not area:
        bad("`area:` is missing or empty")
    klass = fields.get("class", "")
    if not CLASS_NAME.match(klass):
        shown = f"`{klass}`" if klass else "missing"
        bad(
            f"class is {shown}; it must be one kebab-case word from the class "
            "vocabulary (or a new one, defined there in one line)"
        )
    title = next(
        (line[2:].strip() for line in body.splitlines() if line.startswith("# ")), ""
    )
    if not title:
        bad("no `# title` line after the frontmatter")

    if problems:
        return None, problems
    return Entry(name, date, status, found, area, klass, title), problems


def parse_frontmatter(text: str) -> tuple[dict[str, str] | None, str]:
    """Flat `key: value  # comment` frontmatter, and the body after it.

    A comment starts at a `#` that begins the value or follows whitespace,
    as in YAML. An indented line that is not a list item continues the key
    above it (a wrapped plain scalar). Quotes and backticks around a value
    are dropped.
    """
    lines = text.split("\n")
    if not lines or lines[0].rstrip() != "---":
        return None, text
    fields: dict[str, str] = {}
    last: str | None = None
    for i in range(1, len(lines)):
        line = lines[i]
        if line.rstrip() == "---":
            return {k: unquote(v) for k, v in fields.items()}, "\n".join(lines[i + 1 :])
        m = FM_KEY.match(line)
        if m:
            last = m.group(1)
            fields[last] = strip_comment(m.group(2))
        elif last and line[:1].isspace():
            rest = line.strip()
            if rest and not rest.startswith(("-", "#")):
                more = strip_comment(rest)
                fields[last] = f"{fields[last]} {more}".strip() if more else fields[last]
    return None, text  # no closing `---`


def strip_comment(value: str) -> str:
    m = re.search(r"(?:^|\s)#", value)
    return (value[: m.start()] if m else value).strip()


def unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'`":
        return value[1:-1].strip()
    return value


def valid_date(s: str) -> bool:
    try:
        datetime.date.fromisoformat(s)
    except ValueError:
        return False
    return True


def read_vocabulary(directory: str) -> set[str]:
    """The classes README.md's "Class vocabulary" section defines."""
    path = os.path.join(directory, "README.md")
    if not os.path.exists(path):
        return set()
    vocab: set[str] = set()
    in_section = False
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith("## "):
                in_section = line.strip() == "## Class vocabulary"
            elif in_section and (m := VOCAB_ITEM.match(line)):
                vocab.add(m.group(1))
    return vocab


def lint(directory: str) -> list[str]:
    problems: list[str] = []
    for name in entry_files(directory):
        problems.extend(load_entry(directory, name)[1])
    readme = os.path.join(directory, "README.md")
    if os.path.exists(readme):
        with open(readme, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                if README_ROW.match(line):
                    problems.append(
                        f"docs/defects/README.md:{n}: a hand-written index row. The "
                        "index is built from each entry's frontmatter "
                        "(`just defects-index`); delete the row and check the "
                        "entry's frontmatter instead"
                    )
    return problems


def render_table(entries: list[Entry], with_class: bool) -> str:
    head = ["Date"] + (["Class"] if with_class else []) + ["Status", "Entry", "Area"]
    rows = [
        "| " + " | ".join(head) + " |",
        "| " + " | ".join("---" for _ in head) + " |",
    ]
    for e in entries:
        cells = [e.date]
        if with_class:
            cells.append(e.klass)
        cells += [
            f"**{e.status}**" if e.status == "open" else e.status,
            f"[{cell(e.title)}](docs/defects/{e.file})",
            cell(e.area),
        ]
        rows.append("| " + " | ".join(cells) + " |")
    if not entries:
        rows.append("| " + " | ".join("—" for _ in head) + " |")
    return "\n".join(rows)


def render_by_class(entries: list[Entry], vocab: set[str]) -> str:
    groups: dict[str, list[Entry]] = {}
    for e in entries:
        groups.setdefault(e.klass, []).append(e)
    order = sorted(groups, key=lambda k: (-len(groups[k]), k))

    def label(k: str) -> str:
        return k if k in vocab else f"{k} †"

    out = [
        "| Class | Entries | Open | Newest |",
        "| --- | ---: | ---: | --- |",
    ]
    for k in order:
        es = groups[k]
        n_open = sum(e.status == "open" for e in es)
        out.append(f"| {label(k)} | {len(es)} | {n_open} | {es[0].date} |")
    if any(k not in vocab for k in order):
        out += [
            "",
            "† not in the class vocabulary (docs/defects/README.md, \"Class "
            "vocabulary\"): define it there in one line, or reclass the entry.",
        ]
    for k in order:
        es = groups[k]
        n_open = sum(e.status == "open" for e in es)
        out += ["", f"## {label(k)} — {len(es)} ({n_open} open)", ""]
        out.append(render_table(es, with_class=False))
    return "\n".join(out)


def cell(s: str) -> str:
    return " ".join(s.split()).replace("|", "\\|")


def self_test() -> int:
    good = "---\nstatus: fixed  # #1080\nfound: 2026-10-09  # ci\narea: lp-x\nclass: fidelity\n---\n# A title\n"
    cases = {
        # name -> (file text, expected substring of a problem, or None for clean)
        "2026-10-09-good.md": (good, None),
        "2026-10-09-wrapped-area.md": (
            good.replace("area: lp-x", "area: lp-x\n  continued here"),
            None,
        ),
        "undated.md": (good, "not named YYYY-MM-DD-slug.md"),
        "2026-02-30-bad-date.md": (good, "is not a date"),
        "2026-10-09-no-frontmatter.md": ("# Title\n\n- **Status:** fixed\n", "no frontmatter"),
        "2026-10-09-unclosed.md": ("---\nstatus: fixed\n# Title\n", "no frontmatter"),
        "2026-10-09-shouting.md": (good.replace("status: fixed", "status: FIXED 2026-09-08"), "status is `FIXED 2026-09-08`"),
        "2026-10-09-mitigated.md": (good.replace("status: fixed", "status: mitigated"), "status is `mitigated`"),
        "2026-10-09-no-found.md": (good.replace("found: 2026-10-09  # ci", "found: ci"), "`found:` must start"),
        "2026-10-09-no-area.md": (good.replace("area: lp-x", "area:"), "`area:` is missing"),
        "2026-10-09-no-class.md": (good.replace("class: fidelity\n", ""), "class is missing"),
        "2026-10-09-prose-class.md": (good.replace("class: fidelity", "class: stack imbalance"), "class is `stack imbalance`"),
        "2026-10-09-no-title.md": (good.replace("# A title\n", "body\n"), "no `# title`"),
    }
    readme = (
        "# Defect registry\n\n## Class vocabulary\n\n- **`fidelity`** — x.\n\n"
        "## Index\n\n| Class | Date | Entry |\n| --- | --- | --- |\n"
        "| fidelity | 2026-10-09 | [good](2026-10-09-good.md) |\n"
    )
    failures: list[str] = []
    with tempfile.TemporaryDirectory() as d:
        for name, (text, _) in cases.items():
            with open(os.path.join(d, name), "w", encoding="utf-8") as f:
                f.write(text)
        with open(os.path.join(d, "README.md"), "w", encoding="utf-8") as f:
            f.write(readme)
        problems = lint(d)
        for name, (_, want) in cases.items():
            mine = [p for p in problems if p.startswith(f"docs/defects/{name}:")]
            if want is None and mine:
                failures.append(f"{name}: expected clean, got {mine}")
            elif want is not None and not any(want in p for p in mine):
                failures.append(f"{name}: expected a problem containing {want!r}, got {mine}")
        if not any(p.startswith("docs/defects/README.md:11:") for p in problems):
            failures.append("README.md: the hand-written row on line 11 was not flagged")
        entry, _ = load_entry(d, "2026-10-09-wrapped-area.md")
        if entry is None or entry.area != "lp-x continued here":
            failures.append(f"wrapped area did not join: {entry}")
        if read_vocabulary(d) != {"fidelity"}:
            failures.append(f"vocabulary read wrong: {read_vocabulary(d)}")
        table = render_by_class(
            [e for e in load_entries(d) if e is not None], read_vocabulary(d)
        )
        if "| fidelity | 2 | 0 | 2026-10-09 |" not in table:
            failures.append(f"by-class summary wrong:\n{table}")
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    if failures:
        return 1
    print(f"defects-index self-test ok: {len(cases)} fixture entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
