#!/usr/bin/env python3
"""When a pinned firmware figure moves on CI, hand back the accepted-state patch.

The chip jobs' figure checks (the three emulator suites, the chip and engine
heap ratchets) fail when a FIGURE of the firmware image moves — the records
under `lp-emu/esp/figures/` and `scripts/heap-budget-record/`. Accepting one
used to mean rebuilding the firmware on a desk and running `just bless-chips`.
This script is the CI half of accepting it without that: after a check step
failed, it re-runs exactly what failed as a bless, against the images the job
already built, and publishes the records' diff. The job STILL FAILS; nothing is
pushed. See docs/chip-figures.md "When CI hands back the patch".

Three subcommands, in the order a job runs them:

  bless    one failed check step: decide whether it failed ONLY on figures,
           and if so re-record them. Appends a line to steps.jsonl.
  collect  the job's verdict: `figures.patch` (a `git diff --full-index` of the
           records) only when every failed step was blessed cleanly, plus
           `summary.json` naming each moved figure old -> new.
  comment  (its own job) read every job's summary and post/update ONE sticky
           PR comment.

A patch is produced only when the failure was a figure move and nothing else.
"Nothing else" is decided by re-running, not by reading: a bless rewrites
figures and ASSERTS everything else, so a bless that passes proves every
other assertion in those tests held; a heap re-baseline is followed by the
ordinary check, which must then pass. Anything else — an EXACT pin, a
transcript, a structural check, a crash — leaves the step "not a figure move"
and the whole job without a patch.

Stdlib only; the runner's python has nothing else.
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import sys

RECORD_PATHS = ["lp-emu/esp/figures", "scripts/heap-budget-record"]
MARKER = "<!-- lp-figures-patch -->"
# Keys that a re-baseline restamps and that are not figures.
STAMP_KEYS = {"recorded", "commit"}
TEST_FIGURE_MARKER = "pinned firmware figure"
REBASELINE_MARKER = "Re-baseline with"
HEAP_ERROR = "::error::heap-budget:"


def out_dir():
    base = os.environ.get("FIGURES_PATCH_DIR") or os.path.join(
        os.environ.get("RUNNER_TEMP", "/tmp"), "figures-patch"
    )
    os.makedirs(base, exist_ok=True)
    return base


# ── bless ────────────────────────────────────────────────────────────────


def cmd_bless(args):
    with open(args.log, encoding="utf-8", errors="replace") as f:
        log = f.read()
    kind = "tests" if args.cargo_test else "heap"
    verdict = classify_log(kind, log)
    step = {"name": args.name, "kind": kind}
    if verdict is not None:
        step.update(result="not-a-figure-move", why=verdict)
        return record_step(step)

    slug = re.sub(r"[^a-z0-9]+", "-", args.name.lower()).strip("-")
    if kind == "tests":
        names = failed_tests(log)
        # `--no-fail-fast`: every binary that holds one of the failed tests
        # runs, not just the first. Names are filters (substring match), so a
        # name that also matches a passing test in another binary only runs
        # that one too — harmless, it passed a moment ago.
        cmd = f"{args.cargo_test} --no-fail-fast -- --include-ignored " + " ".join(
            shlex.quote(n) for n in names
        )
        ok = run(cmd, {"LP_EMU_BLESS": "1"}, f"bless-{slug}.log")
        if not ok:
            step.update(
                result="not-a-figure-move",
                why="the bless re-run of the failed tests still failed: something other "
                "than a pinned figure failed (a bless rewrites figures and asserts everything else)",
                tests=names,
            )
        else:
            step.update(result="blessed", tests=names)
    else:
        ok = run(args.heap_bless, {}, f"bless-{slug}.log")
        if ok:
            ok = run(args.heap_check, {"LP_EMU_BLESS": "0"}, f"recheck-{slug}.log")
            why = (
                "the ratchet still failed after re-baselining: the failure is not one a "
                "re-baseline accepts"
            )
        else:
            why = "the re-baseline itself failed"
        step.update(result="blessed" if ok else "not-a-figure-move")
        if not ok:
            step["why"] = why
    return record_step(step)


def classify_log(kind, log):
    """None when the log's failure is (only) figure moves, else why not."""
    if kind == "tests":
        if TEST_FIGURE_MARKER not in log:
            return "no pinned figure is named in the failure"
        if not failed_tests(log):
            return "the log names no failed test to re-run"
        return None
    errors = [l for l in log.splitlines() if HEAP_ERROR in l]
    if not errors:
        return "the ratchet failed without naming a moved figure"
    other = [l for l in errors if REBASELINE_MARKER not in l]
    if other:
        return "the ratchet failed on a check a re-baseline does not accept: " + other[0].split(
            HEAP_ERROR, 1
        )[1].strip()[:200]
    return None


