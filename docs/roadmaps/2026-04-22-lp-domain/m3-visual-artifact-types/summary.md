# M3 — Visual artifact types + canonical examples + TOML grammar — summary

Roadmap: [`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`](../../roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md)  
Plan: this directory. Design + working notes: [`00-design.md`](./00-design.md), [`00-notes.md`](./00-notes.md).

### What was built

- **P01** — `Binding::Bus` on-disk JSON: flat `{"bus":"…"}`; tests for `bind = { bus = "…" }` alignment.
- **P02** — `Kind::AudioLevel` + per-Kind branches (default bind `audio/in/0/level`, three-float storage, etc.).
- **P03** — Custom `Slot` `Deserialize` / `Serialize` / hand-written `JsonSchema` for `quantity.md` §10 (implicit scalar, params, reserved words).
- **P04** — `Constraint` peer-key grammar; new `ShaderRef` and `VisualInput` untagged enums with `deny_unknown_fields` per variant.
- **P05** — `ParamsTable` (implicit `Shape::Struct`); TOML literal `ValueSpec` / color / `AudioLevel` table parsing via kind-aware load paths.
- **P06** — Leaf Visuals: `Pattern`, `Effect`, `Transition` + `impl Artifact` + serde/schema.
- **P07** — Composed Visuals: `Stack`, `Live`, `Playlist` + `TransitionRef`, entries, `PlaylistBehavior`, re-exports.
- **P08** — `load_artifact` + `LoadError` over `LpFs`, `schema_version`, `ValueSpec` materialization (`std`).
- **P09** — Eight example TOMLs + `fbm/main.glsl` under `examples/v1/`; old `docs/design/lpfx/…` copies removed from design tree.
- **P10** — `tests/round_trip.rs`: per-example load/serialize/structural round-trip; JSON-schema validation when `schema-gen`.
- **P11** — Design docs (e.g. `docs/design/lpfx/overview.md`) repointed to `examples/v1/`; grammar text matches shipped types.

### Decisions for future reference

#### `Constraint` / `ShaderRef` / `VisualInput` use tuple-carrying enum variants, not open struct tables

- **Decision:** Keep mutex peer keys on untagged variants with per-variant `deny_unknown_fields` (as implemented), not a single struct with optional fields.
- **Why:** Serde’s `deny_unknown_fields` does not apply the same way to struct variants; strict mutex errors catch authoring mistakes for shader/input/constraint forms.
- **Rejected alternatives:** Single flattened struct (permits invalid key combinations); untagged without field discipline (worse errors).
- **Revisit when:** A variant genuinely needs extensible open metadata beyond the peer-key set — then a tagged outer envelope may be better.

#### Kind-aware default parsing lives in `value_spec` (`from_toml_for_kind` / `from_toml_for_shape`), not only on `LpsValue`

- **Decision:** TOML literals for `default` are resolved with `Kind` and `Shape` context from the owning `Slot` / `ParamsTable` path.
- **Why:** `ValueSpec::Literal` stores `LpsValue`, but the on-disk grammar is Kind-shaped (e.g. color table, `AudioLevel` triplet); only the deserializer path has that context.
- **Rejected alternatives:** Require every literal to be written in wire `ValueSpec` form in TOML (unfriendly); push parsing into `LpsValue` only (loses `Shape` for arrays/structs).
- **Revisit when:** `LpsValue` gains first-class TOML literal serde matching all Kinds (possible consolidation then).

#### `Slot`’s `JsonSchema` is hand-written (wire `oneOf`), not derived from the `Shape` enum

- **Decision:** Custom `JsonSchema` on `Slot` matches the custom `Serialize` shape (peer constraint keys, implicit scalar, elided `shape` when default).
- **Why:** A derive from `Shape` + `Slot` would describe a different JSON tree than what serde emits; schema-vs-loader tests would lie.
- **Rejected alternatives:** Derive and accept drift; derive and duplicate serde (fragile).
- **Revisit when:** The serialized `Slot` form is made isomorphic to a single derive-friendly enum (bigger format bump).

#### `Live` and `Playlist` stay minimal for v1

- **Decision:** No `[selection]` on `Live`; no per-`entries[]` `transition` on `Playlist`.
- **Why:** No v1 product requirement to drive switching complexity; a placeholder live + single default transition keeps the type surface small.
- **Rejected alternatives:** Full selection SM — heavy UX and timing semantics; per-entry transitions — more serde and runtime without a driving example.
- **Revisit when:** A concrete engine/live UX needs switching or a playlist wants mixed transition styles per cue.

#### Only `AudioLevel` is added as a new signal Kind in M3

- **Decision:** Do not add `Audio` / `AudioFft` / beat/touch Kinds until an example in the v1 corpus needs them; `fluid.pattern.toml` is satisfied with `AudioLevel` + bus bind.
- **Why:** Roadmap (Q9) says add signal Kinds incrementally; each Kind is ongoing surface area (storage, defaults, tests).
- **Rejected alternatives:** Full audio/touch/motion set in M3 (unused, untested).
- **Revisit when:** A shipped example reads FFT bands, full audio buffers, or touch streams — add the corresponding `Kind` + `ValueSpec` arm then.

