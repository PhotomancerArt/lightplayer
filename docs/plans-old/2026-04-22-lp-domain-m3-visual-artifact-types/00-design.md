# M3 — Visual artifact types + canonical examples + TOML grammar — design

This is the design we agreed on with the user. It is the contract every
phase implements. Phases must read both this file and
[`00-notes.md`](./00-notes.md) before starting work.

## Scope of work

Implement the typed Rust structs for all six Visual kinds, the TOML
grammar that drives `Slot` shape inference, and the canonical example
corpus at `lp-domain/lp-domain/examples/v1/`. Round-trip tests prove the
loader and serializer agree.

1. **Six typed Visual structs** in `lp-domain/lp-domain/src/visual/`,
   each `impl Artifact` with `KIND` and `CURRENT_VERSION = 1`:
   `pattern.rs`, `effect.rs`, `transition.rs`, `stack.rs`, `live.rs`,
   `playlist.rs`. `Pattern` collapses former ShaderPattern + BuiltinPattern
   into one type via the unified `[shader]` section.
2. **Common substructure types**: `ShaderRef` (inline GLSL OR file OR
   builtin); `VisualInput` (compose-child OR bus-channel) for Stack and
   Effect; `LiveCandidate` / `TransitionRef` / `PlaylistEntry` /
   `PlaylistBehavior` for Live and Playlist; raw `Vec<(String, Binding)>`
   for the `[bindings]` cascade.
