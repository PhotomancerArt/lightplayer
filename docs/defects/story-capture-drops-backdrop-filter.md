# Story capture bakes glass surfaces without their blur

**Status:** fixed (2026-08-30, PR #479 — conditional `captureBeyondViewport`)
**Filed:** 2026-08-30 (design-language refresh, PR #467)

## Mechanism

`lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs` captures every
story via CDP `Page.captureScreenshot` with `captureBeyondViewport:
true` — unconditionally, even though `fitViewportToStory` has already
sized the viewport to the story. Chromium silently drops
`backdrop-filter` from beyond-viewport captures, so every glass surface
(`.ux-glass-panel` sheets and detail popovers, the merged-outline
popover's `.ux-popover-glass` layer, gallery card bars) bakes into
baselines as a flat translucent fill with **no blur** — not what a user
sees.

## Why it is not currently failing anything

Captures are self-consistent: both the old and new baselines lack the
blur, so diffs remain deterministic and the visual-regression contract
still catches unrelated drift. The defect is coverage, not flake — blur
regressions on glass surfaces are invisible to CI.

## Fix (PR #479)

A plain flip to `false` turned out to be wrong: `fitViewportToStory`
deliberately RESTORES the base 760px viewport before the shot (so
viewport-keyed layout growth is not baked into tall baselines), which
leaves 290/1940 baseline stories taller than the viewport at capture
time (up to 5162px) — with the flag off they truncate at the fold. The
landed fix computes the flag per shot: `false` when the capture clip
fits the viewport (1650/1940 stories, including every glass surface —
glass is overlays-only by design), `true` only when the clip overflows
(tall stories keep the previous behaviour exactly).

Verified locally (macOS): full 1940-story A/B against the unpatched
script was 1924/1940 byte-identical, the remainder being run-to-run
churn (same-script double-capture differed on 11/160 in the same story
families); no truncation anywhere; glass blur present in captures.
Caveat: this macOS Chrome does NOT exhibit the drop (flag true/false
byte-identical), so the drop is environment-dependent — the pinned CI
Chrome's re-baseline on PR #479 is the authoritative readout, via the
normal drift-comment merge-is-acceptance flow. Either way the
conditional flag pins the more correct capture request.

## References

- Verified during the Aurora design-language refresh: CDP
  `captureBeyondViewport: true` drops backdrop-filter even at viewport
  size (P2/P5 findings, plan 2026-08-29-1536-design-language-refresh).
