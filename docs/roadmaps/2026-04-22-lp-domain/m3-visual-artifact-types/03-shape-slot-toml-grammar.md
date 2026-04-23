# Phase 03 — `Slot` TOML grammar: custom `Deserialize` + `Serialize` + `JsonSchema`

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phases 01 (binding shape) and 02 (`Kind::AudioLevel`)
> must be merged. `cargo test -p lp-domain` must pass before this
> phase runs.
>
> **Parallel with:** none. Step 03 changes `shape.rs` in nontrivial
> ways; downstream steps (04, 05) depend on its output.

## Scope of phase

Replace the derived `Deserialize` / `Serialize` / `JsonSchema` on
`Slot` with custom impls that match `quantity.md` §10's TOML grammar:

- `shape` field defaults to `"scalar"` if omitted.
- Reserved keywords (`element` only valid inside `Shape::Array`,
  `props` only valid inside `Shape::Struct`, `params` is a top-level
  reserved word handled by `ParamsTable` in step 05).
- Slot fields (`label`, `description`, `bind`, `present`) coexist
  with shape-specific fields at the same TOML level.

The custom `JsonSchema` impl describes the same grammar (so
schema-aware TOML editors get accurate completion and validation).

Reference: [`docs/design/lightplayer/quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar).

**In scope:**

- `lp-domain/lp-domain/src/shape.rs`:
  - Remove `#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]`
    from `Slot` (keep them on `Shape`).
  - Hand-write `impl<'de> Deserialize<'de> for Slot`.
  - Hand-write `impl Serialize for Slot`.
  - Hand-write `impl JsonSchema for Slot` (gated on `feature =
    "schema-gen"`).
  - Reserved-keyword check (`element`, `props`) lives here.
  - Update existing `shape.rs::tests` round-trip tests to use TOML
    (via `toml::to_string` / `toml::from_str`) in addition to the
    JSON ones — both must round-trip.
  - Add new tests for the §10 grammar: omitted `shape` defaults to
    scalar, reserved-keyword violations are errors, the
    `JsonSchema` matches the `Deserialize` for representative
    inputs.
- `lp-domain/lp-domain/Cargo.toml`:
  - Add `toml = { workspace = true }` (or pin a version if not
    in workspace.dependencies); add it to the `std` feature's
    pulled-in deps if needed. Read the existing `[dependencies]`
    section first; mirror the pattern used by `serde`.
  - Add `serde_json` to `[dev-dependencies]` if not already there
    (existing tests use it).

**Out of scope:**

- `Constraint` peer-key inference (Phase 04).
- `ShaderRef`, `VisualInput` (Phase 04).
- `ParamsTable` and the implicit-top-level-struct rule (Phase 05).
- Any Visual struct (Phases 06, 07).
- `Color` value defaults (Phase 05).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md) § "Code organization in Rust source files":

- Tests at the **bottom** of `shape.rs`, never at the top.
- Inside `mod tests`: `#[test]` functions first, helpers
  (`scalar_amplitude_slot`, etc.) below.
- Custom serde impls live below the type definition; helpers below
  the impls; tests at the bottom.
- Document the implicit-`shape` rule in the `Slot` rustdoc with a
  link to `quantity.md` §10.
- Custom impls should NOT be commented-line-by-line; one paragraph
  of docs at the impl level explaining the §10 mapping is enough.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add `ParamsTable` or implicit-top-level-struct logic.
  That belongs in Phase 05.
- Do **not** rewrite `Constraint` to use peer-key inference. Phase
  04 owns that.
- Do **not** suppress warnings.
- Do **not** disable existing tests; the JSON round-trips for
  `Slot` from M2 must keep passing.
- Use `toml::Value` as the intermediate IR (per `00-notes.md`
  Q-D6c).
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, the LOC
  count of the new custom impls, any deviations.

## Implementation

### TOML grammar reminder (from `quantity.md` §10)

Scalar slot (`shape` omitted, defaults to `"scalar"`):

