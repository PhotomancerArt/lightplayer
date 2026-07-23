---
status: fixed
found: 2026-07-22      # how: hardware-walk
fixed: rides this fix commit (branch claude/runtime-pool-m4, hash at commit time)
area: lpa-studio-web/base/popover
class: stand-in-divergence
related: ["docs/adr/2026-07-15-popover-svg-merged-outline.md"]
---
# Opening any popover grew its trigger's line box, reflowing the page a few px

**Symptom** — Opening a device card's detail popover (the "i" trigger at the
card header's right edge) made the CARD itself ~3.5px taller, visibly
reflowing the Home gallery grid. Measured against the app's compiled CSS:
closed header 41.0px / card 87.0px, open header 44.5px / card 90.5px. Every
`PopoverButton` consumer was affected, not just the device card.

**Root cause** — While a popover is open+attached, the trigger's visual
re-parents into the browser top layer and the in-flow button stays behind as
a placeholder holding layout. The placeholder swapped ALL of the trigger's
classes for a bare `ux-popover-trigger-placeholder` (transparent colors
only) and rendered **no content**, keeping only an inline width/height pin.
Width and height are not the whole layout contract: an inline-level box with
no in-flow content synthesizes its **baseline at its bottom margin edge**,
whereas the closed button's baseline is its icon/text baseline (~20px from
the top of the 32px box). Baseline-aligning the empty 32px box in the header
wrapper's inline formatting context pushed the whole box above the baseline,
and the line's strut descent (~3.5px at the inherited font) then extended
the line box below it — growing the header, the card, and reflowing the
grid. The placeholder was pinned by size but diverged in a dimension the pin
never modeled.

**Fix** — Pattern-level, in the machinery (`base/popover.rs` +
`src/style.css`), so every consumer inherits it: the attached placeholder
now keeps its full open-state classes AND its trigger content — its box,
content, and baseline are byte-identical to the measured pre-attach state —
and `.ux-popover-trigger-placeholder` becomes a paint-only override
(`opacity: 0`, unlayered so it beats the Tailwind utility layer). The inline
size pin stays as a guard. Consequence made explicit in the component doc:
the trigger subtree renders twice while open (in-flow invisible + top-layer
visual), so triggers must stay presentational (icons/text).

**Regression coverage** — `base::popover::tests::attached_placeholder_keeps_open_classes`
pins the class composition. The geometric identity itself (line-box height)
is not assertable in a unit test; the roster-card open-popover story
(`studio__roster__roster-card__device-detail-running-behind`) vs the closed
`running-behind` story is the visual gate — after this fix the card in the
open story renders at the closed card's size.

**Lesson** — A stand-in that replaces a real element must reproduce every
layout output of the original, not just the ones that are easy to copy.
Inline layout has a hidden output — the baseline — that width/height pins do
not capture, and an emptied box silently changes it. When re-parenting
content out of the flow, the cheapest correct placeholder is the original
subtree itself, made invisible (`opacity: 0` keeps size, baseline, focus,
and hit-testing), rather than a reconstructed facsimile of it.
