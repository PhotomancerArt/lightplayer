#!/bin/bash
set -euo pipefail

# watch-pr.sh: wait on a PR's CI (or on the PR merging) with silence diagnosed.
#
#   scripts/watch-pr.sh [<pr>]            watch checks to completion
#                                         exit 0 = all green, 1 = a check failed
#   scripts/watch-pr.sh --merged <pr>     wait until the PR merges
#                                         exit 0 = merged, 1 = closed unmerged
#
# <pr> defaults to the PR for the current branch. Designed to run as a
# background task (`run_in_background`), which notifies on exit — never as a
# foreground sleep loop.
#
# The value over bare `gh pr checks --watch` is the registration phase:
# `--watch` exits immediately when a PR has no checks yet, and in this repo
# "no checks" has three legitimate causes that look identical to a hang or an
# instant pass. This script waits for checks to appear and names those causes
# if they don't:
#   - path-filtered CI: no job matches the diff (see .github/workflows)
#   - stacked PR: bases other than main get NO CI until retargeted
#   - GITHUB_TOKEN pushes (e.g. the story-baseline auto-commit) trigger no runs

# GitHub also reports an EMPTY check list transiently during run transitions
# (right after a push replaces the head commit, or while a check suite
# re-registers), so emptiness only counts against this timeout while it is
# consecutive — any sighting of checks resets the clock.
REGISTER_TIMEOUT="${WATCH_PR_REGISTER_TIMEOUT:-600}"   # consecutive-empty seconds before giving up
POLL_INTERVAL="${WATCH_PR_POLL_INTERVAL:-15}"          # seconds between polls

mode="checks"
if [[ "${1:-}" == "--merged" ]]; then
  mode="merged"
  shift
fi
pr="${1:-}"

# gh infers the PR from the branch when $pr is empty; keep args as an array so
# an explicit number/URL passes through unchanged.
pr_args=()
[[ -n "$pr" ]] && pr_args=("$pr")

view() {
  gh pr view "${pr_args[@]}" --json "$1" --jq "$2"
}

state="$(view state .state)"
base="$(view baseRefName .baseRefName)"

if [[ "$mode" == "merged" ]]; then
  while true; do
    case "$state" in
      MERGED) echo "merged."; exit 0 ;;
      CLOSED) echo "closed without merging." >&2; exit 1 ;;
    esac
    sleep "$POLL_INTERVAL"
    state="$(view state .state)"
  done
fi

case "$state" in
  MERGED) echo "already merged; nothing to watch."; exit 0 ;;
  CLOSED) echo "PR is closed." >&2; exit 1 ;;
esac

if [[ "$base" != "main" ]]; then
  echo "note: base is '$base', not main — stacked PRs get no CI until retargeted." >&2
fi

no_ci_diagnostic() {
  cat >&2 <<EOF
no checks registered after ${REGISTER_TIMEOUT}s. Likely causes:
  - path-filtered CI: no workflow job matches this diff (.github/workflows)
  - stacked PR: base '$base' — CI only runs against main; retarget the PR
  - the last push was made with GITHUB_TOKEN (e.g. story-baseline
    auto-commit), which never triggers workflows — push any commit to kick CI
EOF
}

# empty_deadline is set while the check list is empty and cleared the moment
# checks appear, so only CONSECUTIVE emptiness exhausts REGISTER_TIMEOUT.
empty_deadline=""
while true; do
  if [[ "$(view statusCheckRollup '.statusCheckRollup | length')" == "0" ]]; then
    [[ -n "$empty_deadline" ]] || empty_deadline=$((SECONDS + REGISTER_TIMEOUT))
    if ((SECONDS >= empty_deadline)); then
      no_ci_diagnostic
      exit 2
    fi
    sleep "$POLL_INTERVAL"
    continue
  fi
  empty_deadline=""

  # --fail-fast exits on the first failed required check; exit status reflects
  # the outcome (0 green, nonzero otherwise). Output is captured because the
  # watch itself can die with "no checks reported" when the list goes empty
  # mid-run — that is the same transient state as above, so re-enter the poll
  # loop instead of passing gh's failure through.
  if out="$(gh pr checks "${pr_args[@]}" --watch --fail-fast --interval "$POLL_INTERVAL" 2>&1)"; then
    rc=0
  else
    rc=$?
  fi
  if ((rc != 0)) && [[ "$out" == *"no checks reported"* ]]; then
    sleep "$POLL_INTERVAL"
    continue
  fi
  [[ -n "$out" ]] && printf '%s\n' "$out"
  exit "$rc"
done
