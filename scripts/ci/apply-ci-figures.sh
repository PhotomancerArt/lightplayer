#!/usr/bin/env bash
# Apply the figure patches CI produced for a pull request.
#
#   scripts/ci/apply-ci-figures.sh [pr]     (default: the current branch's PR)
#
# Downloads every `figures-patch-*` artifact of the PR's latest CI run for its
# head commit (one per job whose figure check failed ONLY on moved figures —
# scripts/ci/figures-patch.py) and applies them to this checkout in one
# `git apply`, so several jobs' patches land together. Nothing is committed or
# pushed: review `git diff`, commit with the change that moved the figures,
# push.
#
# The patches were made on GitHub's merge ref (your head merged into main). If
# main moved a record since, a plain apply can fail; the script then fetches
# origin and retries with --3way, which merges from the blobs the patch names.
#
# See docs/chip-figures.md.
set -euo pipefail
cd "$(dirname "$0")/../.."

command -v gh >/dev/null || { echo "apply-ci-figures: needs the gh CLI" >&2; exit 2; }

pr="${1:-}"
if [ -z "$pr" ]; then
    pr="$(gh pr view --json number -q .number 2>/dev/null || true)"
    [ -n "$pr" ] || { echo "apply-ci-figures: no PR for this branch; name one: just apply-ci-figures <pr>" >&2; exit 2; }
fi

head="$(gh pr view "$pr" --json headRefOid -q .headRefOid)"
local_head="$(git rev-parse HEAD)"
if [ "$head" != "$local_head" ]; then
    echo "apply-ci-figures: warning: PR #$pr's head is ${head:0:10}, this checkout is at ${local_head:0:10}." >&2
    echo "apply-ci-figures: the patch is for the PR's head; applying anyway (it only touches the records)." >&2
fi

records=(lp-emu/esp/figures scripts/heap-budget-record)
if ! git diff --quiet HEAD -- "${records[@]}"; then
    echo "apply-ci-figures: the records already have uncommitted changes; commit or discard them first:" >&2
    git --no-pager diff --stat HEAD -- "${records[@]}" >&2
    exit 1
fi

# The newest pull_request run of the CI workflow on that exact head.
run="$(gh run list --workflow pre-merge.yml --commit "$head" --event pull_request \
    --json databaseId,status,conclusion -q '.[0] | "\(.databaseId) \(.status) \(.conclusion)"' 2>/dev/null || true)"
[ -n "$run" ] || { echo "apply-ci-figures: no CI run for ${head:0:10} yet" >&2; exit 1; }
read -r run_id status conclusion <<<"$run"
echo "apply-ci-figures: PR #$pr, head ${head:0:10}, run $run_id ($status${conclusion:+/$conclusion})"
if [ "$status" != "completed" ]; then
    echo "apply-ci-figures: the run is still going; applying the patches it has uploaded so far." >&2
    echo "apply-ci-figures: a chip job that has not finished may still add one — re-run this after it does." >&2
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if ! gh run download "$run_id" --pattern 'figures-patch-*' --dir "$tmp" 2>"$tmp/err"; then
    if grep -qi "no valid artifacts\|no artifact" "$tmp/err"; then
        echo "apply-ci-figures: run $run_id has no figure patch: no figure check failed on figures alone." >&2
        exit 1
    fi
    cat "$tmp/err" >&2
    exit 1
fi

patches=()
while IFS= read -r p; do patches+=("$p"); done < <(find "$tmp" -name figures.patch | sort)
for s in $(find "$tmp" -name summary.json | sort); do
    job="$(basename "$(dirname "$s")")"
    verdict="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["verdict"])' "$s")"
    echo "  ${job#figures-patch-}: $verdict"
done
if [ "${#patches[@]}" -eq 0 ]; then
    echo "apply-ci-figures: run $run_id produced no patch — the failures it saw are not figure moves (see the PR comment)." >&2
    exit 1
fi

if git apply --index "${patches[@]}" 2>"$tmp/apply.err"; then
    :
else
    echo "apply-ci-figures: plain apply failed ($(head -1 "$tmp/apply.err")); fetching origin and retrying with --3way" >&2
    git fetch -q origin
    git apply --3way "${patches[@]}"
fi

echo
git --no-pager diff --cached --stat -- "${records[@]}"
echo
echo "apply-ci-figures: applied ${#patches[@]} patch(es) from run $run_id and staged them."
echo "apply-ci-figures: review 'git diff --cached', commit with the change that moved them, and push."
