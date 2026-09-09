#!/bin/bash
set -euo pipefail

# watch-pr.sh: wait on a PR's CI (or on the PR merging) with silence diagnosed.
#
#   scripts/watch-pr.sh [<pr>]            watch checks to completion
#                                         exit 0 = all green, 1 = a check failed
#                                         exit 2 = no checks registered in time
#                                         exit 3 = PR is conflicting (no CI runs)
#                                         exit 4 = the head moved while watching
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
#
# Worse than empty is PARTIAL, and that is what this script used to call
# green. Seconds after a push the rollup holds whatever registered first —
# often only `cla`, which passes in five seconds — and `gh pr checks --watch`
# sees one pass, nothing pending, and exits 0. The caller reads "green" for a
# head whose real CI has not started.
#
# So a green is believed only when **every workflow run for the PR's current
# head commit has completed** (`head_runs_all_completed`). A run that is still
# `queued` is the registration phase, whatever the rollup says. That is the
# `gh run list --branch` cross-check the agent harness notes tell everyone to
# do by hand, done here instead.
#
# And a CANCELLED run is not a failure. `cancel-in-progress` evicts a run the
# moment a newer push arrives, and GitHub reports the eviction as a red check;
# a caller sent to read that log finds nothing in it. A red whose runs carry
# no real failure conclusion is reported as SUPERSEDED and waited out.
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
#
# Always expand it as ${pr_args[@]+"${pr_args[@]}"}. macOS ships bash 3.2,
# where a bare "${pr_args[@]}" on an EMPTY array is an unbound-variable error
# under `set -u` — i.e. exactly the no-argument case this script exists to
# support. The +alternate form yields zero words instead of tripping set -u.
pr_args=()
[[ -n "$pr" ]] && pr_args=("$pr")

view() {
  gh pr view ${pr_args[@]+"${pr_args[@]}"} --json "$1" --jq "$2"
}

state="$(view state .state)"
base="$(view baseRefName .baseRefName)"
branch="$(view headRefName .headRefName)"
head="$(view headRefOid .headRefOid)"

# The workflow runs GitHub has for the PR's current head, as
# "<status> <conclusion>" lines. Empty output means no run exists for this
# commit yet — which is the registration phase, not a pass.
runs_for_head() {
  gh run list --branch "$branch" --limit 20 \
    --json headSha,status,conclusion \
    --jq "[.[] | select(.headSha == \"$head\")] | .[] | \"\(.status) \(.conclusion)\"" \
    2>/dev/null || true
}

# Has every run for the head finished? Empty (no run yet) is NOT finished:
# that is the registration phase this script exists to wait out.
head_runs_all_completed() {
  local out
  out="$(runs_for_head)"
  [[ -n "$out" ]] && ! grep -qv '^completed' <<<"$out"
}

# Did any run for the head fail for a reason worth reading a log about?
# `cancelled` is not one — see the header.
head_has_a_real_failure() {
  runs_for_head | grep -qE 'failure|timed_out|startup_failure|action_required'
}

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
partial_deadline=""
while true; do
  if [[ "$(view statusCheckRollup '.statusCheckRollup | length')" == "0" ]]; then
    # A conflicted PR gets NO pull_request CI at all: no run will ever
    # register, so waiting out the timeout only delays the news. (mergeable
    # is UNKNOWN while GitHub recomputes it after a push; only the settled
    # CONFLICTING verdict short-circuits.)
    if [[ "$(view mergeable .mergeable)" == "CONFLICTING" ]]; then
      echo "PR is CONFLICTING — GitHub runs no pull_request CI on a conflicted PR." >&2
      echo "Merge the base branch into it (resolve, push), then re-watch." >&2
      exit 3
    fi
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
  if out="$(gh pr checks ${pr_args[@]+"${pr_args[@]}"} --watch --fail-fast --interval "$POLL_INTERVAL" 2>&1)"; then
    rc=0
  else
    rc=$?
  fi
  if ((rc != 0)) && [[ "$out" == *"no checks reported"* ]]; then
    sleep "$POLL_INTERVAL"
    continue
  fi

  # The head can move under a watch. Say so rather than reporting a verdict
  # about a commit nobody is building any more.
  new_head="$(view headRefOid .headRefOid)"
  if [[ "$new_head" != "$head" ]]; then
    echo "the head moved ${head:0:9} → ${new_head:0:9} while watching; re-run to watch the new one." >&2
    exit 4
  fi

  # A red whose runs carry no real failure is a supersede. Wait for whatever
  # replaced it instead of sending the caller to an empty log.
  if ((rc != 0)) && ! head_has_a_real_failure; then
    echo "superseded: no run on ${head:0:9} failed — the red checks are a cancelled run." >&2
    echo "waiting for its replacement." >&2
    partial_deadline=""
    sleep "$POLL_INTERVAL"
    continue
  fi

  # A green is only a green once every run on this head has finished. The
  # rollup can be all-pass while the real CI run is still queued.
  if ((rc == 0)) && ! head_runs_all_completed; then
    if [[ -z "$partial_deadline" ]]; then
      partial_deadline=$((SECONDS + REGISTER_TIMEOUT))
      echo "checks report green, but no completed workflow run on ${head:0:9} yet — waiting." >&2
    fi
    if ((SECONDS >= partial_deadline)); then
      no_ci_diagnostic
      exit 2
    fi
    sleep "$POLL_INTERVAL"
    continue
  fi
  partial_deadline=""

  [[ -n "$out" ]] && printf '%s\n' "$out"
  exit "$rc"
done
