# ADR: Two-sided space model — explicit render entries, producer-side projection

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Every shader was implicitly 2D (`vec4 render(vec2)` hard-validated as the
single entry), and dimensionality existed only as a fixture-side fact
(`render_size`, `mapping`). WLED-style 1D content could not be honestly
1D, and nothing could express "this ring-mapped scarf should run a 1D
effect along the strip, ignoring the map." The full design record —
including the LED-scarf and serpentine-matrix proof cases, the Digital
Ambiance fat-signature anti-pattern, and the UX spike
(`spikes/dimensionality/`) — lives in the planning directory
`2026-08-07-1025-dimensionality-first-class/` (vision.md D1–D20).

## Decision

1. **Space is declared on both ends, authored not derived.** The shader
   declares `ShaderSpace` (primary variant `OneD`/`TwoD` carrying
   per-target answer cells — invalid pairs unrepresentable); the fixture
   declares `strip_order_meaningful` (the one authored bit: wire order
   always *exists* but only sometimes *means*) plus a consumer policy
   `VisualConsumerSpace` (per-pair default projection + `force`).
2. **GLSL entries are explicit and dimension-named**: `vec4
   render_2d(vec2 pos)` / `vec4 render_1d(float pos)`. A bare `render`
   is a hard error — no alias; project format v6's migration rewrites
   existing assets (alpha ruling: pay the rename once instead of
   carrying API debt). The signature and the declaration are two
   different jobs; they cross-validate, and the mismatch error class is
   what keeps the declaration truthful. Multi-entry (a shader defining
   both, the "gameboy-color" capability set) is deliberately refused for
   now but requires no future renaming.
3. **Selection = intersection preferring the effect's intent.** A 1D
   effect on a `{1D, 2D}` scarf samples strip positions and ignores the
   map; a 2D effect on the same fixture samples the map's UVs. Handled
   entirely by the consumer choosing which of its own coordinate sets to
   send.
4. **The producer executes projection; requests are space-tagged.**
   `VisualProduct` stays `{node, output}` — product space metadata is a
   query through the reference. Render/sample requests carry the target
   `VisualSpace` plus the consumer's policy; the producer resolves
   precedence (force → consumer default; else its own declared answer;
   else consumer default) and runs the shared coordinate-map library
   (`products/visual/coordinates.rs`: extrude, radial with corner-reach-1
   normalization, angular, mirror, centre scanline). Zero graph nodes in
   every default path.

## Consequences

- 1D effects are first-class through the whole stack: compiler (1-lane
  synth on CPU, float varying on the GPU tier), engine sampling, and
  device targets — validated by filetests across interp/wasm/rv32.
- The scarf behavior and the precedence ladder are pinned as engine unit
  tests (`fixture_node.rs` negotiation suite).
- Format v6: pre-v6 projects migrate automatically (`lpa-upgrade`
  v5_to_v6 rewrites GLSL assets); devices and studio refuse v5 without
  the migration, per the existing format gate.
- `space` doubles as browse/registry metadata for future packs
  (dimensionality is the first categorization axis).

## Alternatives Considered

- **Inferring space from the GLSL signature** (no slot): rejected — the
  declaration would be invisible to the node layer, UI, and registry.
- **One overloaded `render` name**: rejected — explicit names make the
  capability set legible and multi-entry additive.
- **A `render` alias for compatibility**: rejected while in alpha —
  migration machinery already existed; debt avoided.
- **Consumer-side projection execution** (fixture rewrites coordinates):
  rejected — the producer knows its own opinion, keeps precedence in one
  place, and any future 1D source (palettes) reuses the same answer path.
- **Per-dimension node kinds** (`Shader1d`/`Fixture2d`…): rejected —
  capability sets cannot live in kinds; growing an entry would force a
  node-kind migration.

## Follow-ups

- Multi-entry execution (both entries in one shader; `Native` answer
  cells).
- Explicit projection node (the escape valve for parameterized/bindable
  projections) reusing `coordinates.rs`.
- Palette-side space declaration (palettes are values, not nodes — open
  question Q5 in the planning directory).
- 3D/voxel cells; authored 2D→1D scanline choice; 1D mappings.
- Studio surface (space sections, projection picker, preview space
  toggles) — Plan B of the planning directory.
