# M2 — lp-domain skeleton + foundational types — plan notes

Source milestone:
[`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`](../../roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md).

Authoritative design references this milestone implements:

- [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
  — Kind, Dimension, Unit, Constraint, Shape, Slot, ValueSpec,
  Binding, Presentation. **The single source of truth.**
- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md)
  — broader vocabulary (Node, Artifact, ChannelName, NodePath,
  PropPath, NodePropSpec, ArtifactSpec).
- [`docs/design/color.md`](../../design/color.md) — color storage
  recipes for `Color`, `ColorPalette`, `Gradient`.
- [`docs/design/q32.md`](../../design/q32.md) — Q16.16 fallback
  (relevant for storage discussions; F32-only in v0).
- [`docs/roadmaps/2026-04-22-lp-domain/notes-quantity.md`](../../roadmaps/2026-04-22-lp-domain/notes-quantity.md)
  — historical Q&A trail (Q1–Q9) that produced the design above.

## Scope of work

Stand up `lp-domain/lp-domain/` as a `no_std + alloc` crate
containing:

1. **Identity & addressing types** — `Uid`, `Name`, `NodePath`,
   `PropPath` (alias of `lps_shared` value-path segments),
   `NodePropSpec`, `ArtifactSpec`, `ChannelName`.
2. **Quantity model** — `Kind` (open enum), `Dimension`, `Unit`,
   `Colorspace`, `InterpMethod`, `Constraint`, `Shape`, `Slot`,
   `ValueSpec`, `TextureSpec`, `Binding`, `BindingResolver`
   (trait stub), `Presentation`, plus per-Kind impls
   (`storage`, `dimension`, `default_constraint`,
   `default_presentation`, `default_bind`).
3. **Trait surface** — `Node` trait, `Artifact` trait,
   `Migration` trait, empty `Registry` shape.
4. **Re-exports from `lps-shared`** — `LpsType`,
   `LpsValueF32` re-exported as `LpsValue`, `TextureStorageFormat`,
   `TextureBuffer`. Plus an optional `schemars` feature on
   `lps-shared` for `LpsType`.
5. **schemars discipline** — every public type derives
   `JsonSchema`; recursive `Shape`/`Slot` round-trip is verified
   in M2 (early surfacing of any schemars issues).
6. **Tests** — path parsing, `Binding` serde round-trip,
   recursive `Slot`/`Shape` serde round-trip on hand-built values,
   `Kind::storage()` exhaustive check, `schemars::schema_for!` on
   every public type.

Out of scope (per milestone file):

- Visual artifact types (M3).
- Audio/Beat/Touch/Motion/AudioFft/AudioLevel Kinds (added in M3
  when first example demands them).
- TOML grammar for `Slot` (M3).
- Schema codegen tooling, `lp-cli schema generate`, drift gates
  (M4 — derives land here, tooling is M4).
- Migration registry implementation beyond trait shape (M5).
- `LpFs`-based artifact loader (M3).
- `BindingResolver` real implementation (M3+).
- Render hookup, runtime behavior of any kind.
- Q32 specialization (F32-only for now).

## Current state of the codebase

- **`lp-base/lpfs/`** exists (M1 complete). Provides `LpFs`,
  `LpFsMemory`, `LpFsView`, `LpFsStd`, `FsChange`, `FsVersion`.
  `no_std + alloc`, `std` feature for `LpFsStd`. M2 doesn't use
  it directly (the M2 surface is pure types + traits) but lp-domain
  will declare it as an optional dep so M3's loader has a place to
  land.
- **`lp-shader/lps-shared/`** provides:
  - `LpsType` (Float / Int / UInt / Bool / Vec2..Vec4 /
    IVec2..UVec4 / BVec2..BVec4 / Mat2..Mat4 / Array{element, len}
    / Struct{name, members}).
  - `LpsValueF32` (matching variant set, plus `Array(Box<[..]>)`
    and `Struct{name, fields}`).
  - `value_path` (path-based get/set on `LpsValueF32`),
    `path` (parser for `field.foo[0].x` syntax).
  - `TextureStorageFormat` (Rgba16Unorm, Rgb16Unorm, R16Unorm),
    `TextureBuffer`.
  - **No** `serde` derives, **no** `schemars` derives today on
    `LpsType` or `LpsValueF32`. Adding these is part of this
    milestone's scope (the lps-shared `schemars` feature; serde
    on `LpsType` only — serializing `LpsValueF32` in TOML isn't
    needed for M2 because `Slot.default` is `ValueSpec`, not
    `LpsValue`).
  - lps-shared has no `Color` storage tag — the `Color` Kind
    composes `Struct{space: I32, coords: Vec3}` from existing
    `LpsType` variants per `color.md`. No new lps-shared variant
    needed. Confirmed by quantity.md §3.
