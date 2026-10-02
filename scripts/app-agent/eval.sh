#!/usr/bin/env bash
# The app-agent evals' live leg (plan lp2025/2026-10-01-0126-app-agent-harness):
# stage A (a model builds the project in a headless Studio, project-level
# checks) then stage B (the tree it wrote deployed to an emulated C6, frames
# decoded off D6 = GPIO16).
#
#   scripts/app-agent/eval.sh <scenario|all> [--model <slug>] [--run <id>] [--repeat <n>]
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
while [[ $# -gt 0 ]]; do
    case "$1" in
        --model) model="$2"; shift 2 ;;
        --run) run="$2"; shift 2 ;;
        --repeat) repeat="$2"; shift 2 ;;
        --no-emu) emu=""; shift ;;
        *) echo "eval.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$model" ]]; then
    model="${LPA_EVAL_MODEL:-}"
fi
if [[ -z "$model" ]]; then
    echo "eval.sh: --model <openrouter slug> (or LPA_EVAL_MODEL) is required" >&2
    exit 2
fi
if [[ -z "$run" ]]; then
    run="$(date +%Y%m%d-%H%M%S)-$(echo "$model" | tr '/:' '__')"
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
run_dir="$root/target/app-agent-evals/$run"
mkdir -p "$run_dir"
echo "app-agent-eval: run $run, scenario $scenario, model $model, $repeat repeat(s)"

LPA_APP_EVAL_SCENARIO="$scenario" \
LPA_EVAL_MODEL="$model" \
LPA_APP_EVAL_RUN="$run" \
LPA_APP_EVAL_REPEAT="$repeat" \
    cargo test -p lpa-studio-core --lib app_agent_eval_live -- --ignored --nocapture

if [[ -z "$emu" ]]; then
    echo "app-agent-eval: stage B skipped (--no-emu)"
    exit 0
fi

# Stage B on every project stage A wrote (scenario dirs, possibly per repeat).
status=0
while IFS= read -r report; do
    dir="$(dirname "$report")"
    [[ -d "$dir/project" ]] || continue
    leds="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["leds"])' "$report")"
    echo "app-agent-eval: stage B on $dir ($leds LEDs)"
    if LP_APP_AGENT_PROJECT="$dir/project" LP_APP_AGENT_LEDS="$leds" LP_EMU_BUILD_FW="${LP_EMU_BUILD_FW:-1}" \
        "$root/scripts/ci/ci-images.py" with esp32c6 -- \
        cargo test -p lp-cli --release --test app_agent_emu_decode -- --ignored --nocapture an_agent_built_project \
        > "$dir/stage-b.log" 2>&1; then
        python3 - "$report" pass <<'PY'
import json, sys
p, verdict = sys.argv[1], sys.argv[2]
r = json.load(open(p)); r["stage_b"] = verdict; json.dump(r, open(p, "w"), indent=2)
PY
        echo "  stage B: pass"
    else
        python3 - "$report" fail <<'PY'
import json, sys
p, verdict = sys.argv[1], sys.argv[2]
r = json.load(open(p)); r["stage_b"] = verdict; json.dump(r, open(p, "w"), indent=2)
PY
        echo "  stage B: FAIL (see $dir/stage-b.log)"
        status=1
    fi
done < <(find "$run_dir" -name report.json | sort)
exit "$status"