## Milestone acceptance (evidence)

| Milestone bullet | Met by |
| ---------------- | ------ |
| Six typed `Artifact` Visual structs | `lp-domain/lp-domain/src/visual/{pattern,effect,transition,stack,live,playlist}.rs` (P06, P07) |
| `ShaderRef`, `[bindings]`, `[input]` | `visual/shader_ref.rs`, `visual/visual_input.rs`, `binding.rs` (P01, P04); `Live` / `Playlist` `bindings` (P07) |
| `Slot` TOML grammar (shape default, params, reserved words, constraints) | `shape.rs` (P03), `value_spec.rs` kind-aware defaults (P05) |
| New signal Kinds for examples | `Kind::AudioLevel` only in `kind.rs` (P02) — `fluid.pattern.toml` |
| Eight canonical TOML examples + sibling shader | `lp-domain/lp-domain/examples/v1/**` (P09); eight `.toml` + `patterns/fbm/main.glsl` |
| Round-trip + schema-drift tests | `lp-domain/lp-domain/tests/round_trip.rs` (P10) |
| `LpFs` loader stub | `artifact/load.rs` (P08) |
| Design docs + removed old `docs/design/lpfx/{patterns,…}/` TOMLs | P11; repo shows those paths deleted with corpus under `examples/v1/` |

## Validation evidence (Phase 12)

- `cargo check -p lp-domain` — pass, zero warnings.
- `cargo check -p lp-domain --features std` — pass, zero warnings.
- `cargo check -p lp-domain --features schema-gen` — pass, zero warnings.
- `cargo check -p lp-domain --features std,schema-gen` — pass, zero warnings.
- `cargo test -p lp-domain` — 132 lib tests passed.
- `cargo test -p lp-domain --features std` — 137 lib + 11 `round_trip` = 148 passed.
- `cargo test -p lp-domain --features schema-gen` — 174 lib + 18 `round_trip` = 192 passed (`schema-gen` implies `std`).
- `cargo test -p lp-domain --features std,schema-gen` — 174 + 18 = 192 passed.
- `cargo test -p lp-domain --features std --test round_trip` — 11 passed.
- `cargo test -p lp-domain --features std,schema-gen --test round_trip` — 18 passed.
- `cargo doc -p lp-domain --no-deps --features schema-gen` — pass, **zero** rustdoc warnings (after P12 link cleanup).
- `just check` — pass (fmt + clippy workspace-wide).
- `rg 'TODO(M3)' lp-domain/` — 0 matches.
- Stale `docs/design/lpfx/{patterns,…}/` path sweep under `docs/design/` — 0 matches.

## Examples corpus (`lp-domain/lp-domain/examples/v1/`)

- `patterns/rainbow.pattern.toml` — inline GLSL
- `patterns/fbm.pattern.toml` + `patterns/fbm/main.glsl` — file ref
- `patterns/fluid.pattern.toml` — builtin + `AudioLevel` / bus wiring
- `effects/tint.effect.toml`, `effects/kaleidoscope.effect.toml`
- `transitions/crossfade.transition.toml`, `transitions/wipe.transition.toml`
- `stacks/psychedelic.stack.toml`
- `lives/main.live.toml`
- `playlists/setlist.playlist.toml`

## Deferred to later milestones

- `Live` `[selection]` and richer switching — deferred (see [`00-design.md`](./00-design.md)); Live stays barebones.
- Per-entry `Playlist` transition overrides — deferred; single default `transition` for v1.
- Cross-artifact resolution, binding-key parsing / validation — artifact-resolution / binding work.
- Further audio/signal Kinds — add when an example needs them.
- Q32 numeric specialization, `Constraint` → full `LpsValue` width — per M2/M3 roadmaps.
- Schema codegen to `schemas/v1/`, migration framework — M4/M5.
- **P12 / tooling:** plan dir not moved to `docs/plans-old/` here — main agent / plans tooling.

## Known dirt (acceptable for v1)

- `VisualInput` / live / playlist entry `params` overrides: `BTreeMap<String, toml::Value>` (no cross-artifact type-check) until resolution.
- `[bindings]` keys stored as opaque strings; parsing deferred (see design).
- `Constraint` remains F32-narrowed where M2 left it, unless a future example widens it.

## Files touched in Phase 12 (cleanup only)

- `lp-domain/lp-domain/src/schema_gen_smoke.rs` — `schema_for!` smoke coverage for M3 Visual + substructure types.
- `lp-domain/lp-domain/src/value_spec.rs`, `artifact/load.rs` — rustdoc link fixes for warning-free `cargo doc`.
- `lp-domain/lp-domain/src/shape.rs` — clippy `uninlined_format_args` in struct `fields` deserializer.
- `cargo fmt` — files touched by `just check` in the workspace.
