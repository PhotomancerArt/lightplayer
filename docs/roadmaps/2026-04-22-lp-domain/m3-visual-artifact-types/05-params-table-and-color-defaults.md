# Phase 05 — `ParamsTable` newtype + literal-form `ValueSpec` (Color / AudioLevel)

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting. Read `quantity.md` §10 — the worked example —
> carefully; the literal-form rewrite of `ValueSpec` is the heaviest
> change in this phase and §10's examples are the contract.
>
> **Depends on:** Phase 03 (Slot custom serde) merged. `cargo test
> -p lp-domain` passing.
>
> **Parallel with:** Phase 04 (`Constraint` peer-key + ShaderRef +
> VisualInput). No file overlap.

## Scope of phase

Two related deliverables:

1. **`ParamsTable` newtype** that wraps a `Slot` whose `Shape` is
   always `Struct`. Its custom `Deserialize` reads a TOML table and
   synthesizes the implicit `Shape::Struct` from the table's keys
   (`quantity.md` §10 "Top-level `[params]`"). Custom `Serialize`
   round-trips it back. Custom `JsonSchema` describes a free-form
   object whose values are `Slot`s.
2. **`ValueSpec` literal-form rewrite**, so `default = 1.0` in TOML
   parses, not `default = { kind = "literal", value = { f32 = 1.0 } }`.
   This is necessary for the example corpus (Phase 09) to even be
   readable, and the `quantity.md` §10 worked example shows
   `default = 1.0`, `default = { space = "oklch", coords = [...] }`,
   `default = "black"`, etc.

The literal-form rewrite is Kind-aware: the `default = 1.0` form's
target `LpsValue` variant depends on the slot's `Kind` (`f32` for
Amplitude, `i32` for Count, etc.). Kind-awareness happens at *parse*
time inside `Slot::deserialize` — `ValueSpec::Literal(LpsValue)`
keeps its current shape; what changes is how the loader populates it.

References:
- [`quantity.md` §10 worked example](../../design/lightplayer/quantity.md#10-toml-grammar).
- [`color.md`](../../design/color.md) — Color value shape `{ space,
  coords }`.

**In scope:**

- `lp-domain/lp-domain/src/visual/params_table.rs`:
  - `pub struct ParamsTable(pub Slot);` (Slot's Shape is always
    `Struct`).
  - Custom `Deserialize<'de>` (~30 LOC): read TOML table, every
    key becomes a struct field name, every value re-parses as
    `Slot`.
  - Custom `Serialize` (~15 LOC): inverse; emit each field as a
    TOML sub-table.
  - Custom `JsonSchema` (~25 LOC): free-form object with
    additional-properties = `Slot`.
  - Tests inline at the bottom.
- `lp-domain/lp-domain/src/visual/mod.rs` — add `pub mod
  params_table;` and `pub use params_table::ParamsTable;`.
- `lp-domain/lp-domain/src/value_spec.rs` — extend the existing
  `ValueSpec` `Deserialize` to accept the §10 literal forms.
  Specifically:
  - **Scalar literal forms:** numbers (i32 / f32) → matching
    `LpsValue`; bool → `LpsValue::Bool`; string `"black"` (or other
    documented texture spec strings) → `ValueSpec::Texture`.
  - **Struct literal forms:** inline tables map field-by-field to
    `LpsValue::Struct` in the order declared by the slot's expected
    `LpsType::Struct` member list.
  - **Array literal forms:** TOML arrays → `LpsValue::Array`,
    element-by-element.
  - **Color / AudioLevel forms:** `{ space, coords }` and
    `{ low, mid, high }` are special cases of the struct literal
    form; the field order in serialized output matches
    `Kind::Color::storage()` / `Kind::AudioLevel::storage()`'s
    member order (M2 non-negotiable §4).
- `lp-domain/lp-domain/src/shape.rs::Slot::deserialize` — extend
  to call into a Kind-aware literal converter when populating the
  `default` field of `Shape::Scalar`. Same for the optional
  composed-default of `Shape::Array` / `Shape::Struct`.

**Out of scope:**

- The Visual structs themselves (Phases 06, 07).
- Any `lp-cli` schema-gen tooling.
- Project-wide const-bound bumps (`MAX_PALETTE_LEN` stays at 16).
- Texture spec strings beyond `"black"` (others land additively).
- `Choice` literal-form weirdness — `Constraint::Choice.choices`
  stays as `Vec<f32>` per Phase 04.

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md) § "Code organization in Rust source files":

