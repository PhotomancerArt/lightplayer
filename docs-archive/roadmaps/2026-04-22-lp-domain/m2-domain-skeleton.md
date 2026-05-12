# Milestone 2: lp-domain crate skeleton + foundational types

## Goal

Create the `lp-domain/lp-domain/` crate with the foundational vocabulary
of the Lightplayer domain model:

- **Identity / addressing**: UID, NodePath, ArtifactSpec, NodePropSpec,
  ChannelName.
- **Quantity model**: Kind, Dimension, Unit, Constraint, Shape, Slot,
  ValueSpec / TextureSpec, Binding, Presentation. (See
  [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
  for the canonical spec.)
- **Trait surface**: Node, Artifact, Migration (signatures only).

No artifacts yet, no migrations yet, no schema codegen tooling yet
(but `schemars` derives go on day one — see Key decisions). This
milestone is just the bones, but the bones have to be right because
everything else hangs off them.

## Suggested plan name

`lp-domain-m2`

## Scope

**In scope:**

- New crate at `lp-domain/lp-domain/` with `Cargo.toml` configured
  `no_std + alloc` by default; `std`, `schema-gen` features defined.
  `schemars` is **always available** (Q6) — `schema-gen` becomes a
  flag for the *codegen tooling* in M4, not for the derives
  themselves.

- **Re-exports from `lps-shared`** (Q4):
  - `LpsType`, `LpsValueF32` (re-exported as `LpsValue`),
    `TextureStorageFormat`, `TextureBuffer`.
  - lp-domain depends on lps-shared; does not redefine these types.
  - Optional `schemars` feature added to `lps-shared` for its
    `LpsType` (single `#[cfg_attr(feature = "schemars", derive(JsonSchema))]`).

- **Identity & addressing types** in `lp-domain/lp-domain/src/types.rs`:
  - `UID` (runtime-only, base-62 string newtype)
  - `Name` (human-readable label)
  - `NodePath` (slash-separated, `<name>.<type>` segments)
  - `PropPath` (re-export or alias of `lps_shared::value_path`)
  - `NodePropSpec` (NodePath + PropPath, joined by `#`)
  - `ArtifactSpec` (string newtype with parsing; v0 supports
    file-relative only, per the artifact-resolution future-roadmap
    note)
  - `ChannelName` (string newtype; convention
    `<kind>/<dir>/<channel>[/<sub>...]`, see
    [`quantity.md` §11](../../design/lightplayer/quantity.md#11-bus-channel-naming-convention)).
    No format enforcement in v0; convention only.

- **Quantity model** (per
  [`quantity.md`](../../design/lightplayer/quantity.md)):
  - `lp-domain/lp-domain/src/kind.rs` — `Kind` enum (open
    enumeration), `Dimension`, `Unit`. v0 Kind set:
    - Scalars: `Instant`, `Duration`, `Frequency`, `Angle`, `Phase`,
      `Amplitude`, `Ratio`, `Count`, `Bool`, `Choice`,
      `Position2d`, `Position3d`.
    - Color family: `Color`, `ColorPalette`, `Gradient`.
    - Opaque-handle: `Texture`.
    - Audio / Beat / Touch / Motion are explicitly *not* in M2 —
      they land in M3 when an example needs them (Q9).
  - Per-Kind impls: `storage()`, `dimension()`,
    `default_constraint()`, `default_presentation()`,
    `default_bind()`.
  - `Colorspace` enum (`#[repr(i32)]`, stable values), `InterpMethod`
    enum.
  - Project-wide constants: `MAX_PALETTE_LEN = 16`,
    `MAX_GRADIENT_STOPS = 16`.
  - `lp-domain/lp-domain/src/constraint.rs` — `Constraint` enum
    (`Free`, `Range`, `Choice`).
  - `lp-domain/lp-domain/src/shape.rs` — `Shape` enum
    (`Scalar { kind, constraint }`,
    `Array { element: Box<Slot>, length }`,
    `Struct { fields: Vec<(String, Slot)> }` — **ordered**, Q4),
    and `Slot` struct.
  - `lp-domain/lp-domain/src/value_spec.rs` — `ValueSpec`
    (`Literal`, `Texture`), `TextureSpec` (v0: `Black` only),
    `materialize(&self, ctx: &mut LoadCtx) -> LpsValue` (stub for
    v0; M3 wires the real loader).
  - `lp-domain/lp-domain/src/binding.rs` — `Binding` enum (single
    v0 variant `Bus { channel: ChannelName }`), and a stub
    `BindingResolver` trait for compose-time validation. **Renamed
    from `BindSource` per Q2.**
  - `lp-domain/lp-domain/src/presentation.rs` — `Presentation` enum
    (10 v0 variants per Q3).

- **Trait surface** (no concrete impls beyond what's needed for tests):
  - `lp-domain/lp-domain/src/node/mod.rs` — `Node` trait
    (UID, NodePath, get/set Property by PropPath).
  - `lp-domain/lp-domain/src/schema/mod.rs`:
    - `Artifact` trait with `KIND: &'static str` and
      `CURRENT_VERSION: u32` consts.
    - `Migration` trait with `KIND`, `FROM`,
      `migrate(&mut toml::Value)`.
    - Stub `Registry` shape — empty for now (M5 fills it in).

- **schemars discipline** (Q6):
  - Every public type added in this milestone gets
    `#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]`
    where applicable.
  - Recursive types (`Shape`, `Slot`) get verified to round-trip
    cleanly through schemars during this milestone — surfacing
    issues early instead of three milestones later.
  - Fallback chain documented for issues: manual `JsonSchema` impl →
    hand-written schema → alternative generator → drop.

- **Tests** for the parsing-heavy bits and the recursive types:
  - Path parsing (NodePath, PropPath, NodePropSpec, ArtifactSpec,
    ChannelName).
  - `Binding` serde round-trip (`bind = { bus = "time" }` form).
  - `Slot` / `Shape` recursive serde round-trip on hand-built
    in-memory values (TOML grammar lands in M3).
  - `Kind::storage()` matches expected `LpsType` for all variants.
  - `schemars::schema_for!` succeeds on every public type.

- **Anything missing from `lps-shared`** that lp-domain needs (named
  vector types, `Bool` value variant, `Color` storage tag, optional
  `schemars` derive, etc.) gets added there in the same milestone.

**Out of scope:**

- Visual artifact types (M3).
- Audio / Beat / Touch / Motion / AudioFft / AudioLevel Kinds —
  land in M3 when first used (Q9).
- TOML grammar for `Slot` (`shape = "scalar" | "array" | "struct"`,
  `[params]` implicit struct, `props`/`element` keywords) — lands in
  M3 with the visual types (Q7).
- Schema codegen *tooling* (M4 — `lp-cli schema generate`, drift
  gates). The derives themselves land here.
- Migration registry implementation beyond trait shape (M5).
- `LpFs`-based artifact loader (lands in M3).
- `BindingResolver` real impl (M3+).
- Render hookup, runtime behavior of any kind.
- Q32 specialization (Q4 — F32-only for now; revisit if firmware
  benchmarks demand).

## Key decisions

- **Read [`quantity.md`](../../design/lightplayer/quantity.md) first.**
  It is the canonical spec for everything in the Quantity model
  section above. This milestone implements that document.
- **`lp-domain → lps-shared`** (Q4). lp-domain depends on lps-shared
  and re-exports `LpsType` / `LpsValueF32` (as `LpsValue`) directly.
  `lps-shared` is the GPU-truth foundational layer; lp-domain layers
  semantic meaning on top.
- **`Shape::Struct` is `Vec<(String, Slot)>`, not `BTreeMap`** (Q4).
  Matches `lps-shared`'s ordered `LpsType::Struct`, preserves
  authored TOML field order, gives Visual panels a deterministic
  field order for UI.
- **F32 is canonical numeric type**; Q16.16 (Q32) is the int-only
  firmware fallback (Q4). v0 ships F32-only; soft-floats on
  int-only firmware are tried first; Q32 specialization is a
  fallback if benchmarks demand.
- **`Binding` (renamed from `BindSource`)** (Q2). Single v0
  variant: `Bus { channel: ChannelName }`. Direction is contextual
  (input vs. output Slot) — same enum, same TOML form.
- **`Presentation` is enum-only in v0** (Q3). 10 variants, no
  per-variant config; UI hints deferred until concrete examples
  demand them.
- **`Slot.default` is `ValueSpec`, not `LpsValue`** (Q8). Handles
  opaque-handle Kinds (Texture) whose author-time defaults aren't
  expressible as raw byte data. Authored form round-trips on save.
- **No `SignalType` layer** (Q9). Signals (Audio, Beat, Touch,
  Motion, ...) are `Kind` variants. Bus channels are Kind-typed
  (first binding wins, mismatches are compose-time errors).
- **schemars from day one** (Q6). All public types derive
  `JsonSchema` alongside serde. M4 becomes pure tooling work.
- **Modules are internal**; one crate, no `lpd-*` split (Q1).
- **`no_std + alloc` default**; `std` feature for the loader; the
  M2 `schema-gen` stub is for M4 codegen tooling, not for derives.
- **Each module's tests live in the module file** or a `tests`
  submodule; no separate top-level `tests/` directory yet (this is
  design surface; integration tests come in M3).

## Deliverables

- `lp-domain/lp-domain/Cargo.toml` with feature flags and the
  `lps-shared` dep.
- `lp-domain/lp-domain/src/`:
  - `lib.rs` (re-exports of lps-shared types as `LpsType` /
    `LpsValue`, plus the lp-domain public surface).
  - `types.rs` (UID, Name, NodePath, PropPath, NodePropSpec,
    ArtifactSpec, ChannelName).
  - `kind.rs` (Kind, Dimension, Unit, Colorspace, InterpMethod,
    constants).
  - `constraint.rs` (Constraint).
  - `shape.rs` (Shape, Slot).
  - `value_spec.rs` (ValueSpec, TextureSpec, materialize stub).
  - `binding.rs` (Binding enum, BindingResolver trait stub).
  - `presentation.rs` (Presentation).
  - `node/mod.rs` (Node trait + Property model).
  - `schema/mod.rs` (Artifact, Migration traits; empty Registry).
  - `artifact/mod.rs` (placeholder + trait re-exports).
- Workspace `Cargo.toml` includes `lp-domain/lp-domain`.
- Anything new in `lp-shader/lps-shared/` for value-type extension
  and the optional `schemars` feature.
- `cargo build -p lp-domain --no-default-features` and
  `cargo build -p lp-domain --features std` both pass.
- `cargo test -p lp-domain` passes (unit tests cover path parsing,
  channel name parsing, Binding serde, Shape/Slot recursive serde,
  `Kind::storage()` exhaustive check, `schemars::schema_for!` on all
  public types).

## Dependencies

- M1 complete (lpfs available — though M2 doesn't actually use it
  yet, M3 will).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: real design surface in each module. The Quantity
model alone is several interconnected modules (Kind/Constraint/
Shape/Slot/ValueSpec/Binding/Presentation) that all need to land
together. Plus identity types, plus schemars verification on
recursive types. 5–6 phases with parallelisable work after the
foundational types land.

Suggested phase shape (decided during `/plan`, sketched here):

- Phase A: `types.rs` (paths, UIDs, channel names) + lps-shared
  re-exports + lps-shared `schemars` feature — solo
- Phase B: `kind.rs` (Kind, Dimension, Unit, Colorspace,
  InterpMethod, constants) + `constraint.rs` — depends on A
- Phase C: `shape.rs` (Shape, Slot) + `value_spec.rs` +
  `binding.rs` + `presentation.rs` — depends on B
- Phase D: `node/`, `schema/`, `artifact/` trait skeletons — solo,
  parallel with B/C
- Phase E: schemars round-trip verification + recursive-type tests
  + `cargo check` matrix — depends on B+C+D
- Phase F: integration cleanup, README, deliverable verify — final

Estimated size: ~12–16 source files, ~2500–3500 LOC including
tests (the Quantity model adds materially to the original M2
estimate).

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
