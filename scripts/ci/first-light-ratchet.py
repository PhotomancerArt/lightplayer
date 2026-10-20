#!/usr/bin/env python3
"""The C6's first-light ratchet: power-on to the first lit frame, gated on t4.

The one deliberate exception to "never gate on emulated time"
(docs/adr/2026-09-10-the-emulator-first-device-walk.md, Amendment
2026-10-18): `lp-cli/tests/emu_first_light.rs` boots the SPLIT image ROM-up
through our bootloader on `lp-emu:esp32c6:t4`, `projects/test/basic` with a
saved Wi-Fi network on the emulated LAN (`basic+net`), and reads the
firmware's own `[perf] first light at <ms> ms` line. The c40-sized project's
figure (`c40+net`) is reported, never gated.

The rules (Yona, 2026-10-18):

  > 500 ms                  fail (red)
  > 400 ms                  a warning annotation (yellow)
  > record + 5 ms           fail until YONA blesses it (`just bless-first-light`,
                            plus the PR label `first-light-blessed`); CI hands
                            back no patch and `just apply-ci-figures` refuses one
  < record                  the record ratchets down: CI's figure patch, applied
                            with `just apply-ci-figures` like any figure
  > record + 5 ms, and the  the emulator (or the measurement) moved, not the
  image did not change      product: CI re-records it with a note, no Yona needed

The record is `scripts/first-light-record/esp32c6.json`. Its `recorded_by`
says who wrote the value: `ci-ratchet` (a ratchet down), `emulator-model` (a
re-record on an image-unchanged PR) or `yona` (`just bless-first-light`).

Subcommands:

  check        read a run's JSON, apply the rules, print annotations, and (on
               CI) write the job's figures-patch artifact (summary.json and,
               when the record may move without Yona, figures.patch) in the
               shape `scripts/ci/figures-patch.py comment` reads.
  bless        Yona's bless: write the record at a value, `recorded_by: yona`.
  guard-apply  run by `apply-ci-figures.sh` after it applied CI's patches:
               refuse (and roll back) a staged record that rose, unless CI
               re-recorded it for an image-unchanged change.

Stdlib only; the runner's python has nothing else.
"""

import argparse
import datetime
import json
import os
import re
import subprocess
import sys

RECORD = "scripts/first-light-record/esp32c6.json"
GATED_CASE = "basic+net"
REPORTED_CASES = ["c40+net"]
RED_MS = 500
YELLOW_MS = 400
RISE_MS = 5
BLESS_LABEL = "first-light-blessed"
LINE = re.compile(r"\[perf\] first light at (\d+) ms")
JOB = "c6-first-light"
JOB_NAME = "C6 first light (x64)"

ABOUT = (
    "The C6's first-light ratchet (docs/first-light-ratchet.md): `first_light_ms` is "
    "the firmware's own `[perf] first light at <ms> ms`, read off a ROM-up power-on boot "
    "of the split image through our bootloader on lp-emu:esp32c6:t4, projects/test/basic "
    "with a saved Wi-Fi network on the emulated LAN, the higher of two boots. CI fails "
    "above 500 ms, warns above 400 ms, fails more than 5 ms above this value until Yona "
    "blesses it (`just bless-first-light`, label `first-light-blessed`), and hands back a "
    "patch when it drops. `recorded_by`: initial | ci-ratchet | emulator-model | yona."
)


# ── reading ──────────────────────────────────────────────────────────────


def read_run(path):
    """{case: (ms, [lines])} from an `emu-first-light-t4.json`; the worst boot."""
    with open(path, encoding="utf-8") as f:
        run = json.load(f)
    config = run.get("configuration", "")
    if not config.endswith(":t4"):
        sys.exit(f"first-light-ratchet: {path} is {config!r}; the ratchet reads t4 only")
    cases = {}
    for row in run["rows"]:
        m = LINE.search(row.get("line") or "")
        if not m:
            continue
        ms = int(m.group(1))
        prev = cases.get(row["project"])
        lines = (prev[1] if prev else []) + [row["line"].split("[perf] ", 1)[-1]]
        cases[row["project"]] = (max(ms, prev[0]) if prev else ms, lines)
    return config, cases


def read_record(path=RECORD):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return None


def record_at(rev, path=RECORD):
    try:
        text = subprocess.run(
            ["git", "show", f"{rev}:{path}"], check=True, capture_output=True, text=True
        ).stdout
        return json.loads(text)
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return None