- **No** `lp-domain/` directory exists yet.
- Workspace `Cargo.toml` already has `lp-base/lpfs` in members
  and default-members from M1.
- AGENTS.md `no_std` rule — lp-domain is `no_std + alloc` by
  default; `std` is feature-gated; the on-device JIT pipeline is
  the non-negotiable; this milestone respects that (host-only
  schema codegen lands in M4 behind `schema-gen`, derives are
  always-on but their codegen surface is host-side).

### Crate naming and layout

Per Q1 in roadmap notes: outer dir `lp-domain/` matching the
existing `lpfx/lpfx/` and `lp-base/lpfs/` shape. Inner crate is
`lp-domain` (single-crate today; reserves room for an `lpd-*`
split later by reusing the outer dir).

```
lp-domain/lp-domain/
  Cargo.toml
  src/
    lib.rs
    types.rs
    kind.rs
    constraint.rs
    shape.rs
    value_spec.rs
    binding.rs
    presentation.rs
    node/
      mod.rs
    schema/
      mod.rs
    artifact/
      mod.rs
```

## User-directed change to baseline (carried forward)

> **UID is `u32`, not a base-62 string.** Strings on the embedded
> target (ESP32-C6) cost cycles + heap; `u32` is one register-wide
> compare and free to log. The original m2-domain-skeleton.md spec
> said "base-62 string newtype"; we override that here.

Implications already worked through:

- `Uid` becomes `pub struct Uid(pub u32);` — `Copy + Eq + Hash +
  Ord` (string version was `Clone`-only). Display prints decimal.
- UID is **runtime-only** (the original spec already said so) —
  it does not appear in TOML, does not need stable serialization
  across versions. UID *generation* is a runtime concern (M3+)
  and out of scope for M2.
- `NodePath` (the human-readable, slash-separated, `<name>.<type>`
  segment path used to address Nodes in TOML and diagnostics) is
  unaffected. It's the "what does the user write" form; UID is the
  "what the runtime hands around" form. They coexist.
- `NodePropSpec` is unaffected for the same reason — it's a
  *string* identifier (`/main.show/fluid.vis#speed`) used at the
  authoring/diagnostic layer. No UID embedded.
- No serde or migration impact (UID is runtime-only).
- Side benefit: the type is `Copy`, so APIs that took `&Uid`
  before take `Uid` by value cleanly.

## Confirmation-style questions (please scan and answer in one pass)

| #  | Question                                                                              | Context (1 line)                                            | Suggested answer                       |
| -- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------- | -------------------------------------- |
| Q1 | `Uid` shape: `pub struct Uid(pub u32)` with `Copy + Eq + Hash + Ord + Display`?       | Per your direction — embed perf-friendly                    | Yes                                    |
| Q2 | Skip `Uid` *generation* in M2 (no allocator, no counter) — type only?                 | Allocation is a runtime concern; M3+ will own it            | Yes                                    |
| Q3 | `Name` is `pub struct Name(pub String)` (validated `[A-Za-z0-9_]+` at parse time)?    | Used for NodePath `<name>.<type>` segments                  | Yes                                    |
| Q4 | `PropPath` is a re-export of `lps_shared::path::LpsPathSeg` + parser, not a new type? | Avoid duplicating the existing parser                       | Yes                                    |
| Q5 | `ArtifactSpec(String)` v0 stores any string + parses on `as_path()` (no validation)?  | File-relative-only model; v0 is permissive                  | Yes                                    |
| Q6 | `ChannelName(String)` — convention only (`<kind>/<dir>/<channel>`), no enforcement?   | Per quantity.md §11                                         | Yes                                    |
| Q7 | Add **serde + schemars derives** to `lps_shared::LpsType` (behind `schemars` feat)?   | Required for `Shape::Scalar` to embed `LpsType`             | Yes — serde always; schemars feat-gated |
| Q8 | **Skip serde on `LpsValueF32`** in M2 (Slot uses `ValueSpec`, not raw `LpsValue`)?    | M2 doesn't serialize values; M3 might revisit               | Yes                                    |
| Q9 | `Constraint::Range { min, max, step }` typed as `LpsValue` (matches quantity.md §5)?  | Wide enough for any scalar Kind; F32 in practice            | Yes                                    |
| Q10 | Module layout (one concept per file, `node/`, `schema/`, `artifact/` are dirs)?      | Roadmap overview specifies this layout                      | Yes                                    |
| Q11 | `lp-domain`'s package name in Cargo.toml is `lp-domain` (matches `lpfx/lpfx/`)?      | Workspace convention                                        | Yes                                    |
| Q12 | Use `toml = { workspace = true }` for the future `Migration::migrate(&mut toml::Value)` signature? | Avoids redefining the type; keeps trait shape stable | Yes                                    |
| Q13 | Each module's tests live inline (`#[cfg(test)] mod tests`); no top-level `tests/` dir? | Per milestone file ("design surface; integration tests come in M3") | Yes                                    |
| Q14 | Skip the `lp-cli` integration in M2 (no CLI changes; M4 owns `schema generate`)?     | The schemars derives land here; CLI wiring waits            | Yes                                    |

