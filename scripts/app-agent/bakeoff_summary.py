#!/usr/bin/env python3
"""The app-agent bake-off table (plan lp2025/2026-10-01-0126-app-agent-harness, P07).

Reads every `report.json` under a bake-off run directory
(`target/app-agent-evals/<run>/<model-slug>/<scenario>-rN/report.json`) and
prints, as Markdown:

- per model × scenario: passes (stage A AND stage B) out of runs;
- per model: median turns per scenario run, mean tokens in/out, mean and total
  cost as OpenRouter reported it;
- every failed run with its failure category and first reason.

A run passes when every stage-A check passed and stage B passed (or was not
asked for: a project-less scenario has nothing to deploy).

    scripts/app-agent/bakeoff_summary.py <run-dir> [bakeoff.toml]
"""

import glob
import json
import os
import statistics
import sys
import tomllib

SCENARIOS = ["e1-sean-from-empty", "e2-make-it-300", "e3-never-guess-the-board"]
SHORT = {
    "e1-sean-from-empty": "E1 Sean from empty",
    "e2-make-it-300": "E2 make it 300",
    "e3-never-guess-the-board": "E3 ask the board",
}


def category(report):
    """One word-ish label for why a run failed, most telling first."""
    checks = {c["name"]: c for c in report.get("checks", [])}
    failed = [name for name, c in checks.items() if not c["passed"]]
    stopped = report.get("stopped") or ""
    if "provider error" in stopped:
        return "provider error"
    if "budget" in stopped:
        return "budget"
    if "no_d_label_before_board" in failed:
        return "guessed the board"
    if "asked_about_board" in failed:
        return "never asked the board"
    if not failed and report.get("stage_b") == "fail":
        return "dark on the emulated C6"
    order = [
        ("output_on_d6", "wrong pin"),
        ("target_is_xiao_c6", "board not set"),
        ("strip_of", "wrong strip"),
        ("playlist_cycles", "no cycling playlist"),
        ("graph_wired", "not wired"),
        ("minimal_diff", "changed too much"),
        ("all_nodes_ok", "node in error"),
        ("saved", "not saved"),
    ]
    for name, label in order:
        if name in failed:
            return label
    return "other"


def passed(report):
    if not report.get("passed"):
        return False
    return report.get("stage_b", "pass") == "pass"


def main():
    run_dir = sys.argv[1]
    reference = set()
    if len(sys.argv) > 2:
        config = tomllib.load(open(sys.argv[2], "rb"))
        reference = {c["model"] for c in config.get("candidate", []) if c.get("reference")}

    by_model = {}
    for path in sorted(glob.glob(os.path.join(run_dir, "*", "*", "report.json"))):
        if os.sep + "probe" in path or "-probe" + os.sep in path:
            continue
        report = json.load(open(path))
        transcript_path = os.path.join(os.path.dirname(path), "transcript.json")
        stopped = None
        if os.path.exists(transcript_path):
            for step in json.load(open(transcript_path)).get("steps", []):
                if "stopped" in step:
                    stopped = step["stopped"].get("reason")
        report["stopped"] = stopped
        model = report["driver"].removeprefix("openrouter:")
        report["_dir"] = os.path.relpath(os.path.dirname(path), run_dir)
        by_model.setdefault(model, []).append(report)

    total = 0.0
    lines = ["| model | " + " | ".join(SHORT[s] for s in SCENARIOS)
             + " | median turns | mean tokens in/out | mean $/run | total $ |",
             "|---|" + "---:|" * (len(SCENARIOS) + 4)]
    failures = []
    for model, reports in sorted(by_model.items(), key=lambda kv: -sum(passed(r) for r in kv[1])):
        cells = []
        for scenario in SCENARIOS:
            runs = [r for r in reports if r["scenario"] == scenario]
            cells.append(f"{sum(passed(r) for r in runs)}/{len(runs)}" if runs else "–")
        turns = statistics.median(r.get("turns", 0) for r in reports)
        tin = statistics.mean(r.get("tokens_in", 0) for r in reports)
        tout = statistics.mean(r.get("tokens_out", 0) for r in reports)
        costs = [r.get("cost_usd") or 0.0 for r in reports]
        total += sum(costs)
        name = f"{model} (reference)" if model in reference else model
        lines.append(
            f"| {name} | " + " | ".join(cells)
            + f" | {turns:g} | {tin:,.0f} / {tout:,.0f} | {statistics.mean(costs):.4f} | {sum(costs):.3f} |"
        )
        for r in reports:
            if not passed(r):
                first = next((c for c in r["checks"] if not c["passed"]), None)
                reason = first["reason"] if first else (r.get("stopped") or "stage B: see stage-b.log")
                failures.append(f"- {model} · {r['_dir'].split(os.sep)[-1]}: **{category(r)}** — {reason[:220]}")
    print("\n".join(lines))
    print(f"\nTotal reported cost: ${total:.3f}\n")
    if failures:
        print("### Failed runs\n")
        print("\n".join(failures))


if __name__ == "__main__":
    main()
