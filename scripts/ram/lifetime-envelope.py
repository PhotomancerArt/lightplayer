#!/usr/bin/env python3
"""Size each segregated region for EVERY workload at once, from
`lifetime-census.py --out` JSONs: a region a design would actually reserve is
the largest footprint its group needed in any workload, not one trace's.

For each grouping: every group's footprint across the workloads and its
maximum (the envelope); the reservation that envelope costs; and the one big
block it leaves — the budget less every other group's envelope — against the
smallest largest-free-block today showed in each workload (after its first
project load). The absorber's guarantee assumes the regions can be laid out
(packed whole, or one group split across the two areas, as the census's own
spill does): a flat figure, slightly optimistic for a split.

Usage: lifetime-envelope.py OUT_DIR/*.json [--budget 252384]
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("json", nargs="+")
    ap.add_argument("--budget", type=int, default=186_848 + 65_536)
    args = ap.parse_args()
    runs = [json.load(open(p)) for p in args.json]
    names = list(runs[0]["groupings"])
    print("| grouping | group | " + " | ".join(r["label"] for r in runs) + " | envelope |")
    print("|---|---|" + "---:|" * (len(runs) + 1))
    for nm in names:
        g0 = runs[0]["groupings"][nm]
        env = []
        for gi, gname in enumerate(g0["groups"]):
            vals = [r["groupings"][nm]["footprint"][gi] for r in runs]
            env.append(max(vals))
            tag = " (absorber)" if gi == g0["absorber"] else ""
            print(f"| {nm} | {gname}{tag} | " + " | ".join(f"{v:,}" for v in vals) +
                  f" | **{max(vals):,}** |")
        others = sum(v for gi, v in enumerate(env) if gi != g0["absorber"])
        guarantee = args.budget - others
        need = env[g0["absorber"]]
        print(f"| {nm} | Σ envelope / one big block left (budget {args.budget:,}) | " +
              " | ".join("" for _ in runs) +
              f" | **{sum(env):,}**; big block **{guarantee:,}** (needs {need:,}"
              f"{'' if guarantee >= need else ', DOES NOT FIT'}) |")
    print()
    print("| workload | today's peak live | min largest free today (after first load) |")
    print("|---|---:|---:|")
    for r in runs:
        mt = r["groupings"][names[0]]["summary"]["min_today"]
        print(f"| {r['label']} | {r['rust_peak']:,} | {mt:,} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
