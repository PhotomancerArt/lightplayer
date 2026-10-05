#!/usr/bin/env bash
# The app-agent bake-off (plan lp2025/2026-10-01-0126-app-agent-harness, P07):
# every candidate in lpa-studio-core/tests/fixtures/app_agent/bakeoff.toml ×
# the scenarios (S1–S3, E1–E3's, by default) × `runs`, stage A then stage B
# on each project, one summary table.
#
#   scripts/app-agent/bakeoff.sh [--run <id>] [--jobs <n>] [--only <model>]
#       [--scenarios S1,S2,S3]
#
# Cost: the first candidate's first E1 run is measured alone; if the whole
# bake-off would exceed `cost_ceiling_usd` at that rate (×1.5 headroom), it
# stops and says so before spending more. The key comes from
# OPENROUTER_API_KEY or ~/.lightplayer/settings.json and is written nowhere.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
config="$root/lp-app/lpa-studio-core/tests/fixtures/app_agent/bakeoff.toml"
run="bakeoff-$(date +%Y%m%d-%H%M)"
jobs=3
only=""
scenarios="S1,S2,S3"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --run) run="$2"; shift 2 ;;
        --jobs) jobs="$2"; shift 2 ;;
        --only) only="$2"; shift 2 ;;
        --scenarios) scenarios="$2"; shift 2 ;;
        *) echo "bakeoff.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done

read -r runs ceiling < <(python3 - "$config" <<'PY'
import sys, tomllib
c = tomllib.load(open(sys.argv[1], "rb"))
print(c.get("runs", 3), c.get("cost_ceiling_usd", 25.0))
PY
)
models=()
while IFS= read -r model; do
    models+=("$model")
done < <(python3 - "$config" "$only" <<'PY'
import sys, tomllib
c = tomllib.load(open(sys.argv[1], "rb"))
for cand in c["candidate"]:
    if not sys.argv[2] or cand["model"] == sys.argv[2]:
        print(cand["model"])
PY
)
base="$root/target/app-agent-evals/$run"
mkdir -p "$base"
count="$(echo "$scenarios" | tr ',' '\n' | grep -c .)"
echo "bakeoff $run: ${#models[@]} models × $count scenarios ($scenarios) × $runs runs; ceiling \$$ceiling"

slug() { echo "$1" | tr '/:' '__'; }

# 1. One E1 run of the first candidate, to price the rest.
first="${models[0]}"
"$root/scripts/app-agent/eval.sh" S1 --model "$first" --run "$run/$(slug "$first")-probe" --no-emu \
    > "$base/probe.log" 2>&1 || true
probe_cost="$(python3 - "$base/$(slug "$first")-probe" <<'PY'
import json, glob, sys
costs = [json.load(open(p)).get("cost_usd") or 0 for p in glob.glob(sys.argv[1] + "/*/report.json")]
print(max(costs) if costs else 0)
PY
)"
projected="$(python3 -c "print(round($probe_cost * ${#models[@]} * $count * $runs * 1.5, 2))")"
echo "probe: one E1 on $first cost \$$probe_cost; projected bake-off ≤ \$$projected (×1.5 headroom)"
if python3 -c "import sys; sys.exit(0 if $projected > $ceiling else 1)"; then
    echo "bakeoff: projected \$$projected exceeds the \$$ceiling ceiling — stopping before spending more." >&2
    exit 3
fi

# 2. Every candidate, `jobs` at a time.
pids=()
for model in "${models[@]}"; do
    (
        "$root/scripts/app-agent/eval.sh" all --only "$scenarios" --max-usd "$ceiling" \
            --model "$model" --run "$run/$(slug "$model")" --repeat "$runs" \
            > "$base/$(slug "$model").log" 2>&1 || true
        echo "bakeoff: $model done"
    ) &
    pids+=($!)
    while [[ "$(jobs -rp | wc -l)" -ge "$jobs" ]]; do sleep 5; done
done
wait

# 3. The table.
python3 "$root/scripts/app-agent/bakeoff_summary.py" "$base" "$config" | tee "$base/summary.md"
