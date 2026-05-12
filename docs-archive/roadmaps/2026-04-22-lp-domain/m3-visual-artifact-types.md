# Milestone 3: Visual artifact types + canonical examples + TOML grammar

## Goal

Implement the typed Rust structs for all six Visual kinds (Pattern,
Effect, Transition, Stack, Live, Playlist), implement the TOML
grammar that drives `Slot` shape inference, move the canonical
example TOMLs from `docs/design/lpfx/` into
`lp-domain/lp-domain/examples/v1/`, and prove end-to-end round-trip
via TOML → typed struct → TOML.

`schema_version = 1` everywhere. The `examples/v1/` directory **is**
the v1 baseline corpus that M5's migration framework will exercise
against (Q5).

No render hookup. No migrations yet (M5 adds the machinery).

## Suggested plan name

`lp-domain-m3`

## Scope

**In scope:**

- One typed struct per Visual kind in
  `lp-domain/lp-domain/src/visual/`:
  - `pattern.rs` → `Pattern` (collapses former
    `ShaderPattern`/`BuiltinPattern`)
  - `effect.rs` → `Effect`
  - `transition.rs` → `Transition`
  - `stack.rs` → `Stack`
  - `live.rs` → `Live`
  - `playlist.rs` → `Playlist`

- **TOML grammar for `Slot`** (per
  [`quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar),
  Q7):
  - Custom serde `Deserialize` for `Slot`.
  - `shape` field defaults to `"scalar"` when missing.
  - Top-level `[params]` is implicitly `Shape::Struct` (special-cased).
  - `shape = "array"` requires `length` + `[X.element]` table.
  - `shape = "struct"` requires `[X.props.<name>]` field tables.
  - Reserved field-map keywords: `params` (top-level only),
    `element`, `props`. Loader rejects user fields with these names.
  - Constraint fields (`range`, `step`, `choices`, `labels`) live as
    peers on a Scalar Slot.
  - `default` may be omitted on compositions; loader computes from
    children's defaults.
  - Round-trip on save: re-emit using the same grammar (omit
    `shape = "scalar"`).
  - ~30 LOC of custom parser glue beyond `#[derive(Deserialize)]`.

- **Common substructure types**:
  - `ShaderRef` (inline GLSL string OR builtin name; the unified
    `[shader]` section per the design doc).
  - `[bindings]` table representation (cascade rules implemented in
    M3; resolution order matches `quantity.md` §8).
  - `[input]` polymorphic slot for Stack / Effect (visual or channel),
    as documented in `lpfx/concepts.md`.
  - Entry list shapes for Live / Playlist.

- **Signal Kinds** (Q9): land in this milestone when the first
  example needs them. Likely:
  - `Kind::Texture` (already in M2).
  - `Kind::Audio`, `Kind::AudioFft`, `Kind::AudioLevel`, `Kind::Beat`,
    `Kind::Touch`, `Kind::Motion` — added one at a time as concrete
    examples exercise them. Each adds a Kind variant + per-Kind
    impls + a project-wide constant for any size parameters
    (`AUDIO_FRAME_LEN`, `AUDIO_FFT_BINS`, `MAX_TOUCH_POINTS`, ...).
  - Streamed/bulk Kinds (likely `Audio`, possibly `Touch`) use
    opaque-handle storage like `Texture`. `ValueSpec` extends with
    their source variants when added (`Audio(AudioSpec)`, etc.) —
    same pattern as `TextureSpec`.

- **All structs implement `Artifact`** trait (from M2) with
  appropriate `KIND` and `CURRENT_VERSION = 1`.

- **Serde discipline**:
  - `#[serde(default)]` for optional fields.
  - **No** `deny_unknown_fields`.
  - All TOML tag conventions match the design docs.

- **Canonical examples** land in `lp-domain/lp-domain/examples/v1/<kind>/`
  (note the **`v1/` prefix** — Q5 establishes this as the baseline
  corpus; future versions land at `v2/`, etc., via M5's migration
  workflow):
  - `v1/pattern/{rainbow,fbm,fluid}.pattern.toml`
  - `v1/effect/{tint,kaleidoscope}.effect.toml`
  - `v1/transition/{crossfade,wipe}.transition.toml`
  - `v1/stack/psychedelic.stack.toml`
  - `v1/live/main.live.toml`
  - `v1/playlist/setlist.playlist.toml`
  - At least one example exercises an audio-side Kind (likely
    `fluid.pattern.toml` consuming `audio/in/0/bands` or similar) so
    we land at least one signal Kind in this milestone.
- Each example file gets `schema_version = 1` added.
- Each example uses the unified `[shader]` section (vs. the old
  `[builtin]` / `[shader]` split).
- Each example uses explicit `bind = { bus = "..." }` form (Q2).
- Each example uses the new bus channel naming convention
  (`audio/in/0` not `audio/in`; per `domain.md` Q5 + `quantity.md`
  §11).
- `docs/design/lpfx/{patterns,effects,transitions,stacks,lives,playlists}/`
  TOMLs are deleted; the design docs (`overview.md`, `concepts.md`)
  are updated to point at the canonical paths under
  `lp-domain/lp-domain/examples/v1/`.

- **Round-trip integration tests**:
  - For each example file: load → assert structural fields →
    serialize → re-load → equals first load.
  - Textual round-trip is a stretch goal (TOML formatting may
    differ); structural equality is sufficient.

- **`LpFs`-based loader stub** at
  `lp-domain/lp-domain/src/artifact/load.rs` (std-only, used by
  tests). Materializes `ValueSpec`s at load time per `quantity.md` §7.

**Out of scope:**

- Schema codegen *tooling* (M4). schemars derives are already in
  place from M2 (Q6).
- Migration framework (M5).
- `examples/v1/<kind>/history/` — doesn't exist until there's a v2
  to migrate from. M5's synthetic `v0_5 → v1` smoke test
  *populates* a synthetic history; real history accrues over
  future schema bumps.
- `schemas/v1/` directory (M4).
- Render hookup, runtime behavior.
- Cross-artifact validation (Stack referencing a non-existent
  pattern, cycle detection on Visual references — deferred to
  artifact resolution roadmap).
- Q32 numeric specialization (Q4).

## Key decisions

- **Read [`quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar)
  before designing the parser.** It locks in the grammar.
- **Examples live under `examples/v1/`**, not `examples/` (Q5).
  Establishes the v1 baseline corpus that M5 exercises.
- **Pattern is one type, not two.** No `ShaderPattern`/`BuiltinPattern`
  split. The `[shader]` section can contain inline source OR a
  builtin reference.
- **All bindings use the explicit table form**: `bind = { bus = "..." }`.
  No bare-string shorthand in v0 (Q2). Future variants land as
  additional table forms (`bind = { node = "..." }`,
  `bind = { value = ... }`).
- **`Slot.default` may be omitted on compositions** (struct, array)
  and is computed from children's defaults at load time. Always
  present on the in-memory `Slot`.
- **Signal Kinds (Audio/Beat/Touch/...) added incrementally** as
  examples demand them (Q9). The first audio-consuming example
  (likely `fluid.pattern.toml`) drives the `Audio` / `AudioFft` /
  `AudioLevel` Kind landing. Don't preemptively add Kinds
  unexercised by example.
- **Bus channel names follow the
  `<kind>/<dir>/<channel>[/<sub>...]` convention** per
  `quantity.md` §11. Examples use `time`, `audio/in/0`,
  `audio/in/0/bands`, `video/in/0`, `video/out/0`, `touch/in/0`.
  Convention only; no parser enforcement in v0.

## Deliverables

- Six typed structs (one per Visual kind), all implementing
  `Artifact`.
- Common substructure types (`ShaderRef`, `[bindings]` model,
  `[input]` polymorphic slot, Live/Playlist entries).
- TOML grammar for `Slot` (custom `Deserialize`, ~30 LOC), with
  unit tests covering the inference rules from `quantity.md` §10.
- Any new signal `Kind` variants needed by examples (with
  `storage()`, `dimension()`, `default_constraint()`,
  `default_presentation()`, `default_bind()`, and project-wide
  constants).
- Eight canonical example TOMLs in
  `lp-domain/lp-domain/examples/v1/<kind>/`, each with
  `schema_version = 1`, unified shader section, explicit
  `bind = { ... }` tables, and the new channel naming convention.
- Round-trip integration tests in
  `lp-domain/lp-domain/tests/round_trip.rs`.
- Stub `load.rs` using `LpFs` to read examples in tests.
- Updated `docs/design/lpfx/overview.md` and any other design docs
  pointing at the new canonical paths under `examples/v1/`.
- Old TOMLs in `docs/design/lpfx/{patterns,...}/` deleted.

## Dependencies

- M2 complete (foundational types: identity, Quantity model, trait
  surface, schemars derives all in place).
- M1 indirectly (loader uses `lpfs::LpFsStd` in tests).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Six Visual kinds with real cross-cutting work
(shared substructures, custom TOML grammar for `Slot`, example
move, design-doc cleanup, possibly multiple new signal Kinds).
Naturally splits into parallel leaf-vs-composed Visual phases after
the shared types and the TOML grammar land.

Suggested phase shape (decided during `/plan`, sketched here):

- Phase A: TOML grammar for `Slot` (custom `Deserialize`, inference
  rules) + shared substructures (`ShaderRef`, `[bindings]`,
  `[input]`) — solo
- Phase B: Pattern + Effect + Transition (leaf Visual kinds) —
  depends on A
- Phase C: Stack + Live + Playlist (composed Visual kinds) —
  depends on A, parallel with B
- Phase D: Add signal Kinds needed by examples
  (`Audio`/`AudioFft`/`AudioLevel`/`Beat`/`Touch`/`Motion` as
  needed) — depends on examples-being-drafted; can run parallel
  with B/C using stub Kinds
- Phase E: Move/edit canonical examples to `examples/v1/`; delete
  old design TOMLs; update design docs — depends on B+C+D
- Phase F: Round-trip tests + `LpFs` load stub + workspace verify —
  final

Estimated size: ~1200–1800 LOC across struct/serde + ~100 LOC TOML
grammar + ~200 LOC of round-trip tests; eight example files
migrated; design-doc cleanup; new signal Kinds as needed.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
