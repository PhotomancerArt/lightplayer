# Phase 04 — `Constraint` + `ShaderRef` + `VisualInput` peer-key inference

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phase 03 (Slot custom serde) merged. `cargo test
> -p lp-domain` passing.
>
> **Parallel with:** Phase 05 (`ParamsTable` + Color defaults). Phase
> 04 writes `constraint.rs` + `visual/shader_ref.rs` +
> `visual/visual_input.rs`. Phase 05 writes `visual/params_table.rs`
> + `value_spec.rs`. No file overlap.

## Scope of phase

Convert `Constraint` to peer-key inference (matching `quantity.md`
§10's `range = […]` / `choices = […]` literal grammar) and introduce
two new peer-key enums:

- `ShaderRef` — `glsl` / `file` / `builtin` mutex (Q-D2).
- `VisualInput` — `visual` / `bus` mutex (Q-D3).

All three use stock `#[serde(untagged)]` + per-variant
`#[serde(deny_unknown_fields)]` for free-derive `Deserialize` /
`Serialize` / `JsonSchema`.

**In scope:**

- `lp-domain/lp-domain/src/constraint.rs`:
  - Replace the `#[serde(tag = "type")]` form with the
    `#[serde(untagged)]` peer-key form per `00-design.md`.
  - `Constraint::Range` carries `range: [f32; 2]` (a 2-tuple, not
    `min` / `max` separate keys).
  - Update or remove the existing `kind.rs::default_constraint`
    branches that construct `Constraint::Range { min, max, step }`
    — they need the new field shape.
  - Update existing `constraint.rs::tests` to assert the new
    on-disk shape (`{"range": [0, 5], "step": 0.1}` etc.).
- `lp-domain/lp-domain/src/visual/mod.rs` — new module file,
  `pub mod shader_ref; pub mod visual_input;` plus public re-exports.
- `lp-domain/lp-domain/src/visual/shader_ref.rs` — new file with
  `ShaderRef` enum + tests.
- `lp-domain/lp-domain/src/visual/visual_input.rs` — new file with
  `VisualInput` enum + tests.
- `lp-domain/lp-domain/src/lib.rs` — wire `pub mod visual;` and
  re-export `ShaderRef` / `VisualInput` at crate root.

**Out of scope:**

- `ParamsTable` (Phase 05).
- `Color { space, coords }` value defaults (Phase 05).
- `TransitionRef` and `LiveCandidate` (Phase 07; they use
  `ArtifactSpec`, not `VisualInput`).
- The Visual structs themselves (Phases 06, 07).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md) § "Code organization in Rust source files":

- Tests at the **bottom** of each module file.
- Inside `mod tests`: `#[test]` functions first, helpers below.
- Each new file: module-level rustdoc → uses → public type → impls
  → helpers → `#[cfg(test)] mod tests`.
- Document each enum's role in 1–2 short paragraphs with a link to
  the relevant design section. Don't narrate variant-by-variant if
  the variant names + types already say everything.
