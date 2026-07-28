---
status: open
found: 2026-07-28      # how: ci (PR #163 baseline auto-refresh ping-pong)
area: lpa-studio-web story capture — auto-generated `overview` composite stories
class: nondeterministic-capture
related:
  - 2026-07-27-code-editor-gutter-misaligned.md
  - 2026-07-27-story-check-tolerance-ignores-amplitude.md
  - ../debt/story-capture-pipeline.md
---
# Overview composite captures race in-page transitions and flip-flop

**Symptom** — On PR #163, consecutive `validate-stories` runs ping-ponged
`studio__roster__roster-card__overview__lg.png` between two renderings
(766823 ↔ 775606 bytes, commits 2201e9041 → 05eed3900), immediately after
`studio__home__home-gallery__opening-a-project__sm.png` had ping-ponged the
same way (4714e8789 → 4529c52c8, that one since fixed by the
`StaticThumbPreviews` story gate). `base__code-editor__overview__sm.png`
flipped in the same window (01f324923). Every auto-refresh commit
retriggers CI, whose capture flips a composite again — an unbounded churn
loop as long as runs keep being approved.

**Diagnosis** — In the roster flip, the "Op overlay indeterminate" and
"Unresponsive" (troubleshoot sheet) states alternate between fully-drawn
and mid-transition (content blurred/faded, overlay/sheet chrome absent).
The per-story captures of the same states are stable — only the
`overview` composites flip. Composites render every story of a component
on one page; under that load the sheet/overlay blur transition has not
settled when the ready-wait fires, and the two-shot stability check can
land both shots in the same unsettled frame.

**Class signal** — both currently-known flip-floppers are `overview`
composites (`roster-card`, `code-editor`). Candidate fixes, for a
follow-up: disable CSS transitions/animations in story-png mode (a
`story-png` root class or `prefers-reduced-motion` emulation in the
capture script), or drop `overview` composites from the committed
baseline set (they are galleries of already-captured states).

**Workaround** — bounded approvals of the auto-refresh runs; stop
approving when a composite flips twice, and merge on the substantive-run
evidence.
