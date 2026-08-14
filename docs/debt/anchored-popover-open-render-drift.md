# Anchored popovers can render their open-state face differently than the closed trigger

**Status:** open condition
**Filed:** 2026-08-09 (sighted on the dimensionality section's
projection dropdown; Yona: "this is an issue we've had in a bunch of
places, feels like debt")

## The class

`PopoverButton`'s anchored mode (`lpa-studio-web/src/base/popover.rs`)
renders the anchor's content TWICE while open: the in-flow original
stays as a hidden placeholder, and a top-layer copy — the
`anchor_visual` Element — paints over the measured anchor rect inside
`div.ux-popover-open-trigger`. That container is NOT the trigger's
container: the default CSS
(`.ux-popover-open-trigger:not(.ux-popover-open-trigger-boxed)` in
`style.css`) applies `display: grid; place-items: center` and drops
padding/border, so any `anchor_visual` with real internal layout
(glyph + label + caret in a flex row) reflows the moment the popover
opens — children stack and center instead of keeping the closed
field's row layout.

## The mechanism

The duplicated face Element is mounted in a different container
context. The closed face's layout comes from classes on the caller's
own wrapper (or the trigger button's `class`), and nothing carries
those classes onto the top-layer copy: `layer_keeps_layout` /
`open_class` apply only to the NON-anchored branch. Each anchored
caller is therefore expected to hand in an `anchor_visual` that is
self-wrapping — and callers that pass bare fragments drift.

## Sightings

- **Projection field** (dimensionality section, `space_section.rs`) —
  glyph and label overlapped roughly centered while open. RESOLVED BY
  ESCAPE, not by fixing the class: the inline-tiles ruling (2026-08-09)
  removed the popover from the section entirely, so this surface no
  longer exercises anchored mode at all.
- **Surviving anchored callers** (`grep anchor_visual`): the palette
  swatch chooser (`panel/palette_swatch_field.rs`), the panel control
  fields (`panel/panel_control.rs`, wraps in `anchor_visual_class`),
  the module panel control (`module/module_panel_control.rs`), and the
  pass-throughs (`slot_detail_button.rs`, `base/icon_menu.rs`,
  `base/detail_popover.rs`). These currently look right because each
  hand-mirrors its layout in its own wrapper — parity by discipline,
  not by construction.

## The shape of the systemic fix (not done here)

One shared wrapper that guarantees the top-layer copy inherits the
trigger's layout classes: either `PopoverButton` grows an
`anchor_visual_class` prop it applies to the copy's container (so the
caller states the layout ONCE), or the anchored branch reuses
`open_class`/`layer_keeps_layout` exactly like the non-anchored branch
does. Doing it in `PopoverButton` touches every anchored popover's
baseline at once — schedule it as its own change with story-baseline
review, not as a drive-by.