3. **TOML grammar for `Slot`** per
   [`quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar):
   custom serde `Deserialize` + `Serialize` + `JsonSchema` for `Slot`,
   `shape` defaults to `"scalar"`, top-level `[params]` is implicit
   `Shape::Struct` (via the `ParamsTable` newtype), reserved keywords
   (`params`, `element`, `props`).
4. **Peer-key inference enums**: `Constraint` (`range` vs `choices` vs
   neither), `ShaderRef` (`glsl` vs `file` vs `builtin`), `VisualInput`
   (`visual` vs `bus`) all use stock `#[serde(untagged)]` + per-variant
   `#[serde(deny_unknown_fields)]`. No custom `Deserialize` for these.
5. **Add `Kind::AudioLevel`** as the only new signal Kind in M3, used
   by `fluid.pattern.toml`'s `intensity` parameter. Default bind to
   `audio/in/0/level`.
6. **Eight canonical example TOMLs** under
   `lp-domain/lp-domain/examples/v1/<kind>/` with `schema_version = 1`,
   unified `[shader]` section, explicit `bind = { … }` table, OKLCH
   color defaults, and `<kind>/<dir>/<channel>` channel naming.
7. **Move + delete**: drop the `docs/design/lpfx/{patterns,effects,
   transitions,stacks,lives,playlists}/` TOMLs; update design docs to
   point at the canonical paths under `examples/v1/`.
8. **Round-trip integration tests** at
   `lp-domain/lp-domain/tests/round_trip.rs`: load → assert key
   structural fields → serialize → re-load → compare structurally;
   parallel test validates each example against the generated JSON
   schema (catches `Deserialize` ↔ `JsonSchema` drift).
9. **`LpFs`-based loader stub** at `src/artifact/load.rs` (std-only,
   used by the round-trip tests). Materializes `ValueSpec` at load
   time per `quantity.md` §7.

Out of scope (deferred):

- Schema codegen tooling (M4); schemars derives are already in place.
- Migration framework (M5).
- `examples/v1/<kind>/history/` (only created when v2 lands).
- `schemas/v1/` directory (M4).
- Render hookup, runtime behavior.
- Cross-artifact validation (Stack referencing a missing Pattern,
  cycle detection on Visual references) — deferred to artifact
  resolution roadmap.
- Q32 numeric specialization.
- Pre-existing `Constraint` widening to `LpsValue` (M2 left a TODO;
  stays F32-only in M3 unless an example forces the issue).
- `Live` `[selection]` block — `Live` ships barebones in M3; the
  `min_hold` / `debounce` shape is deferred (may want per-candidate
  not Live-wide; design conversation needed).
- Per-entry transition overrides on `Playlist` — entries play through
  the playlist's default `[transition]` for v0.
- Type-checking the inline `params` overrides on `VisualInput::Visual`
  / `LiveCandidate` / `PlaylistEntry` / `TransitionRef`. Stored as
  raw `BTreeMap<String, toml::Value>`; cross-artifact resolution
  lands later.
- Transitions' `[input]` blocks — Transitions have **no** `input`
  field; `inputA` / `inputB` are implicit shader uniforms named by
  convention; the parent runtime wires them.
- `[bindings]` key parsing / cross-artifact validation — keys stored
  as raw relative-NodePropSpec strings; parsing lands with binding
  resolution work.

## File structure

```
lp-domain/
└── lp-domain/
    ├── Cargo.toml                          # UPDATE: add toml + serde_with deps; std feature pulls in lpfs/std + serde_with/std + toml/parse.
    ├── examples/                           # NEW
    │   └── v1/
    │       ├── patterns/
    │       │   ├── rainbow.pattern.toml    # NEW (inline GLSL)
    │       │   ├── fbm.pattern.toml        # NEW (file = "main.glsl")
    │       │   ├── fbm/main.glsl           # NEW (sibling shader source)
    │       │   └── fluid.pattern.toml      # NEW (builtin = "fluid")
    │       ├── effects/
    │       │   ├── tint.effect.toml        # NEW
    │       │   └── kaleidoscope.effect.toml# NEW
    │       ├── transitions/
    │       │   ├── crossfade.transition.toml # NEW
    │       │   └── wipe.transition.toml    # NEW
    │       ├── stacks/
    │       │   └── psychedelic.stack.toml  # NEW
    │       ├── lives/
    │       │   └── main.live.toml          # NEW (barebones, no [selection])
    │       └── playlists/
    │           └── setlist.playlist.toml   # NEW (no per-entry transitions)
    ├── src/
    │   ├── lib.rs                          # UPDATE: re-export visual::*; expose AudioLevel; expose toml grammar pieces.
    │   ├── kind.rs                         # UPDATE: add Kind::AudioLevel + per-Kind branches.
    │   ├── constraint.rs                   # UPDATE: rewrite to peer-key inference via #[serde(untagged)].
    │   ├── shape.rs                        # UPDATE: custom Deserialize + Serialize + JsonSchema for Slot; reserved-keyword check.
    │   ├── binding.rs                      # UPDATE: fix on-disk JSON shape to bind = { bus = "…" }.
    │   ├── value_spec.rs                   # UPDATE: extend ValueSpec serde to handle Color { space, coords } table form.
    │   ├── visual/                         # NEW
    │   │   ├── mod.rs                      # NEW (re-exports)
    │   │   ├── shader_ref.rs               # NEW (ShaderRef enum + JsonSchema)
    │   │   ├── visual_input.rs             # NEW (VisualInput enum + JsonSchema)
    │   │   ├── transition_ref.rs           # NEW (TransitionRef struct)
    │   │   ├── params_table.rs             # NEW (ParamsTable newtype; custom Deserialize + JsonSchema)
    │   │   ├── pattern.rs                  # NEW (Pattern struct + impl Artifact)
    │   │   ├── effect.rs                   # NEW (Effect struct + impl Artifact)
    │   │   ├── transition.rs               # NEW (Transition struct + impl Artifact)
    │   │   ├── stack.rs                    # NEW (Stack struct + impl Artifact)
    │   │   ├── live.rs                     # NEW (Live struct + LiveCandidate + impl Artifact; barebones)
    │   │   └── playlist.rs                 # NEW (Playlist struct + PlaylistEntry + PlaylistBehavior + impl Artifact)
    │   └── artifact/
    │       ├── mod.rs                      # UPDATE: re-export load_artifact + LoadError.
    │       └── load.rs                     # NEW (std-only LpFs-based loader)
    └── tests/
        └── round_trip.rs                   # NEW (integration: load → serialize → reload; schema-vs-loader drift)
```

## Conceptual architecture

### Visual taxonomy (per `domain.md`)

```
Visual ::= Pattern    { shader: ShaderRef,  params: ParamsTable }
        | Effect     { shader: ShaderRef,  input: Option<VisualInput>, params: ParamsTable }
        | Transition { shader: ShaderRef,  params: ParamsTable }    // no input field; A/B are implicit uniforms
        | Stack      { input: Option<VisualInput>, effects: Vec<EffectRef>, params: ParamsTable }
        | Live       { candidates: Vec<LiveCandidate>, transition: TransitionRef, bindings: ... }
        | Playlist   { entries:    Vec<PlaylistEntry>, transition: TransitionRef, behavior: PlaylistBehavior, bindings: ... }
```

Every Visual carries the same fixed top-level header
(`schema_version: u32`, `title: String`, `description:
Option<String>`, `author: Option<String>`).

### `ShaderRef` — three mutex variants

```rust
pub enum ShaderRef {
    Glsl    { glsl: String },   // inline GLSL source
    File    { file: String },   // sibling path; language inferred from extension
    Builtin { name: Name },     // builtin Rust impl
}
```

TOML form (mutex by which key is present):

```toml
[shader]
glsl = """…"""
# OR
file = "main.glsl"
# OR
builtin = "fluid"
```

`#[serde(untagged)]` + `#[serde(deny_unknown_fields)]` per variant.
Loader fails with `LoadError::ShaderRefShape` if zero or two+ keys are
present. Used by Pattern, Effect, Transition; not used by Stack, Live,
Playlist (those reference other Visuals via `ArtifactSpec`).

### `VisualInput` — composition vs routing

```rust
pub enum VisualInput {
    Visual { spec: ArtifactSpec, params: BTreeMap<String, toml::Value> }, // compose child
    Bus    { channel: ChannelName },                                       // route from bus
}
```

TOML form:

```toml
[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0
```

OR

```toml
[input]
bus = "video/in/0"
```

`#[serde(untagged)]` + `#[serde(deny_unknown_fields)]` per variant.
`bus` key (not `channel`) intentionally matches the `bind = { bus = "…" }`
vocabulary on Slot bindings. `Stack.input: Option<VisualInput>` and
`Effect.input: Option<VisualInput>`. Transitions have no `input` field.

`Binding` stays exactly as M2 left it (after step 01's on-disk-shape
fix). `[input]` is **structural composition**, not a binding; bindings
are pure routing and never instantiate nodes.

### `Constraint` — peer-key inference

Rewritten in step 04 to dispatch on which peer key is present:

```rust
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Constraint {
    Range  { range: [f32; 2], #[serde(default, skip_serializing_if = "Option::is_none")] step: Option<f32> },
    Choice { choices: Vec<f32>, labels: Vec<String> },
    Free   {},
}
```

Per-variant `#[serde(deny_unknown_fields)]` so a typo in `range` /
`choices` / `step` / `labels` becomes a hard load error rather than a
silent fallback. TOML form (driven by `quantity.md` §10):

```toml
[params.octaves]
kind  = "count"
range = [1, 6]
step  = 1
```

```toml
[params.mode]
kind    = "choice"
choices = [0, 1, 2]
labels  = ["off", "warm", "cold"]
```

```toml
[params.boost]
kind = "amplitude"
# (Free — no range, no choices)
```

`Constraint::Range` → `range = [min, max]` is a 2-tuple, **not**
separate `min` / `max` keys. Matches `quantity.md` §10's grammar
literally and reads better in TOML.

### `Slot` TOML grammar

`Slot` is the one type that genuinely requires custom `Deserialize` +
custom `Serialize` + custom `JsonSchema`. The rule (per `quantity.md`
§10):

- `shape` defaults to `"scalar"` if omitted.
- Reserved keywords `element` / `props` only appear at their
  structural positions (Array's `element`, Struct's `props`).
- Top-level `[params]` is implicit `Shape::Struct` — handled by the
  separate `ParamsTable` newtype.

The custom impls go through a `toml::Value::Table` intermediate. The
JSON Schema marks `shape` optional with default `"scalar"`. The
round-trip test pair (loader-vs-serializer + loader-vs-schema) catches
drift.

### `ParamsTable` newtype — implicit struct synthesis

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct ParamsTable(pub Slot);
```

`ParamsTable::deserialize` reads a table; each table key becomes a
struct member name; each value re-feeds through `Slot::deserialize`.
Synthesizes a `Shape::Struct { fields, default: None }` from the
table's keys. `Slot::deserialize` stays unaware of the implicit
top-level rule.

`Pattern { ..., params: ParamsTable }` instead of `Pattern { ...,
params: Slot }` makes the deserialization rule type-driven, which is
what we want.

### Live / Playlist entry shapes

```rust
pub struct LiveCandidate { visual: ArtifactSpec, priority: f32, params: BTreeMap<String, toml::Value> }
pub struct TransitionRef { visual: ArtifactSpec, duration: f32, params: BTreeMap<String, toml::Value> }
pub struct PlaylistEntry { visual: ArtifactSpec, duration: Option<f32>, params: BTreeMap<String, toml::Value> }
pub struct PlaylistBehavior { #[serde(default)] r#loop: bool }

pub struct Live {
    schema_version: u32, title: String, description: Option<String>, author: Option<String>,
    candidates: Vec<LiveCandidate>,
    transition: TransitionRef,
    bindings: Vec<(String, Binding)>,
    // No `selection` field; deferred.
}

pub struct Playlist {
    schema_version: u32, title: String, description: Option<String>, author: Option<String>,
    entries: Vec<PlaylistEntry>,
    transition: TransitionRef,         // default for all entries
    behavior: PlaylistBehavior,
    bindings: Vec<(String, Binding)>,
}
```

`PlaylistEntry.duration: None` means wait-for-cue. `Live` is barebones
in M3 (no `[selection]`); see deferred-list above.

### `Color` defaults — `{ space, coords }` table

OKLCH is the default authoring space:

```toml
[params.color]
kind    = "color"
default = { space = "oklch", coords = [0.7, 0.15, 90] }
```

`space` is a snake-case `Colorspace` string (already in `kind.rs`);
`coords` is always a 3-tuple of f32. Field order in serialized output
is `space` first, `coords` second (matches `Kind::Color::storage()`'s
member order; deterministic round-trip).

### `Kind::AudioLevel`

The only new signal Kind in M3:

- **Storage:** `LpsType::Struct` of `{ low: F32, mid: F32, high: F32 }`.
- **Default constraint:** `Free` (RMS levels can exceed 1.0 with boost).
- **Default presentation:** `NumberInput` (placeholder; level-meter
  widget lands additively).
- **Default bind:** `Some(Binding::Bus { channel: "audio/in/0/level" })`.
- **Dimension:** `Dimensionless`.

Used in `fluid.pattern.toml`:

```toml
[params.intensity]
kind    = "audio_level"
default = { low = 0.0, mid = 0.0, high = 0.0 }
```

The default `bind` from `Kind::AudioLevel::default_bind()` wires
`intensity` to `audio/in/0/level` automatically; no explicit `bind`
needed unless overriding.

### Loader API surface

```rust
pub fn load_artifact<T, F>(fs: &F, path: &str) -> Result<T, LoadError>
where
    T: Artifact + serde::de::DeserializeOwned,
    F: lpfs::LpFs,
{ … }
```

Single generic entry point. `LoadError` enum captures parse errors,
shape-mutex violations (ShaderRef, VisualInput, Constraint), reserved
keyword violations, schema_version mismatches, and IO failures.
std-feature-gated (LpFs needs std).

### Round-trip + schema-drift tests

Two paired integration tests per example file:

1. **load → serialize → reload → structural-equal.** Catches
   `Deserialize` ↔ `Serialize` drift.
2. **parsed `toml::Value` validates against
   `schema_for::<Pattern>()`.** Catches `Deserialize` ↔ `JsonSchema`
   drift.

Both must pass for every file in `examples/v1/`.

## Notable design decisions (carried into the implementation)

These come from the user's chat answers + `00-notes.md`. Summary; the
notes file has the full reasoning.

- **Mixed Visual struct shape, not single big Slot** (Q-D1).
- **Mutex flat keys for `[shader]`**, three variants: `glsl` / `file` /
  `builtin` (Q-D2).
- **`VisualInput` is its own type, not a `Binding` variant** (Q-D3).
  Composition vs routing.
- **`bus` key (not `channel`)** in `[input] bus = "…"` (Q-D3,
  vocabulary alignment with `bind = { bus = "…" }`).
- **Per-entry transition overrides dropped** from Playlist (Q-D4).
- **`Live` ships barebones, no `[selection]`** (Q-D4d).
- **`AudioLevel` is the only new Kind in M3** (Q-D5).
- **Hybrid serde strategy:** `untagged` for peer-key enums, custom for
  `Slot` + `ParamsTable` only (Q-D6). Custom `JsonSchema` for the
  custom types so completion stays accurate.
- **`{ space, coords }` table for Color defaults**, OKLCH default
  authoring space, 3-tuple coords (Q-D7).
- **`[bindings]` cascade kept** (Q2 reversal): `Vec<(String, Binding)>`
  with raw relative-NodePropSpec keys; no parsing in M3.
- **`Constraint` stays F32-only** (Q3); the existing M2 TODO carries
  forward.

## Phase split

```
01. binding.rs on-disk shape fix                         [sub-agent: yes,  parallel: 02]
02. Kind::AudioLevel                                     [sub-agent: yes,  parallel: 01]
03. Slot custom serde + JsonSchema                       [sub-agent: yes,  parallel: -]   (depends on 01, 02)
04. Constraint + ShaderRef + VisualInput peer-key enums  [sub-agent: yes,  parallel: 05]   (depends on 03)
05. ParamsTable + Color { space, coords } defaults       [sub-agent: yes,  parallel: 04]   (depends on 03)
06. Pattern, Effect, Transition Visual structs           [sub-agent: yes,  parallel: 07-prep]   (depends on 04, 05)
07. Stack, Live, Playlist Visual structs                 [sub-agent: yes,  parallel: -]   (depends on 06)
08. LpFs-based load_artifact loader                      [sub-agent: yes,  parallel: 09]   (depends on 07)
09. examples/v1/ corpus migration                        [sub-agent: yes,  parallel: 08]   (depends on 07)
10. Round-trip + schema-drift integration tests          [sub-agent: yes,  parallel: -]   (depends on 08, 09)
11. Update docs/design/lpfx/overview.md + sibling docs   [sub-agent: yes,  parallel: -]   (depends on 09)
12. Cleanup, validation, summary.md                      [sub-agent: supervised]
```

Phases 04 / 05 are split because the user's "schemas matter for code
completion" success criterion makes step 05 the riskiest custom-serde
step; isolating it keeps the diff small. Step 04 is mostly mechanical
`#[serde(untagged)]` boilerplate.

Step 12 owns the `summary.md` writeup and any final edits to the M3
roadmap / overview docs.
