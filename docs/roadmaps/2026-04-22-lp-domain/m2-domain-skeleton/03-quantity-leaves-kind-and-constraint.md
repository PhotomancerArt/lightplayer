# Phase 3 — Quantity model leaves: `Kind` + `Constraint`

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** Phase 2 (`lp-domain` skeleton + identity types)
> must be complete and `cargo test -p lp-domain` must pass before
> this phase runs.
>
> **Parallel with:** Phase 4 (`Node` / `Artifact` / `Migration`
> trait surface). The two phases touch disjoint files:
> - This phase: `kind.rs`, `constraint.rs`.
> - Phase 4: `node/mod.rs`, `schema/mod.rs`, `artifact/mod.rs`.
>
> Neither phase modifies `lib.rs` or `types.rs`.

## Scope of phase

Implement the leaf types of the Quantity model:

1. `kind.rs` — `Kind` open enum (12 v0 variants), supporting types
   (`Dimension`, `Unit`, `Colorspace`, `InterpMethod`), the
   collection-bound constants, and the per-Kind impl block
   (`storage`, `dimension`, `default_constraint`,
   `default_presentation`, `default_bind`).
2. `constraint.rs` — `Constraint` enum (`Free` / `Range` /
   `Choice`).

Reference: [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
sections 3, 4, 5, and the storage table in §3.

**In scope:**

- `lp-domain/lp-domain/src/kind.rs` — replace the stub with the
  full implementation described below.
- `lp-domain/lp-domain/src/constraint.rs` — replace the stub.
- Tests inline in each file (per `.cursorrules`, tests at the top
  of the module).

**Out of scope:**

- `Shape`, `Slot`, `ValueSpec`, `Binding`, `Presentation` —
  phase 5. **However**, this phase has unavoidable forward
  references to `Constraint`, `Presentation`, and `Binding`
  inside `Kind`'s impl methods. The fix is below: phase 5's
  types use `pub(crate)` placeholder enums in this phase that
  phase 5 replaces in-place. Read the "Forward refs" subsection
  carefully before starting.
- TOML parsing of `Kind` / `Constraint`. Default serde derives
  only; custom parsers are M3.

## Forward refs (read this before coding)

`Kind::default_presentation` returns a `Presentation`, which
phase 5 owns. Same for `Kind::default_bind` returning
`Option<Binding>`. To keep phase 3 and phase 5 buildable
independently:

- This phase **declares** minimal `Presentation` and `Binding`
  types as **`pub`** in their respective stub files
  (`presentation.rs` and `binding.rs`), large enough for
  `Kind::default_presentation` and `Kind::default_bind` to
  return real values. Phase 5 will then *extend* (not replace)
  these definitions.
- The minimum surface this phase must add to `presentation.rs`:

  ```rust
  //! Presentation enum (UI widget hint). Phase 5 extends derives + tests.

  #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
  #[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
  pub enum Presentation {
      Knob,
      Fader,
      Toggle,
      NumberInput,
      Dropdown,
      XyPad,
      ColorPicker,
      PaletteEditor,
      GradientEditor,
      TexturePreview,
  }
  ```

- The minimum surface this phase must add to `binding.rs`:

  ```rust
  //! Binding enum (bus connection). Phase 5 adds BindingResolver trait stub.

  use crate::types::ChannelName;

  #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
  #[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
  pub enum Binding {
      Bus { channel: ChannelName },
  }
  ```

These two types being placed by phase 3 (vs. phase 5) is
deliberate — without them `Kind::default_*` can't compile, and
nesting the parallelism around that is more pain than just
declaring two enums up front.

Mark each minimal block with a `TODO(phase 5):` comment so phase 5
knows what to extend.

## Code Organization Reminders

- Tests at the **top** of each module.
- Helper functions at the **bottom**.
- One concept per file (`kind.rs` for everything Kind-related;
  `constraint.rs` for `Constraint`; presentation/binding for
  their bare-minimum types only).
- No comments narrating what code does.
- All public types derive `serde::{Serialize, Deserialize}` and
  `#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]`.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** expand scope.
- Do **not** add `Shape` or `Slot` types in this phase. Stop and
  report if you find yourself needing them.
- Do **not** implement `materialize` or `BindingResolver` —
  those are phase 5.
- Do **not** suppress warnings or `#[allow(...)]` problems away.
- Do **not** disable, skip, or weaken existing tests.
- If something blocks completion, stop and report back.
- Report back: list of files changed, validation output, any
  deviations.

## Implementation Details

### `lp-domain/lp-domain/src/kind.rs`

```rust
//! Kind: semantic identity of a value. See docs/design/lightplayer/quantity.md §3.

use crate::binding::Binding;
use crate::constraint::Constraint;
use crate::presentation::Presentation;
use crate::types::ChannelName;
use crate::LpsType;
use alloc::boxed::Box;
use alloc::string::String;

#[cfg(test)]
mod tests {
    // tests at the top per .cursorrules
}

// --- Constants ----------------------------------------------------------

pub const MAX_PALETTE_LEN: u32 = 16;
pub const MAX_GRADIENT_STOPS: u32 = 16;

// --- Dimension ----------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Dimension {
    Dimensionless,
    Time,
    Frequency,
    Angle,
}

// --- Unit ---------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Unit {
    None,
    Seconds,
    Hertz,
    Radians,
}

// --- Colorspace ---------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Colorspace {
    Oklch,
    Oklab,
    LinearRgb,
    Srgb,
}

// --- InterpMethod -------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum InterpMethod {
    Linear,
    Cubic,
    Step,
}

// --- Kind ---------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    // Scalars (Dimensionless)
    Amplitude,
    Ratio,
    Phase,
    Count,
    Bool,
    Choice,

    // Scalars (with Dimension)
    Instant,
    Duration,
    Frequency,
    Angle,

    // Structured value-Kinds
    Color,
    ColorPalette,
    Gradient,
    Position2d,
    Position3d,

    // Bulk / opaque-handle Kinds
    Texture,
}

impl Kind {
    /// The structural LpsType the GPU/serializer sees.
    pub fn storage(self) -> LpsType {
        match self {
            // Float scalars
            Self::Amplitude
            | Self::Ratio
            | Self::Phase
            | Self::Instant
            | Self::Duration
            | Self::Frequency
            | Self::Angle => LpsType::Float,
            // Int scalars
            Self::Count | Self::Choice => LpsType::Int,
            // Bool
            Self::Bool => LpsType::Bool,
            // Vec scalars
            Self::Position2d => LpsType::Vec2,
            Self::Position3d => LpsType::Vec3,
            // Struct-shaped Kinds
            Self::Color => color_struct(),
            Self::ColorPalette => color_palette_struct(),
            Self::Gradient => gradient_struct(),
            Self::Texture => texture_struct(),
        }
    }

    pub fn dimension(self) -> Dimension {
        match self {
            Self::Instant | Self::Duration => Dimension::Time,
            Self::Frequency => Dimension::Frequency,
            Self::Angle => Dimension::Angle,
            _ => Dimension::Dimensionless,
        }
    }

    pub fn default_constraint(self) -> Constraint {
        use Constraint::*;
        match self {
            Self::Amplitude | Self::Ratio => Range {
                min: f32_value(0.0),
                max: f32_value(1.0),
                step: None,
            },
            Self::Phase => Range {
                min: f32_value(0.0),
                max: f32_value(1.0),
                step: None,
            },
            Self::Count => Range {
                min: i32_value(0),
                max: i32_value(i32::MAX),
                step: None,
            },
            // Most others are unconstrained at the Kind layer; per-Slot overrides apply.
            _ => Free,
        }
    }

    pub fn default_presentation(self) -> Presentation {
        use Presentation::*;
        match self {
            Self::Instant | Self::Count => NumberInput,
            Self::Duration | Self::Amplitude | Self::Ratio => Fader,
            Self::Frequency | Self::Angle | Self::Phase => Knob,
            Self::Bool => Toggle,
            Self::Choice => Dropdown,
            Self::Color => ColorPicker,
            Self::ColorPalette => PaletteEditor,
            Self::Gradient => GradientEditor,
            Self::Position2d => XyPad,
            Self::Position3d => NumberInput,
            Self::Texture => TexturePreview,
        }
    }

    pub fn default_bind(self) -> Option<Binding> {
        match self {
            Self::Instant => Some(Binding::Bus {
                channel: ChannelName(String::from("time")),
            }),
            Self::Texture => Some(Binding::Bus {
                channel: ChannelName(String::from("video/in/0")),
            }),
            _ => None,
        }
    }
}

// --- helpers (at the bottom) --------------------------------------------

fn color_struct() -> LpsType {
    use lps_shared::types::StructMember;
    LpsType::Struct {
        name: Some(String::from("Color")),
        members: alloc::vec![
            StructMember { name: Some(String::from("space")), ty: LpsType::Int },
            StructMember { name: Some(String::from("coords")), ty: LpsType::Vec3 },
        ],
    }
}

fn color_palette_struct() -> LpsType {
    use lps_shared::types::StructMember;
    LpsType::Struct {
        name: Some(String::from("ColorPalette")),
        members: alloc::vec![
            StructMember { name: Some(String::from("space")), ty: LpsType::Int },
            StructMember { name: Some(String::from("count")), ty: LpsType::Int },
            StructMember {
                name: Some(String::from("entries")),
                ty: LpsType::Array { element: Box::new(LpsType::Vec3), len: MAX_PALETTE_LEN },
            },
        ],
    }
}

fn gradient_struct() -> LpsType {
    use lps_shared::types::StructMember;
    let stop = LpsType::Struct {
        name: Some(String::from("GradientStop")),
        members: alloc::vec![
            StructMember { name: Some(String::from("at")), ty: LpsType::Float },
            StructMember { name: Some(String::from("c")), ty: LpsType::Vec3 },
        ],
    };
    LpsType::Struct {
        name: Some(String::from("Gradient")),
        members: alloc::vec![
            StructMember { name: Some(String::from("space")), ty: LpsType::Int },
            StructMember { name: Some(String::from("method")), ty: LpsType::Int },
            StructMember { name: Some(String::from("count")), ty: LpsType::Int },
            StructMember {
                name: Some(String::from("stops")),
                ty: LpsType::Array { element: Box::new(stop), len: MAX_GRADIENT_STOPS },
            },
        ],
    }
}

fn texture_struct() -> LpsType {
    use lps_shared::types::StructMember;
    LpsType::Struct {
        name: Some(String::from("Texture")),
        members: alloc::vec![
            StructMember { name: Some(String::from("format")), ty: LpsType::Int },
            StructMember { name: Some(String::from("width")), ty: LpsType::Int },
            StructMember { name: Some(String::from("height")), ty: LpsType::Int },
            StructMember { name: Some(String::from("handle")), ty: LpsType::Int },
        ],
    }
}

fn f32_value(v: f32) -> crate::LpsValue {
    crate::LpsValue::F32(v)
}

fn i32_value(v: i32) -> crate::LpsValue {
    crate::LpsValue::I32(v)
}
```

> **Naming caveat — `Kind::Angle` vs `Dimension::Angle`.** Both
> exist; that's intentional. `Kind` is a 12-variant enum that
> includes `Angle`; `Dimension` is a 4-variant classifier that
> also includes `Angle`. Don't try to disambiguate via aliases —
> they're different types and clippy won't confuse them.

### `lp-domain/lp-domain/src/constraint.rs`

```rust
//! Constraint: what values are legal for a Slot. See docs/design/lightplayer/quantity.md §5.

use crate::LpsValue;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_constraint_round_trips_serde() {
        let c = Constraint::Free;
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    // Range / Choice round-trips: see below
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    Free,
    Range {
        min: LpsValue,
        max: LpsValue,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        step: Option<LpsValue>,
    },
    Choice {
        values: Vec<LpsValue>,
        labels: Vec<String>,
    },
}
```

> **Note on `LpsValue` serde.** `LpsValueF32` (re-exported as
> `LpsValue`) does **not** have serde derives in M2. That breaks
> the `Constraint` derives above. Two options:
>
> 1. **Carry `f32` directly** in `Constraint::Range` for v0
>    (since real ranges are F32 in practice per `00-notes.md`):
>    `min: f32, max: f32, step: Option<f32>`. `Choice::values`
>    carries `Vec<f32>` as well in v0. Simpler but type-narrows
>    the spec.
> 2. **Add serde to `LpsValueF32`** in this phase, modifying
>    `lps-shared` as a follow-on. Wider surface, more honest to
>    the spec.
>
> **Choose option 1 for v0.** Rationale: ranges in v0 are F32 in
> practice; `Choice` over int/bool is rare; `00-notes.md`
> explicitly punted serde on `LpsValueF32`. Document the
> narrowing with a `TODO(quantity widening): Constraint
> currently F32-only; widen to LpsValue when LpsValueF32 gets
> serde.` comment in the file.
>
> If a future phase needs the wider form, it's an additive
> change.

So the actual definition becomes:

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    Free,
    Range {
        min: f32,
        max: f32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        step: Option<f32>,
    },
    Choice {
        values: Vec<f32>,
        labels: Vec<String>,
    },
}
```

And `kind.rs`'s `default_constraint` uses `Constraint::Range {
min: 0.0, max: 1.0, step: None }` (drop the `f32_value` /
`i32_value` helpers — they're no longer needed). Phase 3 sub-
agent: pick this F32-narrowed form, drop the unused helpers,
and add the TODO comment as instructed.

#### Tests for `kind.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_is_exhaustive_and_concrete() {
        for k in [
            Kind::Amplitude, Kind::Ratio, Kind::Phase, Kind::Count, Kind::Bool, Kind::Choice,
            Kind::Instant, Kind::Duration, Kind::Frequency, Kind::Angle,
            Kind::Color, Kind::ColorPalette, Kind::Gradient, Kind::Position2d, Kind::Position3d,
            Kind::Texture,
        ] {
            // Every Kind has a non-panicking storage projection.
            let _ = k.storage();
        }
    }

    #[test]
    fn float_scalar_storages() {
        assert_eq!(Kind::Amplitude.storage(), LpsType::Float);
        assert_eq!(Kind::Frequency.storage(), LpsType::Float);
    }

    #[test]
    fn int_scalar_storages() {
        assert_eq!(Kind::Count.storage(), LpsType::Int);
        assert_eq!(Kind::Choice.storage(), LpsType::Int);
    }

    #[test]
    fn position_storages() {
        assert_eq!(Kind::Position2d.storage(), LpsType::Vec2);
        assert_eq!(Kind::Position3d.storage(), LpsType::Vec3);
    }

    #[test]
    fn texture_storage_has_four_int_fields() {
        let s = Kind::Texture.storage();
        match s {
            LpsType::Struct { members, .. } => {
                assert_eq!(members.len(), 4);
                for m in members {
                    assert_eq!(m.ty, LpsType::Int);
                }
            }
            _ => panic!("Texture storage must be a Struct"),
        }
    }

    #[test]
    fn dimension_assignment() {
        assert_eq!(Kind::Instant.dimension(), Dimension::Time);
        assert_eq!(Kind::Duration.dimension(), Dimension::Time);
        assert_eq!(Kind::Frequency.dimension(), Dimension::Frequency);
        assert_eq!(Kind::Angle.dimension(), Dimension::Angle);
        assert_eq!(Kind::Amplitude.dimension(), Dimension::Dimensionless);
        assert_eq!(Kind::Phase.dimension(), Dimension::Dimensionless);
    }

    #[test]
    fn default_presentation_table() {
        assert_eq!(Kind::Amplitude.default_presentation(), Presentation::Fader);
        assert_eq!(Kind::Frequency.default_presentation(), Presentation::Knob);
        assert_eq!(Kind::Bool.default_presentation(), Presentation::Toggle);
        assert_eq!(Kind::Color.default_presentation(), Presentation::ColorPicker);
        assert_eq!(Kind::Position2d.default_presentation(), Presentation::XyPad);
        assert_eq!(Kind::Texture.default_presentation(), Presentation::TexturePreview);
    }

    #[test]
    fn default_bind_for_instant_is_time() {
        match Kind::Instant.default_bind() {
            Some(Binding::Bus { channel }) => assert_eq!(channel.0, "time"),
            other => panic!("expected Bus(time), got {other:?}"),
        }
    }

    #[test]
    fn default_bind_for_color_is_none() {
        assert!(Kind::Color.default_bind().is_none());
    }

    #[test]
    fn default_constraint_for_amplitude_is_unit_range() {
        match Kind::Amplitude.default_constraint() {
            Constraint::Range { min, max, step } => {
                assert_eq!(min, 0.0);
                assert_eq!(max, 1.0);
                assert!(step.is_none());
            }
            _ => panic!("expected Range[0,1]"),
        }
    }

    #[test]
    fn kind_serde_round_trips() {
        let k = Kind::Frequency;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"frequency\"");
        let back: Kind = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }
}
```

#### Tests for `constraint.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_constraint_round_trips() {
        let c = Constraint::Free;
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_constraint_round_trips() {
        let c = Constraint::Range { min: 0.0, max: 5.0, step: Some(0.1) };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_omits_step_when_none() {
        let c = Constraint::Range { min: 0.0, max: 1.0, step: None };
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("step"));
    }

    #[test]
    fn choice_round_trips() {
        let c = Constraint::Choice {
            values: alloc::vec![0.0, 1.0, 2.0],
            labels: alloc::vec![
                String::from("low"), String::from("med"), String::from("high"),
            ],
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}
```

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**. Optional:
`cargo +nightly fmt` on the new files.

## Definition of done

- `kind.rs`, `constraint.rs`, `presentation.rs`, `binding.rs` all
  contain real types (not stubs).
- `Kind` enum has 12 v0 variants per spec.
- All Kind methods (`storage`, `dimension`, `default_constraint`,
  `default_presentation`, `default_bind`) implemented.
- `Constraint` is the F32-narrowed v0 form with the documented
  TODO.
- All tests pass with no warnings.
- No `Shape`, `Slot`, `ValueSpec`, or `BindingResolver` types
  added (phase 5 owns those).
- No commit.

Report back with: list of changed files, validation output, and
whether the F32 narrowing of `Constraint` was done as planned.
