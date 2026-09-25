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
#   scripts/watch-pr.sh --dry-run [<pr>]  pin, print one snapshot, exit 0
#
# <pr> defaults to the PR for the current branch. Designed to run as a
# background task (`run_in_background`), which notifies on exit — never as a
# foreground sleep loop.
#
# PINNING. The PR (number + repo) and its head sha are resolved ONCE, at the
# start, and printed on the first line. Every later call names that number and
# repo explicitly, and every check reported is read from the same API response
# that proves the head is still the pinned sha. This script used to re-resolve
# on every call — up to six `gh` calls a poll, and `gh pr checks` with no
# argument in the no-arg case — so a watch left running in the background
# followed whatever the worktree had checked out *now*, not what it was asked
# to watch; and `gh pr checks --watch` followed the PR's *current* head, so
# the checks it printed could belong to a newer push than the one pinned.
# Several agent worktrees watching several PRs at once is exactly where that
# showed up (2026-09-24). If the head moves, the watch stops and says so
# (exit 4); it never switches silently to the new commit.
#
# The value over bare `gh pr checks --watch` is the registration phase:
# `--watch` exits immediately when a PR has no checks yet, and in this repo
# "no checks" has three legitimate causes that look identical to a hang or an
# instant pass. This script waits for checks to appear and names those causes
# if they don't:
#   - path-filtered CI: no job matches the diff (see .github/workflows)
#   - stacked PR: bases other than main get NO CI until retargeted
#   - GITHUB_TOKEN pushes (e.g. the story-baseline auto-commit) trigger no runs
#
# GitHub also reports an EMPTY check list transiently during run transitions
# (right after a push replaces the head commit, or while a check suite
# re-registers), so emptiness only counts against this timeout while it is
# consecutive — any sighting of checks resets the clock.
#
# Worse than empty is PARTIAL. Seconds after a push the rollup holds whatever
# registered first — often only `cla`, which passes in five seconds — so a
# green is believed only when **every workflow run for the pinned head on the
# pinned branch has completed** (`head_runs_all_completed`).
#
# And a CANCELLED run is not a failure. `cancel-in-progress` evicts a run the
# moment a newer push arrives, and GitHub reports the eviction as a red check;
# a caller sent to read that log finds nothing in it. A red whose runs carry
# no real failure conclusion is reported as SUPERSEDED and waited out.
REGISTER_TIMEOUT="${WATCH_PR_REGISTER_TIMEOUT:-600}"   # consecutive-empty seconds before giving up
POLL_INTERVAL="${WATCH_PR_POLL_INTERVAL:-15}"          # seconds between polls

mode="checks"
case "${1:-}" in
  --merged) mode="merged"; shift ;;
  --dry-run) mode="dry-run"; shift ;;
esac
given="${1:-}"

# ---- pin -------------------------------------------------------------------

# The one and only resolution. An empty $given lets gh infer the PR from the
# current branch — once, here — and the number it found is used from then on.
pin_json_fields="number,url,state,baseRefName,headRefName,headRefOid"
if [[ -n "$given" ]]; then
  pin="$(gh pr view "$given" --json "$pin_json_fields" \
    --jq '[.number, .url, .state, .baseRefName, .headRefName, .headRefOid] | @tsv')"
  how="as given ('$given')"
else
  pin="$(gh pr view --json "$pin_json_fields" \
    --jq '[.number, .url, .state, .baseRefName, .headRefName, .headRefOid] | @tsv')"
  how="from the current branch '$(git branch --show-current 2>/dev/null || echo '?')'"
fi
IFS=$'\t' read -r num url state base branch head <<<"$pin"

# owner/name from https://github.com/<owner>/<name>/pull/<n>
repo="$(sed -E 's#^https?://[^/]+/([^/]+/[^/]+)/pull/[0-9]+.*$#\1#' <<<"$url")"
if [[ -z "$num" || -z "$head" || "$repo" == "$url" ]]; then
  echo "could not pin a PR ($how): got '$pin'" >&2
  exit 1
fi

tag="PR #$num @ ${head:0:9}"
echo "watching $tag ($repo, branch '$branch' → '$base'), resolved $how"

view() {
  gh pr view "$num" -R "$repo" --json "$1" --jq "$2"
}