- Tests at the **bottom** of each file.
- Inside `mod tests`: `#[test]` functions first, helpers below.
- Document `ParamsTable`'s implicit-`Shape::Struct` rule with a
  link to `quantity.md` §10. One paragraph; no narration.
- Document the literal-form converter in `value_spec.rs` with a
  Kind-by-Kind table of accepted on-disk shapes; this is the one
  place where verbosity is justified because the conversion logic
  *is* the contract for authors.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add Visual struct definitions (Phases 06, 07).
- Do **not** change the `LpsValue` API in `lps-shared`.
- Do **not** change `ValueSpec::Literal`'s payload type. It still
  carries `LpsValue`. The change is only in how the loader
  populates it from TOML.
- Do **not** change the existing `ValueSpec::Texture` / `TextureSpec`
  enum unless a new texture-spec string is being added (which is
  out of scope).
- Do **not** suppress warnings.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, total
  LOC of the literal-converter, and any deviations.

## Implementation

### `ParamsTable` newtype

```rust
//! [`ParamsTable`]: the implicit-[`Shape::Struct`] form of a Visual's
//! top-level `[params]` block. See `quantity.md` §10 "Top-level
//! `[params]` is implicit `Shape::Struct`."
//!
//! Wrapping the [`Slot`] in a newtype lets us give it a custom
//! [`serde::Deserialize`] that reads a flat TOML table and synthesizes
//! the struct shape, without bleeding the special case into
//! [`Slot::deserialize`].

use crate::shape::{Shape, Slot};
use crate::types::Name;
use alloc::string::String;
use alloc::vec::Vec;

/// A Visual's `[params]` block: a [`Slot`] whose [`Shape`] is always
/// [`Shape::Struct`], synthesized from the TOML table keys.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamsTable(pub Slot);

impl Default for ParamsTable {
    fn default() -> Self {
        ParamsTable(Slot {
            shape: Shape::Struct { fields: Vec::new(), default: None },
            label: None,
            description: None,
            bind: None,
            present: None,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ParamsTable {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let table: toml::Table = toml::Table::deserialize(de)?;
        let mut fields: Vec<(Name, Slot)> = Vec::with_capacity(table.len());
        for (k, v) in table {
            let name = Name::parse(&k).map_err(|e| {
                serde::de::Error::custom(alloc::format!("invalid param name `{k}`: {e}"))
            })?;
            let slot: Slot = v.try_into().map_err(serde::de::Error::custom)?;
            fields.push((name, slot));
        }
        Ok(ParamsTable(Slot {
            shape: Shape::Struct { fields, default: None },
            label: None,
            description: None,
            bind: None,
            present: None,
        }))
    }
}

impl serde::Serialize for ParamsTable {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let fields = match &self.0.shape {
            Shape::Struct { fields, .. } => fields,
            _ => return Err(serde::ser::Error::custom("ParamsTable inner shape must be Struct")),
        };
        let mut map = ser.serialize_map(Some(fields.len()))?;
        for (name, slot) in fields {
            map.serialize_entry(&name.0, slot)?;
        }
        map.end()
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ParamsTable {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ParamsTable".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // additionalProperties: <Slot schema>; no required keys; no min length.
        let slot_schema = <Slot as schemars::JsonSchema>::json_schema(generator);
        // Build an object schema: { type: "object", additionalProperties: <slot> }.
        // Mirror existing schemars-API style in the workspace; if no precedent,
        // build via serde_json::json! and convert to schemars::Schema.
        // Goal: schema-aware editors offer Slot completion under any property name.
        // ... concrete construction left to the sub-agent ...
        slot_schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::Kind;

    #[test]
    fn empty_params_round_trips() {
        let p = ParamsTable::default();
        let s = toml::to_string(&p).unwrap();
        assert_eq!(s.trim(), "");
        let back: ParamsTable = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn single_scalar_param_loads() {
        let toml_str = r#"
            [speed]
            kind    = "amplitude"
            default = 1.0
        "#;
        let p: ParamsTable = toml::from_str(toml_str).unwrap();
        match &p.0.shape {
            Shape::Struct { fields, .. } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0.0, "speed");
                assert!(matches!(fields[0].1.shape, Shape::Scalar { kind: Kind::Amplitude, .. }));
            }
            _ => panic!("expected Struct"),
        }
    }

    #[test]
    fn multi_scalar_params_preserve_order() {
        let toml_str = r#"
            [time]
            kind    = "instant"
            default = 0.0

            [speed]
            kind    = "amplitude"
            default = 1.0

            [saturation]
            kind    = "amplitude"
            default = 0.8
        "#;
        let p: ParamsTable = toml::from_str(toml_str).unwrap();
        match &p.0.shape {
            Shape::Struct { fields, .. } => {
                assert_eq!(fields.len(), 3);
                // BTreeMap orders by key; document this in rustdoc if it
                // surprises (toml::Table is a BTreeMap<String, Value>).
                let names: Vec<_> = fields.iter().map(|(n, _)| n.0.as_str()).collect();
                assert!(names.contains(&"time"));
                assert!(names.contains(&"speed"));
                assert!(names.contains(&"saturation"));
            }
            _ => panic!("expected Struct"),
        }
    }
}
```

