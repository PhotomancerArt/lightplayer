#!/usr/bin/env bash
# The agent activity corpus (plan lp2025/2026-10-01-1255-agentic-ui-roadmap/
# m-agent-activity-corpus): every active scenario (or a selection), live,
# then one report — per scenario, per tag, per persona, and the diff against
# the last corpus run with the same model.
#
#   scripts/app-agent/corpus.sh [--model <slug>] [--max-usd 2] [--repeat n]
#       [--only S4,S7] [--tag t] [--persona p] [--include-pending]
#       [--against <run id or dir>] [--run <id>] [--no-emu] [--dry-run]
#
# Defaults: every active scenario once, at LPA_EVAL_MODEL or z-ai/glm-5.3,
# capped at $2 of reported spend. Costs real money; never CI. Writes
# target/app-agent-evals/<run>/{<scenario>/…, corpus.md, corpus.json}.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
model="${LPA_EVAL_MODEL:-z-ai/glm-5.3}"
run=""
against=""
dry=""
pass=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --model) model="$2"; shift 2 ;;
        --run) run="$2"; shift 2 ;;
        --against) against="$2"; shift 2 ;;
        --dry-run) dry="1"; pass+=("$1"); shift ;;
        --no-emu|--include-pending) pass+=("$1"); shift ;;
        --max-usd|--repeat|--only|--tag|--persona) pass+=("$1" "$2"); shift 2 ;;
        *) echo "corpus.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$run" ]]; then
    run="corpus-$(date +%Y%m%d-%H%M%S)-$(echo "$model" | tr '/:' '__')"
fi

status=0
"$root/scripts/app-agent/eval.sh" all --model "$model" --run "$run" ${pass[@]+"${pass[@]}"} || status=$?
if [[ -n "$dry" ]]; then
    exit "$status"
fi
report_args=("$root/target/app-agent-evals/$run")
if [[ -n "$against" ]]; then
    report_args+=(--against "$against")
fi
python3 "$root/scripts/app-agent/corpus_report.py" "${report_args[@]}"
exit "$status"
