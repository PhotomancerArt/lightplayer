# Dimensionality follow-ups (parked from the engine-core plan)

The two-sided space model shipped its engine core (ADR
`2026-08-07-two-sided-space-model.md`; planning dir
`2026-08-07-1025-dimensionality-first-class/`, vision.md D1–D20) and then
its studio surface (ADR `2026-08-09-dimensionality-authoring-surface.md`;
planning dir `2026-08-08-0959-dimensionality-studio-surface/`). Parked
deliberately, in rough order of expected pickup:

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
- **`declare_space` agent tool**: the Studio shader agent's tools are
  `iterate` / `upsert_param` / `speed` — none writes `ShaderDef::space`,
  so an agent can stage a `render_1d` body onto a `TwoD` node and then
  cannot repair the mismatch it just created. The system prompt also
  states the 2D entry unconditionally, which is false on a `OneD`
  shader. Sized during the studio-surface plan's P6 and chipped as its
  own change: it is a full tool across `lpa-agent` + `lpa-studio-core`
  plus a prompt-snapshot change, not a small addition.