> **Note on field ordering.** `toml::Table` is a `BTreeMap<String,
> Value>`, so insertion order is **not** preserved on the way in.
> Two options for M3:
>
> 1. Accept BTreeMap ordering (alphabetical) — simpler; some TOML
>    files will reorder on round-trip.
> 2. Use `toml::Table` with the `preserve_order` feature (backed
>    by `indexmap`) — round-trip preserves authoring order.
>
> **Recommended:** option 2. Authors will be confused if their
> `[params.time]` jumps below `[params.saturation]` after a
> save. Add `toml = { version = "...", features = ["preserve_order"] }`
> to `lp-domain/Cargo.toml` (or wire the feature on the workspace
> dep). Document the choice in the file's module docs.
>
> Same applies to the rest of `Slot`'s TOML round-trip path
> (Phase 03 may have already addressed this; verify before
> implementing).

### `ValueSpec` literal-form rewrite

The on-disk grammar (per `quantity.md` §10):

| Slot kind                       | TOML default form                                       | Materializes to                                                          |
| ------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------ |
| `Kind::Amplitude` / `Ratio` / `Phase` / `Instant` / `Duration` / `Frequency` / `Angle` | `default = 1.0` (or any TOML number)                | `LpsValue::F32`                                                          |
| `Kind::Count`                   | `default = 4` (TOML integer)                            | `LpsValue::I32`                                                          |
| `Kind::Choice`                  | `default = 1` (index)                                   | `LpsValue::I32`                                                          |
| `Kind::Bool`                    | `default = true` / `false`                              | `LpsValue::Bool`                                                         |
| `Kind::Color`                   | `default = { space = "oklch", coords = [0.7, 0.15, 90] }` | `LpsValue::Struct { name: Some("Color"), fields: [("space", I32(<id>)), ("coords", Vec3([…]))] }` |
| `Kind::ColorPalette`            | `default = { space = "oklch", entries = [[…], […]] }`   | `LpsValue::Struct` matching `Kind::ColorPalette::storage()` member order |
| `Kind::Gradient`                | `default = { space, method, stops = [{at, c}, …] }`     | matching struct                                                          |
| `Kind::Position2d` / `Position3d` | `default = [x, y]` / `[x, y, z]`                      | `LpsValue::Vec2` / `Vec3`                                                |
| `Kind::AudioLevel`              | `default = { low = 0.0, mid = 0.0, high = 0.0 }`        | `LpsValue::Struct` matching `Kind::AudioLevel::storage()` member order   |
| `Kind::Texture`                 | `default = "black"` (string)                            | `ValueSpec::Texture(TextureSpec::Black)`                                 |
| `Shape::Array<T>`               | `default = [<T-form>, <T-form>, …]`                     | `LpsValue::Array` recursively                                            |
| `Shape::Struct { fields }`      | `default = { <field> = <T-form>, … }`                   | `LpsValue::Struct` recursively, in the field declaration order           |