```toml
[params.speed]
kind    = "amplitude"
default = 1.0
```

Equivalent explicit form:

```toml
[params.speed]
shape   = "scalar"
kind    = "amplitude"
default = 1.0
```

Array slot:

```toml
[params.controls]
shape  = "array"
length = 4

[params.controls.element]
kind    = "amplitude"
default = 0.0
```

Struct slot:

```toml
[params.gain]
shape = "struct"

[params.gain.props.left]
kind    = "amplitude"
default = 1.0

[params.gain.props.right]
kind    = "amplitude"
default = 1.0
```

Slot metadata fields are peers of the shape-specific keys:

```toml
[params.speed]
kind        = "amplitude"
default     = 1.0
label       = "Speed"
description = "How fast the rainbow scrolls."
bind        = { bus = "audio/in/0/level" }
present     = "fader"
```

### Approach

1. Hand-write `Slot::deserialize`:
   - Deserialize input into `toml::Value`.
   - Demand `Value::Table`; otherwise error.
   - Pluck out `label`, `description`, `bind`, `present` (all
     optional; `Option::take`-style removal so they don't
     interfere with the shape decode).
   - Pluck out `shape`. If absent, insert `"scalar"`.
   - Reserved-keyword check: if `shape == "scalar"`, error if
     either `element` or `props` is present at this level.
     Similarly for `array` (no `props`) and `struct` (no
     `element`).
   - Re-feed the remaining table through derived
     `Shape::deserialize`.
   - Construct the `Slot` and return.

2. Hand-write `Slot::serialize`:
   - Construct an inline `toml::Value::Table`.
   - Serialize the inner `Shape` first (it produces a Table with
     `shape` + shape-specific keys).
   - Strip `shape = "scalar"` from the output if the shape is
     `Scalar` (implicit-default round-trip).
   - Add `label`, `description`, `bind`, `present` only when not
     `None`.
   - Serialize the resulting Table through the user's chosen
     serializer.

3. Hand-write `JsonSchema for Slot`:
   - Compose from `<Shape as JsonSchema>::json_schema()`.
   - Add the four optional metadata fields.
   - Mark `shape` optional with default `"scalar"`.
   - Result: a `oneOf`-discriminated schema with metadata fields
     overlaid onto each shape-variant's properties.

### Skeleton

```rust
use core::fmt;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;

impl<'de> serde::Deserialize<'de> for Slot {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = toml::Value::deserialize(de)?;
        let table = value
            .as_table_mut()
            .ok_or_else(|| de::Error::custom("Slot must be a TOML table"))?;

        // Remove Slot metadata fields first.
        let label       = take_string(table, "label")?;
        let description = take_string(table, "description")?;
        let bind        = take_typed::<Binding>(table, "bind")?;
        let present     = take_typed::<Presentation>(table, "present")?;

        // Default `shape = "scalar"` if absent.
        if !table.contains_key("shape") {
            table.insert("shape".into(), toml::Value::String("scalar".into()));
        }

        // Reserved-keyword check.
        let shape_str = table.get("shape").and_then(|v| v.as_str()).unwrap_or("");
        match shape_str {
            "scalar" => {
                if table.contains_key("element") {
                    return Err(de::Error::custom("`element` only valid inside `shape = \"array\"`"));
                }
                if table.contains_key("props") {
                    return Err(de::Error::custom("`props` only valid inside `shape = \"struct\"`"));
                }
            }
            "array" => {
                if table.contains_key("props") {
                    return Err(de::Error::custom("`props` only valid inside `shape = \"struct\"`"));
                }
            }
            "struct" => {
                if table.contains_key("element") {
                    return Err(de::Error::custom("`element` only valid inside `shape = \"array\"`"));
                }
            }
            other => return Err(de::Error::custom(format!("unknown shape `{other}`"))),
        }

        let shape: Shape = value.try_into().map_err(de::Error::custom)?;

        Ok(Slot { shape, label, description, bind, present })
    }
}

