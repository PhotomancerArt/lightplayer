# ADR 2026-06-18: Studio Story PNG Baselines

## Status

Accepted.

## Context

The native Studio storybook can generate PNGs for each component story. The
initial decision kept those PNGs local-only to avoid repository bloat and
browser-dependent screenshot churn.

During early Studio UI work, the most valuable developer experience is being
able to see which component stories changed in the same commit as the source
change. LightPlayer is currently a small, solo-developed project, and the
initial story PNG set is modest enough that the visibility is worth trying
before investing in CI visual-regression infrastructure.

## Decision

Commit a curated baseline PNG set for `lpa-studio-web` stories.

- Committed baselines live under `lp-app/lpa-studio-web/story-images/`.
- Scratch review PNGs stay gitignored under
  `lp-app/lpa-studio-web/story-images/.scratch/`.
- Fresh check output lives under gitignored
  `lp-app/lpa-studio-web/story-images/.new/`.
- `just studio-story-baselines` regenerates the committed baseline set.
- `just studio-story-check` compares fresh story PNGs to committed baselines
  without updating them.
- `just studio-story-baselines-if-needed` runs baseline generation only when
  non-generated files under `lp-app/lpa-studio-web/` changed since `HEAD`.
- Story captures are clipped to the marked story canvas content rather than the
  full browser viewport.
- Baseline and check commands require `oxipng` so fresh captures are normalized
  the same way as committed images.
- Agents should run the helper before committing Studio UI work and include
  changed baseline PNGs in the same commit.
- Do not use an auto-mutating Git hook for now.

## Consequences

Studio UI commits can show source changes and visual story changes together,
which makes review much easier while the UI foundation is still moving quickly.

The tradeoff is that binary files will enter the repo and may churn when
browser rendering, fonts, or story fixtures change. To keep that acceptable, the
baseline set should stay curated, volatile content should be avoided in stories,
and baseline updates should remain intentional.

CI can later run `just studio-story-check`, but CI should not commit updated
PNGs. If the image set grows substantially or churn becomes painful, revisit
this decision before adding Git LFS or hard visual gates.

## 2026-06-23 Addendum: Responsive Baseline Matrix

The project editor foundation makes responsive layout a first-order part of the
Studio UI. The accepted baseline set now captures each story at `sm`, `md`, and
`lg` viewports. Baseline filenames include the viewport id, such as
`studio__editor-shell__sm.png`, so check mode can compare the full story by
viewport matrix and report changed, new, or removed images precisely.

This increases baseline count and disk usage, but keeps responsive regressions
visible while the editor shell, node tree, and device rail are still taking
shape. The same `oxipng` normalization remains required for baseline and check
modes.
