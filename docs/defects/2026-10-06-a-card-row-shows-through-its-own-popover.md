---
status: fixed
found: 2026-10-06      # how: hardware-walk (Yona's G1 desk walk, Studio's Wi-Fi popover)
fixed: this change
area: lpa-studio-web `base::popover` (`panel_overlaps_trigger`, the top-layer trigger copy)
class: assumed-context
related:
  - docs/adr/2026-07-15-popover-svg-merged-outline.md
---
# A card row shows through its own popover

**Symptom** — in the Wi-Fi popover's "Connect to a network" list, the
device card's "Wi‑Fi · set up ›" row was drawn over the network rows,
between "breadington city" and "GoogleHomeSpeaker…".

**Root cause** — while a popover is open, its trigger's visual is copied
into the browser's top layer, above the panel's outline, so the merged
trigger-and-panel shape reads as one surface. When there is no room above
or below, the panel is clamped to the viewport and slides back across its
own trigger; the copy was skipped then only if the panel covered the
**whole** trigger. A card's Wi‑Fi row spans the card and is wider than
the 320 px panel, so it was never "covered", and its copy painted over the
panel's rows.

**Fix** — the copy is skipped whenever the settled panel overlaps the
trigger past the welded seam (`panel_overlaps_trigger`), not only when it
covers it. It is the shared popover, so the Access popover and every other
popover with a wide trigger get it too.

**Regression coverage** — `popover::tests::a_trigger_the_panel_overlaps_is_detected_and_a_welded_one_is_not`
(a full-width row under a clamped panel, and both welded cases); story
`wifi_popover_open_over_its_card_row`, the Wi‑Fi popover with a long
Nearby list open over its own row.

**Lesson** — "the panel covers the trigger" assumed a trigger narrower
than its panel. Card rows are the first wide triggers to open tall panels.
