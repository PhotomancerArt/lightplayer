---
status: fixed
found: 2026-07-26      # how: report
fixed: this change
area: lpa-studio-web/base/popover
class: stale-measurement
related: ["docs/adr/2026-07-15-popover-svg-merged-outline.md", "2026-07-23-popover-open-resizes-card.md"]
---
# Popover outline and clip-path went stale when panel content resized

**Symptom** — In the settings popover, switching the provider to Custom
renders an extra Base-URL block
(`app/layout/studio_settings_popover.rs`): the panel's DOM grew, but the
SVG merged outline and the panel's `clip-path` stayed at the pre-growth
size — content spilled past the drawn border, or the border floated past
shrunken content when switching back. `Above`-placement popovers also
kept their stale `top`, so a grown panel no longer sat flush against the
trigger.

**Root cause** — Popover chrome is JS-measured: `panel_size` is captured
at panel `onmounted` plus stabilization re-measures at 50/250ms after
open, and `PopoverAutoUpdate` re-measures on window `scroll`/`resize`
only. Nothing observed the panel ELEMENT itself, so a content-driven
resize after the stabilization window had no trigger to re-measure —
every consumer whose panel content changes while open was affected.

**Fix** — `PanelResizeObserver` in `base/popover.rs`: a `ResizeObserver`
on the panel element whose callback fires the existing rAF-coalesced
`request_popover_update` path, installed at panel mount / the open
effect and dropped (disconnected) on close and component drop, mirroring
`PopoverAutoUpdate`'s lifecycle. `measure_trigger_once` now writes
`panel_size` only when the measurement changed beyond a 0.1px epsilon,
so observe → measure → set cannot re-render (or re-fire observers)
endlessly. An unavailable `ResizeObserver` degrades to the previous
behavior.

**Regression coverage** —
`base::popover::tests::panel_size_epsilon_passes_real_changes_and_eats_noise`
pins the loop guard. The `base/popover/content-growth` story renders an
open popover whose content grows one effect tick after mount, so the
baseline PNG shows the outline fitting the grown content. The
content-changes-long-after-open case (past the 250ms stabilization
window) is live-verified only: story captures gate on settled
measurements and deliberately have no timers.

**Lesson** — A measurement cache is only as fresh as the set of events
that invalidate it, and "window scroll + resize" is not that set when
the measured thing can change itself. When geometry is derived from an
element's size, the element itself must be observed (`ResizeObserver`),
not just its environment — and any observer that feeds a re-measure loop
needs a change-gate on the write side, or the loop is self-sustaining.
