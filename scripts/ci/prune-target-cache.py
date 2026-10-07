#!/usr/bin/env python3
"""Prune a cargo target dir down to what a fresh checkout can reuse, before
it is saved to the Actions cache — or, with `--report`, just say what is in it.

Written for the stories job's bundle (`validate-stories` in
`.github/workflows/pre-merge.yml`), which caches `target/` itself rather
than going through sccache. Everything removed here is something the next
run rebuilds whether or not it is present:

  * **path packages** — workspace members, `third_party/*`, the sibling
    `lp-regalloc2` checkout: every package `cargo metadata` reports with no
    `source`. A fresh checkout gives their sources a new mtime, so cargo
    rebuilds them regardless of what the cache holds (the reason
    Swatinem/rust-cache prunes them too). Removed unit by unit — the
    `.fingerprint/<pkg>-<hash>` dir, `build/<pkg>-<hash>`, and every
    `deps/` and `examples/` file whose stem ends in that `-<hash>` — so no
    fingerprint is ever left pointing at a missing output;
  * **final artifacts** — the regular files at the top of each profile dir
    (the uplifted `.wasm`/`.rlib`/`.d` copies of those same path packages);
  * **`incremental/`** — rustc's incremental state, which only path
    packages have;
  * the named output dirs passed with `--drop` (the dx bundle, the copied
    sidecars): outputs, not inputs.

What stays is third-party crates compiled for each profile and target —
the part that costs minutes to rebuild and is identical between runs.

It does NOT remove third-party units left behind by an older Cargo.lock;
the job keeps those out by restoring only an exact key on main, so the
build that saves the entry starts from nothing old. See
docs/debt/actions-cache-budget.md.

`--report` prints the same table and deletes nothing.

Stdlib only; the runner's python has nothing else.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys

HASH = re.compile(r"^(?P<pkg>.+)-(?P<hash>[0-9a-f]{16})$")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("target", help="the cargo target dir to prune")
    ap.add_argument(
        "--manifest-path",
        default="Cargo.toml",
        help="the workspace manifest whose path packages are pruned",
    )
    ap.add_argument(
        "--drop",
        action="append",
        default=[],
        help="a directory under TARGET to remove whole (repeatable)",
    )
    ap.add_argument("--report", action="store_true", help="measure only; delete nothing")
    args = ap.parse_args()

    target = os.path.abspath(args.target)
    if not os.path.isdir(target):
        print(f"prune-target-cache: {target} does not exist — nothing to do")
        return 0

    local = path_packages(args.manifest_path)
    total = tree_size(target)
    gone = {"path packages": 0, "final artifacts": 0, "incremental": 0, "--drop dirs": 0}

    profiles = profile_dirs(target)
    rows = {}
    # Top-level entries that hold no profile dir (dx's bundle, copied
    # sidecars, ...), measured before anything goes.
    for first in sorted(os.listdir(target)):
        a = os.path.join(target, first)
        if any(p == a or p.startswith(a + os.sep) for p in profiles):
            continue
        dropped = first in args.drop
        rows[first + ("/ (--drop)" if dropped else "")] = (tree_size(a), 0)
    for rel in args.drop:
        path = os.path.join(target, rel)
        if os.path.exists(path):
            n = rm(path, args.report)
            gone["--drop dirs"] += n
            if rel + "/ (--drop)" in rows:
                rows[rel + "/ (--drop)"] = (rows[rel + "/ (--drop)"][0], n)

    per_profile = rows
    for profile in profiles:
        size = tree_size(profile)
        part = prune_profile(profile, local, args.report)
        per_profile[os.path.relpath(profile, target)] = (size, sum(part.values()))
        for k, v in part.items():
            gone[k] += v

    report(target, total, per_profile, gone, args.report)
    return 0


def path_packages(manifest: str) -> set[str]:
    out = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", manifest],
        text=True,
    )
    meta = json.loads(out)
    return {p["name"] for p in meta["packages"] if p.get("source") is None}


def profile_dirs(target: str) -> list[str]:
    """Every dir holding a `.fingerprint/`: target/<profile> and
    target/<triple>/<profile>."""
    found = []
    for first in sorted(os.listdir(target)):
        a = os.path.join(target, first)
        if not os.path.isdir(a):
            continue
        if os.path.isdir(os.path.join(a, ".fingerprint")):
            found.append(a)
            continue
        for second in sorted(os.listdir(a)):
            b = os.path.join(a, second)
            if os.path.isdir(os.path.join(b, ".fingerprint")):
                found.append(b)
    return found


def prune_profile(profile: str, local: set[str], report: bool) -> dict[str, int]:
    gone = {"path packages": 0, "final artifacts": 0, "incremental": 0}
    hashes = set()
    fp = os.path.join(profile, ".fingerprint")
    for name in os.listdir(fp):
        m = HASH.match(name)
        if m and m["pkg"] in local:
            hashes.add(m["hash"])
            gone["path packages"] += rm(os.path.join(fp, name), report)
    build = os.path.join(profile, "build")
    if os.path.isdir(build):
        for name in os.listdir(build):
            m = HASH.match(name)
            if m and m["hash"] in hashes:
                gone["path packages"] += rm(os.path.join(build, name), report)
    for sub in ("deps", "examples"):
        d = os.path.join(profile, sub)
        if not os.path.isdir(d):
            continue
        for name in os.listdir(d):
            m = HASH.match(name.split(".", 1)[0])
            if m and m["hash"] in hashes:
                gone["path packages"] += rm(os.path.join(d, name), report)
    inc = os.path.join(profile, "incremental")
    if os.path.isdir(inc):
        gone["incremental"] += rm(inc, report)
    for name in os.listdir(profile):
        path = os.path.join(profile, name)
        if os.path.isfile(path) and not os.path.islink(path) and not name.startswith("."):
            gone["final artifacts"] += rm(path, report)
    return gone


def rm(path: str, report: bool) -> int:
    n = tree_size(path)
    if not report:
        if os.path.isdir(path) and not os.path.islink(path):
            shutil.rmtree(path)
        else:
            os.remove(path)
    return n


def tree_size(path: str) -> int:
    if os.path.islink(path) or os.path.isfile(path):
        return os.lstat(path).st_size
    total = 0
    for root, _dirs, files in os.walk(path):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_size
            except FileNotFoundError:
                pass
    return total


def report(target, total, per_profile, gone, report_only) -> None:
    def mib(n: int) -> str:
        return f"{n / 1048576:.1f}"

    kept = total - sum(gone.values())
    verb = "would remove" if report_only else "removed"
    lines = [f"target/ bundle at {target}{' (report only, nothing deleted)' if report_only else ''}", ""]
    lines += ["| dir | MiB | " + verb + " MiB |", "|---|---:|---:|"]
    for k, (size, part) in sorted(per_profile.items()):
        lines.append(f"| `{k}` | {mib(size)} | {mib(part)} |")
    lines += ["", "| " + verb + " | MiB |", "|---|---:|"]
    for k, v in gone.items():
        lines.append(f"| {k} | {mib(v)} |")
    lines += ["", f"**{mib(total)} MiB before, {mib(kept)} MiB {'would be ' if report_only else ''}kept.**", ""]
    text = "\n".join(lines)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as f:
            f.write("#### prune-target-cache\n\n" + text + "\n")


if __name__ == "__main__":
    sys.exit(main())