impl serde::Serialize for Slot {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Round-trip via toml::Value so we can normalize the implicit-`shape`
        // and metadata layout uniformly across serializers.
        let mut value = toml::Value::try_from(&self.shape).map_err(serde::ser::Error::custom)?;
        let table = value.as_table_mut().ok_or_else(|| {
            serde::ser::Error::custom("Shape must serialize as a TOML table")
        })?;

        // Drop `shape = "scalar"` for implicit round-trip.
        if matches!(self.shape, Shape::Scalar { .. }) {
            table.remove("shape");
        }

        if let Some(s) = &self.label       { table.insert("label".into(),       toml::Value::String(s.clone())); }
        if let Some(s) = &self.description { table.insert("description".into(), toml::Value::String(s.clone())); }
        if let Some(b) = &self.bind        { table.insert("bind".into(),        toml::Value::try_from(b).map_err(serde::ser::Error::custom)?); }
        if let Some(p) = &self.present     { table.insert("present".into(),     toml::Value::try_from(p).map_err(serde::ser::Error::custom)?); }

        value.serialize(ser)
    }
}

fn take_string(t: &mut toml::Table, key: &str) -> Result<Option<String>, /* error */ > {
    match t.remove(key) {
        Some(toml::Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(/* … */),
        None    => Ok(None),
    }
}
fn take_typed<T: serde::de::DeserializeOwned>(t: &mut toml::Table, key: &str)
    -> Result<Option<T>, /* error */ >
{
    match t.remove(key) {
        Some(v) => v.try_into().map(Some).map_err(/* … */),
        None    => Ok(None),
    }
}
```

(Pseudocode — flesh out the error types using the local idioms in
`shape.rs`. The sub-agent should pick names and error-conversion
patterns that match the current crate style.)

### Custom `JsonSchema for Slot`

```rust
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for Slot {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Slot".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Start from Shape's schema (a oneOf over Scalar/Array/Struct).
        let mut shape_schema = <Shape as schemars::JsonSchema>::json_schema(generator);
        // Mark `shape` optional with default "scalar" — the custom Deserialize
        // accepts omitted `shape` for Scalar slots, so the schema must too.
        // Then layer the four metadata fields onto every variant via allOf.
        // … (concrete schema construction; exact API depends on schemars
        //    version; mirror the style used by other custom JsonSchema impls
        //    in the workspace if any exist; otherwise keep it small + obvious.)
        shape_schema
    }
}
```

The sub-agent should pick the simplest schemars API surface that
expresses: optional `shape` (default `"scalar"`); plus four optional
metadata properties (`label`, `description`, `bind`, `present`)
overlaid onto every variant. Using `Schema::new_object_with(...)` or
direct `serde_json::json!` construction is fine — the goal is
**accuracy**, not minimum LOC.

If schemars' API is awkward, the fallback is documented in
`00-design.md`: emit a tighter schema that requires explicit
`shape`, and accept that authoring `shape = "scalar"` is required
for IDE completion. **Avoid** that fallback unless schemars' impl
becomes a real time sink — log the issue and move on, but flag it
in the report.

### Tests (at the bottom of `shape.rs`)

Keep the existing JSON round-trip tests; add TOML round-trip tests
for the same fixtures. New tests:

```rust
#[test]
fn scalar_slot_roundtrips_with_implicit_shape_in_toml() {
    let slot = scalar_amplitude_slot();
    let s = toml::to_string(&slot).unwrap();
    assert!(!s.contains("shape"), "implicit shape must be elided: got {s}");
    let back: Slot = toml::from_str(&s).unwrap();
    assert_eq!(slot, back);
}

#[test]
fn scalar_slot_loads_when_shape_is_omitted() {
    let toml = r#"
        kind    = "amplitude"
        default = 1.0
    "#;
    let s: Slot = toml::from_str(toml).unwrap();
    match s.shape {
        Shape::Scalar { .. } => {}
        _ => panic!("expected Scalar"),
    }
}

