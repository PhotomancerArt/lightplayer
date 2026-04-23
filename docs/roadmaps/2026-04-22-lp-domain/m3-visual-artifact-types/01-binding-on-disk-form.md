# Phase 01 — `binding.rs` on-disk shape fix

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** nothing (starts the plan).
>
> **Parallel with:** Phase 02 (`Kind::AudioLevel`). Disjoint files.

## Scope of phase

Fix the `TODO(M3)` in
[`lp-domain/lp-domain/src/binding.rs`](../../../lp-domain/lp-domain/src/binding.rs)
so the on-disk JSON shape of `Binding::Bus` matches `quantity.md` §8's
`bind = { bus = "<channel>" }` grammar.

The current shape is externally-tagged: `{"bus":{"channel":"…"}}`.
The target shape is `{"bus":"…"}` (a flat string under a `bus` key,
matching the inline-table TOML form `bind = { bus = "video/in/0" }`).

**In scope:**

- `lp-domain/lp-domain/src/binding.rs` — change the serde shape;
  remove the `TODO(M3)` comment; update the existing tests; add a
  test that asserts the JSON shape is exactly `{"bus":"…"}`.

**Out of scope:**

- Adding new `Binding` variants (Visual, Constant, etc. — those are
  later milestones, if ever).
- TOML loading. JSON serde tests are sufficient here; the TOML loader
  in step 08 will exercise the inline-table form via toml's serde
  bridge.
- `BindingResolver` real implementation (still M3+).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md) § "Code organization in Rust source files":

- Tests at the **bottom** of the file, never at the top.
- Inside the `tests` module, `#[test]` functions first; helpers
  below.
- Top → bottom in the file: docs → uses → public types → impls →
  helpers → `#[cfg(test)] mod tests`.
- Document the domain model well but not overly much: short rustdoc
  on each public item; link to the design doc for "why"; no
  field-by-field narration on derived structs.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope (no new `Binding` variants).
- Do **not** suppress warnings.
- Do **not** disable / weaken existing tests.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, any deviations.

## Implementation

The grammar target (per `quantity.md` §8 and `00-notes.md` Q-D3
vocabulary alignment):

```toml
# In a Slot:
bind = { bus = "video/in/0" }
```

```json
// JSON form (used by tests + schema-gen):
{ "bus": "video/in/0" }
```

The cleanest way to land that with serde is to give `Binding::Bus` a
single unnamed field that serializes as a string, via a tuple variant
or a custom `serde(rename_all = "snake_case", untagged)` setup.
Tuple-variant approach is simpler:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    /// Read or write (per container context) the named channel. See
    /// `docs/design/lightplayer/quantity.md` §8 and §11.
    Bus(ChannelName),
}
```

`ChannelName` is `pub struct ChannelName(pub String)` (per M2's
`types.rs`); serde derives a transparent string serialization for it
already, so `Binding::Bus(ChannelName(...))` round-trips as
`{"bus":"audio/in/0"}` with no extra plumbing.

Verify by reading `lp-domain/lp-domain/src/types.rs` and confirming
that `ChannelName` already has a string-transparent serde shape (M2
should have set it up that way; if not, add `#[serde(transparent)]`
on the newtype as part of this phase — that's a tiny, in-scope fix).

### Updated `binding.rs`

Full file (post-edit):

```rust
//! **Bindings** connect parameter slots to **runtime signals** on the implicit
//! bus (`docs/design/lightplayer/quantity.md` §8).
//!
//! There is no separate "bus object" in authored files: **channels exist** when
//! at least one binding references them; direction (read vs write) comes from
//! the slot's **role** in its container (e.g. under `params` vs an output
//! declaration), not from the [`Binding`] enum (`quantity.md` §8 "Direction is
//! contextual"). The first writer/reader to a channel establishes its
//! [`Kind`](crate::kind::Kind); mismatches are compose-time errors (same
//! section).
//!
//! # On-disk shape
//!
//! Bindings serialize as the inline form `bind = { bus = "<channel>" }` in
//! TOML and `{"bus":"<channel>"}` in JSON. New variants land additively as
//! sibling keys (`bind = { constant = ... }`, etc.); the on-disk grammar
//! stays a flat key-mutex on the `bind` table.

use crate::types::ChannelName;

/// A **connection** from a slot to a bus channel. v0 has a single variant.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    /// Read or write (per container context) the named channel. Convention:
    /// names like `time`, `video/in/0`, `audio/in/0/level` — see
    /// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming).
    Bus(ChannelName),
}

/// **Compose-time** lookup for "what [`Kind`](crate::kind::Kind) does this
/// channel carry?", used to validate that a slot's kind matches the bus. A
/// real implementation lands in M3+; this is only a trait shape
/// (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`).
pub trait BindingResolver {
    /// The kind currently associated with `channel`, if any. `None` means the
    /// channel will be **declared** by this binding (first use), per
    /// `docs/design/lightplayer/quantity.md` §8 "Compose-time validation".
    fn channel_kind(&self, channel: &ChannelName) -> Option<crate::kind::Kind>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn bus_binding_serde_round_trips() {
        let b = Binding::Bus(ChannelName(String::from("audio/in/0")));
        let json = serde_json::to_string(&b).unwrap();
        let back: Binding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn bus_binding_json_form_is_flat_string_under_bus_key() {
        let b = Binding::Bus(ChannelName(String::from("audio/in/0/level")));
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"bus":"audio/in/0/level"}"#);
    }

    #[test]
    fn bus_binding_deserializes_from_inline_string_form() {
        let b: Binding = serde_json::from_str(r#"{"bus":"video/in/0"}"#).unwrap();
        match b {
            Binding::Bus(ChannelName(s)) => assert_eq!(s, "video/in/0"),
        }
    }
}
```

### Possible follow-on edit: `ChannelName` `serde(transparent)`

If `ChannelName` doesn't already serialize transparently as a string,
add the attribute. In `lp-domain/lp-domain/src/types.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct ChannelName(pub String);
```

Run the existing `types.rs` tests after the change — none should
break, but if any explicitly check for an object-form (e.g.
`{"0":"…"}`), update them to expect the string form.

### Cross-crate users to fix

Search for `Binding::Bus { channel: …` in the workspace and
update each to `Binding::Bus(ChannelName(…))`. Currently this is
only `kind.rs::default_bind` (two call sites in M2: `Self::Instant`
and `Self::Texture`). Both already construct `ChannelName(String::…)`;
the patch is mechanical.

```rust
// Before:
Some(Binding::Bus { channel: ChannelName(String::from("time")) })
// After:
Some(Binding::Bus(ChannelName(String::from("time"))))
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

- `binding.rs` `TODO(M3)` comment removed.
- `Binding::Bus` JSON form is `{"bus":"<channel>"}` exactly (test
  asserts this).
- `kind.rs::default_bind` call sites updated to the new
  tuple-variant constructor.
- `ChannelName` is string-transparent in serde (verify or add
  `#[serde(transparent)]`).
- Existing `binding.rs` tests still pass.
- Two new tests added: flat-string JSON shape; round-trip from the
  flat-string form.
- No commit.

Report back with: list of changed files, validation output, and
whether `ChannelName` needed the transparent attribute (i.e. was M2's
shape already correct, or did this phase fix it too).
