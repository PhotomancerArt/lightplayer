# Phase 07 — `Stack`, `Live`, `Playlist` Visual structs (+ supporting types)

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phase 06 merged. `cargo test -p lp-domain` passing.
>
> **Parallel with:** none. Step 07 depends on Phase 06's
> `visual/mod.rs` re-exports being in place.

## Scope of phase

Implement the three "composer" Visual structs and their supporting
substructure types:

- `Stack` — composes a base input with a chain of `Effect`s.
- `Live` — barebones placeholder with `candidates` + a default
  `transition` + `bindings`. **No `selection` field** in M3.
- `Playlist` — sequenced entries with a single default `transition`,
  optional `loop` behavior, and `bindings`. **No per-entry transition
  overrides** in M3.

Supporting types:

- `EffectRef` — `Stack`'s effect-chain entry, references an `Effect`
  artifact with optional param overrides.
- `LiveCandidate` — Live's candidate entry, references a Visual with
  a priority.
- `TransitionRef` — Live's and Playlist's transition reference, with
  a `duration` and optional param overrides.
- `PlaylistEntry` — Playlist's entry with optional `duration`
  (None = wait-for-cue).
- `PlaylistBehavior` — `loop: bool` only in M3.

`[bindings]` cascade lives on Live and Playlist as
`Vec<(String, Binding)>` with raw relative-NodePropSpec keys (no
parsing in M3, per Q2 reversal).

References:
- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md).
- [`docs/design/lpfx/overview.md`](../../design/lpfx/overview.md) —
  Stack / Live / Playlist authoring vocabulary, `[bindings]` cascade.
- `00-design.md` for the locked-in struct shapes.

**In scope:**

- `lp-domain/lp-domain/src/visual/stack.rs` — `Stack` + `EffectRef`
  + tests.
- `lp-domain/lp-domain/src/visual/live.rs` — `Live` + `LiveCandidate`
  + tests.
- `lp-domain/lp-domain/src/visual/playlist.rs` — `Playlist` +
  `PlaylistEntry` + `PlaylistBehavior` + tests.
- `lp-domain/lp-domain/src/visual/transition_ref.rs` —
  `TransitionRef` (shared by Live and Playlist) + tests.
- `lp-domain/lp-domain/src/visual/mod.rs` — add modules + re-exports.
- `lp-domain/lp-domain/src/lib.rs` — add the three Visual types
  to the crate-root re-exports.

**Out of scope:**

- `Live::selection` / `min_hold` / `debounce` — explicitly deferred.
- Per-entry transitions on Playlist — explicitly deferred.
- Type-checking the inline `params` overrides — they stay as raw
  `BTreeMap<String, toml::Value>`.
- `[bindings]` key parsing / cross-artifact validation — stay as raw
  `String` keys.
- Loader API (Phase 08).
- Examples (Phase 09).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- Tests at the **bottom** of each file.
- `#[test]` first, helpers below inside `mod tests`.
- Document each Visual + supporting type with 1-2 paragraphs and
  a `# Examples` block. Don't field-narrate.
- `bindings: Vec<(String, Binding)>` — document explicitly that
  the key is a raw relative-NodePropSpec string in M3 with no
  parsing yet, with a TODO comment pointing at the binding
  resolution roadmap.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add a `selection` field to `Live`.
- Do **not** add a `transition: Option<TransitionRef>` field to
  `PlaylistEntry`.
- Do **not** type-check `params` overrides.
- Do **not** try to parse the `bindings` keys into `NodePropSpec`s.
- Do **not** suppress warnings.
- All Visual structs use `#[serde(deny_unknown_fields)]`.
- `Artifact::KIND` strings: `"stack"`, `"live"`, `"playlist"`.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, any
  deviations.

## Implementation

### `TransitionRef` (shared)

