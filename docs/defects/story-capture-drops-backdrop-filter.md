# Story capture bakes glass surfaces without their blur

**Status:** open
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

## Fix shape (untried)

Flip `captureBeyondViewport` to `false`; the viewport is already fitted
per story, so the flag should be redundant. Risks to check before
trusting it: stories whose content overflows the fitted viewport
(truncation), and the capture-wedge/concurrency lore in
`docs/adr/2026-07-26-ci-canonical-story-capture.md` and the
story-capture memory files. A full re-baseline follows whichever way it
lands (merge-is-acceptance).

## References

- Verified during the Aurora design-language refresh: CDP
  `captureBeyondViewport: true` drops backdrop-filter even at viewport
  size (P2/P5 findings, plan 2026-08-29-1536-design-language-refresh).
