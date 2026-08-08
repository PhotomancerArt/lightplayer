# Dimensionality follow-ups (parked from the engine-core plan)

The two-sided space model shipped its engine core (ADR
`2026-08-07-two-sided-space-model.md`; planning dir
`2026-08-07-1025-dimensionality-first-class/`, vision.md D1–D20). Parked
deliberately, in rough order of expected pickup:

- **Plan B — studio surface**: mirrored `space` sections on shader and
  fixture cards (shared controls), projection picker with live tiles
  *inside* the enum dropdowns (vision D16), preview space checkboxes
  (checkbox-style, both-on = stacked), fixture "Shape" moment on any
  fixture-create (not wizard-owned), and the three gallery WLED ports
  (fire2012, palette-waves, comet). UI wording is intentionally
  unsettled ("opinion" is not final).
- **Multi-entry** (the "gameboy-color" capability set): a shader defining
  both `render_1d` and `render_2d`; validation currently refuses with
  "not yet". Model needs a `Native` answer variant (additive).
- **Explicit projection node**: parameterized/bindable projections
  (radial centre on a bus, etc.) reusing `products/visual/coordinates.rs`;
  also the artifact a guided flow leaves behind.
- **Palette-side space declaration**: palettes are values, not nodes —
  where the declaration lives is open (planning dir Q5). Today palettes
  are shader inputs and never hit the sampling boundary.
- **3D/voxel cells**, authored 2D→1D scanline choice, 1D mappings.
- **web-demo** still uses its own pre-uniform 3-arg `render` signature —
  stale before the entry rename; retire or re-port.
