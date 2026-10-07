---
status: carried
since: 2026-10-07      # the day `edited` left pre-merge's pull_request types
logged: 2026-10-07
area: CI / pre-merge.yml pull_request trigger
related:
  - ../../.github/workflows/pre-merge.yml
  - ../defects/2026-07-30-stacked-prs-get-no-ci.md
  - ../../scripts/watch-pr.sh
---
# A stacked PR retargeted to main gets no CI until its next push

**Shape** — pre-merge's `pull_request` trigger filters on `branches: ["main",
"feature/*"]`, so a PR based on a `claude/*` branch (a stacked PR) gets no CI
at all. Retargeting it to main changes its base, which GitHub reports as an
`edited` event, and `edited` is not in the default types (`opened`,
`synchronize`, `reopened`). It was added for exactly this in #195 and removed
on 2026-10-07, because `edited` also fires on every PR title and body edit,
and each of those starts a fresh run that cancels the one in flight. So a
retargeted PR now has **no CI run** until something else triggers one.

A job-level `if:` on `github.event.changes.base` was not used instead: a
body-edit run would post its jobs as *skipped* checks, newer than the real
run's, and a skipped check reads as passing, so it can sit over an older red
as a false green.

**Carrying cost** — a retargeted PR looks clean (an empty checks list, or
only the CLA) and is mergeable while never validated. Main has no branch
protection or required checks, so nothing stops the merge. Stacked PRs are
uncommon, which is why this is carried rather than fixed.

**Workarounds**
- After retargeting, push any commit (`git commit --allow-empty -m "ci: kick
  CI after retarget"`, then push). The push is a `synchronize` event.
- Or close and reopen the PR (a `reopened` event); this is how #195 was
  recovered in 2026-07 (`docs/defects/2026-07-30-stacked-prs-get-no-ci.md`).
- `gh workflow run` is **not** available: `pre-merge.yml` has no
  `workflow_dispatch` trigger.
- Do not push with `GITHUB_TOKEN`; such a push triggers no workflow either.

**Safety net** — `scripts/watch-pr.sh` on a PR with no `CI` run exits 2 after
`WATCH_PR_REGISTER_TIMEOUT` (default 600 s) and names the cause: "stacked PR:
base '<base>' — CI only runs against main; retarget the PR". It also prints
a `note:` at start when the base is not main. The agent merge path therefore
cannot read a missing CI as green; a human reading the checks list still can.

**Incident log**
- 2026-07-30 — #195, the original: stacked on #194, retargeted to main after
  #194 merged, never validated; recovered with a close/reopen. This is why
  `edited` was added.
- 2026-10-05 — #987, the cost that drove its removal: a PR body edit mid-CI
  started a new run and cancelled the one in flight, losing a full CI cycle
  (similar cancelled runs showed on #972).

**Exit criteria** — retargeting a PR to main starts CI without a body or
title edit also doing so: a trigger that fires on a base change only (GitHub
Actions has no such activity type today; `edited` with a base-change filter
costs the false-green risk above), or a stacked-PR flow that retargets and
pushes in one step. Until then, retire only if stacked PRs stop being used.