def failed_tests(log):
    """Test names from libtest's closing `failures:` lists."""
    names = []
    lines = log.splitlines()
    for i, line in enumerate(lines):
        if line.strip() != "failures:":
            continue
        for nxt in lines[i + 1 :]:
            m = re.match(r"^    (\S+)$", nxt)
            if not m:
                break
            if m.group(1) not in names:
                names.append(m.group(1))
    return names


def run(cmd, extra_env, log_name):
    env = dict(os.environ, **extra_env)
    path = os.path.join(out_dir(), log_name)
    print(f"figures-patch: $ {cmd}", flush=True)
    with open(path, "w", encoding="utf-8") as log:
        proc = subprocess.Popen(
            ["bash", "-o", "pipefail", "-c", cmd],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
        )
        for line in proc.stdout:
            sys.stdout.write(line)
            log.write(line)
        proc.wait()
    # Keep the artifact small: the tail is what a reader of a failed bless needs.
    with open(path, encoding="utf-8", errors="replace") as f:
        tail = f.readlines()[-300:]
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(tail)
    print(f"figures-patch: exit {proc.returncode}", flush=True)
    return proc.returncode == 0


def record_step(step):
    with open(os.path.join(out_dir(), "steps.jsonl"), "a", encoding="utf-8") as f:
        f.write(json.dumps(step) + "\n")
    print(f"figures-patch: {step['name']}: {step['result']}" + (f" — {step['why']}" if "why" in step else ""))
    return 0


# ── collect ──────────────────────────────────────────────────────────────


