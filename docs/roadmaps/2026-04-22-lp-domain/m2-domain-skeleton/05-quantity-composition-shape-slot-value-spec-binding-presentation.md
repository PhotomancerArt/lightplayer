# Phase 5 — Quantity composition: `Shape`, `Slot`, `ValueSpec`, `Binding` extension, `Presentation` tests

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** Phases 3 + 4. Both must be complete and
> `cargo test -p lp-domain` must pass before this phase runs.

## Scope of phase

Implement the composition layer of the Quantity model:

1. `value_spec.rs` — `ValueSpec`, `TextureSpec`, `LoadCtx` stub,
   `materialize` stub.
2. `binding.rs` — extend phase-3's minimal `Binding` enum with
   the `BindingResolver` trait stub. Add tests.
3. `presentation.rs` — extend phase-3's minimal `Presentation`
   enum with tests. (Type itself is already complete; just add
   tests + any documentation.)
4. `shape.rs` — `Shape` enum (with **Q15 Option A**: composed
   variants carry `default: Option<ValueSpec>`, scalar carries
   mandatory `default: ValueSpec`). `Slot` struct (no top-level
   `default` field). `Slot::default_value(ctx)` and
   `Slot::storage()` projection.

This is the heaviest phase. It bundles all the composition-layer
types because they share one mental model and splitting forces
forward-ref dance.

**In scope:**

- `lp-domain/lp-domain/src/value_spec.rs` — full implementation.
- `lp-domain/lp-domain/src/binding.rs` — extend with
  `BindingResolver` trait stub + tests.
- `lp-domain/lp-domain/src/presentation.rs` — add a test module
  (the enum itself was added in phase 3).
- `lp-domain/lp-domain/src/shape.rs` — full implementation
  (Shape, Slot, Slot::default_value, Slot::storage).
- Tests inline at the top of each module.

**Out of scope:**

- TOML parsing of `Slot` (custom `Deserialize`) — M3.
- `BindingResolver` real impl — M3+ (only the trait stub here).
- `LoadCtx` real impl — M3+ (only the stub shape here).
- Concrete `TextureSpec` variants beyond `Black` — M3 grows.
- `LpsValue` round-trip serde — M3 (we deliberately don't add
  serde to `LpsValueF32`; defaults flow through `ValueSpec`).

## Q15 — Composed-Shape defaults — **Option A**

**The defining decision of this milestone.** Read
[`00-notes.md`](./00-notes.md) Q15 section before coding.

```rust
pub enum Shape {
    Scalar { kind: Kind, constraint: Constraint, default: ValueSpec },
    Array  { element: alloc::boxed::Box<Slot>, length: u32, default: Option<ValueSpec> },
    Struct { fields: alloc::vec::Vec<(crate::types::Name, Slot)>, default: Option<ValueSpec> },
}
```

- `Scalar`: mandatory `default`.
- `Array` / `Struct`: `Option<ValueSpec>` override; `None` means
  derive from children at materialize time.
- Round-trip parity via `#[serde(skip_serializing_if =
  "Option::is_none")]` on the composed-default fields.

`Slot` *loses* its top-level `default` field:

