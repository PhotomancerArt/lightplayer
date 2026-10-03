#!/usr/bin/env python3
"""The agent activity corpus report (plan lp2025/2026-10-01-1255-agentic-ui-roadmap/
m-agent-activity-corpus).

Reads every `report.json` under a run directory
(`target/app-agent-evals/<run>/<scenario>[-rN]/report.json`) and writes,
beside them:

- `corpus.md`: a table per scenario (pass n/N, turns, tokens, cost,
  unscripted questions, cards handed, the first failing check's reason);
  rollups per tag and per persona (pass rate, median turns, total and mean
  cost); and the diff against the last corpus run with the same model
  (scenarios that flipped, turns and cost deltas);
- `corpus.json`: the same data, for the next run's diff.

A run passes when every stage-A check passed and stage B passed or did not
apply (`n/a`, or not run). `bakeoff_summary.py` reads reports through the
same `load_reports`.

    scripts/app-agent/corpus_report.py <run-dir> [--against <run id or dir>] [--quiet]
"""

import glob
import json
import os
import re
import statistics
import sys

EVALS = os.path.join(os.path.dirname(__file__), "..", "..", "target", "app-agent-evals")


def load_reports(run_dir, depth=1):
    """Every report under `run_dir`, `depth` directories down, with its
    transcript's stop reason folded in and its directory name kept."""
    pattern = os.path.join(run_dir, *(["*"] * depth), "report.json")
    reports = []
    for path in sorted(glob.glob(pattern)):
        report = json.load(open(path))
        report["_dir"] = os.path.relpath(os.path.dirname(path), run_dir)
        if "stopped" not in report:
            transcript = os.path.join(os.path.dirname(path), "transcript.json")
            report["stopped"] = None
            if os.path.exists(transcript):
                for step in json.load(open(transcript)).get("steps", []):
                    if "stopped" in step:
                        report["stopped"] = step["stopped"].get("reason")
        reports.append(report)
    return reports


def passed(report):
    """Stage A passed, and stage B passed or did not apply."""
    if not report.get("passed"):
        return False
    return report.get("stage_b", "n/a") in ("pass", "n/a")


def first_reason(report):
    """Why a run failed, in one line."""
    if report.get("passed") and report.get("stage_b") == "fail":
        return "stage B: the emulated C6 did not light (see stage-b.log)"
    first = next((c for c in report.get("checks", []) if not c["passed"]), None)
    if first:
        return f"{first['name']}: {first['reason']}"
    return report.get("stopped") or ""


def category(report):
    """One word-ish label for why a run failed, most telling first."""
    stopped = report.get("stopped") or ""
    if "provider error" in stopped:
        return "provider error"
    if "budget" in stopped:
        return "budget"
    failed = [c.get("kind") or c["name"] for c in report.get("checks", []) if not c["passed"]]
    if not failed:
        return "dark on the emulated C6" if report.get("stage_b") == "fail" else "other"
    labels = {
        "no_d_label_before_board": "guessed the board",
        "asked_before": "acted before asking",
        "asked": "never asked",
        "asked_about_board": "never asked the board",
        "output_on": "wrong pin",
        "output_on_d6": "wrong pin",
        "target_is": "board not set",
        "target_is_xiao_c6": "board not set",
        "strip_of": "wrong strip",
        "lamp_count": "wrong lamp count",
        "playlist": "no cycling playlist",
        "graph_wired": "not wired",
        "minimal_diff": "changed too much",
        "unchanged": "changed something",
        "all_nodes_ok": "node in error",
        "saved": "not saved",
        "never": "did a must-not",
        "card_handed": "no card",
        "board_runs_project": "board not running it",
        "board_firmware": "firmware not as expected",
        "max_questions": "asked too much",
        "max_turns": "too slow",
        "said_any": "didn't say it",
        "said_none": "said a must-not",
    }
    return labels.get(failed[0], failed[0])


def scenario_key(report):
    return report.get("scenario") or re.sub(r"-r\d+$", "", report["_dir"])


def id_order(identifier):
    match = re.match(r"S(\d+)$", identifier or "")
    return int(match.group(1)) if match else 1_000


