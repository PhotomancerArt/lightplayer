---
status: carried
since: 2026-08-13 # accepted at the one-project-canvas plan's Q6 ruling
logged: 2026-08-13
area: lpa-mapping-editor/canvas
related:
  [
    "../adr/2026-08-13-one-project-canvas.md",
    "planning lp2025/2026-08-13-0859-one-project-canvas (Q6)",
  ]
---

# Placement rotation quirks: doc-space marquee and rotated wiring numbers

**Shape** — The one project canvas renders a dived fixture's document
through its placement transform (translate ∘ rotate ∘ uniform scale),
and two affordances are doc-space by construction:

- The **marquee** is an axis-aligned doc-space rect
  (`marquee_select(min, max)` containment is doc-space), so over a
  rotated fixture it RENDERS rotated — the drag corners follow the
  document's axes, not the screen's.
- **Wiring numbers** (and any doc-space text) rotate with the
  document, so a 90°-placed fixture shows sideways lamp numbers.

Both were accepted for v1 at the plan's Q6 ruling: the alternative — a
screen-space marquee with rotated-rect containment, and counter-rotated
text — buys polish at real geometry cost, and nobody has hit it in
anger yet. Structural because every doc-space affordance added to the
canvas inherits the same choice.

**Carrying cost** — Cosmetic-to-mild UX confusion when editing fixtures
placed at large rotations; zero cost at identity or small tilts.

**Workarounds** — Select by clicking/⇧-clicking lamps when a rotated
marquee reads badly, or do heavy lamp editing before rotating the
placement.

**Incident log** — 2026-08-13: filed at acceptance (one-project-canvas
P5); no user reports yet.

**Exit criteria** — Marquee drags screen-axis-aligned over a rotated
placement (rotated-rect containment in doc space), and wiring numbers
render upright regardless of placement rotation — most plausibly as
part of the parked viewport-rotation / snap-viewport-to-fixture work,
which needs the same screen-space machinery.