def write_record(ms, recorded_by, commit, config="lp-emu:esp32c6:t4", path=RECORD):
    rec = {
        "_about": ABOUT,
        "case": GATED_CASE,
        "commit": commit,
        "configuration": config,
        "first_light_ms": ms,
        "recorded": datetime.date.today().isoformat(),
        "recorded_by": recorded_by,
    }
    os.makedirs(os.path.dirname(path), exist_ok=True)
    text = json.dumps(rec, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def head_commit():
    sha = os.environ.get("FIGURES_HEAD_SHA") or ""
    if not sha:
        try:
            sha = subprocess.run(
                ["git", "rev-parse", "HEAD"], check=True, capture_output=True, text=True
            ).stdout.strip()
        except subprocess.CalledProcessError:
            sha = ""
    return sha[:9]


# ── check ────────────────────────────────────────────────────────────────


def cmd_check(args):
    config, cases = read_run(args.json)
    if GATED_CASE not in cases:
        sys.exit(f"first-light-ratchet: no `{GATED_CASE}` row with a first-light line in {args.json}")
    ms, lines = cases[GATED_CASE]
    record = read_record()
    base = record_at(args.base) if args.base else None
    labels = {l.strip() for l in (args.labels or "").split(",") if l.strip()}
    image_changed = args.image_changed != "false"

    failures, warnings, notes, rows = [], [], [], []
    patch = False
    verdict = "no-check-failed"

    print(f"first light ({config}, {GATED_CASE}): {ms} ms")
    for l in lines:
        print(f"  {l}")
    for case in REPORTED_CASES:
        if case in cases:
            print(f"first light ({config}, {case}, reported, not gated): {cases[case][0]} ms")
            for l in cases[case][1]:
                print(f"  {l}")

    # 1. The record itself: may this PR raise it?
    if record and base and record["first_light_ms"] > base["first_light_ms"]:
        rose = f"the record rose {base['first_light_ms']} → {record['first_light_ms']} ms"
        by = record.get("recorded_by")
        if by == "yona" and BLESS_LABEL in labels:
            notes.append(f"{rose}: blessed by Yona (`recorded_by: yona`, label `{BLESS_LABEL}`).")
        elif by == "emulator-model" and not image_changed:
            notes.append(f"{rose}: re-recorded for an emulator/measurement change; the image did not change.")
        else:
            why = (
                f"needs `recorded_by: yona` (`just bless-first-light`) AND the PR label `{BLESS_LABEL}`"
                if by != "emulator-model"
                else "`recorded_by: emulator-model`, but this PR changes the image"
            )
            failures.append(f"{rose} without Yona's bless: {why}.")
            verdict = "needs-yona"

    # 2. The absolute budget.
    if ms > RED_MS:
        failures.append(f"first light {ms} ms is over the {RED_MS} ms budget.")
        verdict = "over-budget"
    elif ms > YELLOW_MS:
        warnings.append(f"first light {ms} ms is over {YELLOW_MS} ms (the budget fails at {RED_MS}).")

    # 3. Against the record.
    if record is None:
        failures.append(f"no record at {RECORD}: run `just bless-first-light {ms}`.")
        verdict = "needs-yona" if verdict == "no-check-failed" else verdict
    else:
        r = record["first_light_ms"]
        if ms > RED_MS:
            # Over budget: no record move of any kind is offered.
            if ms != r:
                rows.append((r, ms))
        elif ms > r + RISE_MS:
            if not image_changed:
                notes.append(
                    f"first light {r} → {ms} ms with the image unchanged: the emulator (or the "
                    "measurement) moved, so CI re-recorded it (`recorded_by: emulator-model`) — "
                    "no Yona needed. Check the emulator change explains it."
                )
                write_record(ms, "emulator-model", head_commit(), config)
                rows.append((r, ms))
                patch = True
                failures.append(f"first light {r} → {ms} ms (image unchanged): apply CI's re-record.")
                if verdict == "no-check-failed":
                    verdict = "figure-move"
            else:
                failures.append(
                    f"first light rose {r} → {ms} ms (+{ms - r}, over +{RISE_MS}): NEEDS YONA "
                    f"(`just bless-first-light {ms}` and the label `{BLESS_LABEL}`); "
                    "`just apply-ci-figures` will not apply it."
                )
                rows.append((r, ms))
                if verdict in ("no-check-failed", "figure-move"):
                    verdict = "needs-yona"
        elif ms < r:
            notes.append(f"first light dropped {r} → {ms} ms: the record ratchets down.")
            write_record(ms, "ci-ratchet", head_commit(), config)
            rows.append((r, ms))
            patch = True
            failures.append(f"first light dropped {r} → {ms} ms: apply CI's patch to ratchet the record down.")
            if verdict == "no-check-failed":
                verdict = "figure-move"
        elif ms > r:
            print(f"first light {ms} ms is within +{RISE_MS} ms of the record ({r} ms).")
        else:
            print(f"first light {ms} ms equals the record.")

    if verdict in ("needs-yona", "over-budget"):
        # Never hand back a patch the rules say only Yona may write.
        patch = False
        if read_record() != record:
            restore_record()

    for w in warnings:
        print(f"::warning title=First light::{w}")
    for f in failures:
        print(f"::error title=First light::{f}")
    for n in notes:
        print(f"first-light-ratchet: note: {n}")

    if args.artifact:
        write_artifact(verdict, ms, record, rows, notes + warnings, failures, cases, config, patch)
    return 1 if failures else 0


def restore_record():
    subprocess.run(["git", "checkout", "--", RECORD], check=False)


def write_artifact(verdict, ms, record, rows, notes, failures, cases, config, patch):
    d = os.environ.get("FIGURES_PATCH_DIR") or os.path.join(
        os.environ.get("RUNNER_TEMP", "/tmp"), "figures-patch"
    )
    os.makedirs(d, exist_ok=True)
    s, r, i = (os.environ.get(k) for k in ("GITHUB_SERVER_URL", "GITHUB_REPOSITORY", "GITHUB_RUN_ID"))
    summary = {
        "job": JOB,
        "job_name": JOB_NAME,
        "run_url": f"{s}/{r}/actions/runs/{i}" if s and r and i else "",
        "sha": os.environ.get("FIGURES_HEAD_SHA") or os.environ.get("GITHUB_SHA", ""),
        "verdict": verdict,
        "steps": [{"name": "first light (t4)", "kind": "first-light",
                   "result": "blessed" if verdict == "figure-move" else
                   ("not-a-figure-move" if failures else "passed"),
                   "why": " ".join(failures)}] if failures else [],
        "changes": [
            {"chip": "esp32c6", "record": f"first light, {config}", "file": RECORD,
             "figure": f"{GATED_CASE} first_light_ms", "old": old, "new": new}
            for old, new in rows
        ],
        "notes": notes,
        "reported": {c: cases[c][0] for c in [GATED_CASE] + REPORTED_CASES if c in cases},
    }
    with open(os.path.join(d, "summary.json"), "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2)
        f.write("\n")
    if patch:
        diff = subprocess.run(
            ["git", "diff", "--full-index", "--", RECORD], check=True, capture_output=True, text=True
        ).stdout
        if diff.strip():
            with open(os.path.join(d, "figures.patch"), "w", encoding="utf-8") as f:
                f.write(diff)
    step = os.environ.get("GITHUB_STEP_SUMMARY")
    if step:
        with open(step, "a", encoding="utf-8") as f:
            f.write(f"#### First light ({config})\n\n| case | ms |\n|---|---:|\n")
            for c, v in summary["reported"].items():
                gated = " (gated)" if c == GATED_CASE else " (reported)"
                f.write(f"| `{c}`{gated} | {v} |\n")
            if record:
                f.write(f"\nRecord: {record['first_light_ms']} ms (`recorded_by: {record.get('recorded_by')}`).\n")
            for line in failures + notes:
                f.write(f"\n- {line}")
            f.write("\n")


# ── bless ────────────────────────────────────────────────────────────────


def cmd_bless(args):
    ms = args.ms
    config = "lp-emu:esp32c6:t4"
    if ms is None:
        if not args.json:
            sys.exit("first-light-ratchet bless: give the value (ms) or a run's JSON")
        config, cases = read_run(args.json)
        ms = cases[GATED_CASE][0]
    old = read_record()
    write_record(ms, "yona", head_commit(), config)
    was = f"{old['first_light_ms']} ms ({old.get('recorded_by')})" if old else "no record"
    print(f"first-light-ratchet: {RECORD}: {was} → {ms} ms (recorded_by: yona)")
    if old and ms > old["first_light_ms"]:
        print(
            f"first-light-ratchet: a rise — commit it, and put the label `{BLESS_LABEL}` on the PR "
            "(then re-run the `C6 first light` job): CI accepts a raised record only with both."
        )
    return 0


# ── guard-apply ──────────────────────────────────────────────────────────


def cmd_guard_apply(_args):
    """After `git apply --index`: refuse a staged rise CI could not have written."""
    old = record_at("HEAD")
    new = record_at("")  # `git show :path` reads the index
    if not old or not new or new == old:
        return 0
    if new["first_light_ms"] <= old["first_light_ms"]:
        print(f"first-light-ratchet: first light {old['first_light_ms']} → {new['first_light_ms']} ms: a ratchet down, applied.")
        return 0
    if new.get("recorded_by") == "emulator-model":
        print(
            f"first-light-ratchet: first light {old['first_light_ms']} → {new['first_light_ms']} ms, "
            "re-recorded by CI for an image-unchanged change: applied (no Yona needed)."
        )
        return 0
    print(
        f"first-light-ratchet: REFUSED: the patch raises first light {old['first_light_ms']} → "
        f"{new['first_light_ms']} ms. A rise is Yona's to bless (`just bless-first-light`, label "
        f"`{BLESS_LABEL}`), never a CI patch.",
        file=sys.stderr,
    )
    return 1


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--json", required=True, help="the run's emu-first-light-t4.json")
    c.add_argument("--base", help="git rev holding the base branch's record (a PR's base)")
    c.add_argument("--labels", default="", help="the PR's labels, comma-joined")
    c.add_argument("--image-changed", default="true", choices=["true", "false"],
                   help="whether this change touches the shipped C6 image (unknown: true)")
    c.add_argument("--artifact", action="store_true", help="write the figures-patch artifact")
    b = sub.add_parser("bless")
    b.add_argument("ms", nargs="?", type=int)
    b.add_argument("--json", help="take the value from a run's JSON instead")
    sub.add_parser("guard-apply")
    args = ap.parse_args()
    return {"check": cmd_check, "bless": cmd_bless, "guard-apply": cmd_guard_apply}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