```rust
pub struct Slot {
    pub shape:       Shape,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label:       Option<alloc::string::String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<alloc::string::String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bind:        Option<crate::binding::Binding>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub present:     Option<crate::presentation::Presentation>,
}

impl Slot {
    pub fn default_value(&self, ctx: &mut crate::value_spec::LoadCtx) -> crate::LpsValue {
        match &self.shape {
            Shape::Scalar { default, .. } => default.materialize(ctx),
            Shape::Array { element, length, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let mut elems = alloc::vec::Vec::with_capacity(*length as usize);
                    for _ in 0..*length {
                        elems.push(element.default_value(ctx));
                    }
                    crate::LpsValue::Array(elems.into_boxed_slice())
                }
            },
            Shape::Struct { fields, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let entries = fields
                        .iter()
                        .map(|(name, slot)| (name.0.clone(), slot.default_value(ctx)))
                        .collect();
                    crate::LpsValue::Struct { name: None, fields: entries }
                }
            },
        }
    }

    pub fn storage(&self) -> crate::LpsType {
        match &self.shape {
            Shape::Scalar { kind, .. } => kind.storage(),
            Shape::Array { element, length, .. } => crate::LpsType::Array {
                element: alloc::boxed::Box::new(element.storage()),
                len: *length,
            },
            Shape::Struct { fields, .. } => crate::LpsType::Struct {
                name: None,
                members: fields.iter().map(|(name, slot)| {
                    lps_shared::types::StructMember {
                        name: Some(name.0.clone()),
                        ty: slot.storage(),
                    }
                }).collect(),
            },
        }
    }
}
```

> **Verify `LpsValueF32::Array` and `LpsValueF32::Struct`
> shapes** in `lp-shader/lps-shared/src/lps_value_f32.rs` before
> writing `default_value`. Match the variant fields exactly. The
> `Struct { name: None, fields: ... }` form above is a guess —
> adapt to whatever lps_shared actually exposes (it might be
> `fields: Vec<(String, LpsValueF32)>` or
> `fields: Vec<StructMemberValue>` etc.).

## Code Organization Reminders

- Tests at the **top** of each module.
- Helper functions at the **bottom**.
- One concept per file — `Shape` and `Slot` go together in
  `shape.rs` because they're inseparable; everything else in its
  own file.
- All public types derive `serde::{Serialize, Deserialize}` and
  `#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]`.
- No comments narrating what code does.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** expand scope (no TOML parser; no real
  `BindingResolver` impl; no real `LoadCtx`).
- Do **not** suppress warnings or `#[allow(...)]` problems away.
- Do **not** disable, skip, or weaken existing tests.
- Do **not** modify `kind.rs`, `constraint.rs`, `types.rs`, or
  `lib.rs` — phases 2/3 own them.
- If something blocks completion, stop and report back.
- Report back: list of files changed, validation output, any
  deviations.

## Implementation Details

### `lp-domain/lp-domain/src/value_spec.rs`

```rust
//! ValueSpec: author-time defaults (literal values + opaque-handle recipes).
//! See docs/design/lightplayer/quantity.md §7.

use crate::LpsValue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_materializes_to_itself() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Literal(LpsValue::F32(0.5));
        match spec.materialize(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 0.5),
            other => panic!("expected F32(0.5), got {other:?}"),
        }
    }

    #[test]
    fn texture_black_materializes_to_handle_zero() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Texture(TextureSpec::Black);
        let v = spec.materialize(&mut ctx);
        // Texture storage is Struct{format,width,height,handle}; handle is the 4th member.
        // Phase 5's stub ctx hands out handle = 0 deterministically.
        match v {
            LpsValue::Struct { fields, .. } => {
                let handle = fields.iter().find(|(n, _)| n == "handle").expect("handle field");
                match &handle.1 {
                    LpsValue::I32(h) => assert_eq!(*h, 0),
                    _ => panic!("handle must be I32"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }
}

/// Loader-side context. Phase 5 ships a stub; M3 fills with a real
/// texture handle allocator and asset cache.
#[derive(Default)]
pub struct LoadCtx {
    pub next_texture_handle: i32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueSpec {
    Literal(LpsValue),
    Texture(TextureSpec),
    // TODO(M3+): Audio(AudioSpec), Video(VideoSpec), File(FileSpec), ...
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextureSpec {
    Black,
    // TODO(M3+): Solid { color: [f32; 3] }, File { path: String }, Procedural { ... }, ...
}

impl ValueSpec {
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            Self::Literal(v) => v.clone(),
            Self::Texture(spec) => spec.materialize(ctx),
        }
    }
}

impl TextureSpec {
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            // 1×1 fully-opaque black; the universal "no texture" default.
            Self::Black => texture_handle_value(ctx, /*format=*/0, /*w=*/1, /*h=*/1),
        }
    }
}

// --- helpers (at the bottom) --------------------------------------------

fn texture_handle_value(ctx: &mut LoadCtx, format: i32, width: i32, height: i32) -> LpsValue {
    let handle = ctx.next_texture_handle;
    // TODO(M3): a real allocator increments ctx.next_texture_handle here. The
    // stub leaves it at 0 so tests can assert determinism.
    use alloc::string::String;
    LpsValue::Struct {
        name: None,
        fields: alloc::vec![
            (String::from("format"), LpsValue::I32(format)),
            (String::from("width"),  LpsValue::I32(width)),
            (String::from("height"), LpsValue::I32(height)),
            (String::from("handle"), LpsValue::I32(handle)),
        ],
    }
}
```