def summarize(reports):
    """The corpus's data: per scenario, rollups, totals."""
    by_scenario = {}
    for report in reports:
        by_scenario.setdefault(scenario_key(report), []).append(report)
    rows = []
    for name, runs in by_scenario.items():
        first = runs[0]
        failing = next((r for r in runs if not passed(r)), None)
        rows.append({
            "scenario": name,
            "id": first.get("id", ""),
            "persona": first.get("persona", ""),
            "tags": first.get("tags", []),
            "seat": first.get("seat", ""),
            "passes": sum(passed(r) for r in runs),
            "runs": len(runs),
            "median_turns": statistics.median(r.get("turns", 0) for r in runs),
            "tokens_in": sum(r.get("tokens_in", 0) for r in runs),
            "tokens_out": sum(r.get("tokens_out", 0) for r in runs),
            "cost_usd": sum(r.get("cost_usd") or 0.0 for r in runs),
            "unscripted_questions": sum(r.get("unscripted_questions", 0) for r in runs),
            "cards_handed": sum(r.get("cards_handed", 0) for r in runs),
            "stage_b": sorted({r.get("stage_b", "n/a") for r in runs}),
            "first_failure": first_reason(failing) if failing else "",
            "category": category(failing) if failing else "",
        })
    rows.sort(key=lambda row: (id_order(row["id"]), row["scenario"]))

    def rollup(key_of):
        groups = {}
        for row in rows:
            for key in key_of(row):
                groups.setdefault(key, []).append(row)
        out = []
        for key, members in sorted(groups.items()):
            runs = sum(m["runs"] for m in members)
            cost = sum(m["cost_usd"] for m in members)
            out.append({
                "key": key,
                "scenarios": len(members),
                "passes": sum(m["passes"] for m in members),
                "runs": runs,
                "median_turns": statistics.median(m["median_turns"] for m in members),
                "cost_usd": cost,
                "mean_cost_usd": cost / runs if runs else 0.0,
            })
        return out

    models = sorted({r.get("driver", "").removeprefix("openrouter:") for r in reports})
    return {
        "model": models[0] if len(models) == 1 else ", ".join(models),
        "scenarios": rows,
        "by_tag": rollup(lambda row: row["tags"]),
        "by_persona": rollup(lambda row: [row["persona"]]),
        "totals": {
            "passes": sum(row["passes"] for row in rows),
            "runs": sum(row["runs"] for row in rows),
            "cost_usd": sum(row["cost_usd"] for row in rows),
            "tokens_in": sum(row["tokens_in"] for row in rows),
            "tokens_out": sum(row["tokens_out"] for row in rows),
            "unscripted_questions": sum(row["unscripted_questions"] for row in rows),
        },
    }


def previous_corpus(run_dir, model, against):
    """The corpus to diff against: `--against` (a run id or a directory), else
    the newest other corpus.json with the same model."""
    if against:
        path = against if os.path.isdir(against) else os.path.join(EVALS, against)
        candidate = os.path.join(path, "corpus.json")
        return (json.load(open(candidate)), path) if os.path.exists(candidate) else (None, None)
    here = os.path.realpath(run_dir)
    best = None
    for path in glob.glob(os.path.join(EVALS, "*", "corpus.json")):
        if os.path.realpath(os.path.dirname(path)) == here:
            continue
        try:
            data = json.load(open(path))
        except (OSError, ValueError):
            continue
        if data.get("model") != model:
            continue
        mtime = os.path.getmtime(path)
        if best is None or mtime > best[0]:
            best = (mtime, data, os.path.dirname(path))
    return (best[1], best[2]) if best else (None, None)


