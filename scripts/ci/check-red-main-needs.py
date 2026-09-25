#!/usr/bin/env python3
"""Fail when pre-merge.yml's `red-main-suspects` job does not `needs:` every
other job in the workflow.

`red-main-suspects` runs on `failure()` of the jobs it needs. A job missing
from that list is invisible to it: the job can fail after the suspects job
has already run (or been skipped), and a red main then names no suspects.
The list is hand-maintained, so this lint is what keeps it complete — run
it after adding a job, and it tells you the name to add.

Offline and stdlib-only (PyYAML is not in the stdlib, and the runner's
python may not have it). It reads just enough of the file's shape: job ids
are the keys two spaces in under the top-level `jobs:`, and the needs list
is either a block list or an inline `[a, b]` flow list under the suspects
job. Anything it cannot read is an error, never a pass.

Usage: scripts/ci/check-red-main-needs.py [workflow.yml]
"""

import re
import sys

WORKFLOW = ".github/workflows/pre-merge.yml"
TARGET = "red-main-suspects"

JOB_KEY = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(#.*)?$")
NEEDS_BLOCK = re.compile(r"^    needs:\s*(#.*)?$")
NEEDS_INLINE = re.compile(r"^    needs:\s*(\[.*\]|[A-Za-z0-9_-]+)\s*(#.*)?$")
NEEDS_ITEM = re.compile(r"^      - ([A-Za-z0-9_-]+)\s*(#.*)?$")


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else WORKFLOW
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()

    jobs, needs = parse(lines, path)
    if TARGET not in jobs:
        return fail(path, f"no `{TARGET}` job found under `jobs:`")
    if needs is None:
        return fail(path, f"`{TARGET}` has no `needs:` this lint can read")

    others = [j for j in jobs if j != TARGET]
    missing = [j for j in others if j not in needs]
    unknown = [n for n in needs if n not in jobs]
    if missing or unknown:
        for j in missing:
            print(f"{path}: job `{j}` is missing from `{TARGET}`'s needs: list")
        for n in unknown:
            print(f"{path}: `{TARGET}` needs `{n}`, which is not a job in this workflow")
        print(
            f"{TARGET} must need every other job, or a red main can finish "
            "before a failing job does and name no suspects. Add the job "
            "to its needs: list."
        )
        return 1
    print(f"{path}: {TARGET} needs all {len(others)} other jobs")
    return 0


def parse(lines, path):
    """Return (job ids in file order, the target's needs list or None)."""
    try:
        start = lines.index("jobs:") + 1
    except ValueError:
        sys.exit(fail(path, "no top-level `jobs:` line"))

    jobs = []
    current = None
    needs = None
    in_needs = False
    for line in lines[start:]:
        if line and not line.startswith(" ") and not line.startswith("#"):
            break  # next top-level key: the jobs map is over
        m = JOB_KEY.match(line)
        if m:
            current = m.group(1)
            jobs.append(current)
            in_needs = False
            continue
        if current != TARGET:
            continue
        if in_needs:
            item = NEEDS_ITEM.match(line)
            if item:
                needs.append(item.group(1))
                continue
            if line.strip() == "" or line.lstrip().startswith("#"):
                continue
            in_needs = False
        if NEEDS_BLOCK.match(line):
            needs = []
            in_needs = True
            continue
        m = NEEDS_INLINE.match(line)
        if m:
            value = m.group(1)
            if value.startswith("["):
                needs = [v.strip() for v in value[1:-1].split(",") if v.strip()]
            else:
                needs = [value]
    return jobs, needs


def fail(path, msg):
    print(f"{path}: {msg}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