> **`LpsValue::Struct` shape: verify before coding.** Same caveat
> as the `Slot::default_value` example above — check the actual
> variant in `lps_value_f32.rs` and adjust.
>
> `LpsValueF32::Array` and `LpsValueF32::Struct` *might* not have
> the exact fields named here. Do NOT invent. Read the source,
> match it.

This applies to **all** `LpsValue::Array {...}` and
`LpsValue::Struct {...}` constructions in this phase — including
the ones in `Slot::default_value`. Adjust phase-5 code to match
the real `LpsValueF32` variant signatures.

### `lp-domain/lp-domain/src/binding.rs` (extend phase 3's stub)

Phase 3 left this file with just the `Binding` enum. Extend it
with:

```rust
//! Binding enum (bus connection) + BindingResolver trait stub.
//! See docs/design/lightplayer/quantity.md §8.

use crate::types::ChannelName;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    #[test]
    fn bus_binding_serde_round_trips() {
        let b = Binding::Bus { channel: ChannelName(String::from("audio/in/0")) };
        let json = serde_json::to_string(&b).unwrap();
        let back: Binding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn bus_binding_serde_form_matches_spec() {
        // Spec says: bind = { bus = "audio/in/0" }
        let b = Binding::Bus { channel: ChannelName("audio/in/0".to_string()) };
        let json = serde_json::to_string(&b).unwrap();
        // Untagged-style: { "bus": "audio/in/0" }. We use serde's `tag = "type"`-free
        // adjacent style by relying on enum default (externally tagged).
        // Verify the actual serialized form and adjust this assertion to whatever
        // matches the spec example. If the form doesn't match, change the derive
        // strategy to match the spec.
        assert!(json.contains("audio/in/0"));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    Bus { channel: ChannelName },
    // Future: Const { value: LpsValue }, Modulator { source: NodePropSpec }, Bus { channel, transform }
}

/// Trait stub — compose-time validation that a Slot's binding is
/// type-compatible with its target bus channel. Real impl lands in M3+.
pub trait BindingResolver {
    /// The Kind that the channel currently carries (set by the first
    /// binding to it). `None` means the channel doesn't exist yet and
    /// will be declared by this binding.
    fn channel_kind(&self, channel: &ChannelName) -> Option<crate::kind::Kind>;
}
```

> **Spec-form question:** The spec example is
> `bind = { bus = "audio/in/0" }`. With externally-tagged serde
> on `Binding::Bus { channel }`, you get
> `{"Bus": {"channel": "audio/in/0"}}`. To match the spec form,
> use one of:
>
> - `#[serde(tag = "type", rename_all = "snake_case")]` →
>   `{"type":"bus","channel":"audio/in/0"}` (still doesn't match
>   the spec).
> - **`#[serde(untagged)]` plus a single-field-per-variant
>   wrapper** → `{"bus":"audio/in/0"}`. This is what the spec
>   wants.
>
> The cleanest approach for v0 is to define a private helper
> struct that serializes as `{"bus": "audio/in/0"}` and have
> `Binding` use a custom `Serialize` / `Deserialize` impl. **But
> M2 doesn't need on-disk fidelity — TOML parsing lands in M3.**
> So for M2: just pick whichever serde form compiles cleanly,
> verify the round-trip works, and add a `TODO(M3): align
> serialization shape with quantity.md §8 spec form
> "{bus = ...}".` comment. Don't waste the milestone on it.

