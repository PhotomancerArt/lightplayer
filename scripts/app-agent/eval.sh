#!/usr/bin/env bash
# The app-agent evals' live leg (plans lp2025/2026-10-01-0126-app-agent-harness
# and …/m-agent-activity-corpus): stage A (a model works the scenario in its
# seat — a headless Studio, or the device bench with a fake board — and the
# checks judge what it left) then stage B (the tree it left deployed to an
# emulated C6, frames decoded off the scenario's pad).
#
#   scripts/app-agent/eval.sh <scenario|all> [--model <slug>] [--run <id>] [--repeat <n>]
#       [--only S4,S7] [--tag t1,t2] [--persona p1,p2] [--include-pending]
#       [--max-usd <usd>] [--dry-run] [--no-emu]
#
# `<scenario>` is a name, a name prefix (`e1`, `s18`) or an id (`S4`).
# `--max-usd` (default 2) caps the run's reported spend: each scenario's
# own budget is cut to the room left, and none starts with under $0.05.
# `--dry-run` lists what would run (and why the rest would not) with no
# model and no key.
#
# The key comes from OPENROUTER_API_KEY or ~/.lightplayer/settings.json
# (agent.openrouter_api_key); it is read by the test process and never
# written anywhere. Reports land in target/app-agent-evals/<run>/.
# Stage B needs a C6 image: LP_CI_IMAGES (`just fetch-ci-images`) or
# LP_EMU_BUILD_FW=1. `--no-emu` skips it.
set -euo pipefail

scenario="${1:-all}"
shift || true
model=""
run=""
repeat="1"
emu="1"
only=""
tags=""
personas=""
pending=""
max_usd="2"
dry=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --model) model="$2"; shift 2 ;;
        --run) run="$2"; shift 2 ;;
        --repeat) repeat="$2"; shift 2 ;;
        --no-emu) emu=""; shift ;;
        --only) only="$2"; shift 2 ;;
        --tag) tags="$2"; shift 2 ;;
        --persona) personas="$2"; shift 2 ;;
        --include-pending) pending="1"; shift ;;
        --max-usd) max_usd="$2"; shift 2 ;;
        --dry-run) dry="1"; shift ;;
        *) echo "eval.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$model" ]]; then
    model="${LPA_EVAL_MODEL:-}"
fi
if [[ -z "$model" && -z "$dry" ]]; then
    echo "eval.sh: --model <openrouter slug> (or LPA_EVAL_MODEL) is required" >&2
    exit 2
fi
if [[ -z "$run" ]]; then
    run="$(date +%Y%m%d-%H%M%S)-$(echo "${model:-dry}" | tr '/:' '__')"
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
run_dir="$root/target/app-agent-evals/$run"
echo "app-agent-eval: run $run, scenario $scenario, model ${model:-(dry run)}, $repeat repeat(s), cap \$$max_usd"

export LPA_APP_EVAL_SCENARIO="$scenario"
export LPA_EVAL_MODEL="$model"
export LPA_APP_EVAL_RUN="$run"
export LPA_APP_EVAL_REPEAT="$repeat"
export LPA_APP_EVAL_ONLY="$only"
export LPA_APP_EVAL_TAGS="$tags"
export LPA_APP_EVAL_PERSONAS="$personas"
export LPA_APP_EVAL_INCLUDE_PENDING="$pending"
export LPA_APP_EVAL_MAX_USD="$max_usd"
export LPA_APP_EVAL_DRY="$dry"
cargo test -p lpa-studio-core --lib app_agent_eval_live -- --ignored --nocapture

if [[ -n "$dry" ]]; then
    exit 0
fi
mkdir -p "$run_dir"
if [[ -z "$emu" ]]; then
    echo "app-agent-eval: stage B skipped (--no-emu)"
    exit 0
fi

# Stage B on every project stage A wrote whose scenario names a C6 pad
# (`stage_b_plan` in report.json); the rest are `n/a`.
status=0
while IFS= read -r report; do
    dir="$(dirname "$report")"
    plan="$(python3 -c '
import json, sys
plan = json.load(open(sys.argv[1])).get("stage_b_plan", "n/a")
print("n/a" if plan == "n/a" else f"{plan[\"pad\"]} {plan[\"leds\"]}")
' "$report")"
    if [[ "$plan" == "n/a" || ! -d "$dir/project" ]]; then
        python3 - "$report" "n/a" <<'PY'
import json, sys
p, verdict = sys.argv[1], sys.argv[2]
r = json.load(open(p)); r["stage_b"] = verdict; json.dump(r, open(p, "w"), indent=2)
PY
        continue
    fi
    read -r pad leds <<< "$plan"
    echo "app-agent-eval: stage B on $dir ($leds LEDs on gpio$pad)"
    if LP_APP_AGENT_PROJECT="$dir/project" LP_APP_AGENT_LEDS="$leds" LP_APP_AGENT_PAD="$pad" \
        LP_EMU_BUILD_FW="${LP_EMU_BUILD_FW:-1}" \
        "$root/scripts/ci/ci-images.py" with esp32c6 -- \
        cargo test -p lp-cli --profile host-test --test app_agent_emu_decode -- --ignored --nocapture an_agent_built_project \
        > "$dir/stage-b.log" 2>&1; then
        verdict=pass
        echo "  stage B: pass"
    else
        verdict=fail
        echo "  stage B: FAIL (see $dir/stage-b.log)"
        status=1
    fi
    python3 - "$report" "$verdict" <<'PY'
import json, sys
p, verdict = sys.argv[1], sys.argv[2]
r = json.load(open(p)); r["stage_b"] = verdict; json.dump(r, open(p, "w"), indent=2)
PY
done < <(find "$run_dir" -name report.json | sort)
exit "$status"