def cmd_collect(args):
    d = out_dir()
    steps = []
    p = os.path.join(d, "steps.jsonl")
    if os.path.exists(p):
        with open(p, encoding="utf-8") as f:
            steps = [json.loads(l) for l in f if l.strip()]
    diff = git("diff", "--full-index", "--", *RECORD_PATHS)
    changed = [l for l in git("diff", "--name-only", "--", *RECORD_PATHS).splitlines() if l]
    summary = {
        "job": args.job,
        "job_name": args.job_name or args.job,
        "run_url": run_url(),
        "sha": os.environ.get("FIGURES_HEAD_SHA") or os.environ.get("GITHUB_SHA", ""),
        "steps": steps,
        "changes": [],
    }
    if not steps:
        summary["verdict"] = "no-check-failed"
    elif any(s["result"] != "blessed" for s in steps):
        summary["verdict"] = "not-a-figure-move"
    elif not diff.strip():
        summary["verdict"] = "no-move"
    else:
        summary["verdict"] = "figure-move"
        for path in changed:
            summary["changes"].extend(file_changes(path))
        with open(os.path.join(d, "figures.patch"), "w", encoding="utf-8") as f:
            f.write(diff)
    with open(os.path.join(d, "summary.json"), "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2)
        f.write("\n")
    print(json.dumps(summary, indent=2))
    step_summary(summary)
    return 0


def file_changes(path):
    chip, record = describe_path(path)
    try:
        old = json.loads(git("show", f"HEAD:{path}"))
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        old = {}
    with open(path, encoding="utf-8") as f:
        new = json.load(f)
    rows = []
    for key, a, b in leaf_changes(old, new, ""):
        rows.append({"chip": chip, "record": record, "file": path, "figure": key, "old": a, "new": b})
    return rows


def describe_path(path):
    parts = path.split("/")
    name = os.path.splitext(parts[-1])[0]
    if path.startswith("lp-emu/esp/figures/"):
        return name, "test figures"
    if path.startswith("scripts/heap-budget-record/chips/"):
        return name, "chip heap record"
    if path.startswith("scripts/heap-budget-record/engine/"):
        project = path[len("scripts/heap-budget-record/engine/") : -len(".json")]
        return "engine", f"engine heap record `{project}`"
    return "other", path


def leaf_changes(a, b, prefix):
    """(key, old, new) for every leaf that differs; text arrays line by line."""
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k in STAMP_KEYS and not prefix:
                continue
            if k.startswith("_"):
                continue
            yield from leaf_changes(a.get(k), b.get(k), f"{prefix}.{k}" if prefix else k)
        return
    if a == b:
        return
    if (
        isinstance(a, list)
        and isinstance(b, list)
        and all(isinstance(x, str) for x in a + b)
    ):
        for i in range(max(len(a), len(b))):
            x = a[i] if i < len(a) else None
            y = b[i] if i < len(b) else None
            if x != y:
                yield (f"{prefix} line {i + 1}", x, y)
        return
    yield (prefix, a, b)


def step_summary(summary):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as f:
        f.write("\n".join(job_section(summary)) + "\n")


# ── comment ──────────────────────────────────────────────────────────────


def cmd_comment(args):
    summaries = []
    for root, _dirs, files in os.walk(args.dir):
        if "summary.json" in files:
            with open(os.path.join(root, "summary.json"), encoding="utf-8") as f:
                s = json.load(f)
            s["has_patch"] = "figures.patch" in files
            summaries.append(s)
    summaries.sort(key=lambda s: s["job"])
    reported = [s for s in summaries if s["verdict"] != "no-check-failed"]
    body = render(reported, args)
    if args.dry_run:
        print(body or "(nothing to post)")
        return 0
    comments = json.loads(
        gh("api", f"repos/{args.repo}/issues/{args.pr}/comments", "--paginate", "--slurp")
    )
    existing = next(
        (c for page in comments for c in page if (c.get("body") or "").startswith(MARKER)), None
    )
    if not reported:
        if not existing:
            print("figures-patch: no figure check failed and no comment to update — nothing to post")
            return 0
        body = render_clear(args)
    tmp = os.path.join(out_dir(), "comment.md")
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(body)
    if existing:
        gh("api", "--method", "PATCH", f"repos/{args.repo}/issues/comments/{existing['id']}", "-F", f"body=@{tmp}")
        print(f"figures-patch: updated comment {existing['id']}")
    else:
        gh("api", "--method", "POST", f"repos/{args.repo}/issues/{args.pr}/comments", "-F", f"body=@{tmp}")
        print(f"figures-patch: posted a comment on #{args.pr}")
    return 0


def render(summaries, args):
    if not summaries:
        return ""
    patched = [s for s in summaries if s["verdict"] == "figure-move"]
    other = [s for s in summaries if s["verdict"] != "figure-move"]
    sha = (args.sha or "")[:10]
    out = [MARKER]
    if patched:
        out += [
            "### Pinned firmware figures moved — CI has the patch",
            "",
            f"CI re-ran the failed figure checks as a bless against the images it had already built "
            f"(under `GITHUB_ACTIONS=true`, so `[positional]` figures are written too), and every other "
            f"assertion in them held. The records below are the accepted state for `{sha}`. "
            "**The jobs stay red**: nothing is pushed for you. If the firmware changed on purpose, accept it with",
            "",
            "```bash",
            f"just apply-ci-figures {args.pr}",
            "```",
            "",
            "then commit the records with the change that moved them and push. If only the *emulator* "
            "changed, a moved figure is a finding: do not apply it. "
            f"([docs/chip-figures.md](https://github.com/{args.repo}/blob/main/docs/chip-figures.md))",
            "",
        ]
        for s in patched:
            out += job_section(s, heading="####")
    else:
        out += [
            "### A figure check failed — this failure is not a figure move",
            "",
            "No patch: re-running the failed checks as a bless did not make them pass, so the failure "
            "is something a bless cannot accept (an EXACT pin, a transcript, a structural check, or an "
            "ordinary test failure). Read the failure; do not bless it.",
            "",
        ]
    if other:
        if patched:
            out += ["#### Not a figure move", ""]
        for s in other:
            out += job_section(s, heading="####" if not patched else None)
        if patched:
            out += [
                "",
                "Those still need reading: applying the patch will not turn them green.",
                "",
            ]
    out += [f"<sub>Updated for `{sha}` by [this run]({args.run_url}). "
            "One comment per PR, rewritten on every run.</sub>"]
    return "\n".join(out) + "\n"


def job_section(s, heading="####"):
    name = s.get("job_name") or s["job"]
    link = f"[{name}]({s['run_url']})" if s.get("run_url") else name
    out = []
    if s["verdict"] == "figure-move":
        if heading:
            out += [f"{heading} {link} — `figures-patch-{s['job']}`", ""]
        out += ["| chip | record | figure | old | new |", "|---|---|---|---|---|"]
        for c in s["changes"]:
            out.append(
                f"| {c['chip']} | {c['record']} | `{c['figure']}` | {cell(c['old'])} | {cell(c['new'])} |"
            )
        out.append("")
        return out
    if s["verdict"] == "no-move":
        why = ("the failed checks passed when re-run as a bless and no record changed — the failure "
               "did not reproduce, so it was not a figure move")
        out.append(f"- {link}: **this failure is not a figure move** — {why}.")
        return out
    withheld = any(st["result"] == "blessed" for st in s["steps"])
    for st in s["steps"]:
        if st["result"] != "blessed":
            line = f"- {link}, *{st['name']}*: **this failure is not a figure move** — {st['why']}."
            if withheld:
                line += " (This job's other figure moves are withheld until it is fixed: no patch for a job that is red for another reason.)"
            out.append(line)
    if s["verdict"] == "no-check-failed":
        out.append(f"- {link}: no figure check failed.")
    return out


def cell(v):
    if v is None:
        return "*(absent)*"
    text = v if isinstance(v, str) else json.dumps(v)
    if isinstance(v, int) and not isinstance(v, bool):
        text = str(v)
    text = text.replace("|", "\\|").replace("`", "'")
    if len(text) > 120:
        text = text[:117] + "…"
    return f"`{text}`"


def render_clear(args):
    sha = (args.sha or "")[:10]
    return (
        f"{MARKER}\n### Pinned firmware figures match their records\n\n"
        f"No figure check failed on `{sha}`. (This comment held a figure patch or a figure-check "
        f"failure on an earlier push.)\n\n<sub>[Run]({args.run_url})</sub>\n"
    )


# ── helpers ──────────────────────────────────────────────────────────────


def git(*a):
    return subprocess.run(["git", *a], check=True, capture_output=True, text=True).stdout


def gh(*a):
    return subprocess.run(["gh", *a], check=True, capture_output=True, text=True).stdout


def run_url():
    s, r, i = (os.environ.get(k) for k in ("GITHUB_SERVER_URL", "GITHUB_REPOSITORY", "GITHUB_RUN_ID"))
    return f"{s}/{r}/actions/runs/{i}" if s and r and i else ""


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("bless")
    b.add_argument("--name", required=True, help="the check step, as a reader should see it")
    b.add_argument("--log", required=True, help="the failed step's output")
    g = b.add_mutually_exclusive_group(required=True)
    g.add_argument("--cargo-test", help="`cargo test -p <crate>` with its image env, no `--` args")
    g.add_argument("--heap-bless", help="the ratchet's re-baseline command")
    b.add_argument("--heap-check", help="the ratchet's check command, re-run after --heap-bless")
    c = sub.add_parser("collect")
    c.add_argument("--job", required=True)
    c.add_argument("--job-name")
    m = sub.add_parser("comment")
    m.add_argument("--dir", required=True, help="the downloaded figures-patch-* artifacts")
    m.add_argument("--repo", required=True)
    m.add_argument("--pr", required=True)
    m.add_argument("--sha", default="")
    m.add_argument("--run-url", default=run_url())
    m.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    if args.cmd == "bless" and args.heap_bless and not args.heap_check:
        ap.error("--heap-bless needs --heap-check")
    return {"bless": cmd_bless, "collect": cmd_collect, "comment": cmd_comment}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
