---
status: open
found: 2026-07-27      # how: ci (investigating a tolerated high-amplitude diff)
area: lpa-studio-web/scripts/studio-story-pngs.mjs (baseline check) + .github/workflows/pre-merge.yml
class: stand-in-divergence
related:
  - 2026-07-27-code-editor-gutter-misaligned.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-canonical-story-capture.md
  - ../adr/2026-07-26-ci-story-auto-commit.md
---
# The story check tolerates any diff by pixel count, and then discards it

**Symptom** — Run 30309630982 reported two baselines as "within tolerance
(informational)":

```
studio__home__home-gallery__opening-a-project__md.png (166/444960 px (0.037%) exceed Δ64 [840 any-diff, max Δ199])
studio__home__home-gallery__opening-a-project__lg.png (190/667440 px (0.028%) exceed Δ64 [894 any-diff, max Δ191])
```

The job passed, no baseline churned — and the fresh PNGs were thrown
away, because the `story-images-fresh` artifact is only uploaded when the
check *fails*. The run 32 minutes later on the same branch reproduced
those baselines byte-identically, so both renders are reachable and the
evidence for the differing one no longer exists anywhere. `max Δ199` on
this theme is the amplitude of a glyph appearing, not of antialiasing.

**Root cause** — `compareBaselines` reduces "does this look the same" to a
single statistic: the count of pixels whose per-channel delta exceeds
`significanceDelta` (64), failed against `maxSignificantPixelRatio`
(0.0005 of the image). The stand-in models *how many* pixels moved and
nothing about *how far* any of them moved, or whether they are clustered.
Consequences:

- A one-pixel Δ255 regression passes forever. So does a 165-pixel one at
  720×618. There is no amplitude ceiling and no cluster/locality term.
- Because the informational bucket cannot fail the job, it also cannot
  trigger the artifact upload, so the only durable record is one line of
  log text — counts, no pixels. The code-editor defect
  ([2026-07-27-code-editor-gutter-misaligned](2026-07-27-code-editor-gutter-misaligned.md))
  was diagnosable precisely because it *exceeded* the ratio and its fresh
  PNG survived as an artifact. The same investigation on a tolerated diff
  dead-ends.
- The ratio is per-image, so the same absolute defect is judged more
  leniently at `lg` than at `sm` (0.028% vs 0.042% for identical pixel
  counts — visible in run 30310410368's numbers).

**Fix** — mitigated 2026-07-27 (`this change`); the gate itself remains
count-only, so the entry stays open. What landed:

1. **Evidence retention, unconditional on the verdict**: whenever a
   complete check has a non-empty tolerated set, the workflow uploads
   `story-images-tolerated` — fresh **and** committed-baseline variants
   of just the tolerated files plus the manifest (a few hundred KB,
   14-day retention). Deliberately a different artifact name from
   `story-images-fresh`: `story-pull` selects runs by that artifact's
   presence, and a passing run must never shadow a real drift capture.
   The manifest gained a `tolerated` name list to drive it
   (`story-apply-refresh` ignores the field by construction).
2. **Warn-only amplitude line**: a tolerated file with
   `significantPixels > 0` — under the ratio but containing deltas the
   significance test itself calls real — prints a loud WARNING naming
   this entry. The condition is principled, not a new tunable: every
   benign churner class in the calibration below diffs at **0**
   significant pixels, and the first post-#153 healthy run's tolerated
   set topped out at Δ6 (1036 byte-identical / 5 tolerated, all Δ≤6) —
   the Δ199 outlier is thirty times outside the benign class.

Still open, in promotion order once a few real runs have confirmed the
boundary:

1. Promote the warn line to a gate (fail on tolerated-with-significant),
   with the retained artifacts as the calibration data.
2. A cluster/locality term, and printing the **significant/any-diff
   ratio** in the summary line — the cheap class discriminator measured
   below. Both are single-story-calibrated so far.

**Fingerprint calibration** — synthesised on the pinned story build in a
Linux container that reproduces CI's exact layout (720×618 / 1080×618 /
390×1080; macOS does not), by injecting one perturbation at a time into
`studio/home/home-gallery/opening-a-project` and diffing against the
unperturbed capture:

| perturbation | any-diff | >Δ64 | max Δ | significant |
| --- | --- | --- | --- | --- |
| compositor layer promotion (`will-change`, `translateZ`, `opacity:.999`) | 8 630 | 0 | **2** | 0% |
| sub-pixel text jitter (0.1–0.3px) | 1 320–4 322 | 1–89 | **≤123** | 0–2% |
| sub-pixel jitter, single text run | 976 | 62 | 123 | 6% |
| whole-pixel shift, one text run | 284–1 249 | 174–753 | 176–229 | **57–67%** |
| whole-pixel shift, whole text block | 11 334 | 879 | 229 | 8% |
| one text line hidden | 520 | 370 | 219 | **71%** |
| CSS transition storm, captured mid-flight | 46 645 | 0 | 35 | 0% |
| **the CI diff being explained** | **840** | **166** | **199** | **20%** |

Two things fall out. The known churner class (version-badge,
shader-face) is the layer/raster row: **Δ≤2, 0% significant** — thirty
times below the diff in question, so "it is the usual raster noise" is
not available as an explanation here. And the significant fraction
separates the classes cleanly: ≤8% is rasterisation, >50% is content.
The CI diff sits at 20% with content-class amplitude, matching no single
synthetic class — a broad faint halo plus ~166 hard pixels.

**Regression coverage** — none automated. A test would assert that a
synthetic one-pixel Δ255 change fails the check; today it passes (with a
WARNING since the mitigation — verified manually on a live filtered
check: a 9-px Δ200 patch warns and names this entry, a 9-px Δ30 patch
stays silent, a pristine capture is byte-identical).

**Lesson** — A tolerance threshold is a stand-in for a human looking at
the image, and it inherits the burden of proof for everything it
suppresses. This one suppresses on a dimension (count) that is
independent of the dimension that makes a diff interesting (amplitude,
locality), so the cases it most needs to surface — a small, high-contrast,
localised change, which is what most real single-widget regressions look
like — are exactly the ones it hides best. Worse, suppression here is
*destructive*: the informational bucket cannot fail the job, the artifact
upload is keyed to failure, and so the pixels are gone. Whatever the
gate's policy, the evidence retention must not be keyed to the gate's
verdict — you cannot investigate what the check already deleted.
