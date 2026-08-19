# ADR 2026-08-17: Story Baselines Live in a Companion Repo; Merge Is Acceptance

## Status

Accepted. Supersedes
[ADR 2026-07-26: CI Auto-Commits Story Baseline Refreshes](2026-07-26-ci-story-auto-commit.md)
entirely, and the storage/delivery half of
[ADR 2026-07-26: CI-Canonical Story Capture](2026-07-26-ci-canonical-story-capture.md)
and [ADR 2026-06-18: Studio Story PNG Baselines](2026-06-18-studio-story-png-baselines.md)
(the pinned capture environment, CI-canonical rule, viewport matrix, and
story-id naming all stand; committed baseline files do not).

## Context

Committed baseline PNGs caused a documented, recurring failure class:

- Auto-commits to PR branches created bot-head commits; merging the wrong
  head stranded work, and the bot commit's run-approval requirement stalled
  gated workflows.
- Bot-baseline heads and main's own baseline commits chained into add/add
  PNG conflicts; a conflicted PR silently gets **no** `pull_request` CI at
  all, and nondeterministic stories (clock faces, tape transport) oscillated
  and never converged.
- PRs merged without a refresh left `main`'s validate-stories in a failing
  "watch state".
- Story-image blobs were **0.97 GiB of the repo's 1.07 GiB pack** — nearly
  the entire clone weight, growing with every refresh.

The PR-comment review surface (changed-story thumbnails) is the
highest-value review evidence and had to survive the change.

## Decision

Baselines live in the public companion repo
**`PhotomancerArt/lightplayer-stories`**; nothing PNG is committed to this
repo.

- **Snapshot = one git commit**: `images/` (the full capture set) +
  `manifest.json` (source sha, run URL, pinned tool versions, stale +
  tolerated lists). Git content-addressing dedups unchanged PNGs across
  snapshots; the marginal cost of a snapshot is its changed files.
- **Snapshot commits are parented on the snapshot they were compared
  against**, so GitHub's compare view renders exactly the changed PNGs with
  swipe/onion-skin — this replaces the Files-changed PNG viewer as the full
  review surface.
- **Refs**: `sha-<full-main-sha>` per captured main commit; `pr-<number>`
  (force-updated) per PR run; `latest` always tracks the newest main
  snapshot (the root README embeds its raw URLs; GC-exempt).
- **Merge-to-main is acceptance.** A PR run resolves its baseline as the
  nearest captured first-parent ancestor of the merge-ref's first parent,
  captures, pushes a `pr-<n>` snapshot, and posts a sticky comment (counts,
  ≤8 before/after thumbnails via `raw.githubusercontent.com` commit-sha
  URLs, compare link). **Visual changes pass CI**; the job fails only on a
  crashed/incomplete capture or when no captured ancestor exists within the
  lookup walk (50 first-parent commits). Merging accepts the changes; the
  main-push run then publishes `sha-<merge-sha>` (parented on its
  predecessor's snapshot) and reports the delta vs parent in its step
  summary — the standing churn/nondeterminism monitor.
- **Auth**: CI pushes snapshots over SSH with a write deploy key
  (`STORIES_DEPLOY_KEY` secret). Fork PRs have no secret: they still
  capture and compare, and fall back to the `story-images-fresh` artifact
  instead of a snapshot push + comment.
- **Retention**: a weekly GC workflow in the stories repo deletes `pr-*`
  refs older than 30 days and `sha-*` refs older than 180 days while always
  keeping the newest 50 `sha-*` refs (the lookup floor); `latest` and the
  default branch are never touched. Object reclamation is GitHub's
  background gc; comment images on PRs older than the retention window are
  best-effort.
- **Local flows**: `just studio-story-check` auto-fetches the branch's
  baseline snapshot into `target/story-baselines/current`;
  `studio-story-baselines` writes a local, never-committed set;
  `studio-story-pull` is deleted. Local output remains non-authoritative
  (macOS rendering ≠ pinned CI).

## Alternatives considered

- **Orphan branch in this repo** — solves conflicts but not weight: snapshot
  objects stay in every clone's pack and CI checkout, and the 0.97 GiB
  problem keeps growing in place.
- **Object store (S3/R2/B2)** — paid infra + credentials, no content dedup
  without extra machinery, no compare view, and a separate hosting story for
  comment images. More moving parts for no additional capability at this
  scale.
- **GitHub Releases / Actions artifacts** — artifacts expire (≤90 d) and
  their URLs require auth (unusable in comments); releases store full
  uncompressed sets per snapshot and 1,800-asset uploads are API-hostile.

## Consequences

- Bot-head commits, add/add PNG conflicts, baseline oscillation-as-conflict,
  `[validate-stories]` commits, and run-approval stalls are structurally
  impossible — nothing writes to this repo.
- Review moves entirely to the sticky comment + compare link; a PR's diff
  never contains PNGs. Reviewers must read the comment before merging —
  CI green no longer implies "no visual changes".
- Nondeterministic capture now degrades comment signal (phantom rows)
  instead of blocking merges; the main-run delta summary makes churn
  observable per-commit. The FitReconcile/ready-gate determinism work
  remains load-bearing for signal quality.
- Historical PNG blobs still bloat clone packs until the history purge
  (planned separately; executed as P5 of the same plan, with its own gate).
- The stories repo is now infrastructure: deleting it, its deploy key, or
  the seed/retained refs blinds PR comparisons (remedy: any green main run
  re-seeds forward).

## Amendment 2026-08-18: history purge executed

The deferred purge ran as planned (P5 of the same plan): `git filter-repo`
stripped `lp-app/lpa-studio-web/story-images` and the pre-rename
`lp-app/lp-studio-web/story-images` from all history, and the rewritten
refs were force-pushed. Pack size went from 953 MiB to 42.6 MiB; a fresh
clone carries zero story-PNG objects. Every commit SHA after the first PNG
commit changed (old main `3e53a042ea` → new main `8dab339274`); all 440
branches and 353 tags were rewritten in place (one deploy tag created
mid-window was re-pointed through the commit map afterwards — a
same-content duplicate of the new main tag). The stories repo's `sha-*`
keys were re-keyed to the rewritten SHAs. The old→new commit map lives in
the planning archive (`2026-08-14-1132-story-baselines-external/purge/`).
Pre-rewrite SHAs in older docs/ADRs are historical labels and no longer
resolve.