```rust
//! [`TransitionRef`]: a reference to a [`crate::visual::Transition`]
//! artifact with a duration and optional inline param overrides. Used
//! by [`crate::visual::Live`] and [`crate::visual::Playlist`] as the
//! default transition between candidates / entries.

use crate::types::ArtifactSpec;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Reference to a Transition with playback parameters.
///
/// `duration` is in seconds. `params` overrides are stored as raw
/// `toml::Value` in v0; type-checking against the referenced
/// Transition's param schema lands with cross-artifact resolution.
///
/// # Example
///
/// ```text
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 1.5
///
/// [transition.params]
/// softness = 0.7
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TransitionRef {
    pub visual: ArtifactSpec,
    pub duration: f32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_ref_round_trips() {
        let t: TransitionRef = toml::from_str(r#"
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.5
        "#).unwrap();
        let s = toml::to_string(&t).unwrap();
        let back: TransitionRef = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn transition_ref_with_params_round_trips() {
        let t: TransitionRef = toml::from_str(r#"
            visual   = "../transitions/wipe.transition.toml"
            duration = 2.0
            [params]
            angle = 0.785
        "#).unwrap();
        assert!(t.params.contains_key("angle"));
        let s = toml::to_string(&t).unwrap();
        let back: TransitionRef = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }
}
```

### `Stack`

```rust
//! [`Stack`]: composes a base input with a sequence of [`Effect`]
//! references. See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::schema::Artifact;
use crate::types::ArtifactSpec;
use crate::visual::{params_table::ParamsTable, visual_input::VisualInput};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// One Effect in a Stack's chain. Order is the order of declaration.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EffectRef {
    pub visual: ArtifactSpec,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

/// Composes a base input through a sequence of effects.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Psychedelic"
///
/// [input]
/// visual = "../patterns/fbm.pattern.toml"
///
/// [[effects]]
/// visual = "../effects/tint.effect.toml"
///
/// [[effects]]
/// visual = "../effects/kaleidoscope.effect.toml"
///
/// [params.intensity]
/// kind    = "amplitude"
/// default = 1.0
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<VisualInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectRef>,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Stack {
    const KIND: &'static str = "stack";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn psychedelic_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Psychedelic"

            [input]
            visual = "../patterns/fbm.pattern.toml"

            [[effects]]
            visual = "../effects/tint.effect.toml"

            [[effects]]
            visual = "../effects/kaleidoscope.effect.toml"
        "#
    }

    #[test]
    fn stack_loads_with_two_effects() {
        let s: Stack = toml::from_str(psychedelic_toml()).unwrap();
        assert_eq!(s.effects.len(), 2);
        assert!(matches!(s.input, Some(VisualInput::Visual { .. })));
    }

    #[test]
    fn stack_round_trips() {
        let s: Stack = toml::from_str(psychedelic_toml()).unwrap();
        let out = toml::to_string(&s).unwrap();
        let back: Stack = toml::from_str(&out).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn stack_with_no_input_or_effects_loads() {
        let s: Stack = toml::from_str(r#"
            schema_version = 1
            title          = "Empty"
        "#).unwrap();
        assert!(s.input.is_none());
        assert!(s.effects.is_empty());
    }

    #[test]
    fn stack_kind_constant() {
        assert_eq!(Stack::KIND, "stack");
        assert_eq!(Stack::CURRENT_VERSION, 1);
    }
}
```

### `Live` (barebones, no `selection`)

```rust
//! [`Live`]: a placeholder Visual for live-mode authoring. M3 ships
//! a barebones form: a list of candidates, a default transition,
//! and the `[bindings]` cascade. The `[selection]` block (min_hold,
//! debounce, etc.) is **deferred** — see `00-notes.md` Q-D4d.
//!
//! Live mode requires runtime input wiring that is out of scope
//! for the M3 milestone; this struct exists primarily to reserve
//! the on-disk shape and the artifact KIND.

use crate::binding::Binding;
use crate::schema::Artifact;
use crate::types::ArtifactSpec;
use crate::visual::transition_ref::TransitionRef;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// One candidate Visual in a Live show.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LiveCandidate {
    pub visual: ArtifactSpec,
    #[serde(default = "default_priority")]
    pub priority: f32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

fn default_priority() -> f32 { 1.0 }

/// Live show: candidates + default transition + bindings cascade.
/// **No selection block in M3** (Q-D4d).
///
/// `bindings` keys are raw relative-NodePropSpec strings in M3; cross-
/// artifact validation lands with the binding resolution roadmap.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Main Live"
///
/// [[candidates]]
/// visual   = "../patterns/rainbow.pattern.toml"
/// priority = 1.0
///
/// [[candidates]]
/// visual   = "../stacks/psychedelic.stack.toml"
/// priority = 0.5
///
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 2.0
///
/// [bindings]
/// "rainbow.pattern#params.speed" = { bus = "audio/in/0/level" }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Live {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub candidates: Vec<LiveCandidate>,
    pub transition: TransitionRef,
    // TODO(binding-resolution): keys are raw relative-NodePropSpec
    // strings in M3; parse + validate against the candidates' param
    // schemas when binding resolution lands.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<(String, Binding)>,
}

impl Artifact for Live {
    const KIND: &'static str = "live";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main_live_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Main"

            [[candidates]]
            visual   = "../patterns/rainbow.pattern.toml"
            priority = 1.0

            [[candidates]]
            visual   = "../patterns/fluid.pattern.toml"
            priority = 0.5

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 2.0
        "#
    }

    #[test]
    fn live_loads_with_two_candidates() {
        let l: Live = toml::from_str(main_live_toml()).unwrap();
        assert_eq!(l.candidates.len(), 2);
        assert_eq!(l.transition.duration, 2.0);
    }

    #[test]
    fn live_round_trips() {
        let l: Live = toml::from_str(main_live_toml()).unwrap();
        let s = toml::to_string(&l).unwrap();
        let back: Live = toml::from_str(&s).unwrap();
        assert_eq!(l, back);
    }

    #[test]
    fn live_with_bindings_loads() {
        // `bindings` round-trip: keys are raw strings, values are
        // Binding. The TOML inline form `bind = { bus = "..." }`
        // must apply.
        let toml = r#"
            schema_version = 1
            title = "Main"

            [[candidates]]
            visual = "../patterns/rainbow.pattern.toml"

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0

            [bindings]
            "rainbow.pattern#params.speed" = { bus = "audio/in/0/level" }
        "#;
        let l: Live = toml::from_str(toml).unwrap();
        assert_eq!(l.bindings.len(), 1);
        assert_eq!(l.bindings[0].0, "rainbow.pattern#params.speed");
    }

    #[test]
    fn selection_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Main"
            [[candidates]]
            visual = "../patterns/rainbow.pattern.toml"
            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0
            [selection]
            min_hold = 5.0
        "#;
        let res: Result<Live, _> = toml::from_str(toml);
        assert!(res.is_err(), "Live has no [selection] field in M3");
    }

    #[test]
    fn live_kind_constant() {
        assert_eq!(Live::KIND, "live");
        assert_eq!(Live::CURRENT_VERSION, 1);
    }
}
```

> **Note on `Vec<(String, Binding)>` serde shape.** The default
> serde shape for a `Vec<(K, V)>` is a JSON array of 2-element
> arrays, *not* an object. Authors expect `[bindings]` to be a
> TOML table, so we need either:
>
> 1. `BTreeMap<String, Binding>` (loses insertion order).
> 2. `IndexMap<String, Binding>` if `indexmap` is already a dep
>    (preserves order).
> 3. A custom serde wrapper that round-trips between
>    `Vec<(String, Binding)>` and a TOML table.
>
> **Recommended:** option 1 (`BTreeMap`). Cascade resolution
> doesn't depend on order; alphabetical order is fine for the
> Visual binding cascade in M3. Update the field type to
> `BTreeMap<String, Binding>`. Document the change in
> `00-notes.md` and the design doc.
>
> Sub-agent should make this swap as part of Phase 07.

### `Playlist`

```rust
//! [`Playlist`]: a sequenced list of Visual entries. Each entry plays
//! for `duration` seconds (or waits for cue if `None`); the playlist
//! cross-fades between entries through its single default transition.
//! See `docs/design/lightplayer/domain.md`.
//!
//! M3 deliberately omits per-entry transition overrides
//! (Q-D4); they are an additive future change.

use crate::binding::Binding;
use crate::schema::Artifact;
use crate::types::ArtifactSpec;
use crate::visual::transition_ref::TransitionRef;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// One entry in a Playlist. `duration: None` means "wait for cue".
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlaylistEntry {
    pub visual: ArtifactSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

/// Playlist behavior flags. M3 carries `loop` only; more land
/// additively (`shuffle`, `random_seed`, etc.).
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlaylistBehavior {
    #[serde(default, rename = "loop")]
    pub r#loop: bool,
}

/// Sequenced Visual entries with a single default transition.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Setlist"
///
/// [[entries]]
/// visual   = "../patterns/rainbow.pattern.toml"
/// duration = 60.0
///
/// [[entries]]
/// visual   = "../stacks/psychedelic.stack.toml"
/// duration = 90.0
///
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 1.5
///
/// [behavior]
/// loop = true
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub entries: Vec<PlaylistEntry>,
    pub transition: TransitionRef,
    #[serde(default)]
    pub behavior: PlaylistBehavior,
    // TODO(binding-resolution): see Live::bindings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, Binding>,
}

impl Artifact for Playlist {
    const KIND: &'static str = "playlist";
    const CURRENT_VERSION: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setlist_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Setlist"

            [[entries]]
            visual   = "../patterns/rainbow.pattern.toml"
            duration = 60.0

            [[entries]]
            visual   = "../patterns/fluid.pattern.toml"
            duration = 90.0

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.5

            [behavior]
            loop = true
        "#
    }

    #[test]
    fn playlist_loads_with_two_entries() {
        let p: Playlist = toml::from_str(setlist_toml()).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.behavior.r#loop, true);
    }

    #[test]
    fn playlist_round_trips() {
        let p: Playlist = toml::from_str(setlist_toml()).unwrap();
        let s = toml::to_string(&p).unwrap();
        let back: Playlist = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn entry_with_no_duration_is_wait_for_cue() {
        let p: Playlist = toml::from_str(r#"
            schema_version = 1
            title = "Cued"
            [[entries]]
            visual = "../patterns/rainbow.pattern.toml"
            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0
        "#).unwrap();
        assert!(p.entries[0].duration.is_none());
    }

    #[test]
    fn per_entry_transition_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Setlist"
            [[entries]]
            visual = "../patterns/rainbow.pattern.toml"
            [entries.transition]
            visual = "../transitions/wipe.transition.toml"
            duration = 0.5
            [transition]
            visual = "../transitions/crossfade.transition.toml"
            duration = 1.0
        "#;
        let res: Result<Playlist, _> = toml::from_str(toml);
        assert!(res.is_err(), "Per-entry transition overrides are explicitly out of scope for M3");
    }

    #[test]
    fn playlist_kind_constant() {
        assert_eq!(Playlist::KIND, "playlist");
        assert_eq!(Playlist::CURRENT_VERSION, 1);
    }
}
```

### `visual/mod.rs` updates

```rust
pub mod params_table;
pub mod pattern;
pub mod effect;
pub mod transition;
pub mod transition_ref;
pub mod stack;
pub mod live;
pub mod playlist;
pub mod shader_ref;
pub mod visual_input;

pub use params_table::ParamsTable;
pub use pattern::Pattern;
pub use effect::Effect;
pub use transition::Transition;
pub use transition_ref::TransitionRef;
pub use stack::{Stack, EffectRef};
pub use live::{Live, LiveCandidate};
pub use playlist::{Playlist, PlaylistEntry, PlaylistBehavior};
pub use shader_ref::ShaderRef;
pub use visual_input::VisualInput;
```

### `lib.rs` updates

```rust
pub use visual::{
    Effect, EffectRef, Live, LiveCandidate, ParamsTable, Pattern,
    Playlist, PlaylistBehavior, PlaylistEntry, ShaderRef, Stack,
    Transition, TransitionRef, VisualInput,
};
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

- `Stack`, `Live`, `Playlist` exist with their fields, derives,
  `Artifact` impls, and rustdoc.
- `EffectRef`, `LiveCandidate`, `TransitionRef`, `PlaylistEntry`,
  `PlaylistBehavior` exist as supporting types.
- `Live` has **no** `selection` field; load test asserts a
  `[selection]` block hard-errors.
- `Playlist` has **no** per-entry `transition` field; load test
  asserts a per-entry override hard-errors.
- All Visuals use `#[serde(deny_unknown_fields)]`.
- `bindings` field type is `BTreeMap<String, Binding>` (or the
  doc'd alternative if the sub-agent picked one) — table form, not
  array-of-pairs. Round-trip test confirms TOML `[bindings]` table
  works.
- Each Visual has at least 4 tests: minimal load, round-trip,
  `KIND` / `CURRENT_VERSION` constants, and at least one negative
  test (rejected field).
- `visual/mod.rs` and `lib.rs` re-exports updated.
- All pre-existing tests still pass.
- No commit.

Report back with: list of changed files, validation output, the
chosen `bindings` field type, any deviations.
