# Phase 06 — `Pattern`, `Effect`, `Transition` Visual structs

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phases 04 (`ShaderRef` / `VisualInput` / new
> `Constraint`) and 05 (`ParamsTable` / literal `ValueSpec`) merged.
> `cargo test -p lp-domain` passing.
>
> **Parallel with:** Phase 07 (Stack / Live / Playlist). Disjoint
> files. Phase 07 reuses `ShaderRef` indirectly via TransitionRef
> referencing a Transition; the type is already exported from Phase 04.

## Scope of phase

Implement the three "leaf" Visual structs: Pattern, Effect, and
Transition. Each:

- Carries the fixed Visual header (`schema_version`, `title`,
  `description`, `author`).
- Has a `[shader]` section (`ShaderRef`).
- Has a `[params]` block (`ParamsTable`).
- Implements the `Artifact` trait with its kind constant and
  `CURRENT_VERSION = 1`.

Pattern and Transition have **no** `[input]` field. Effect has
`input: Option<VisualInput>`.

References:
- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md)
  — Pattern / Effect / Transition definitions.
- [`docs/design/lpfx/overview.md`](../../design/lpfx/overview.md) —
  authoring vocabulary.
- `00-design.md` for the locked-in struct shapes.

**In scope:**

- `lp-domain/lp-domain/src/visual/pattern.rs` — `Pattern` struct +
  `impl Artifact` + tests.
- `lp-domain/lp-domain/src/visual/effect.rs` — `Effect` struct +
  `impl Artifact` + tests.
- `lp-domain/lp-domain/src/visual/transition.rs` — `Transition`
  struct + `impl Artifact` + tests.
- `lp-domain/lp-domain/src/visual/mod.rs` — add the three modules
  and re-export the types.
- `lp-domain/lp-domain/src/lib.rs` — add the three new types to
  the crate-root re-exports.

**Out of scope:**

- Stack / Live / Playlist (Phase 07).
- Loader API (Phase 08).
- Example TOMLs (Phase 09).
- Round-trip integration tests (Phase 10).
- Cross-artifact validation (Stack referencing a missing Pattern,
  etc. — out of M3 entirely).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- Tests at the **bottom** of each module file.
- Inside `mod tests`: `#[test]` first, helpers below.
- Document each Visual type with a 2–3 line rustdoc explaining
  its role + a link to `domain.md` and the example TOML.
- No field-by-field rustdoc on derived structs; field names + types
  + the `Examples` block carry the meaning.
- Each Visual has a `# Examples` block in its rustdoc showing the
  canonical TOML form (one of the corpus examples). The example
  doctest doesn't have to compile — `text` fences are fine.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add a `selection` field to anything (Live placeholder
  belongs in Phase 07; Pattern / Effect / Transition don't have
  one regardless).
- Do **not** add an `[input]` field to Pattern or Transition.
  Effect's `input` is `Option<VisualInput>`.
- Do **not** suppress warnings.
- `Artifact::KIND` strings are snake_case singular: `"pattern"`,
  `"effect"`, `"transition"`.
- Use `#[serde(default)]` on optional fields (`description`,
  `author`, `params`, `bindings` later in Phase 07, etc.).
- Use `#[serde(skip_serializing_if = "Option::is_none")]` on every
  optional field for clean round-trips.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, any
  deviations.

## Implementation

### `Pattern`

```rust
//! [`Pattern`]: a single-output Visual whose pixels are driven by a shader.
//! See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::schema::Artifact;
use crate::visual::{params_table::ParamsTable, shader_ref::ShaderRef};
use alloc::string::String;

/// A texture-producing Visual: shader source + parameter surface. No
/// input slot; Patterns generate their pixels from `time`, params, and
/// any bus-routed bindings on those params.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Rainbow"
/// description    = "Rolling rainbow with HSL hue rotation."
///
/// [shader]
/// glsl = """ … """
///
/// [params.speed]
/// kind    = "amplitude"
/// default = 0.25
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub shader: ShaderRef,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Pattern {
    const KIND: &'static str = "pattern";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_pattern_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Tiny"

            [shader]
            glsl = "void main() {}"
        "#
    }

    #[test]
    fn minimal_pattern_loads() {
        let p: Pattern = toml::from_str(minimal_pattern_toml()).unwrap();
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.title, "Tiny");
        assert!(matches!(p.shader, ShaderRef::Glsl { .. }));
    }

    #[test]
    fn pattern_round_trips_minimal() {
        let p: Pattern = toml::from_str(minimal_pattern_toml()).unwrap();
        let s = toml::to_string(&p).unwrap();
        let back: Pattern = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn pattern_with_file_shader_loads() {
        let toml = r#"
            schema_version = 1
            title = "FBM"
            [shader]
            file = "main.glsl"
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        assert!(matches!(p.shader, ShaderRef::File { ref file } if file == "main.glsl"));
    }

    #[test]
    fn pattern_with_builtin_shader_loads() {
        let toml = r#"
            schema_version = 1
            title = "Fluid"
            [shader]
            builtin = "fluid"
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        assert!(matches!(p.shader, ShaderRef::Builtin { .. }));
    }

    #[test]
    fn pattern_with_params_loads() {
        let toml = r#"
            schema_version = 1
            title = "Tiny"
            [shader]
            glsl = "void main() {}"
            [params.speed]
            kind    = "amplitude"
            default = 1.0
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        // ParamsTable wraps a Slot whose Shape is Struct; one field.
        // Concrete assertion left to ParamsTable::tests; here just
        // confirm the struct field made it through.
        assert!(toml::to_string(&p).unwrap().contains("speed"));
    }

    #[test]
    fn pattern_kind_constant_is_pattern() {
        assert_eq!(Pattern::KIND, "pattern");
        assert_eq!(Pattern::CURRENT_VERSION, 1);
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Tiny"
            future_field = "oops"
            [shader]
            glsl = "void main() {}"
        "#;
        let res: Result<Pattern, _> = toml::from_str(toml);
        assert!(res.is_err(), "unknown top-level field must error");
    }
}
```

