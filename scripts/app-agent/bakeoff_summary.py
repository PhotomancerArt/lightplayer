#!/usr/bin/env python3
"""The app-agent bake-off table (plan lp2025/2026-10-01-0126-app-agent-harness, P07).

Reads every `report.json` under a bake-off run directory
(`target/app-agent-evals/<run>/<model-slug>/<scenario>-rN/report.json`) and
prints, as Markdown:

- per model × scenario: passes (stage A AND stage B) out of runs;
- per model: median turns per scenario run, mean tokens in/out, mean and total
  cost as OpenRouter reported it;
- every failed run with its failure category and first reason.

A run passes when every stage-A check passed and stage B passed (or did not
apply). The scenarios are whichever the reports name (`corpus_report.py`'s
loader), in corpus-id order; the failure categories are the checks' kinds.

    scripts/app-agent/bakeoff_summary.py <run-dir> [bakeoff.toml]
"""

import os
import statistics
import sys
import tomllib

from corpus_report import category, load_reports, passed, scenario_key, id_order


def main():
    run_dir = sys.argv[1]
    reference = set()
    if len(sys.argv) > 2:
        config = tomllib.load(open(sys.argv[2], "rb"))
        reference = {c["model"] for c in config.get("candidate", []) if c.get("reference")}

    by_model = {}
    for report in load_reports(run_dir, depth=2):
        if "probe" in report["_dir"].split(os.sep)[0]:
            continue
        model = report["driver"].removeprefix("openrouter:")
        by_model.setdefault(model, []).append(report)

    ids = {}
    for reports in by_model.values():
        for report in reports:
            name = scenario_key(report)
            ids[name] = ids.get(name) or report.get("id", "")
    scenarios = sorted(((sid, name) for name, sid in ids.items()),
                       key=lambda pair: (id_order(pair[0]), pair[1]))
    total = 0.0
    lines = ["| model | " + " | ".join(f"{sid} {name}".strip() for sid, name in scenarios)
             + " | median turns | mean tokens in/out | mean $/run | total $ |",
             "|---|" + "---:|" * (len(scenarios) + 4)]
    failures = []
    for model, reports in sorted(by_model.items(), key=lambda kv: -sum(passed(r) for r in kv[1])):
        cells = []
        for _, scenario in scenarios:
            runs = [r for r in reports if scenario_key(r) == scenario]
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