### `lp-domain/lp-domain/src/presentation.rs` (extend phase 3's stub)

Phase 3 already added the `Presentation` enum with the 10
variants. This phase just adds a small test module:

```rust
//! Presentation enum (UI widget hint).
//! See docs/design/lightplayer/quantity.md §9.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_round_trips_serde() {
        for p in [
            Presentation::Knob, Presentation::Fader, Presentation::Toggle,
            Presentation::NumberInput, Presentation::Dropdown, Presentation::XyPad,
            Presentation::ColorPicker, Presentation::PaletteEditor,
            Presentation::GradientEditor, Presentation::TexturePreview,
        ] {
            let s = serde_json::to_string(&p).unwrap();
            let back: Presentation = serde_json::from_str(&s).unwrap();
            assert_eq!(p, back);
        }
    }

    #[test]
    fn presentation_serde_form_is_snake_case() {
        // Phase 3 should have used `#[serde(rename_all = "snake_case")]`. Verify.
        let s = serde_json::to_string(&Presentation::ColorPicker).unwrap();
        assert_eq!(s, "\"color_picker\"");
    }
}

// (existing enum from phase 3 stays in place)
```

If phase 3 *didn't* add `#[serde(rename_all = "snake_case")]` to
the `Presentation` enum, add it now (this is a micro-fix that
makes the on-disk form predictable; it counts as completing
phase 3's stub, not as scope creep).

### `lp-domain/lp-domain/src/shape.rs`

Replace the stub with the full implementation per the Q15
section above (the code in this phase file's "Q15 — Option A"
section). Then add tests:

```rust
//! Shape (Scalar / Array / Struct) and Slot. See docs/design/lightplayer/quantity.md §6.

use crate::binding::Binding;
use crate::constraint::Constraint;
use crate::kind::Kind;
use crate::presentation::Presentation;
use crate::types::Name;
use crate::value_spec::{LoadCtx, ValueSpec};
use crate::{LpsType, LpsValue};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar_amplitude_slot() -> Slot {
        Slot {
            shape: Shape::Scalar {
                kind: Kind::Amplitude,
                constraint: Constraint::Range { min: 0.0, max: 1.0, step: None },
                default: ValueSpec::Literal(LpsValue::F32(1.0)),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        }
    }

    #[test]
    fn scalar_default_value_is_literal() {
        let mut ctx = LoadCtx::default();
        match scalar_amplitude_slot().default_value(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 1.0),
            other => panic!("expected F32(1.0), got {other:?}"),
        }
    }

    #[test]
    fn array_with_no_default_derives_from_element() {
        let elem = scalar_amplitude_slot();
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 3,
                default: None,
            },
            label: None, description: None, bind: None, present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 3);
                for item in items.iter() {
                    match item {
                        LpsValue::F32(v) => assert_eq!(*v, 1.0),
                        other => panic!("expected F32, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn array_with_explicit_default_uses_override() {
        let elem = scalar_amplitude_slot();
        let preset: alloc::vec::Vec<LpsValue> = alloc::vec![
            LpsValue::F32(0.2),
            LpsValue::F32(0.7),
        ];
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 2,
                default: Some(ValueSpec::Literal(LpsValue::Array(preset.into_boxed_slice()))),
            },
            label: None, description: None, bind: None, present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 2);
                match (&items[0], &items[1]) {
                    (LpsValue::F32(a), LpsValue::F32(b)) => {
                        assert_eq!(*a, 0.2);
                        assert_eq!(*b, 0.7);
                    }
                    other => panic!("expected two F32s, got {other:?}"),
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn struct_with_no_default_derives_from_fields() {
        let speed = (
            Name::parse("speed").unwrap(),
            scalar_amplitude_slot(),
        );
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None, description: None, bind: None, present: None,
        };
        let mut ctx = LoadCtx::default();
        match struct_slot.default_value(&mut ctx) {
            LpsValue::Struct { fields, .. } => {
                assert_eq!(fields.len(), 1);
                let (name, val) = &fields[0];
                assert_eq!(name, "speed");
                match val {
                    LpsValue::F32(v) => assert_eq!(*v, 1.0),
                    other => panic!("expected F32, got {other:?}"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn slot_storage_projection_scalar() {
        assert_eq!(scalar_amplitude_slot().storage(), LpsType::Float);
    }

    #[test]
    fn slot_storage_projection_array() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 4,
                default: None,
            },
            label: None, description: None, bind: None, present: None,
        };
        match array_slot.storage() {
            LpsType::Array { element, len } => {
                assert_eq!(*element, LpsType::Float);
                assert_eq!(len, 4);
            }
            _ => panic!("expected Array storage"),
        }
    }

    #[test]
    fn slot_serde_round_trip_scalar() {
        let s = scalar_amplitude_slot();
        let json = serde_json::to_string(&s).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn slot_serde_omits_none_overrides_on_composed() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 2,
                default: None,
            },
            label: None, description: None, bind: None, present: None,
        };
        let json = serde_json::to_string(&array_slot).unwrap();
        // The composed-default field should not appear when None.
        assert!(!json.contains("\"default\":null"));
    }

    #[test]
    fn slot_serde_round_trips_recursive() {
        let speed = (
            Name::parse("speed").unwrap(),
            scalar_amplitude_slot(),
        );
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None, description: None, bind: None, present: None,
        };
        let json = serde_json::to_string(&struct_slot).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(struct_slot, back);
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,
    },
    Array {
        element: Box<Slot>,
        length: u32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        default: Option<ValueSpec>,
    },
    Struct {
        fields: Vec<(Name, Slot)>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        default: Option<ValueSpec>,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Slot {
    pub shape: Shape,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bind: Option<Binding>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub present: Option<Presentation>,
}

impl Slot {
    pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue {
        match &self.shape {
            Shape::Scalar { default, .. } => default.materialize(ctx),
            Shape::Array { element, length, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let mut elems = Vec::with_capacity(*length as usize);
                    for _ in 0..*length {
                        elems.push(element.default_value(ctx));
                    }
                    LpsValue::Array(elems.into_boxed_slice())
                }
            },
            Shape::Struct { fields, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let entries = fields
                        .iter()
                        .map(|(name, slot)| (name.0.clone(), slot.default_value(ctx)))
                        .collect();
                    LpsValue::Struct { name: None, fields: entries }
                }
            },
        }
    }

    pub fn storage(&self) -> LpsType {
        match &self.shape {
            Shape::Scalar { kind, .. } => kind.storage(),
            Shape::Array { element, length, .. } => LpsType::Array {
                element: Box::new(element.storage()),
                len: *length,
            },
            Shape::Struct { fields, .. } => LpsType::Struct {
                name: None,
                members: fields.iter().map(|(name, slot)| {
                    lps_shared::types::StructMember {
                        name: Some(name.0.clone()),
                        ty: slot.storage(),
                    }
                }).collect(),
            },
        }
    }
}
```

**Important compatibility facts (verified from
`lp-shader/lps-shared/src/lps_value_f32.rs` at planning time —
re-read the file before coding to catch any drift):**

1. **`LpsValueF32::Array` is `Array(Box<[LpsValueF32]>)`** — a
   boxed slice, not a `Vec`. Construct it via
   `LpsValueF32::Array(elems.into_boxed_slice())` where `elems:
   Vec<LpsValueF32>`. To iterate the inner array in tests,
   pattern-match `LpsValue::Array(items)` and use
   `items.iter()` / `items.len()`.

2. **`LpsValueF32::Struct` is** literally:

   ```rust
   Struct {
       name: Option<alloc::string::String>,
       fields: alloc::vec::Vec<(alloc::string::String, LpsValueF32)>,
   }
   ```

   So `LpsValue::Struct { name: None, fields: ... }` works as
   shown above.

3. **`StructMember` is `{ name: Option<String>, ty: LpsType }`**
   — the construction in `Slot::storage()` is correct.

4. **`LpsValueF32` does NOT derive `PartialEq`.** It has a
   custom `eq(&self, other: &Self) -> bool` method, plus
   `approx_eq` (see the source). This means **`assert_eq!` does
   not work on `LpsValue`** — use `assert!(a.eq(&b))` instead,
   or compare specific fields after pattern-matching. Fix every
   test in this phase that compares `LpsValue`s with
   `assert_eq!`. Example:

   ```rust
   // Wrong (won't compile — PartialEq missing):
   assert_eq!(v, LpsValue::F32(1.0));
   // Right:
   assert!(v.eq(&LpsValue::F32(1.0)));
   ```

   Equivalently, pattern-match the variant and assert the inner
   primitive:

   ```rust
   match v {
       LpsValue::F32(x) => assert_eq!(x, 1.0),
       other => panic!("expected F32(1.0), got {other:?}"),
   }
   ```

   Use whichever reads more clearly per test.

5. Update the same construction-site rules in `value_spec.rs` —
   `texture_handle_value` should also wrap its `Vec<(String,
   LpsValueF32)>` directly in `LpsValueF32::Struct { name:
   None, fields: ... }`. The example above is correct on that
   point (the helper builds a `vec![]`, which goes directly
   into `fields:`).

6. **`PartialEq` for `ValueSpec`, `Shape`, `Slot`.** Because
   `LpsValueF32` doesn't derive `PartialEq`, the
   `#[derive(PartialEq)]` on `ValueSpec` (which has
   `Literal(LpsValue)`) won't compile. Fix by **hand-writing
   `PartialEq` for `ValueSpec`** at the bottom of
   `value_spec.rs`:

   ```rust
   impl PartialEq for ValueSpec {
       fn eq(&self, other: &Self) -> bool {
           match (self, other) {
               (Self::Literal(a), Self::Literal(b)) => a.eq(b),
               (Self::Texture(a), Self::Texture(b)) => a == b,
               _ => false,
           }
       }
   }
   ```

   `TextureSpec` derives `PartialEq` normally (no `LpsValue`
   inside). With `ValueSpec: PartialEq` in place, `Shape` and
   `Slot` can derive `PartialEq` cleanly through the rest of
   their fields. Verify by trying `derive(PartialEq)` on
   `Shape` and `Slot` and confirming the build passes. Do
   **not** propagate hand-written impls beyond `ValueSpec`
   unless the compiler forces you.

7. **Round-trip tests on `Slot`** can still use `assert_eq!`
   because `Slot: PartialEq` (via the chain above). The
   underlying `LpsValueF32::eq` does the right thing for
   `Literal(LpsValue::F32(1.0))` so the comparison stays
   meaningful.

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `value_spec.rs`, `binding.rs` (extended), `presentation.rs`
  (with tests), `shape.rs` all contain real, tested
  implementations.
- `Slot` has no top-level `default` field. Composed `Shape`
  variants carry `Option<ValueSpec>`; scalar carries mandatory
  `ValueSpec`.
- `Slot::default_value` derives correctly from children when the
  composed `default` is `None`.
- `Slot::storage()` projects to `LpsType` correctly for all
  three Shape variants.
- All round-trip tests pass.
- All validation commands pass with no warnings.
- No commit.

Report back with: list of files changed, validation output, any
deviations (especially: the actual `LpsValueF32::Array` /
`LpsValueF32::Struct` variant shapes, and whether the binding
serde form matches the spec).