### `Effect`

Identical shape to `Pattern` plus an `input: Option<VisualInput>`
field. `KIND = "effect"`.

```rust
//! [`Effect`]: a single-input Visual that transforms its input texture
//! through a shader. See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::schema::Artifact;
use crate::visual::{
    params_table::ParamsTable,
    shader_ref::ShaderRef,
    visual_input::VisualInput,
};
use alloc::string::String;

/// An input-transforming Visual: input slot + shader + parameter
/// surface. The shader reads the input via the `inputColor` uniform
/// (convention; not enforced by this layer).
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Tint"
///
/// [shader]
/// glsl = """ … """
///
/// [input]
/// bus = "video/in/0"
///
/// [params.color]
/// kind    = "color"
/// default = { space = "oklch", coords = [0.7, 0.15, 90] }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub shader: ShaderRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<VisualInput>,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Effect {
    const KIND: &'static str = "effect";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_effect_loads_without_input() {
        let toml = r#"
            schema_version = 1
            title = "Identity"
            [shader]
            glsl = "void main() {}"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(e.input.is_none());
    }

    #[test]
    fn effect_with_bus_input_loads() {
        let toml = r#"
            schema_version = 1
            title = "Tint"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(matches!(e.input, Some(VisualInput::Bus { .. })));
    }

    #[test]
    fn effect_with_visual_input_loads() {
        let toml = r#"
            schema_version = 1
            title = "Stacked tint"
            [shader]
            glsl = "void main() {}"
            [input]
            visual = "../patterns/fbm.pattern.toml"
            [input.params]
            scale = 6.0
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(matches!(e.input, Some(VisualInput::Visual { .. })));
    }

    #[test]
    fn effect_round_trips() {
        let toml = r#"
            schema_version = 1
            title = "Tint"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        let s = toml::to_string(&e).unwrap();
        let back: Effect = toml::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn effect_kind_constant() {
        assert_eq!(Effect::KIND, "effect");
        assert_eq!(Effect::CURRENT_VERSION, 1);
    }
}
```

### `Transition`

Same shape as `Pattern` (no `input` field). `KIND = "transition"`.

```rust
//! [`Transition`]: a 2-arity Visual that crossfades / blends two
//! inputs over `progress` ∈ [0, 1]. Inputs are conventional shader
//! uniforms (`inputA`, `inputB`); not declared in the artifact.
//! See `docs/design/lightplayer/domain.md`.

use crate::schema::Artifact;
use crate::visual::{params_table::ParamsTable, shader_ref::ShaderRef};
use alloc::string::String;

/// A 2-input Visual that interpolates between `inputA` and `inputB`
/// based on the `progress` parameter. Used by Live (between
/// candidates) and Playlist (between entries).
///
/// `progress` is conventionally a shader uniform driven by the
/// caller (Live / Playlist runtime); the artifact doesn't declare
/// it as a Slot.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Crossfade"
///
/// [shader]
/// glsl = """ … """
///
/// [params.softness]
/// kind    = "amplitude"
/// default = 1.0
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub shader: ShaderRef,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Transition {
    const KIND: &'static str = "transition";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_transition_loads() {
        let toml = r#"
            schema_version = 1
            title = "Crossfade"
            [shader]
            glsl = "void main() {}"
        "#;
        let t: Transition = toml::from_str(toml).unwrap();
        assert_eq!(t.title, "Crossfade");
    }

    #[test]
    fn transition_round_trips() {
        let toml = r#"
            schema_version = 1
            title = "Wipe"
            [shader]
            glsl = "void main() {}"
            [params.angle]
            kind    = "angle"
            default = 0.0
        "#;
        let t: Transition = toml::from_str(toml).unwrap();
        let s = toml::to_string(&t).unwrap();
        let back: Transition = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn input_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Wipe"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let res: Result<Transition, _> = toml::from_str(toml);
        assert!(res.is_err(), "Transition has no [input] field; deny_unknown_fields must reject it");
    }

    #[test]
    fn transition_kind_constant() {
        assert_eq!(Transition::KIND, "transition");
        assert_eq!(Transition::CURRENT_VERSION, 1);
    }
}
```

### `visual/mod.rs` updates

```rust
pub mod params_table;
pub mod pattern;
pub mod effect;
pub mod transition;
pub mod shader_ref;
pub mod visual_input;

pub use params_table::ParamsTable;
pub use pattern::Pattern;
pub use effect::Effect;
pub use transition::Transition;
pub use shader_ref::ShaderRef;
pub use visual_input::VisualInput;
```

### `lib.rs` updates

```rust
pub use visual::{Effect, ParamsTable, Pattern, ShaderRef, Transition, VisualInput};
```

(Or whatever re-export pattern matches the existing `lib.rs` style.)

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `Pattern`, `Effect`, `Transition` exist with their fields,
  derives, `Artifact` impls, and rustdoc.
- `Effect` has `input: Option<VisualInput>`; `Pattern` and
  `Transition` do not.
- All three use `#[serde(deny_unknown_fields)]` so unknown TOML
  keys hard-error.
- Each type has at least 4 tests: minimal load, round-trip,
  `KIND` / `CURRENT_VERSION` constants, and at least one
  negative test (e.g. unknown field, invalid input).
- `visual/mod.rs` and `lib.rs` re-exports updated.
- All pre-existing tests still pass.
- No commit.

Report back with: list of changed files, validation output, any
deviations.
