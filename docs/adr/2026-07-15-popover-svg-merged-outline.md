# ADR: Studio popover chrome is a single SVG merged-outline path

- **Status:** Accepted
- **Date:** 2026-07-15
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The Studio "contiguous popup" (a trigger button whose border flows continuously
into its popup panel) was implemented in CSS: the trigger, the panel, and a 1px
"bridge" strip each drew part of the shared border, with radial-gradient
pseudo-element fillets forging the concave corners and sub-pixel gradient stops
(`+0.5px`) hand-tuned to hide the seams. Every geometric variation (panel wider
or narrower than the trigger, flush edges, above placement) was a special case
(`show_left_corner`, `nearest_edge`, corner-clearance thresholds), and correct
rendering depended on three elements agreeing on seam colors by hand. CSS
fundamentally cannot express "one border around the union of two boxes" — each
element only draws its own border.

The same pattern had been independently implemented (with the same hack family)
in an unrelated codebase, which prompted a spike (`spikes/contiguous-popup/`)
and a write-up: <https://lab.photomancer.art/post/2026-07-15-contiguous-popup/>.

## Decision

Popover chrome is computed, not forged: the union of the trigger and panel
rects becomes a rectilinear polygon whose corners are rounded with a uniform
arc construction (concave corners are the same code path with a flipped sweep
flag), and ONE SVG path draws the fill, border, and shadow for the whole shape.

- Geometry lives in `lp-app/lpa-studio-web/src/base/outline.rs`: pure Rust
  (no DOM), host-unit-tested. Edge coordinates within 1.25px weld together, so
  near-aligned edges never render hairline jogs; radii clamp per vertex.
- `PopoverButton` renders the path in the popover top layer. While open, the
  trigger's content re-parents into the top layer (the in-flow button remains
  as an invisible size-pinned placeholder holding layout and focus), because
  the top layer paints above everything and would otherwise cover the trigger.
- Open/close animates by interpolating the panel's INPUT rect and re-unioning
  every frame (no path morphing); a rAF loop runs only during transitions and
  `prefers-reduced-motion` jumps to the settled state.
- The `.ux-popover-chrome-*` tone variables are consumed by the path's SVG
  gradient stops and stroke, so one gradient flows continuously across trigger
  and panel — previously impossible.

## Consequences

- The bridge/fillet CSS (~180 lines), corner-visibility logic, and seam-color
  contracts are gone; alignment cases need no special-casing.
- Popover chrome is JS-measured: rendering depends on `getBoundingClientRect`
  and re-measurement on scroll/resize (this was already true of positioning).
- The panel and trigger paint no background/border of their own while open;
  consumer `popup_class` chrome is neutralized via `ux-svg-popover-panel`.
- Geometry is unit-testable on the host; visual regressions are covered by the
  story-PNG baseline gate.
- Rotated ancestors are unsupported (axis-aligned rects only) — acceptable for
  Studio's layout, and no worse than the previous implementation.

## Alternatives Considered

- **Keep patching the CSS approach:** rejected; every new geometry is another
  special case and the failure mode (hairline seams) is invisible to tests.
- **CSS `corner-shape: scoop` for the concave corners:** Chromium-only,
  per-element — still requires seam alignment across elements.
- **No top layer (fixed positioning + z-index):** simpler re-parenting story
  but re-exposes ancestor overflow/stacking traps the top layer exists to
  escape.
- **SVG fill with a hole over the trigger (trigger paints itself):**
  reintroduces seam matching and kills the cross-shape gradient.

## Follow-ups

- TypeScript port of the geometry core for SBS (separate repo).
- Possible generalization to more than two participant rects (attached
  sub-panels) — the geometry already supports it.
