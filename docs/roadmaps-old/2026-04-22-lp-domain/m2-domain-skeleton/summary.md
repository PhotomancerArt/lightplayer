### What was built

- New `lp-domain/lp-domain/` crate (`no_std + alloc`, `std` and
  `schema-gen` features) with the foundational vocabulary for
  the LightPlayer domain model.
- Identity & addressing types in `types.rs`: `Uid(u32)`,
  `Name`, `NodePath` / `NodePathSegment`, `PropPath`
  re-exported from `lps_shared::path`, `NodePropSpec`,
  `ArtifactSpec`, `ChannelName`. Each with parse + Display +
  serde + cfg-attr `JsonSchema` and unit tests for round-trip
  and rejection cases.
- Quantity model leaves: `Kind` (16 v0 variants), `Dimension`,
  `Unit`, `Colorspace`, `InterpMethod`, `MAX_PALETTE_LEN`,
  `MAX_GRADIENT_STOPS`, plus per-Kind impls (`storage`,
  `dimension`, `default_constraint`, `default_presentation`,
  `default_bind`).
- `Constraint` (F32-narrowed `Free` / `Range` / `Choice`) with
  the `TODO(quantity widening)` marker for the future widening
  to `LpsValue` once `LpsValueF32` gets serde.
- Quantity model composition: `Shape` (with Q15 Option A —
  composed variants carry `Option<ValueSpec>`, scalar carries
  mandatory `ValueSpec`), `Slot`, `Slot::default_value(ctx)`,
  `Slot::storage()`.
- `ValueSpec` / `TextureSpec` / `LoadCtx` (stub) with hand-
  written `PartialEq` for `ValueSpec` (since `LpsValueF32` lacks
  `PartialEq`) and a private wire enum for serde (since
  `LpsValueF32` lacks serde derives in M2).
- `Binding::Bus { channel: ChannelName }` + `BindingResolver`
  trait stub (`channel_kind`).
- `Presentation` enum (10 v0 variants) with snake_case serde.
- Trait surface: `Node` (object-safe), `Artifact` (`KIND`,
  `CURRENT_VERSION`), `Migration` (`KIND`, `FROM`,
  `migrate(&mut toml::Value)`), `Registry::new()` placeholder,
  `DomainError` cross-cutting error.
- `lps-shared` extension: `serde::{Serialize, Deserialize}`
  always-on plus optional `schemars` feature for `LpsType`,
  `StructMember`, and `LpsPathSeg` (the last so `lp-domain`'s
  path types can derive serde without hand-written impls).
- Workspace `schemars` dep pinned at `1.2.1` with
  `default-features = false, features = ["derive"]`.
- `schema_gen_smoke.rs` smoke tests under
  `feature = "schema-gen"`: `schemars::schema_for!` on every
  public type in `lp-domain`, plus a recursive-type check on
  `Slot` / `Shape`.
- Schemars fallback chain documented in `lp-domain/lib.rs` for
  future-us if a derive ever breaks.
- `quantity.md` updates: §6 Slot definition rewritten for
  Option-A defaults; §6 "Defaults for compositions"
  reauthored; §12 non-negotiable #1 rewritten; TL;DR item 2 and
  §7 Conventions bullet aligned with the new contract.

Test counts (final): `cargo test -p lp-domain` 51 passed,
`cargo test -p lp-domain --features schema-gen` 73 passed,
`cargo test -p lps-shared` 25 passed, `cargo test -p lps-shared
--features schemars` 26 passed. Zero warnings on every
configuration. `just check` clean.

### Decisions for future reference

#### Q15 — Composed-Shape defaults: `Option<ValueSpec>` on the variant

- **Decision:** `Shape::Array` and `Shape::Struct` carry
  `default: Option<ValueSpec>`; `Shape::Scalar` carries
  mandatory `default: ValueSpec`. `Slot` no longer has a
  top-level `default` field.