#[test]
fn scalar_slot_loads_when_shape_is_explicit() {
    let toml = r#"
        shape   = "scalar"
        kind    = "amplitude"
        default = 1.0
    "#;
    let _: Slot = toml::from_str(toml).unwrap();
}

#[test]
fn slot_metadata_fields_coexist_with_shape() {
    let toml = r#"
        kind        = "amplitude"
        default     = 1.0
        label       = "Speed"
        description = "How fast"
        bind        = { bus = "audio/in/0/level" }
        present     = "fader"
    "#;
    let s: Slot = toml::from_str(toml).unwrap();
    assert_eq!(s.label.as_deref(),       Some("Speed"));
    assert_eq!(s.description.as_deref(), Some("How fast"));
    assert!(s.bind.is_some());
    assert_eq!(s.present, Some(Presentation::Fader));
}

#[test]
fn array_with_props_at_root_is_rejected() {
    let toml = r#"
        shape  = "array"
        length = 2
        [props.x]
        kind = "amplitude"
        default = 0.0
    "#;
    let res: Result<Slot, _> = toml::from_str(toml);
    assert!(res.is_err());
}

#[test]
fn struct_with_element_at_root_is_rejected() {
    let toml = r#"
        shape = "struct"
        [element]
        kind = "amplitude"
        default = 0.0
    "#;
    let res: Result<Slot, _> = toml::from_str(toml);
    assert!(res.is_err());
}

#[test]
fn unknown_shape_is_rejected() {
    let toml = r#"
        shape = "tensor"
        kind  = "amplitude"
    "#;
    let res: Result<Slot, _> = toml::from_str(toml);
    assert!(res.is_err());
}

#[test]
fn array_slot_roundtrips_in_toml() {
    let slot = Slot {
        shape: Shape::Array {
            element: Box::new(scalar_amplitude_slot()),
            length: 3,
            default: None,
        },
        label: None,
        description: None,
        bind: None,
        present: None,
    };
    let s = toml::to_string(&slot).unwrap();
    let back: Slot = toml::from_str(&s).unwrap();
    assert_eq!(slot, back);
}

#[test]
fn struct_slot_roundtrips_in_toml() {
    let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
    let slot = Slot {
        shape: Shape::Struct { fields: alloc::vec![speed], default: None },
        label: None,
        description: None,
        bind: None,
        present: None,
    };
    let s = toml::to_string(&slot).unwrap();
    let back: Slot = toml::from_str(&s).unwrap();
    assert_eq!(slot, back);
}
```

The pre-existing JSON round-trip tests stay; they exercise the same
`Deserialize` / `Serialize` impls through a different format and
catch any format-specific bugs in the `toml::Value` IR.

For `JsonSchema` accuracy:

```rust
#[cfg(feature = "schema-gen")]
#[test]
fn slot_schema_is_non_degenerate() {
    let s = schemars::schema_for!(Slot);
    let json = serde_json::to_string(&s).unwrap();
    // Cheap smoke: the schema is non-empty and references the shape variants.
    assert!(json.contains("Scalar") || json.contains("scalar"));
}
```

Real loader-vs-schema drift detection lands in step 10's
integration tests.

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `Slot` no longer carries derived `Serialize` / `Deserialize` /
  `JsonSchema`; custom impls cover all three.
- Implicit-`shape` rule works on read (omitted ⇒ Scalar) and write
  (Scalar ⇒ omitted).
- Reserved-keyword check rejects misplaced `element` / `props` /
  unknown `shape` values.
- All existing `shape.rs` tests pass.
- New tests cover: TOML round-trip for all three Shape variants;
  implicit-shape on both read and write; reserved-keyword
  rejection; metadata-field coexistence; schema smoke test.
- `lp-domain/Cargo.toml` has the `toml` dependency wired correctly
  (workspace dep if available; otherwise pinned).
- No commit.

Report back with: list of changed files, validation output, LOC
count of the custom `Deserialize` + `Serialize` + `JsonSchema`
impls, whether the schemars custom impl was straightforward or
needed the documented fallback, and any deviations.