The converter sits in `value_spec.rs`:

```rust
impl ValueSpec {
    /// Parse the on-disk `default` TOML literal for a slot of the given
    /// [`Kind`]. The Kind drives variant selection; see the table in this
    /// module's docs for the full mapping.
    ///
    /// Composed slots delegate: see [`Self::from_toml_for_shape`].
    pub(crate) fn from_toml_for_kind(
        value: &toml::Value,
        kind: Kind,
    ) -> Result<Self, FromTomlError> {
        // … big match on kind …
    }

    /// Parse the on-disk `default` TOML literal for a slot of the given
    /// [`Shape`]. Recurses into [`Shape::Array`]'s element / [`Shape::Struct`]'s
    /// fields. For [`Shape::Scalar`], delegates to [`Self::from_toml_for_kind`].
    pub(crate) fn from_toml_for_shape(
        value: &toml::Value,
        shape: &Shape,
    ) -> Result<Self, FromTomlError> {
        // … recursion …
    }
}
```

`FromTomlError` is a small local error type; convert to
`serde::de::Error::custom` at the `Slot::deserialize` boundary.

### `shape.rs::Slot::deserialize` integration

The Phase 03 deserializer pre-defaults `shape` and pulls the metadata
fields. For Scalar slots, the existing flow re-feeds the rest into
derived `Shape::deserialize`, which would call derived
`ValueSpec::deserialize`. That derived path doesn't know the Kind, so
it can't accept `default = 1.0`.

The fix: *don't* re-feed through derived `Shape::deserialize`.
Instead, hand-decompose:

```rust
// Inside Slot::deserialize, after pulling metadata fields and
// defaulting `shape`:
let shape: Shape = match shape_str {
    "scalar" => {
        let kind: Kind = take_typed(table, "kind")?
            .ok_or_else(|| de::Error::missing_field("kind"))?;
        let constraint: Constraint = take_typed(table, "constraint")?
            .or_else(|| try_take_inline_constraint(table))
            .unwrap_or(kind.default_constraint());
        let default_value = table.remove("default").ok_or_else(|| {
            de::Error::missing_field("default")
        })?;
        let default = ValueSpec::from_toml_for_kind(&default_value, kind)
            .map_err(de::Error::custom)?;
        Shape::Scalar { kind, constraint, default }
    }
    "array" => {
        let length: u32 = take_typed(table, "length")?
            .ok_or_else(|| de::Error::missing_field("length"))?;
        let element_value = table.remove("element").ok_or_else(|| {
            de::Error::missing_field("element")
        })?;
        let element: Slot = element_value.try_into().map_err(de::Error::custom)?;
        let default = match table.remove("default") {
            Some(v) => Some(ValueSpec::from_toml_for_shape(&v, &Shape::Array {
                element: Box::new(element.clone()),
                length,
                default: None,
            }).map_err(de::Error::custom)?),
            None => None,
        };
        Shape::Array { element: Box::new(element), length, default }
    }
    "struct" => {
        // Similar: pluck `props` table, convert each entry, then
        // optionally pluck `default` and convert via from_toml_for_shape.
    }
    _ => unreachable!(), // Pre-checked above.
};
```

The `try_take_inline_constraint` helper plucks `range` / `choices` /
`labels` / `step` peer keys and constructs the appropriate
`Constraint` variant (delegates to derived `Constraint::deserialize`
on a small synthesized `toml::Table`). This is where Phase 04's
peer-key inference enum gets exercised through Slot's parser.

> **Note on Slot↔ValueSpec coupling.** This entanglement is the
> reason Phase 03 punted on full TOML round-trips for `default`s.
> Phase 05 makes the coupling explicit: Slot's parser owns Kind
> awareness; ValueSpec's literal-form parser is a free function
> that takes the Kind as a parameter.
>
> The existing derived `Deserialize for ValueSpec` (via
> `ValueSpecWire`) **stays** for the JSON-shaped case (used by
> integration tests not loading from TOML, by serializer
> round-trips at the JSON level, and by schemars). The new
> Kind-aware parser is the **TOML** entry point.