- `#[serde(untagged)]` + `#[serde(deny_unknown_fields)]` per
  variant — annotate why with a one-line comment ("// Mutex flat
  keys; deny_unknown_fields converts typos into hard errors per
  00-design.md §Constraint.").

## Sub-agent reminders

- Do **not** commit.
- Do **not** add `ParamsTable` or `Color` value-default work.
- Do **not** add a `Visual` variant to `Binding` (per Q-D3 — the
  user explicitly rejected that direction).
- Do **not** write custom `Deserialize` for these three enums.
  Stock `#[serde(untagged)]` is the chosen path.
- Do **not** suppress warnings.
- If `#[serde(deny_unknown_fields)]` clashes with a struct variant
  syntactically (it does not, but if a corner case appears), report
  back rather than working around it.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, any
  deviations.

## Implementation

### `Constraint` rewrite

```rust
//! What values are **legal** in a slot. See
//! `docs/design/lightplayer/quantity.md` §5 + §10.
//!
//! Variants are discriminated by which peer key is present in the
//! TOML table (no `type = "..."` discriminator). `range = [a, b]`
//! ⇒ Range; `choices = [...]` ⇒ Choice; neither ⇒ Free. Per-variant
//! `deny_unknown_fields` makes typos hard errors.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Constraint {
    /// Inclusive `[min, max]` with optional discrete `step`. See
    /// `quantity.md` §5.
    #[serde(deny_unknown_fields)]
    Range {
        range: [f32; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f32>,
    },
    /// Discrete choices: parallel `choices` and `labels`. See
    /// `quantity.md` §5.
    #[serde(deny_unknown_fields)]
    Choice {
        choices: Vec<f32>,
        labels: Vec<String>,
    },
    /// No static bound beyond what the kind implies. Encoded as the
    /// **absence** of `range` and `choices`.
    #[serde(deny_unknown_fields)]
    Free {},
}
```

> **Note on `Free`.** `serde(untagged)` requires every variant to
> match *some* concrete shape. `Free {}` (a struct variant with zero
> fields) parses an empty table — which is exactly what "no
> `range`, no `choices`" looks like in TOML. Verify this works
> against the `untagged` decode path; if not, fall back to
> `Free(serde::de::IgnoredAny)` or a dedicated unit struct
> wrapper. Report which one shipped.

### `kind.rs` follow-on

`Kind::default_constraint` constructs `Constraint::Range { min: 0.0,
max: 1.0, step: None }` today. Update those call sites to the new
shape:

```rust
Self::Amplitude | Self::Ratio | Self::Phase => Constraint::Range {
    range: [0.0, 1.0],
    step: None,
},
Self::Count => Constraint::Range {
    range: [0.0, 2_147_483_647.0],
    step: None,
},
_ => Constraint::Free {},
```

(Note `Free {}` not `Free`; the struct-variant form changed.)

### `lp-domain/lp-domain/src/visual/mod.rs`

```rust
//! Visual artifacts and their substructure types.
//!
//! See `docs/design/lightplayer/domain.md` for the Visual taxonomy
//! (Pattern / Effect / Transition / Stack / Live / Playlist) and
//! `docs/design/lpfx/overview.md` for the lpfx vocabulary.

pub mod shader_ref;
pub mod visual_input;

pub use shader_ref::ShaderRef;
pub use visual_input::VisualInput;
```

(Phases 06 / 07 will extend this with the actual Visual types.)

### `lp-domain/lp-domain/src/visual/shader_ref.rs`

```rust
//! [`ShaderRef`]: how a Visual specifies its shader source. Three
//! mutually exclusive forms — inline GLSL, sibling file (language
//! inferred from extension), or builtin Rust impl by name. See
//! `docs/design/lpfx/overview.md` and the M3 design doc.

use crate::types::Name;
use alloc::string::String;

/// The shader source backing a [`crate::visual::pattern::Pattern`],
/// [`crate::visual::effect::Effect`], or [`crate::visual::transition::Transition`].
///
/// TOML form (mutex keys under `[shader]`):
///
/// ```toml
/// [shader]
/// glsl = """ … """          # inline source
/// # OR
/// file = "main.glsl"        # sibling path; language by extension
/// # OR
/// builtin = "fluid"         # built-in Rust impl
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ShaderRef {
    #[serde(deny_unknown_fields)]
    Glsl    { glsl: String },
    #[serde(deny_unknown_fields)]
    File    { file: String },
    #[serde(deny_unknown_fields)]
    Builtin { builtin: Name },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glsl_variant_round_trips() {
        let s = ShaderRef::Glsl { glsl: "void main() {}".into() };
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn file_variant_round_trips() {
        let s = ShaderRef::File { file: "main.glsl".into() };
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn builtin_variant_round_trips() {
        let s = ShaderRef::Builtin { builtin: Name::parse("fluid").unwrap() };
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn two_keys_present_is_an_error() {
        let toml_str = r#"
            glsl = "void main() {}"
            file = "main.glsl"
        "#;
        let res: Result<ShaderRef, _> = toml::from_str(toml_str);
        assert!(res.is_err(), "two mutex keys must error: got {res:?}");
    }

    #[test]
    fn unknown_key_is_an_error() {
        let toml_str = r#"
            wgsl = "fn main() {}"
        "#;
        let res: Result<ShaderRef, _> = toml::from_str(toml_str);
        assert!(res.is_err(), "unknown key must error: got {res:?}");
    }
}
```

> **Note on the `Builtin { builtin: Name }` shape.** The variant
> field is named `builtin` (matching the TOML key) so `untagged`
> decoding works on the peer-key. Same trick used by `Glsl { glsl:
> ... }` and `File { file: ... }`. If `serde(untagged)` complains
> about same-named field-and-variant for some reason, the fallback
> is custom `Deserialize` (~10 LOC) — but try `untagged` first.

### `lp-domain/lp-domain/src/visual/visual_input.rs`

```rust
//! [`VisualInput`]: the polymorphic input slot of a
//! [`crate::visual::stack::Stack`] or
//! [`crate::visual::effect::Effect`]. Either composes a child Visual
//! into the node tree or routes from a bus channel.
//!
//! `[input]` is **structural composition**, not a binding. A
//! [`crate::binding::Binding`] is pure routing: it points to existing
//! values and never instantiates nodes.
//! [`VisualInput::Visual`] *does* instantiate a child node, which is
//! why it lives here and not as a `Binding` variant. See `00-notes.md`
//! Q-D3 for the full discussion.

use crate::types::{ArtifactSpec, ChannelName};
use alloc::collections::BTreeMap;
use alloc::string::String;

/// One input slot of a Stack or Effect.
///
/// TOML form (mutex keys under `[input]`):
///
/// ```toml
/// [input]
/// visual = "../patterns/fbm.pattern.toml"
///
/// [input.params]
/// scale = 6.0
/// ```
///
/// or
///
/// ```toml
/// [input]
/// bus = "video/in/0"
/// ```
///
/// `params` value-overrides on the [`VisualInput::Visual`] variant
/// are kept as raw `toml::Value` in v0; type-checking them needs the
/// referenced Visual's param schema, which is a cross-artifact
/// concern explicitly out of scope for M3.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum VisualInput {
    #[serde(deny_unknown_fields)]
    Visual {
        visual: ArtifactSpec,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        params: BTreeMap<String, toml::Value>,
    },
    #[serde(deny_unknown_fields)]
    Bus {
        bus: ChannelName,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_variant_round_trips() {
        let v = VisualInput::Visual {
            visual: ArtifactSpec("../patterns/fbm.pattern.toml".into()),
            params: BTreeMap::new(),
        };
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn visual_with_params_round_trips() {
        let mut params = BTreeMap::new();
        params.insert("scale".into(), toml::Value::Float(6.0));
        let v = VisualInput::Visual {
            visual: ArtifactSpec("../patterns/fbm.pattern.toml".into()),
            params,
        };
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn bus_variant_round_trips() {
        let v = VisualInput::Bus { bus: ChannelName("video/in/0".into()) };
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn both_keys_present_is_an_error() {
        let toml_str = r#"
            visual = "../patterns/fbm.pattern.toml"
            bus    = "video/in/0"
        "#;
        let res: Result<VisualInput, _> = toml::from_str(toml_str);
        assert!(res.is_err());
    }

    #[test]
    fn neither_key_present_is_an_error() {
        let toml_str = r#""#;
        let res: Result<VisualInput, _> = toml::from_str(toml_str);
        assert!(res.is_err());
    }
}
```

> **Note on the `Visual { visual: ArtifactSpec, params: ... }`
> field-naming.** Same trick as `ShaderRef::Glsl`: variant field
> named after the discriminating TOML key. `params` is a
> non-discriminator nested table; it does not break `untagged`
> selection because the variant is selected by presence of `visual`
> vs `bus`. Verify by running the unit tests; if `untagged` ambiguity
> appears, fall back to custom `Deserialize` (~10 LOC).

### `lib.rs` wiring

```rust
// In lp-domain/lp-domain/src/lib.rs (additive edits):
pub mod visual;

pub use visual::{ShaderRef, VisualInput};
```

(Place near the existing `pub use binding::Binding;` re-exports;
mirror the local style.)

### `Constraint` tests update

The existing `range_constraint_round_trips` and friends use the old
`{type, min, max, step}` shape. Update to the new
`{range: [a, b], step: ?}` shape:

```rust
#[test]
fn range_constraint_round_trips() {
    let c = Constraint::Range { range: [0.0, 5.0], step: Some(0.1) };
    let s = serde_json::to_string(&c).unwrap();
    let back: Constraint = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn range_emits_array_form() {
    let c = Constraint::Range { range: [0.0, 1.0], step: None };
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains("\"range\":[0.0,1.0]"), "got {s}");
    assert!(!s.contains("step"));
}

#[test]
fn choice_round_trips() {
    let c = Constraint::Choice {
        choices: alloc::vec![0.0, 1.0, 2.0],
        labels:  alloc::vec![String::from("low"), String::from("med"), String::from("high")],
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: Constraint = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn free_round_trips_as_empty_object() {
    let c = Constraint::Free {};
    let s = serde_json::to_string(&c).unwrap();
    assert_eq!(s, "{}");
    let back: Constraint = serde_json::from_str("{}").unwrap();
    assert_eq!(c, back);
}

#[test]
fn unknown_field_in_range_is_rejected() {
    // deny_unknown_fields: typo in `step` → hard error, not silent default
    let res: Result<Constraint, _> =
        serde_json::from_str(r#"{"range":[0,1],"setp":0.1}"#);
    assert!(res.is_err());
}

#[test]
fn range_loads_from_toml() {
    let c: Constraint = toml::from_str("range = [0, 5]\nstep = 1\n").unwrap();
    match c {
        Constraint::Range { range, step } => {
            assert_eq!(range, [0.0, 5.0]);
            assert_eq!(step, Some(1.0));
        }
        _ => panic!("expected Range"),
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

All must pass with **zero warnings**.

## Definition of done

- `Constraint` uses peer-key inference via `#[serde(untagged)]`;
  `Range` carries `range: [f32; 2]`; `Free {}` is a zero-field
  struct variant.
- Per-variant `#[serde(deny_unknown_fields)]` on all three
  variants.
- `kind.rs::default_constraint` updated to the new field shapes
  (and `Free {}` form).
- `visual/shader_ref.rs` and `visual/visual_input.rs` exist with
  full tests.
- `visual/mod.rs` declares the modules and re-exports the types.
- `lib.rs` re-exports `ShaderRef` and `VisualInput`.
- Each new enum has at least 5 unit tests: per-variant
  round-trip, two-keys-present error, unknown-key error.
- All pre-existing `Slot`, `Shape`, `Constraint`, `Binding`,
  `Kind` tests still pass.
- No commit.

Report back with: list of changed files, validation output, whether
`#[serde(untagged)]` + `Free {}` worked as expected (or whether the
`IgnoredAny` fallback was needed), and any deviations.
