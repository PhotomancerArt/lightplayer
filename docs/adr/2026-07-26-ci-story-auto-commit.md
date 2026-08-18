# ADR 2026-07-26: CI Auto-Commits Story Baseline Refreshes

## Status

**Superseded entirely by
[ADR 2026-08-17: Story Baselines Live in a Companion Repo](2026-08-17-story-baselines-companion-repo.md):**
CI no longer commits anything to this repo — baselines moved to the
companion stories repo and merging a PR is acceptance. Kept for the
history of the auto-commit era (2026-07-26 → 2026-08-17).

Accepted. Supersedes, in part,
[ADR 2026-07-26: CI-Canonical Story Capture](2026-07-26-ci-canonical-story-capture.md):
the capture environment, pinning, path gate, artifact, and local-capture
demotion all stand; only the "CI never commits — the branch owner pulls and
commits" delivery loop is replaced.

## Context

The CI-canonical capture ADR shipped with a manual delivery loop: on drift the
`validate-stories` job failed, the branch owner ran `just studio-story-pull`,
reviewed, committed, and pushed for a green re-run. The "CI must not commit"
premise was a review misfire — the actual intent was for CI to commit refreshed
baselines directly, with the change called out in the PR. In practice the loop
cost a full extra push→CI round trip (~20–30 min) per UI-touching PR, for a
commit whose content the branch owner never edits (the fresh set is
authoritative by construction; review happens on the PNG diff either way).

## Decision

On story drift with a **complete** capture (`.check-complete` sentinel) from a
**same-repo PR**, `validate-stories`:

1. Commits the fresh baseline set to the PR head branch as
   `github-actions[bot]`, using the same delete-then-copy replacement semantics
   as `studio-story-pull` (story removals propagate), and pushes with the
   job's built-in `GITHUB_TOKEN`. Never `--force`, never a rebase.
2. **Passes** — drift existed, but CI resolved it in the same run.
3. Posts a sticky PR comment (`story-drift-comment.mjs`, keyed by a hidden
   HTML marker, updated in place on later drift runs): change counts, a link
   to the bot commit, up to 8 inline before/after thumbnails via
   `raw.githubusercontent.com` (the repo is public), and the rest of the file
   list in a collapsed section. The PR's Files-changed PNG viewer remains the
   full review surface. A comment failure never fails the job — the push
   already landed, and with no follow-up run a red here would stay red.

Every other drift/crash path keeps the previous behavior — fail, upload the
`story-images-fresh` artifact, and leave `just studio-story-pull` as the
manual fallback: fork PRs (read-only token), push races (non-fast-forward
rejection), crashed captures (no sentinel), and drift found on `main` pushes.

## The token tradeoff (load-bearing)

Pushes made with the built-in `GITHUB_TOKEN` deliberately do not trigger
workflow runs. Two consequences, both accepted:

- **No workflow-trigger loop is possible**, structurally — not merely
  guarded against.
- **The bot commit (the new PR head) gets no CI run of its own.** The green
  run sits one commit behind the head. This is acceptable because the repo has
  no required status checks, and the commit is images-only, produced and
  validated by the very job that pushed it. Anything that "watches CI until
  green" must know the green run is on the pre-bot SHA, and the local branch
  needs `git pull` after a drift run before pushing again.

## Alternatives considered

- **Keep the manual pull loop** (the superseded decision). The conflict
  potential it feared is, in practice, a `git pull` — the bot commit touches
  only `story-images/`, and a genuine race rejects the push and falls back to
  the artifact path.
- **PAT / GitHub App token** so the bot push triggers a real run and the head
  SHA goes green. Rejected for now: a secret to create and rotate, a full
  ~20–30 min CI re-run per drift that validates nothing new, and a loop that
  terminates only by capture determinism rather than by construction. Revisit
  if branch protection / required status checks are ever enabled — at that
  point the checkless head becomes a merge blocker and this decision must be
  amended.
- **Calling changes out by editing the PR description.** A comment is less
  invasive (descriptions are human-authored) and supports update-in-place.

## Consequences

- UI-touching PRs lose the manual round trip: push → drift run auto-commits →
  green, with the delta summarized on the PR.
- After any drift run, the branch owner (human or agent) must `git pull`
  before pushing; a forgotten pull surfaces as a non-fast-forward rejection of
  their own push, not silent damage.
- **Cross-branch baseline conflicts make the PR silently run-less** (observed
  live during rollout, on this feature's own PR): the refresh commits a
  *full* fresh set — including within-tolerance byte wobble — so two branches
  that both refreshed can conflict on PNGs that never meaningfully changed.
  GitHub then cannot build the PR merge ref (`mergeable_state: dirty`) and
  **stops creating `pull_request` runs entirely**; pushes appear to be
  ignored. Remedy: merge `main` and resolve every conflicted PNG by taking
  **main's bytes** — if main's copy is genuinely stale, the next capture
  re-drifts and the bot re-commits the fix (self-healing). If wobble-driven
  conflicts recur, the structural fix is refreshing only stories that fail
  tolerance (plus adds/removes) instead of the full set — the check already
  computes per-file verdicts; it would need to emit a machine-readable drift
  list for the workflow to consume.
- The `validate-stories` job holds `contents: write` +
  `pull-requests: write` permissions (repo default is read-only).
- A deliberate browser/oxipng pin bump PR now self-heals: push the bump and CI
  commits the regenerated set to the branch.