You can answer with e.g. `Q1-Q14 yes` / `all yes` / `lgtm` — or
push back on anything that should graduate to a discussion.

## Discussion-style questions

(None pre-identified — the roadmap's notes-quantity.md already
worked through Q1–Q9 of the design. New discussion-style
questions get added here as they surface from your responses to
the table above.)

# Answers

**Q1–Q14: all yes** (user confirmed in chat 2026-04-22). Suggested
answers stand. Highlights:

- `Uid` is `pub struct Uid(pub u32)`, runtime-only, `Copy + Eq +
  Hash + Ord + Display` (decimal). No allocator in M2.
- `Name` is a `String` newtype with `[A-Za-z0-9_]+` validation at
  parse time. Used inside `NodePath` segments.
- `PropPath` is reused from `lps_shared::path` (same parser, same
  segment enum). `lp-domain` re-exports under a `prop_path`
  alias for ergonomics.
- `ArtifactSpec(String)` and `ChannelName(String)` are permissive
  string newtypes in v0 (convention-only, no enforcement).
- `LpsType` gains `serde::{Serialize, Deserialize}` always-on,
  plus `schemars::JsonSchema` behind a new `lps-shared` `schemars`
  feature. `LpsValueF32` does **not** get serde in M2 (defaults
  flow through `ValueSpec`, not raw values).
- `Constraint::Range { min, max, step }` uses `LpsValue` for
  width; F32 in practice for v0.
- Module layout: one concept per file at the top level; `node/`,
  `schema/`, `artifact/` are directories with their own `mod.rs`.
- Crate is `lp-domain/lp-domain/`, package name `lp-domain`,
  matching the `lpfx/lpfx/` and `lp-base/lpfs/` shapes.
- `toml = { workspace = true }` is wired in for the
  `Migration::migrate(&mut toml::Value)` trait shape, even though
  no migrations exist in M2.
- Tests live inline (`#[cfg(test)] mod tests`); no top-level
  `tests/` directory in M2.
- No `lp-cli` wiring in M2 — the schemars derives land here, the
  CLI surface is M4's job.

# Notes

## Q15 — Composed-Shape defaults — RESOLVED (Option A)

**Decision:** Composed `Shape` variants carry an
`Option<ValueSpec>` override; scalar carries a mandatory
`ValueSpec`.

```rust
pub enum Shape {
    Scalar { kind: Kind, constraint: Constraint, default: ValueSpec },
    Array  { element: Box<Slot>, length: u32,    default: Option<ValueSpec> },
    Struct { fields: Vec<(Name, Slot)>,          default: Option<ValueSpec> },
}
```

- `None` → `Slot::default_value(ctx)` derives by walking children.
- `Some` → materialize as-is (preserves §10 emitter-preset
  pattern, especially important for arrays).
- Round-trip parity: `#[serde(skip_serializing_if =
  "Option::is_none")]` keeps "wasn't there before" out of the
  on-disk form.
- `Slot` loses its top-level `default: ValueSpec` field.
  `Slot::default_value(ctx)` becomes a method that dispatches on
  `Shape`.

**Required follow-up edit to `docs/design/lightplayer/quantity.md`
(folded into the cleanup phase):**

- §6 Slot definition: move `default` from `Slot` into the `Shape`
  variants (mandatory on `Scalar`, `Option<ValueSpec>` on `Array`
  and `Struct`).
- §12 non-negotiable #1: soften from "Every Slot has a `default:
  ValueSpec`. No `Option<>`." to "Every Slot can produce a
  default value at materialize time. Scalar Shapes store an
  explicit `ValueSpec`; composed Shapes either store an override
  or derive one from their children. Round-trip preserves
  whether the composed default was explicit."

**Why this matters most for arrays:** an N-element array shares
one `element: Slot`, so per-element presets aren't expressible
through children — the only way to author "4 specific emitter
positions" is the array-level override.
