# M3 — Visual artifact types + canonical examples + TOML grammar — notes

Roadmap milestone: [`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`](../../roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md).

Authoritative design refs that this plan implements:

- [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
  — primary spec; §10 locks the `Slot` TOML grammar this milestone implements.
- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md)
  — Visual taxonomy (Pattern / Effect / Transition / Stack / Live / Playlist).
- [`docs/design/color.md`](../../design/color.md) — `Color` value carries
  `space` field; default authoring space is OKLCH.
- [`docs/design/lpfx/overview.md`](../../design/lpfx/overview.md) — explicit
  `bind = { … }` form, `[bindings]` cascade rules, ancestor-wins resolution.

Predecessor plan summary: `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md`.

## Conventions for sub-agents executing this plan

These apply to every step in this plan. Each step file should
restate the relevant subset in its own "Conventions" section so a
sub-agent reading a single step file in isolation still gets them.

### File layout (per `AGENTS.md` § "Code organization in Rust source files")

Top → bottom = most important → least important → tests.
Concretely, inside every new `.rs` file:

1. Module-level docs, `use`s, type aliases, constants.
2. Public types / entry points / the headline impl.
3. Supporting types and their impls.
4. Private helper functions.
5. `#[cfg(test)] mod tests { … }` — **always at the bottom**.
   Inside the test module, `#[test]` functions come first; shared
   test helpers live below them.

This is the **opposite** of the "tests first" convention used in
archived `docs/plans-old/` files. That convention is deprecated.
Do not adopt it in new code.

### Domain-model documentation level

Document the domain model **well, but not overly much.** Concretely:

