#!/usr/bin/env bash
# Name the PRs merged since the last green main — the suspects when a
# push-to-main CI run goes red.
#
# CI validates each PR against its own branch, never against the merge result,
# so two independently-green PRs can merge into a red main
# (docs/debt/two-green-prs-can-red-main.md). A main push forces every gate on,
# which makes it the canary; this script makes the canary say WHERE to look,
# so the diagnosis does not start on whichever PR merged last.
#
# Usage:
#   scripts/ci/red-main-suspects.sh [<head-rev>] [--since <green-sha>]
#
#   <head-rev>   the red commit (default HEAD)
#   --since      skip the lookup and use this sha as the last green main
#
# The last green main is found by walking <head-rev>'s FIRST-PARENT line
# backwards (main's own landings: one per merge commit, squash, or direct
# push) and asking the API, per sha, for a successful `pre-merge.yml` push
# run on exactly that commit. It asks by sha rather than listing recent runs:
# main runs are concurrent, so the newest green run by time can be a LATER
# push than the red one, and `gh run list --limit 30` was seen (2026-09-24)
# to return a page three weeks stale on one call and the right page on the
# next. Gives up (exit 0, warning) after MAX_WALK landings (default 50).
#
# Needs full history (checkout fetch-depth: 0) and, without --since, `gh`
# with actions:read (GH_REPO names the repo in CI). Writes a markdown section to $GITHUB_STEP_SUMMARY when
# set and prints an `::error::` annotation under GitHub Actions. Exits 0
# whenever it could name suspects: the run is already red; this step
# explains it rather than adding a second failure.
set -euo pipefail

head_rev=HEAD
since=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --since) since="${2:?--since needs a sha}"; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) head_rev="$1"; shift ;;
  esac
done

head_sha=$(git rev-parse --verify "${head_rev}^{commit}")

max_walk=${MAX_WALK:-50}
if [[ -z "$since" ]]; then
  api_errors=0
  while read -r sha; do
    if ! n=$(gh api \
      "repos/{owner}/{repo}/actions/workflows/pre-merge.yml/runs?head_sha=${sha}&event=push&status=success&per_page=1" \
      --jq '.total_count' 2>/dev/null); then
      api_errors=$((api_errors + 1))
      continue
    fi
    if [[ "$n" =~ ^[1-9] ]]; then
      since="$sha"
      break
    fi
  done < <(git rev-list --first-parent --max-count="$max_walk" "${head_sha}^")
  if [[ -z "$since" ]]; then
    msg="No green pre-merge push run on any of the last ${max_walk} first-parent ancestors of ${head_sha:0:10} (${api_errors} API errors); cannot name suspects."
    echo "$msg" >&2
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then echo "::warning::$msg"; fi
    exit 0
  fi
fi
since=$(git rev-parse --verify "${since}^{commit}")

# First-parent walk: each commit on main's own line is one landing — a merge
# commit ("Merge pull request #N …"), a squash ("… (#N)"), or a direct push.
rows=()
prs=()
while IFS=$'\t' read -r sha subject; do
  [[ -z "$sha" ]] && continue
  pr=""
  if [[ "$subject" =~ ^Merge\ pull\ request\ \#([0-9]+) ]]; then
    pr="${BASH_REMATCH[1]}"
  elif [[ "$subject" =~ \(\#([0-9]+)\)$ ]]; then
    pr="${BASH_REMATCH[1]}"
  fi
  rows+=("$sha"$'\t'"$pr"$'\t'"$subject")
  if [[ -n "$pr" ]]; then prs+=("#$pr"); fi
done < <(git log --first-parent --format='%H%x09%s' "$since..$head_sha")

count=${#rows[@]}
pr_list=""
if [[ ${#prs[@]} -gt 0 ]]; then
  pr_list=$(printf '%s, ' "${prs[@]}")
  pr_list=${pr_list%, }
else
  pr_list="(no PR numbers; direct pushes only)"
fi

summary=$(
  echo "## Red main: what landed since the last green main"
  echo
  echo "Last green \`pre-merge\` run on main: \`${since:0:10}\`. Red commit: \`${head_sha:0:10}\`."
  echo "Each PR below was green on its own branch; CI never built them together."
  echo "Reproduce on a clean \`origin/main\` before blaming the newest one"
  echo "(docs/debt/two-green-prs-can-red-main.md)."
  echo
  echo "| commit | PR | subject |"
  echo "|---|---|---|"
  for row in ${rows[@]+"${rows[@]}"}; do
    IFS=$'\t' read -r sha pr subject <<<"$row"
    echo "| \`${sha:0:10}\` | ${pr:+#$pr} | ${subject//|/\\|} |"
  done
)

echo "$summary"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  echo "$summary" >>"$GITHUB_STEP_SUMMARY"
fi
if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
  echo "::error title=Red main suspects::${count} landing(s) since the last green main (${since:0:10}): ${pr_list}. Any of them, or their combination, may be the cause."
fi
