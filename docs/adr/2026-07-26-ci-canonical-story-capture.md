# ADR 2026-07-26: CI-Canonical Story Capture

## Status

**Storage/delivery superseded by
[ADR 2026-08-17: Story Baselines Live in a Companion Repo](2026-08-17-story-baselines-companion-repo.md):**
the pinned capture environment, path gate, and CI-canonical rule stand;
committed baseline files, the fresh-set artifact delivery loop, and
`just studio-story-pull` do not.

Accepted; superseded in part by
[ADR 2026-07-26: CI Auto-Commits Story Baseline Refreshes](2026-07-26-ci-story-auto-commit.md) —
the "CI never commits / branch owner pulls and commits" delivery loop
(Decision points 3–4 below) is replaced by a direct auto-commit to same-repo PR
branches; the capture environment, pinning, path gate, and local-capture
demotion stand.

Supersedes the local-canonical capture premise of
[ADR 2026-06-18](2026-06-18-studio-story-png-baselines.md); that ADR's core
premise — curated baseline PNGs committed to the repo, visible in PRs — stands
unchanged.

## Context

Story baselines were captured locally: developers (usually agents) ran
`just studio-story-baselines-if-needed` before committing UI changes, and the
committed PNGs were whatever the local macOS Chrome rendered. Two structural
problems accumulated (chronicled in `docs/debt/story-capture-pipeline.md`):

- **The pipeline had no isolation.** Captures ran serially with whatever else
  the machine was doing; every recorded failure class was load- or
  environment-induced — CDP navigate timeouts under CPU/disk pressure, zombie
  headless Chromes starving retries, concurrency wedges on heavy sheets. A
  10–15 minute run per UI change, times retries, became the accepted norm,
  and the dev machine was unusable for anything heavy during it.
- **Enforcement was discipline, not a gate.** No CI job checked baselines at
  all. The `-if-needed` helper diffed only the working tree, silently skipping
  committed changes; shell piping masked failures more than once; cross-machine
  rendering (fonts, Chrome versions) produced a standing set of churner
  stories whose byte-noise had to be manually reverted on every capture.

The constraint that shaped the solution: baseline PNGs must stay in git and in
PRs (image diffs at review time are the point of the whole system), but CI
must not commit to the repo — bot commits add conflict potential and push
races.

## Decision

CI is the only canonical capture environment. The delivery loop keeps the
commit in the branch owner's hands:

1. A path-gated, blocking `validate-stories` job (`.github/workflows/
   pre-merge.yml`) builds the story bundle and runs `just studio-story-check`
   against the committed baselines on every studio-touching PR (and every main
   push, unconditionally).
2. The environment is pinned: x64 `ubuntu-24.04`, Chrome for Testing at an
   exact version, `oxipng` at an exact version, and fonts bundled with the app
   (`lp-app/lpa-studio-web/public/fonts/` — Inter + JetBrains Mono, static
   woff2) so rendering does not depend on OS font stacks. Bumping the browser
   or oxipng pin is a deliberate baseline-refresh PR.
3. On drift the job fails and uploads the full fresh capture set as the
   `story-images-fresh` artifact (7-day retention) with a job-summary story
   list. Check-mode captures are a complete set, so the artifact is always a
   complete baseline candidate — there is no separate "regenerate" mode.
4. `just studio-story-pull` downloads the branch's artifact, full-replaces
   `story-images/` (propagating story deletions), and stages the result. The
   branch owner reviews the PNG diff and commits. It warns (but proceeds) when
   the artifact's head SHA lags local HEAD.
5. Local capture is demoted to interactive scratch review: `pngs` and `check`
   modes accept story-id substring filters so small subsets are cheap;
   `baselines` mode rejects filters (a partial local regen would silently
   delete the rest of the set) and remains only as an emergency escape hatch
   whose output must not be committed.

## Alternatives rejected

- **CI commits refreshed baselines to the PR branch.** Rejected on conflict
  potential and push races; also blurs authorship of what lands. (A
  manually-triggered bot-commit variant remains a fallback if the pull
  round-trip proves annoying.)
- **A pinned capture container usable locally and in CI** (designed in an
  earlier unshipped plan). Its purpose was byte-identical local↔CI rendering;
  retiring local baselines removed that requirement. Containerizing remains
  the escalation path if runner-image updates ever churn captures despite the
  Chrome/font pinning.
- **Artifacts/PR comments only, no PNGs in git.** Loses the in-git history and
  in-PR image diffs that motivated committing baselines in the first place.

## Consequences

- UI-touching PRs pay a round trip: push → ~20–30 min job → `studio-story-pull`
  → commit → green re-run. The dev machine is free during it, and non-UI PRs
  skip the job entirely (path gate; `Cargo.lock` deliberately excluded with
  main-push always-run as the backstop).
- Baselines are Linux-rendered; local checks are non-authoritative. Visual
  review happens in the PR (or from pulled artifacts), which is where it
  belonged anyway.
- The pixel-tolerance check (2026-07-05 addendum to the 2026-06-18 ADR) is
  load-bearing across runner instances: the green re-run after a baseline
  commit ran on a different machine than the capture. Verified at cutover.
- Repo keeps carrying ~45 MB of PNGs and their churn history; Git LFS remains
  the 2026-06-18 ADR's revisit trigger, unchanged by this decision.