- **Yes:**
  - Crate-level / module-level rustdoc explaining what the type
    *is* in the lpfx domain (what it models, where it sits in
    the Visual taxonomy, who composes it).
  - One short paragraph on every public type explaining its role
    and pointing at the design doc that owns the concept (e.g.
    `quantity.md` §10 for `Slot` grammar, `color.md` for
    `Color`).
  - Inline notes where the on-disk grammar differs from the Rust
    shape (e.g. "`shape` defaults to `\"scalar\"` if omitted, see
    `quantity.md` §10").
  - `# Examples` blocks on the headline public types, showing the
    TOML form and what it deserializes to.
- **No:**
  - Re-explaining what the design docs already explain. Link
    instead.
  - `///` on every field of a derived `Deserialize` struct when
    the field name + type already say what it is.
  - Aspirational future-work comments. Future work belongs in
    `docs/future-work/`.
  - Multi-paragraph essays inside type docs. If you need more
    than ~10 lines of rustdoc to explain a type, the design doc
    needs a fix instead.

The goal: someone reading `lp-domain` source for the first time
should be able to follow the Visual taxonomy without flipping to
the design docs, but the design docs remain the single source of
truth for *how* and *why*.

### Test placement vs. unit coverage

Step files in this plan call for unit tests (in-file
`#[cfg(test)]`) for round-trip serde, peer-key inference, and
schema accuracy. These belong **at the bottom of the same file as
the type under test**, not in `lp-domain/lp-domain/tests/` —
those are reserved for the integration round-trip suite (step 10).

## Scope of work

Implement the typed Rust structs for all six Visual kinds plus the TOML
grammar that drives `Slot` shape inference. Ship the canonical example
corpus at `lp-domain/lp-domain/examples/v1/`. Round-trip tests
prove the loader and serializer agree.

In scope (mirrors the milestone file, with M3 priorities surfaced):

1. **Six typed Visual structs** in `lp-domain/lp-domain/src/visual/`,
   each `impl Artifact` with `KIND` and `CURRENT_VERSION = 1`:
   `pattern.rs` (collapses former ShaderPattern + BuiltinPattern),
   `effect.rs`, `transition.rs`, `stack.rs`, `live.rs`, `playlist.rs`.
2. **Common substructure types**: `ShaderRef` (inline GLSL OR builtin
   name); `[bindings]` cascade representation; `[input]` polymorphic
   slot for Stack / Effect; entry shapes for Live / Playlist.
3. **TOML grammar for `Slot`** per [`quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar):
   custom serde `Deserialize` for `Slot`, `shape` defaults to
   `"scalar"`, top-level `[params]` is implicit `Shape::Struct`,
   reserved keywords (`params`, `element`, `props`), `default`
   omitted on compositions and computed from children.
4. **Signal Kinds added incrementally** as examples need them
   (Q9 of the roadmap notes). Probably `AudioLevel` first, driven
   by `fluid.pattern.toml`'s intensity binding.
5. **Eight canonical example TOMLs** under
   `lp-domain/lp-domain/examples/v1/<kind>/` with `schema_version = 1`,
   unified `[shader]` section (no more `[builtin]` split), explicit
   `bind = { … }` table, and `<kind>/<dir>/<channel>` channel naming.
6. **Move + delete**: drop the `docs/design/lpfx/{patterns,effects,
   transitions,stacks,lives,playlists}/` TOMLs; update design docs to
   point at the canonical paths under `examples/v1/`.
7. **Round-trip integration tests** at
   `lp-domain/lp-domain/tests/round_trip.rs`: load → assert key
   structural fields → serialize → re-load → compare structurally.
8. **`LpFs`-based loader stub** at `src/artifact/load.rs` (std-only,
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
  Q8 below confirms it stays F32-only in M3 unless an example
  forces the issue).

## Current state of the codebase

`lp-domain/lp-domain/` after M2:

- `lib.rs` — `no_std + alloc` default; optional `std` and
  `schema-gen` features; re-exports `LpsValue`, `LpsType`,
  `TextureBuffer`, `TextureStorageFormat`.
- `types.rs` — `Uid`, `Name`, `NodePath`, `NodePropSpec`,
  `ArtifactSpec`, `ChannelName`.
- `kind.rs` — open `Kind` enum (16 v0 variants: Amplitude, Ratio,
  Phase, Count, Bool, Choice, Instant, Duration, Frequency, Angle,
  Color, ColorPalette, Gradient, Position2d, Position3d, Texture)
  plus per-Kind impls (`storage`, `dimension`, `default_constraint`,
  `default_presentation`, `default_bind`). `MAX_PALETTE_LEN` and
  `MAX_GRADIENT_STOPS` constants live here.
- `constraint.rs` — `Constraint` (Free / Range / Choice), F32-only
  (TODO to widen).
- `shape.rs` — `Shape` (Scalar / Array / Struct, internally tagged
  on `shape`) and `Slot { shape, label, description, bind, present }`.
  `Slot::default_value()` and `Slot::storage()` implemented; serde
  round-trip tests pass on JSON.
- `value_spec.rs` — `ValueSpec::Literal(LpsValue) | Texture(TextureSpec)`
  with hand-written serde via a wire shadow, `LoadCtx` stub with
  `next_texture_handle`, `TextureSpec::Black` materializes to a
  1×1 handle struct.
- `binding.rs` — `Binding::Bus { channel: ChannelName }`, externally
  tagged. **TODO** to align JSON form with on-disk `bind = { bus = "…" }`
  per `quantity.md` §8 (the M2 form is `{"bus":{"channel":"…"}}`).
- `presentation.rs` — `Presentation` enum (10 v0 variants).
- `artifact/mod.rs` — re-exports `Artifact`, `Migration`, `Registry`.
- `schema/mod.rs` — `Artifact` and `Migration` traits + empty
  `Registry` stub. (`Artifact::KIND`, `CURRENT_VERSION` are
  associated consts; bounds are deliberately minimal — M5 adds
  `DeserializeOwned` + `JsonSchema`.)
- `node/mod.rs` — `Node` trait with property accessors.
- `schema_gen_smoke.rs` — `schemars::schema_for!` smoke tests on
  every public type (compiles only with the `schema-gen` feature).

What's **not** there yet: any concrete Visual struct, any TOML
loading, any examples on disk, the `bind = { bus = "…" }` form.

`docs/design/lpfx/` (the soon-to-be-deleted source of truth for
example TOMLs):

- `patterns/{rainbow,fbm,fluid}.pattern.toml` — old schema:
  `[shader]` for inline GLSL **or** `[builtin]` for impl-backed
  patterns; per-param `type = "f32"` instead of `kind = "…"`;
  `min`/`max` instead of `range = […]`; `unit = "rad"` field that
  v0 explicitly removes; `ui.fader = { step = 0.1 }` widget hints
  that should now flow through `Constraint::step` + Kind-default
  `Presentation`.
- `effects/{tint,kaleidoscope}.effect.toml` — same shape;
  `tint.color = [1.0, 0.6, 0.4]` needs `{ space = "oklch", coords = […] }`
  per `color.md`.
- `transitions/{crossfade,wipe}.transition.toml` — same shape;
  `param_progress` is conventionally driven by the parent Show
  (no default bind).
- `stacks/psychedelic.stack.toml` — `[input] visual = "…"` +
  `[input.params]` + `[[effects]] visual = "…"` + `[effects.params]`.
- `lives/main.live.toml` — `[[candidates]]` with `priority`,
  shared `[transition]`, `[selection]`, `[bindings]` cascade.
- `playlists/setlist.playlist.toml` — `[[entries]]` with
  `duration` + optional per-entry `[entries.transition]`,
  `[behavior] loop = true`, `[bindings]` cascade.

`docs/design/lpfx/overview.md` is the concept layer; the deleted
`concepts.md` (per `git status`) was an older lpfx vocabulary file
that overlapped with `domain.md`. M3 just needs to update
`overview.md` (and any sibling docs that point at `patterns/` etc.)
to reference `lp-domain/lp-domain/examples/v1/<kind>/`.

## Confirmation-style questions (batch)

Answer with `Q1 yes, Q5 no, …` or `all yes`; anything you push back
on graduates to a discussion-style question below.

| #   | Question                                                                                                                | Context (1 line)                                                | Suggested answer                                |
| --- | ----------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------- |
| Q1  | Each Visual artifact carries top-level fields `schema_version: u32`, `title: String`, `description: Option<String>`, `author: Option<String>`? | All current example TOMLs have these, plus `schema_version` is a Q5 deliverable. | Yes |
| Q2  | `[bindings]` table → in-memory `Vec<(String, Binding)>`, keys kept as raw relative-NodePropSpec strings (no parsing in M3)? | M3 punts cross-artifact validation; binding-key parsing belongs to it. | Yes |
| Q3  | `Constraint` stays F32-only in M3 (per the existing `TODO(quantity widening)` in `constraint.rs`)?                       | Existing examples cast Counts (e.g. octaves 1..6) into F32 ranges already. | Yes |
| Q4  | Loader API surface: a single generic `pub fn load_artifact<T: Artifact + DeserializeOwned, F: LpFs>(fs: &F, path: …) -> Result<T, LoadError>`? | Generic keeps the loader tiny; per-Visual wrappers are 3-line trampolines if needed. | Yes |
| Q5  | Round-trip tests use real files via `lpfs::LpFsStd` reading `lp-domain/lp-domain/examples/v1/`, not inline TOML strings? | Exercises both loader and serializer against the canonical corpus. | Yes |
| Q6  | Drop `unit = "…"` fields entirely from new examples (per `quantity.md` §4: stored values are in the Kind's base unit)?  | `Angle` is implicit-radians; `Frequency` implicit-Hz; etc.       | Yes |
| Q7  | Drop `ui.fader = { step = … }` / `ui.stepper` / `ui.color` UI hints; let `Constraint::step` + `Kind::default_presentation()` carry the same intent? | `quantity.md` §9 punts per-variant UI config until a real example demands it. | Yes |
| Q8  | Each new TOML example places `schema_version = 1` as the very first line under a header comment?                         | Trivial / mechanical. | Yes |
| Q9  | Examples use `kind = "…"` with snake-case Kind names (matches `serde(rename_all = "snake_case")` on the `Kind` enum)?    | Existing M2 enum already serializes that way. | Yes |
| Q10 | Old `docs/design/lpfx/{patterns,…}/*.toml` files get deleted in the same plan (no compatibility shim, no copy)?         | Milestone says "are deleted"; fresh corpus at `examples/v1/`.   | Yes |
| Q11 | Update `docs/design/lpfx/overview.md` (and any sibling docs that reference the old paths) to point at `lp-domain/lp-domain/examples/v1/`? | Milestone deliverable: "design docs … updated to point at the canonical paths". | Yes |
| Q12 | The `binding.rs` `TODO(M3)` to align serde JSON shape with `bind = { bus = "…" }` lands in this plan?                   | Q2 of the roadmap notes; on-disk form `bind = { bus = "…" }` is canonical. | Yes |
| Q13 | The `LpFs`-based loader stub gates behind the existing `std` feature on `lp-domain` (no new feature flag)?              | `lpfs::LpFsStd` is std-only and `lpfs` is already an optional `std` dep. | Yes |
| Q14 | Add an integration smoke test that asserts each example file declares `schema_version = 1` (catches drift if someone bumps without going through M5)? | Cheap mechanical guard. | Yes |
| Q15 | Round-trip equality is **structural** (parsed-struct equality), not textual (TOML byte-for-byte)?                       | Milestone explicitly says "Textual round-trip is a stretch goal." | Yes |

## Discussion-style questions

Asked one at a time below; answers folded into `# Notes` as they
land. Each opens with a `## QN: …` header so it's easy to scan.

---

## Q-D1: Visual struct shape — fixed-schema fields plus a `params` slot, or a single big Slot?

A Pattern's TOML carries a fixed set of top-level fields
(`title`, `description`, `author`, `schema_version`, `[shader]`)
**plus** the implicit-`Shape::Struct` `[params]` block that
`quantity.md` §10 makes special. Two ways to model that:

1. **Mixed struct (suggested).** `Pattern` is a hand-rolled Rust
   struct with concrete fields for the fixed bits, plus a
   `params: Slot` field that carries the implicit-struct param
   surface. `[params]` deserializes via the custom `Slot`
   parser; everything else uses derived serde.

2. **Slot-only.** Make the entire Visual a single big `Slot` with
   reserved field names. Forces every Pattern-specific field
   (like `[shader]`) into the Slot grammar's metadata layer.

**Suggested answer:** Mixed struct. The fixed schema is short and
each Visual kind has *different* fixed fields (Stack has `[input]`
+ `[[effects]]`; Live has `[[candidates]]` + `[transition]` +
`[selection]`); collapsing into one Slot would force a sum-type
escape hatch we don't otherwise need. The custom `Deserialize`
for `Slot` only fires when serde encounters a field typed `Slot`
(i.e., the `params:` field).

---

## Q-D2: `[shader]` section shape — one section, two source variants?

The milestone collapses former `ShaderPattern`/`BuiltinPattern`
into a single `Pattern` with a unified `[shader]` section. Three
candidate TOML shapes:

1. **Mutex flat keys (suggested):** `[shader] glsl = "…"` for
   inline source, `[shader] builtin = "fluid"` for builtin
   patterns. Loader fails if both or neither key is present.
   Rust shape:
   ```rust
   pub enum ShaderRef {
       Inline { glsl: String },
       Builtin { name: Name },
   }
   ```
   serde via internal-tag-by-presence (custom `Deserialize` —
   ~10 LOC).

2. **Discriminator field:** `[shader] kind = "inline" | "builtin"`
   plus `glsl` or `name`. Cleaner serde (derived); ~5 more
   characters per file in TOML.

3. **Inline string shorthand:** `shader = "…glsl…"` short form +
   `[shader] builtin = "…"` long form. Mixes scalar and table
   forms; serde gets uglier.

**Suggested answer:** Option 1 (mutex flat keys). Matches the
existing example TOMLs closely (just rename `[builtin] impl =
"fluid"` → `[shader] builtin = "fluid"`), keeps inline source
"first-class," and the custom `Deserialize` is short and obvious.
Effects, Transitions, Stacks, Live, Playlist all reuse `ShaderRef`
where they have a shader (Effect / Transition / Pattern do; Stack
/ Live / Playlist do not — those reference *other* Visuals via
`ArtifactSpec`).

---

## Q-D3: Stack `[input]` and Effect `[input]` polymorphism — visual or channel?

Current stack TOML:

```toml
[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0
```

The milestone says "polymorphic slot for Stack / Effect (visual or
channel)." Two source kinds:

- **Visual reference:** `visual = "…"` (ArtifactSpec) + optional
  `[input.params]` overrides.
- **Channel reference:** `channel = "video/in/0"` (ChannelName).

Suggested Rust shape:

```rust
pub enum VisualInput {
    Visual {
        spec: ArtifactSpec,
        #[serde(default)]
        params: BTreeMap<String, toml::Value>,
    },
    Channel {
        channel: ChannelName,
    },
}
```

Tagged by *presence* of the `visual` vs `channel` key (custom
`Deserialize` — same ~10-LOC pattern as `ShaderRef`).

The `params` overrides are kept as raw `toml::Value` in v0 because
type-checking them requires resolving the referenced Visual's
param schema, which is a cross-artifact concern explicitly out of
scope for M3.

**Suggested answer:** Take the suggestion as-is. Effect's `[input]`
shares the `VisualInput` type (Effect is 1-arity, so it has
exactly one `[input]` block).

Open sub-question: Transitions are 2-arity (`inputA`, `inputB`).
Do they get `[input_a]` + `[input_b]` blocks, or do Transitions
*not* declare inputs in TOML at all (the parent Show wires the
two textures via `[bindings]`)? Current `crossfade.transition.toml`
does **not** declare any `[input*]` block — `inputA`/`inputB` are
implicit shader uniforms wired by the runtime. Suggested:
keep that — Transitions have no `[input]` field; their inputs are
implicit shader uniforms `inputA` / `inputB` named by convention.

---

## Q-D4: Live candidates and Playlist entries — entry shape?

Current TOMLs:

- Live: `[[candidates]] visual + priority`, plus a shared
  `[transition]` block and a `[selection]` block.
- Playlist: `[[entries]] visual + duration`, with optional
  per-entry `[entries.transition]` override and a `[behavior]
  loop = true` block.

Suggested Rust shapes:

```rust
pub struct LiveCandidate {
    pub visual: ArtifactSpec,
    pub priority: f32,
    #[serde(default)]
    pub params: BTreeMap<String, toml::Value>,
}

pub struct TransitionRef {
    pub visual: ArtifactSpec,
    pub duration: f32,
}

pub struct PlaylistEntry {
    pub visual: ArtifactSpec,
    #[serde(default)]
    pub duration: Option<f32>,         // None ⇒ wait-for-cue
    #[serde(default)]
    pub transition: Option<TransitionRef>,
    #[serde(default)]
    pub params: BTreeMap<String, toml::Value>,
}

pub struct LiveSelection {
    pub min_hold: f32,
    pub debounce: f32,
}

pub struct PlaylistBehavior {
    #[serde(default)]
    pub r#loop: bool,
}
```

**Suggested answer:** Take as-is. `params` overrides on candidates
/ entries are kept as raw `toml::Value` for the same reason as
`VisualInput::Visual.params`.

---

## Q-D5: Which signal Kind(s) land in M3?

The milestone says "at least one example exercises an audio-side
Kind (likely `fluid.pattern.toml` consuming `audio/in/0/bands` or
similar)." Walking each example:

| Example                 | Existing params                                               | Audio touch?                                       |
| ----------------------- | ------------------------------------------------------------- | -------------------------------------------------- |
| `rainbow.pattern.toml`  | time, speed, saturation                                       | none                                               |
| `fbm.pattern.toml`      | time, scale, hueShift, octaves                                | none                                               |
| `fluid.pattern.toml`    | resolution, viscosity, fade, solver_hz, intensity, emitter_x/y | yes — fluid is the canonical "drives off audio"   |
| `tint.effect.toml`      | time, color, amount                                           | none                                               |
| `kaleidoscope.effect.toml` | time, slices, rotation                                     | none                                               |
| `crossfade.transition.toml` | time, progress                                            | none                                               |
| `wipe.transition.toml`  | time, progress, angle, softness                               | none                                               |
| `psychedelic.stack.toml` | (composes fbm + kaleidoscope + tint, no audio of its own)    | none                                               |

Suggested:

- Add `Kind::AudioLevel` first (struct of low/mid/high F32s per
  `quantity.md` §3). Lets `fluid.pattern.toml` declare e.g.
  `intensity` bound to `audio/in/0/level` (an AudioLevel-typed
  channel) without bringing in a heavyweight bulk Kind.
- That's the only new signal Kind in M3. Add `Audio` /
  `AudioFft` / `Beat` / `Touch` / `Motion` later when an example
  actually consumes them.

Implementation per Kind: variant on `Kind`; per-Kind branches in
`storage()` / `dimension()` / `default_constraint()` /
`default_presentation()` / `default_bind()`; per-`AudioLevel`
storage struct (Vec3-sized). No project-wide constants needed for
`AudioLevel` (it's a fixed 3-field struct).

**Suggested answer:** Add only `Kind::AudioLevel` in M3. Lock the
fluid example's audio-reactive parameter to that Kind; defer
`Kind::Audio` etc. until M3+ examples demand them.

---

## Q-D5 — Signal Kind(s) added in M3 — RESOLVED

`Kind::AudioLevel` is the only new signal Kind added in M3.

- **Storage:** `LpsType::Struct` of `{ low: F32, mid: F32, high: F32 }`
  per `quantity.md` §3 (one fixed Vec3-shaped struct, no
  project-wide constants).
- **Default constraint:** `Constraint::Free` (RMS levels can
  exceed 1.0 with boost; clamping is downstream policy).
- **Default presentation:** `Presentation::NumberInput` (placeholder
  — no level-meter widget variant yet; lands additively with M3+).
- **Default bind:** `Some(Binding::Bus { channel: "audio/in/0/level" })`
  — channel naming follows the lpfx convention.
- **Dimension:** `Dimensionless`.

Used in `fluid.pattern.toml` directly:

```toml
[params.intensity]
kind    = "audio_level"
default = { low = 0.0, mid = 0.0, high = 0.0 }
```

The default `bind` from `Kind::AudioLevel::default_bind()` wires
`intensity` to `audio/in/0/level` automatically; no explicit
`bind = { … }` needed. Other examples can override with their own
`bind` if they want a different channel.

All other signal Kinds (`AudioFft`, `Beat`, `Touch`, `Motion`,
etc.) defer until an example actually consumes them. Audio is the
first input class lpfx will support, so landing `AudioLevel` now
is the right sequencing.

## Q-D6: TOML parser strategy for `Slot` — custom `Deserialize` mechanics?

`quantity.md` §10 says ~30 LOC of glue beyond `#[derive]`. The
plan:

1. Hand-write `impl<'de> Deserialize<'de> for Slot`. It deserializes
   into a `toml::Value::Table`, peeks at the `shape` field, and
   defaults it to `"scalar"` if missing.
2. Once the discriminator is normalized, *re*-serialize through
   the derived `Deserialize for Shape` to populate `Slot.shape`.
3. The implicit-`[params]` top-level is handled at the *Visual*
   layer (not in `Slot::deserialize`). Each Visual struct that
   has params declares `pub params: ParamsTable` (a newtype
   wrapping `Slot` with `Shape::Struct`); `ParamsTable`'s
   `Deserialize` reads the table fields and synthesizes a
   `Shape::Struct`.
4. Reserved keyword check (`element`, `props`, `params` at
   non-top-level) lives in the `Slot::deserialize` path.
5. Constraint fields (`range`, `step`, `choices`, `labels`) are
   peers of `kind` on Scalar; the `Shape::Scalar` derived
   `Deserialize` already handles them via `Constraint`'s
   internal-tag form **— except** the existing `Constraint` is
   `tag = "type"` internally tagged (`{"type":"range",…}`) which
   doesn't match the §10 grammar (`range = [0, 5]`). Need a
   custom `Deserialize for Constraint` too: peer-key inference
   (presence of `range` ⇒ `Range`, etc.).

So the actual parser glue is:

- `Deserialize for Slot` (~25 LOC: peek `shape`, default,
  re-feed; check reserved keywords).
- `Deserialize for Constraint` (~30 LOC: peer-key inference).
- `Deserialize for ShaderRef` (~15 LOC: peer-key inference).
- `Deserialize for VisualInput` (~15 LOC: peer-key inference).
- `Serialize` for the same: emit `shape = "scalar"` only when
  needed, omit `default = null`, etc. (~30 LOC across types).

That's ~115 LOC of glue across four types — more than the
"30 LOC" headline number in the milestone, but the milestone's
30 was a `Slot`-only count. All other glue is consequence of the
TOML grammar choices.

**Suggested answer:** Take the strategy as-is; budget the LOC.
(Custom serde is *very* well-trodden in Rust, but it adds up.)

---

## Q-D6 — TOML parser strategy + JsonSchema accuracy — RESOLVED

Hybrid strategy: stock `#[serde(untagged)]` for the peer-key
mutex enums; custom `Deserialize` + custom `JsonSchema` only where
truly needed.

### Per-type plan

| Type           | `Deserialize`                            | `JsonSchema`                                            |
| -------------- | ---------------------------------------- | ------------------------------------------------------- |
| `Constraint`   | `#[serde(untagged)]` derive              | derive (free `oneOf`)                                   |
| `ShaderRef`    | `#[serde(untagged)]` derive              | derive (free `oneOf`)                                   |
| `VisualInput`  | `#[serde(untagged)]` derive              | derive (free `oneOf`)                                   |
| `Slot`         | **custom** (~25 LOC, defaults `shape`)   | **custom** (~30 LOC, marks `shape` optional + default)  |
| `ParamsTable`  | **custom** (~10 LOC, implicit struct)    | **custom** (~15 LOC, propertyNames = struct members)    |

`untagged` enums get `#[serde(deny_unknown_fields)]` per variant
to convert silent-fallback bugs (TOML with both `range` and
`choices`, etc.) into hard load errors.

### Custom `Deserialize` mechanics (Q-D6c → `toml::Value` IR)

`Slot::deserialize` and `ParamsTable::deserialize` go through a
`toml::Value::Table` intermediate:

1. Deserialize into `toml::Value::Table`.
2. For `Slot`: peek `shape`; default to `"scalar"` if missing;
   re-feed through derived `Deserialize for Shape`.
3. For `ParamsTable`: every table key becomes a struct member name;
   each value re-feeds through `Slot::deserialize`. Synthesizes the
   `Shape::Struct` from the table's keys.
4. Reserved-keyword check (`element`, `props`, `params`) lives in
   the `Slot::deserialize` path.

Performance is irrelevant at artifact load time; clarity wins.

### Custom `Serialize` mechanics (Q-D6a-ish)

Lands in M3 too. Round-trip output matches the §10 grammar
exactly: `shape = "scalar"` omitted when implicit, `default` not
emitted when `None`, etc. Cost: ~30 LOC across `Slot` /
`ParamsTable`. The round-trip-to-disk path (e.g. dev-loop save in
M5+) stays bug-free instead of needing a follow-up later.

### Total LOC budget

~110 LOC of hand-written serde + JsonSchema across two types
(`Slot` + `ParamsTable`). Everything else is derive.

### Schema accuracy is a goal, not a hope

Per the user, code completion in TOML editors via JSON Schema is
a real success criterion. The custom `JsonSchema` impls for
`Slot` / `ParamsTable` are required to keep the schema accurate
in the face of the implicit-`shape` magic and the implicit
`[params]` struct. Any time the custom `Deserialize` accepts an
input shape, the matching custom `JsonSchema` must describe it.

A round-trip test enforcement:
- Round-trip test loads an example TOML.
- Same example is also validated against the generated JSON
  Schema (via `jsonschema`-style validator on the parsed
  `toml::Value`).
- Both paths must accept the same files. Drift = test fails.

This catches Deserialize/JsonSchema drift the same way the
"loader and serializer agree" round-trip catches Deserialize/
Serialize drift.

### Per Q-D6b: `ParamsTable` newtype

Confirmed. The implicit-`Shape::Struct` synthesis lives in
`ParamsTable::deserialize`, not in `Slot::deserialize`. Keeps
`Slot` ignorant of the special "I'm at the implicit top-level
position" rule.

## Q-D7: `Color` value default in TOML — `{ space, coords }` table?

Per `color.md`, a Color value is `{ space: i32, coords: vec3 }`.
TOML form for a default in `tint.effect.toml`:

```toml
[params.color]
kind    = "color"
default = { space = "oklch", coords = [0.7, 0.15, 90] }
```

`space` is a `Colorspace` enum (Oklch, Oklab, LinearRgb, Srgb)
serialized as a string. The on-disk form converts to/from the
i32 internal form at materialize time per `color.md`.

The `default = { … }` is a `ValueSpec::Literal(LpsValue::Struct {
… })`. The struct field-order matters (M2 non-negotiable §4): TOML
inline tables don't guarantee order, so the loader writes the
ordered field list explicitly (`space` first, `coords` second)
based on `Kind::Color::storage()`'s member order.

**Suggested answer:** Take as-is. Default `space = "oklch"` for
all `Color`-Kinded params unless the example has a reason to
differ.

---

# Notes

## Confirmation-style answers (Q1–Q15) — RESOLVED

All `yes` per suggested answers. Highlights:

- **Q2 — `[bindings]` cascade stays in M3.** Briefly considered
  deferring it to support only inline per-`Slot` bindings, but the
  utility of parent-overrides-child wiring (per
  `docs/design/lpfx/overview.md` "ancestor wins") justifies
  shipping it now. In-memory shape `Vec<(String, Binding)>` with
  raw relative-NodePropSpec keys; no key parsing or cross-artifact
  validation in M3.
- All other Q's accepted as suggested.

## Q-D4 — Live candidates and Playlist entries — entry shape — RESOLVED

Final shapes:

```rust
pub struct LiveCandidate {
    pub visual: ArtifactSpec,
    pub priority: f32,
    #[serde(default)]
    pub params: BTreeMap<String, toml::Value>,
}

pub struct TransitionRef {
    pub visual: ArtifactSpec,
    pub duration: f32,
    #[serde(default)]
    pub params: BTreeMap<String, toml::Value>,
}

pub struct PlaylistEntry {
    pub visual: ArtifactSpec,
    /// `None` ⇒ wait-for-cue (no auto-advance).
    #[serde(default)]
    pub duration: Option<f32>,
    #[serde(default)]
    pub params: BTreeMap<String, toml::Value>,
}

pub struct PlaylistBehavior {
    #[serde(default)]
    pub r#loop: bool,
}
```

```rust
pub struct Live {
    pub schema_version: u32,
    pub title: String,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub author: Option<String>,
    pub candidates: Vec<LiveCandidate>,
    pub transition: TransitionRef,
    #[serde(default)] pub bindings: Vec<(String, Binding)>,
    // No `selection` field in M3 — see scope notes below.
}

pub struct Playlist {
    pub schema_version: u32,
    pub title: String,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub author: Option<String>,
    pub entries: Vec<PlaylistEntry>,
    pub transition: TransitionRef,        // default for all entries
    #[serde(default)] pub behavior: PlaylistBehavior,
    #[serde(default)] pub bindings: Vec<(String, Binding)>,
}
```

### Scope reductions agreed:

- **Per-entry transition overrides dropped from Playlist.** No
  `PlaylistEntry.transition` field. The `[entries.transition]` block
  in `setlist.playlist.toml` is removed when migrating to
  `examples/v1/`. All entries play through the playlist's default
  `[transition]` for v0. Lands additively when needed.
- **No `[selection]` block on `Live` in M3.** Live is shipped as a
  placeholder so all six Visual kinds exist at v1 baseline (per the
  milestone) and so future Live work has somewhere to land. The
  shape we'd want here is genuinely unclear: `min_hold` /
  `debounce` may want to be per-candidate, not Live-wide; design
  conversation deferred. The example `main.live.toml` drops its
  `[selection]` block; comment in the example notes the deferral.
- **Live requires inputs / audio-reactive selection / etc. to be
  meaningful.** Those concepts aren't even planned yet. M3 keeps
  Live alive as a placeholder; real shape lands additively when
  the rest of the model catches up.
- **Per-Q-D4a:** `priority: f32` literal on `LiveCandidate`. If/when
  priority needs to be a Slot with a Binding, additive change.
- **Per-Q-D4b:** Both `LiveCandidate` and `PlaylistEntry` carry a
  `params: BTreeMap<String, toml::Value>` field for inline value
  overrides on the referenced Visual.
- **Per-Q-D4c:** `PlaylistBehavior` has only `loop: bool` for v0.
  Other future fields (shuffle, randomize, etc.) land additively.

`TransitionRef` also carries a `params: BTreeMap<…>` field for
inline overrides on the referenced Transition's params (e.g.
overriding `softness` on a wipe).

## Q-D3 — Stack/Effect `[input]` polymorphism — RESOLVED

`[input]` is **structural composition**, not a binding. A `Binding`
is pure routing: it points to existing things and never
instantiates nodes. `[input] visual = "…"` instantiates a child
Visual into the node tree, which is a different concept that needs
its own type.

```rust
pub enum VisualInput {
    /// Compose a Visual as a child node; its output flows into this input.
    Visual {
        spec: ArtifactSpec,
        #[serde(default)]
        params: BTreeMap<String, toml::Value>,
    },
    /// Read this input from a bus channel (no child node instantiated).
    Bus {
        channel: ChannelName,
    },
}
```

TOML form (mutex via custom `Deserialize`, ~10 LOC):

```toml
[input]
visual = "../patterns/fbm.pattern.toml"

[input.params]
scale = 6.0
```

or

```toml
[input]
bus = "video/in/0"
```

The `bus` key name (not `channel`) intentionally matches the
`bind = { bus = "…" }` source-kind vocabulary on Slot bindings,
so the same word means "the bus" in both routing and structural
contexts.

`Stack.input: Option<VisualInput>`, `Effect.input: Option<VisualInput>`.
`None` ⇒ default semantics deferred to the lpfx renderer milestone.

Transitions have **no** `input` field. `inputA` / `inputB` are
implicit shader uniforms named by convention; the parent
Show / Live wires them through whatever runtime mechanism that
milestone defines. Out of scope for M3.

`Binding` stays exactly as M2 left it. The `binding.rs`
`TODO(M3)` to fix the on-disk JSON shape to `bind = { bus = "…" }`
still lands, but no `Visual` variant gets added.

The `params` value-overrides under `[input.params]` are kept as
raw `BTreeMap<String, toml::Value>` in v0 — typechecking them
needs the referenced Visual's param schema, which is a
cross-artifact concern explicitly out of scope for M3.

## Q-D2 — `[shader]` section shape — RESOLVED

Three mutex variants in a single `[shader]` table:

```toml
[shader]
glsl    = """…"""           # inline GLSL
# OR
file    = "main.glsl"       # sibling file; language inferred from extension
# OR
builtin = "fluid"           # builtin Rust impl
```

```rust
pub enum ShaderRef {
    Glsl    { glsl: String },   // inline GLSL source
    File    { file: String },   // sibling path relative to artifact dir
    Builtin { name: Name },
}
```

Custom `Deserialize` infers the variant by which key is present;
zero or more-than-one is a load error. Mutex still applies when
WGSL lands additively as a sibling key (`wgsl = "…"`) — the inline
keys stay per-language so the language is implicit; the path key
stays neutral and infers language from file extension; builtin
stays neutral.

`ShaderRef::File { file }` is kept in memory (option (a) from
Q-D2a); the loader does **not** read the sibling file at load
time. Downstream (the lpfx renderer in a future milestone)
resolves the path. Matches the `ValueSpec` "authored source forms
round-trip on save" principle.

M3 example corpus exercises all three variants:
- `fbm.pattern.toml` → `file = "main.glsl"` (with sibling
  `main.glsl` shipped in the same dir).
- `fluid.pattern.toml` → `builtin = "fluid"`.
- All other Patterns / Effects / Transitions → inline `glsl = "…"`.

`ShaderRef` is reused by `Pattern`, `Effect`, and `Transition`.
Stack / Live / Playlist do not have a shader of their own — they
reference other Visuals via `ArtifactSpec`.

## Q-D7 — `Color` value default in TOML — RESOLVED

`default` for Color-Kinded params authored as a `{ space, coords }`
inline table:

```toml
[params.color]
kind    = "color"
default = { space = "oklch", coords = [0.7, 0.15, 90] }
```

### Locked decisions

- **`space` is a snake-case string** on the wire (`"oklch"`,
  `"oklab"`, `"linear_rgb"`, `"srgb"`). Matches the rest of the
  corpus's snake_case naming for `kind`, `presentation`, etc.
  Internally an enum (`Colorspace`) that converts to the `i32`
  tag in the runtime `Color` value at materialize time per
  `color.md`.
- **`coords` is always a 3-tuple of f32**, regardless of the
  colorspace. Components that are unused by a particular space
  (e.g. luminance-only tracks) are runtime concerns; the
  authoring grammar stays uniform. Variable-length per-space
  shape is an additive future change if/when an example demands it.
- **Field order locked: `space` first, `coords` second** in the
  serialized output. TOML inline tables don't guarantee key order
  on round-trip; the custom `Serialize` for `LpsValue::Struct`
  emits in the order declared by `Kind::Color::storage()`'s
  member list. Keeps round-trip output deterministic + matches
  M2 non-negotiable §4.
- **Default authoring space is `oklch`.** Established by
  `color.md`. M3 example corpus uses `space = "oklch"` for all
  Color params unless the example has a domain-specific reason to
  differ. `tint.color` migrates to a sensible OKLCH triple
  (e.g. `[0.7, 0.15, 90]` for a warm peach tone); we don't
  preserve the existing sRGB `[1.0, 0.6, 0.4]` as bytewise
  equivalent — the example's intent is "warm tint," not "this
  exact color."
- **`Colorspace` enum lives in `lp-domain`**, sibling to
  `Kind::Color`. It's a domain-level concept (the on-disk
  authoring vocabulary), not a low-level Lps primitive.
  `lps-shared` keeps only the value primitives.

## Q-D1 — Visual struct shape — RESOLVED

Mixed struct (option 1). Each Visual is a hand-rolled Rust struct
with concrete fields for the fixed bits + a `params: ParamsTable`
field for the implicit-`Shape::Struct` `[params]` block.
`ParamsTable` is a thin newtype wrapping a `Slot` whose `Shape` is
always `Struct`; its `Deserialize` impl synthesizes the implicit
struct without bleeding the special case into `Slot::deserialize`.
The custom `Deserialize for Slot` only fires where the field is
typed `Slot`.