def diff(now, before):
    """Scenarios that flipped, and turns and cost deltas, against `before`."""
    old = {row["scenario"]: row for row in before.get("scenarios", [])}
    out = []
    for row in now["scenarios"]:
        prev = old.get(row["scenario"])
        if prev is None:
            out.append({"scenario": row["scenario"], "id": row["id"], "change": "new"})
            continue
        rate = lambda r: r["passes"] / r["runs"] if r["runs"] else 0.0
        change = ""
        if rate(row) > rate(prev):
            change = "fixed" if rate(row) == 1.0 else "better"
        elif rate(row) < rate(prev):
            change = "broke" if rate(prev) == 1.0 else "worse"
        out.append({
            "scenario": row["scenario"],
            "id": row["id"],
            "change": change,
            "was": f"{prev['passes']}/{prev['runs']}",
            "now": f"{row['passes']}/{row['runs']}",
            "turns_delta": row["median_turns"] - prev["median_turns"],
            "cost_delta": row["cost_usd"] - prev["cost_usd"],
        })
    return out


def markdown(data, run_name, against_dir):
    lines = [f"# Agent activity corpus — {run_name}", ""]
    totals = data["totals"]
    lines.append(
        f"Model **{data['model']}**: {totals['passes']}/{totals['runs']} runs passed, "
        f"${totals['cost_usd']:.3f} reported, {totals['tokens_in']:,} in / "
        f"{totals['tokens_out']:,} out tokens, {totals['unscripted_questions']} unscripted "
        "question(s)."
    )
    lines += ["", "## Scenarios", "",
              "| id | scenario | persona | seat | pass | turns | tokens in/out | $ | unscripted Qs | cards | stage B | first failure |",
              "|---|---|---|---|---:|---:|---:|---:|---:|---:|---|---|"]
    for row in data["scenarios"]:
        reason = row["first_failure"].replace("|", "\\|").replace("\n", " ")
        if len(reason) > 160:
            reason = reason[:157] + "…"
        if row["category"]:
            reason = f"**{row['category']}** — {reason}"
        lines.append(
            f"| {row['id']} | {row['scenario']} | {row['persona']} | {row['seat']} | "
            f"{row['passes']}/{row['runs']} | {row['median_turns']:g} | "
            f"{row['tokens_in']:,} / {row['tokens_out']:,} | {row['cost_usd']:.3f} | "
            f"{row['unscripted_questions']} | {row['cards_handed']} | "
            f"{', '.join(row['stage_b'])} | {reason} |"
        )
    for title, key in (("By tag", "by_tag"), ("By persona", "by_persona")):
        lines += ["", f"## {title}", "",
                  "| | scenarios | pass | median turns | total $ | mean $/run |",
                  "|---|---:|---:|---:|---:|---:|"]
        for group in data[key]:
            lines.append(
                f"| {group['key']} | {group['scenarios']} | {group['passes']}/{group['runs']} | "
                f"{group['median_turns']:g} | {group['cost_usd']:.3f} | {group['mean_cost_usd']:.4f} |"
            )
    lines += ["", "## Against the last run", ""]
    if data.get("diff") is None:
        lines.append("No earlier corpus run with this model to compare against.")
    else:
        lines.append(f"Against `{against_dir}`.")
        lines += ["", "| id | scenario | change | was | now | Δ turns | Δ $ |",
                  "|---|---|---|---|---|---:|---:|"]
        for row in data["diff"]:
            if row["change"] == "new":
                lines.append(f"| {row['id']} | {row['scenario']} | new | | | | |")
                continue
            lines.append(
                f"| {row['id']} | {row['scenario']} | {row['change'] or '—'} | {row['was']} | "
                f"{row['now']} | {row['turns_delta']:+g} | {row['cost_delta']:+.3f} |"
            )
    return "\n".join(lines) + "\n"


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 2
    run_dir = args[0]
    against = None
    if "--against" in argv:
        against = argv[argv.index("--against") + 1]
    reports = load_reports(run_dir)
    if not reports:
        print(f"corpus_report: no report.json under {run_dir}", file=sys.stderr)
        return 1
    data = summarize(reports)
    before, against_dir = previous_corpus(run_dir, data["model"], against)
    data["diff"] = diff(data, before) if before else None
    data["against"] = against_dir
    json.dump(data, open(os.path.join(run_dir, "corpus.json"), "w"), indent=2)
    text = markdown(data, os.path.basename(os.path.normpath(run_dir)), against_dir)
    open(os.path.join(run_dir, "corpus.md"), "w").write(text)
    if "--quiet" not in argv:
        print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