# The workflow runs for the PINNED head on the PINNED branch, as
# "<status> <conclusion>" lines. Empty output means no run exists for this
# commit yet — which is the registration phase, not a pass.
runs_for_head() {
  gh run list -R "$repo" --branch "$branch" --commit "$head" --limit 50 \
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

# One snapshot, from ONE API response, so the checks and the head they are
# read against cannot come from different moments:
#   head<TAB><sha>
#   mergeable<TAB><MERGEABLE|CONFLICTING|UNKNOWN>
#   check<TAB><pass|fail|cancel|pending><TAB><name><TAB><link>   (0..n)
snapshot() {
  gh pr view "$num" -R "$repo" --json headRefOid,mergeable,statusCheckRollup --jq '
    def bucket:
      if .__typename == "StatusContext" then
        if .state == "SUCCESS" then "pass"
        elif (.state == "PENDING" or .state == "EXPECTED") then "pending"
        else "fail" end
      else
        if .status != "COMPLETED" then "pending"
        elif (.conclusion == "SUCCESS" or .conclusion == "NEUTRAL" or .conclusion == "SKIPPED") then "pass"
        elif .conclusion == "CANCELLED" then "cancel"
        else "fail" end
      end;
    "head\t\(.headRefOid)",
    "mergeable\t\(.mergeable)",
    ((.statusCheckRollup // [])[]
      | "check\t\(bucket)\t\(if .__typename == "StatusContext" then .context else ((.workflowName // "") + (if .workflowName then " / " else "" end) + .name) end)\t\(.detailsUrl // .targetUrl // "")")'
}

# Stop the moment the PR's head is not the pinned sha. Nothing read in the
# same snapshot is reported: it belongs to a commit nobody asked about.
assert_head() {
  local now="$1"
  if [[ "$now" != "$head" ]]; then
    echo "HEAD MOVED on PR #$num: pinned ${head:0:9}, now ${now:0:9}." >&2
    echo "not reporting checks for ${now:0:9}; re-run to watch the new head." >&2
    exit 4
  fi
}

print_checks() {
  local checks="$1"
  echo "checks for $tag:"
  if [[ -z "$checks" ]]; then
    echo "  (none registered)"
  else
    # sort: failures first, then pending, then passes
    awk -F'\t' '{ o = ($1=="fail")?0:($1=="cancel")?1:($1=="pending")?2:3;
                  printf "%d\t%-8s %s  %s\n", o, $1, $2, $3 }' <<<"$checks" \
      | sort -t$'\t' -k1,1n -k2,2 | cut -f2- | sed 's/^/  /'
  fi
}

# ---- merged mode -----------------------------------------------------------

if [[ "$mode" == "merged" ]]; then
  while true; do
    case "$state" in
      MERGED) echo "PR #$num merged."; exit 0 ;;
      CLOSED) echo "PR #$num closed without merging." >&2; exit 1 ;;
    esac
    sleep "$POLL_INTERVAL"
    state="$(view state .state)"
  done
fi

case "$state" in
  MERGED) echo "PR #$num already merged; nothing to watch."; exit 0 ;;
  CLOSED) echo "PR #$num is closed." >&2; exit 1 ;;
esac

if [[ "$base" != "main" ]]; then
  echo "note: base is '$base', not main — stacked PRs get no CI until retargeted." >&2
fi

if [[ "$mode" == "dry-run" ]]; then
  snap="$(snapshot)"
  assert_head "$(awk -F'\t' '$1=="head"{print $2}' <<<"$snap")"
  print_checks "$(grep '^check' <<<"$snap" | cut -f2- || true)"
  echo "runs on ${head:0:9} (branch '$branch'):"
  runs_for_head | sort | uniq -c | sed 's/^/ /'
  exit 0
fi

no_ci_diagnostic() {
  cat >&2 <<EOF
no checks registered for $tag after ${REGISTER_TIMEOUT}s. Likely causes:
  - path-filtered CI: no workflow job matches this diff (.github/workflows)
  - stacked PR: base '$base' — CI only runs against main; retarget the PR
  - the last push was made with GITHUB_TOKEN (e.g. story-baseline
    auto-commit), which never triggers workflows — push any commit to kick CI
EOF
}

# ---- checks mode -----------------------------------------------------------

# empty_deadline is set while the check list is empty and cleared the moment
# checks appear, so only CONSECUTIVE emptiness exhausts REGISTER_TIMEOUT.
empty_deadline=""
partial_deadline=""
superseded_said=""
while true; do
  snap="$(snapshot)"
  assert_head "$(awk -F'\t' '$1=="head"{print $2}' <<<"$snap")"
  mergeable="$(awk -F'\t' '$1=="mergeable"{print $2}' <<<"$snap")"
  checks="$(grep '^check' <<<"$snap" | cut -f2- || true)"

  if [[ -z "$checks" ]]; then
    # A conflicted PR gets NO pull_request CI at all: no run will ever
    # register, so waiting out the timeout only delays the news. (mergeable
    # is UNKNOWN while GitHub recomputes it after a push; only the settled
    # CONFLICTING verdict short-circuits.)
    if [[ "$mergeable" == "CONFLICTING" ]]; then
      echo "PR #$num is CONFLICTING — GitHub runs no pull_request CI on a conflicted PR." >&2
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

  n_fail="$(grep -cE '^(fail|cancel)' <<<"$checks" || true)"
  n_pending="$(grep -c '^pending' <<<"$checks" || true)"

  if ((n_fail > 0)); then
    # Fail fast on a real failure. A red whose runs carry no real failure is a
    # supersede (a cancelled run); wait for whatever replaces it instead of
    # sending the caller to an empty log.
    if head_has_a_real_failure || grep -q '^fail' <<<"$checks"; then
      print_checks "$checks"
      echo "$tag: FAILED." >&2
      exit 1
    fi
    # Cancelled with nothing left to run: no replacement is coming.
    if ((n_pending == 0)) && head_runs_all_completed; then
      print_checks "$checks"
      echo "$tag: cancelled, and every run on it has finished — nothing is replacing it." >&2
      exit 1
    fi
    if [[ -z "$superseded_said" ]]; then
      echo "superseded: no run on ${head:0:9} failed — the red checks are a cancelled run." >&2
      echo "waiting for its replacement." >&2
      superseded_said=1
    fi
    partial_deadline=""
    sleep "$POLL_INTERVAL"
    continue
  fi

  if ((n_pending > 0)); then
    sleep "$POLL_INTERVAL"
    continue
  fi

  # A green is only a green once every run on this head has finished. The
  # rollup can be all-pass while the real CI run is still queued.
  if ! head_runs_all_completed; then
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

  # Re-read once more: the verdict must be about the pinned head.
  assert_head "$(view headRefOid .headRefOid)"
  print_checks "$checks"
  echo "$tag: green."
  exit 0
done