### `Serialize` round-trip for literals

Round-tripping requires the inverse. `Slot::serialize` (Phase 03)
emits the inner Shape via `toml::Value::try_from(&self.shape)`,
which uses derived `Shape::Serialize`. That path emits
`ValueSpec::Literal(LpsValue::F32(1.0))` as the `ValueSpecWire`
internally-tagged form (`{kind: "literal", value: {f32: 1.0}}`) —
**not** the §10 grammar.

Phase 05's `Slot::serialize` needs to be updated to walk the Shape
manually and emit literal forms via a parallel `ValueSpec::to_toml`
that re-derives the on-disk shape from the cached `LpsValue` +
context. Full table:

| `Shape::Scalar { kind, default: ValueSpec::Literal(v) }` | Emit `default = <toml form of v>` (number / string / inline table per the table above). |
| `Shape::Scalar { kind, default: ValueSpec::Texture(TextureSpec::Black) }` | Emit `default = "black"`. |
| `Shape::Array { default: Some(ValueSpec::Literal(v)) }` | Emit `default = [<toml form of v[0]>, …]`. |
| `Shape::Struct { default: Some(ValueSpec::Literal(v)) }` | Emit `default = { <field> = <toml form>, … }`, in the field declaration order. |
| Composed `default: None` | Omit `default` entirely. |

Approximately ~80 LOC for `to_toml` + ~40 LOC for the inverse
deserializer. Total Phase 05 is in the ~250-350 LOC range; the
heaviest step in the plan.

### Tests

- All existing `value_spec.rs` tests must still pass (the JSON
  wire form for `ValueSpecWire` is unchanged).
- New tests in `value_spec.rs::tests`:
  - `f32_literal_round_trips_in_toml_for_amplitude`
  - `i32_literal_round_trips_in_toml_for_count`
  - `bool_literal_round_trips_in_toml_for_bool`
  - `color_literal_round_trips_in_toml`
  - `audio_level_literal_round_trips_in_toml`
  - `position2d_array_literal_round_trips`
  - `texture_black_string_round_trips`
  - `array_of_amplitude_literals_round_trips`
  - `struct_of_two_amplitudes_round_trips`
  - For each: assert the on-disk shape matches §10 (e.g.
    `default = 1.0`, not `default = { kind = "literal", value = …}`).
- New tests in `shape.rs::tests`: end-to-end `Slot` round-trip
  through TOML for each Kind that has a literal-form mapping.

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `ParamsTable` newtype exists with full custom serde +
  `JsonSchema`.
- `ValueSpec::from_toml_for_kind` and `from_toml_for_shape` parse
  every Kind's §10 literal form correctly (including Color,
  AudioLevel, Texture string, Array, nested Struct).
- `ValueSpec::to_toml_for_kind` emits the inverse forms
  bytewise-faithfully for the round-trip tests.
- `Slot::deserialize` and `Slot::serialize` (from Phase 03) are
  updated to use the literal-form converters instead of derived
  `Shape::Deserialize/Serialize` for the `default` field paths.
- Field order in serialized Color / ColorPalette / Gradient /
  AudioLevel literals matches the order from `Kind::*::storage()`
  member lists (M2 non-negotiable §4).
- All pre-existing `value_spec.rs`, `shape.rs`, `binding.rs`,
  `kind.rs`, `constraint.rs` tests still pass.
- New tests: ~10 in `value_spec.rs`, ~6 in `shape.rs`, ~3 in
  `params_table.rs`.
- `toml` is configured with `preserve_order` feature for round-trip
  fidelity (or the round-trip tests document and accept BTreeMap
  re-ordering).
- No commit.

Report back with: list of changed files, validation output, total
LOC of the literal-converter (`from_toml_for_*` + `to_toml_for_*`),
whether `preserve_order` was enabled, the Slot↔ValueSpec coupling
code structure (free fn vs method), and any deviations.