- **Why:** The original spec required every Slot to carry a
  `default: ValueSpec`. That collided with two real needs —
  (a) deriving composed defaults from children (especially
  arrays whose elements share a single `Slot`), and (b)
  round-trip fidelity (don't write a default to TOML if the
  user didn't author one). Option A lets composed types declare
  an explicit aggregate-level override (e.g. four specific
  emitter positions) **or** delegate to children, with
  `#[serde(skip_serializing_if = "Option::is_none")]` keeping
  the on-disk form honest.
- **Rejected alternatives:** Keep mandatory `ValueSpec` on
  every Shape (lost round-trip parity and forced the loader to
  compute and re-write derived defaults); make every Shape's
  `default` `Option<ValueSpec>` (lost the strong invariant for
  scalars and pushed the materialization fallback into a
  diagnostic — scalars genuinely have nothing to derive from).
- **Revisit when:** A composed Shape variant grows that needs a
  *non-derivable* default (e.g. a tagged-union Shape if we ever
  add one), or if the per-Kind `default_constraint` ends up
  carrying the default itself and the Slot-level field becomes
  redundant.

#### `Uid` is `u32`, not a base-62 string

- **Decision:** `pub struct Uid(pub u32)` — runtime-only,
  `Copy + Eq + Hash + Ord + Display`, no allocator in M2.
- **Why:** Embedded targets (ESP32-class `no_std + alloc`)
  pay a real cost for string identity — allocation, hashing,
  comparison. `Uid` is a runtime handle, never authored, never
  serialized to TOML; the string-shaped paths are
  `NodePath` / `PropPath` / `NodePropSpec`. Splitting identity
  cleanly from addressing means we get cheap `Copy` semantics
  on hot paths and the auth-time strings stay where they
  matter.
- **Rejected alternatives:** Base-62 string (initial spec form
  — too expensive on embedded); `u64` (gratuitous; `u32` gives
  4B unique nodes, more than enough).
- **Revisit when:** A single project ever pushes past
  ~100M live nodes (none in the foreseeable future) or a
  cross-project ID scheme appears.

#### `Constraint` is F32-narrowed in v0

- **Decision:** `Constraint::Range { min: f32, max: f32, step:
  Option<f32> }` and `Constraint::Choice { values: Vec<f32>,
  labels: Vec<String> }`. A `TODO(quantity widening)` is
  recorded in `constraint.rs`.
- **Why:** The spec carries `LpsValue` widths for ranges, but
  `LpsValueF32` doesn't derive serde in M2 (defaults flow
  through `ValueSpec`). Narrowing to `f32` lets `Constraint`
  itself derive serde + `JsonSchema` cleanly without dragging
  `LpsValue` serde into M2's surface — and ranges are F32 in
  practice for v0. Widening later is purely additive.
- **Rejected alternatives:** Add serde to `LpsValueF32` in M2
  (out-of-scope; touches a pile of decisions about how raw
  values serialize that belong to M3); drop `Range` / `Choice`
  entirely until M3 (loses the test surface for the validator
  in M3).
- **Revisit when:** A real example needs an integer-typed
  range / choice (`Count`, `Choice` Kind), or `LpsValueF32`
  acquires serde derives.

#### `ValueSpec` serde via private wire enum

- **Decision:** `ValueSpec` does not derive `Serialize` /
  `Deserialize` directly. A private `LpsValueWire` +
  `ValueSpecWire` pair handles the conversion to/from JSON.
- **Why:** `LpsValueF32` lacks serde in M2 (see above). Hand-
  rolling the wire form on the `lp-domain` side keeps the
  surface change off `lps-shared` until we know the right
  shape. The wire enum is private, so future shifts are
  invisible to callers.
- **Rejected alternatives:** Add serde to `LpsValueF32`
  (out-of-scope); skip `ValueSpec` serde entirely (would block
  the recursive `Slot` round-trip tests).
- **Revisit when:** `LpsValueF32` gets serde derives — the
  wire enum becomes redundant.

#### Forward refs between phases handled by minimal stubs

- **Decision:** Phase 3 (`Kind` impls) and phase 5 (`Shape` /
  `Slot`) had a circular dependency through
  `Kind::default_presentation` / `default_bind`. Phase 3
  declared minimum-surface `Presentation` and `Binding` enums
  (just enough variants to satisfy the Kind impls); phase 5
  extended them in place with serde polish, tests, and the
  `BindingResolver` trait stub.
- **Why:** Lets phase 3 and phase 4 actually run in parallel
  on disjoint files without a third synchronization phase.
- **Revisit when:** Probably never — this is a one-time
  scaffold pattern. Future Quantity-model additions land in
  whatever phase needs them.
