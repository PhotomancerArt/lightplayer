# Palettes are values, not nodes: stops on the bus, textures in the bake cache, `GradientConfig` as the second declared-config kind

- Status: accepted
- Date: 2026-08-04
- Context: closes the palette exploration arc (spike
  `planning/_archive/2026-08-03-1803-palette-exploration-spike`,
  register D1–D17, gates G1–G3 resolved 2026-08-03/04; UX playground
  merged as PR #337, `spikes/palettes/index.html`). The product gap:
  palettes are WLED's aesthetic identity — "anyone from the WLED world
  will miss it immediately" — and today LightPlayer shaders hand-roll
  them (`examples/fyeah-sign/idle.glsl` carries three cosine gradient
  functions and a bare-float switcher). Implemented by
  `planning/2026-08-04-1840-palette-implementation` (M1 model
  foundation; engine fill-path, catalog, and Studio chooser follow as
  M2–M4).
- Plan: `planning/2026-08-04-1840-palette-implementation` (external
  planning root).

## Decision

### 1. A palette is a value on a bus channel — there is no palette node

`Gradient` is a structured value kind (`lp::color::Gradient`): a
space-tagged, method-tagged stop list per `docs/design/color.md` §5–§6.
Shaders consume it through a palette-typed slot; binding that slot to
`bus:palette` is what puts a picker on a panel (publicity via authored
binds, or `default_bind` + `panel:"show"` per ADR
2026-08-03-panel-visibility-is-derived as amended). Sharing is scope
inheritance (modules.md R5/E4): one host-scope writer reskins every
embedded effect, per-subtree detach falls out of R11. No model rule
needed amendment — palettes are vocabulary, kind, and widget additions
only.

Rejected alternative: a `NodeKind::Palette` as the primary carrier
(node ceremony for zero benefit — R3 already makes a bound slot a
control) and stateful panel writers that run closed time-programs
(rejected at G2: "they can't get time" — time is a scoped channel, so
a wall-clock program cycles wrongly under a shadowed clock). A
shrunken palette *config-source* node remains an open follow-up
(plan M6), deliberately deferred until panel-state-only sharing has
been lived with.

### 2. Authored stops ride the bus; textures are an engine-side bake cache

The channel value is the plain structured `LpValue` — serializable,
latchable by panel writers (P2), persistable in `.lp/panel.json`
(P11), previewable by pickers. The engine bakes stops → height-one
texture (256×1 RGBA16, `wrap_x = Repeat`) at value-change time keyed
by value hash, so N consumers of one palette share one texture and a
panel drag re-bakes at most once per tick. Shaders sample a plain
`uniform sampler2D` via the shipped `TextureShapeHint::HeightOne`
contract on both numeric tiers.

Rejected alternative: a fixed-size uniform struct array +
`lpfn_palette` builtin (the pre-ratification 2026-07-27 shaping note)
— it pays per-frame fixed-point conversion for every entry, forces the
visual-shader header generator to learn struct arrays, and duplicates
what the texture path ships today. Also rejected: texture handles on
the bus (cannot round-trip JSON panel state; kills picker previews).

### 3. Cycling is a `GradientConfig`, resolved at uniform fill — the second declared-config kind

`GradientConfig = static(Gradient) | cycle{set, step_seconds,
fade_seconds}` (`lp::color::GradientConfig`) follows `PhasorConfig`
(ADR 2026-08-04-time-is-a-product) as the second instance of the
declared-config pattern: the chooser and panel writers hold plain
config *values* (P3-pure — the control never touches time); the
shader's fill path evaluates them, making a phasor query for cycle
configs. One full-cycle phasor per cycle config (period =
`set.len() × step_seconds`) makes index + blend pure functions of φ —
scrub-exact via the breakpoint log, rate-change-continuous via the
store, with zero palette-specific time machinery. Identity follows the
landed rule: the binding, not the config value (`PhasorKey::Private`
vs `Shared`), which yields private-phase vs phase-locked-shared
cycling for free. `step_seconds <= 0` or non-finite = frozen, the
same rule as `PhasorConfig::rate_hz`.

### 4. Model shape specifics

- Wire/`LpValue` storage is the ratified color.md §5 fixed-shape
  recipe (i32 space/method tags, explicit `count`, zero-padded
  fixed-max arrays); the serde/authored-JSON surface stays
  human-friendly (snake-case string tags, natural lists). The
  divergence is deliberate and documented in the module.
- `MAX_GRADIENT_STOPS` is bumped 16 → 24 at introduction: WLED
  gradients carry up to 18 stops and truncating community imports on
  day one is D2's explicit non-goal. `MAX_CYCLE_SET = 8`.
- `InterpMethod` is resolved in color.md's favor (`Step=0, Linear=1,
  Smooth=2`, `repr(i32)`); the contradictory legacy order was never
  serialized. `Kind::ColorPalette` is folded into `Gradient`
  (discrete lists are `method: Step`) — it had zero references.
- `palette` joins the well-known channel registry with
  `Kind::Gradient`.

## Consequences

- The engine milestone (M2) supplies `ShaderCompileOptions.textures`
  for palette slots and implements the bake cache + cycle evaluation;
  Studio (M4) adds the chooser and the new widget/emit/editor-hint
  variants; content (M3) ships a license-filtered catalog (its own
  ADR covers licensing and third-party asset isolation).
- Dynamic palettes need no engine feature: a compute shader producing
  a `Gradient` value is already inside the produced-slot contract.
- Palette values embed into user projects on pick (copy-on-use), so
  anything the catalog ships is effectively redistributed by users —
  the M3 licensing ADR inherits that constraint.
