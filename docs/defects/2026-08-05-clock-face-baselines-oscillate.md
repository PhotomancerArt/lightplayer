---
status: open
found: 2026-08-05      # how: ci
area: lpa-studio-web story capture (clock-face stories)
class: nondeterministic-capture
related:
  - 2026-07-28-overview-composite-capture-races.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-story-auto-commit.md
---
# Clock-face story baselines alternate between two renderings, one bot commit per CI run

**Symptom** — On PR #349 (a change touching no clock-face code), the
`validate-stories` auto-commit fired on two consecutive runs and moved the
same baselines **back and forth between two states**:

| story | main | after run 30984999205 | after run 30985988371 |
|---|---|---|---|
| `clock-face__crowd__lg` | `66a066b8` | `d7712f79` | `66a066b8` |
| `clock-face__shared__sm` | `cdd2a57b` | `f52a90a1` | `cdd2a57b` |

The third column is **byte-identical to main** — same blob hash, not merely
the same size. Both runs were green, and both were the pinned CI environment
the baselines are canonical for. `clock-face__default__md`,
`default__lg` and `shared__lg` moved too.

**Root cause** — Not yet diagnosed; filed open. What the evidence already
rules out is drift *settling*: a settling baseline converges, and a blob hash
returning exactly to its predecessor is an oscillation between two reachable
renderings. The clock-face face is the surface that could not be frozen by
pausing rAF and instead self-freezes per-paint from inside the canvas (the
mechanism introduced with clock face v2, PR #335); the natural hypothesis is
that the self-freeze admits two settling points — e.g. a paint that lands
before versus after some first-frame state — and the capture photographs
whichever one that run reached. **Confirm with pixels before believing that**;
this pipeline has repeatedly produced "obviously a settling race" diagnoses
that the pixel diff overturned (see the debt entry).

**Fix** — none yet.

**Regression coverage** — none: the auto-commit design means a
nondeterministic baseline produces a *passing* run plus a commit, never a red
check. That is the reason this went unnoticed until two runs happened to land
on one branch close enough together to compare. A guard would need to compare
consecutive captures of the same tree, which nothing does today.

**Lesson** — Auto-committing refreshed baselines converts a nondeterministic
capture from a loud failure into silent churn: every PR that trips the studio
path filter gets a bot commit, each one looking like ordinary drift, and the
branch never reaches a stable head. The ADR accepts "the green run sits one
commit behind the bot's" as a merge-time tradeoff, which is sound for *real*
drift; under oscillation that gap never closes, because the next run disagrees
with the one that just wrote. When judging whether a baseline refresh is
benign, **compare blob hashes against the previous refresh, not just against
main** — a hash that returns to an earlier value is the signature this class
leaves, and file sizes alone can hide it.
