# External Slot Enums Notes

## Scope of Work

Add first-class support for externally tagged enum encoding to the slot system.

The target authored shape is:

```toml
[glsl]
file = "compute.glsl"
```

or, for a structured variant:

```toml
[thing.a]
x = 10
y = 10
```

This plan covers only the slot-system support needed for that syntax:

- Slot shape metadata for enum encoding.
- Dynamic slot TOML read/write support for externally tagged enums.
- Derive macro attributes for opting an enum into external encoding.
- Documentation in Rust docs and project docs.
- Tests that prove externally tagged enum support for scalar, record, and unit payloads.

Out of scope for this plan:

- Migrating shader definitions from `glsl_path` to `glsl`.
- Artifact-manager source dependency modeling.
- Field-presence / `#[slot(key)]` enum discrimination.
- Node hot reload or shader recompilation fixes.

## Current State

Slot data already has a real enum model:

- `EnumSlot<T>` stores the active variant revision and payload.
- `SlottedEnum` / `SlottedEnumMut` expose variant name and payload data.
- `SlotShape::Enum` stores enum variants as `SlotVariantShape` values.
- `#[derive(Slotted)]` already supports Rust enum slot shapes and payload access.

Authored dynamic slot codecs currently support only internally tagged enum objects:

```toml
kind = "Variant"
field = "payload"
```

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape.rs`
  - Defines `SlotShape::Enum { meta, variants }`.
- `lp-core/lpc-model/src/slot/slot_meta.rs`
  - Human-facing metadata only; not suitable for codec semantics.
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
  - `read_enum` always calls `object.expect_discriminator("kind", ...)`.
  - Enum payload reader currently supports record and unit payloads for tagged enum payloads.
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
  - TOML enum writer always emits `kind = "<variant>"`.
  - TOML enum payload writer supports record and unit payloads.
- `lp-core/lpc-slot-macros/src/slotted_enum.rs`
  - Derives enum shapes and variant access.
  - Does not currently emit enum encoding metadata.
- `lp-core/lpc-slot-macros/src/attr.rs`
  - Parses container, field, and variant slot attributes.
  - Has no enum encoding or rename-all support today.

Serde prior art:

- Serde's default enum representation is externally tagged.
- Serde supports externally tagged unit, newtype, tuple, and struct variants.
- Serde uses `rename` and `rename_all` to control external tag names.

Important existing behavior to preserve:

- Existing slot enum TOML syntax must remain unchanged by default.
- Existing `NodeDef` TOML must continue to use `kind = "Shader"`, etc.
- Dynamic slot JSON behavior should not change unless deliberately extended in this plan.

## User Notes

- We have run into enum discrimination by field presence in a few places, but the first implementation should be externally tagged enums.
- Externally tagged enum syntax is preferred now because it is clean, Serde-precedented, and supports structured payloads:

  ```toml
  a = { x = 10, y = 10 }
  ```

- Field-presence enum discrimination remains interesting because it preserves namespace extensibility, but it is out of scope for this initial slot-system plan.
- Documentation matters both in Rust docs and project docs.

## Open Questions

### Q1: Should external enum encoding be opt-in per enum?

Suggested answer: yes.

Context: Existing authored TOML uses internally tagged `kind = "..."` in many places, especially `NodeDef`. Changing the default would be a breaking file-format change. The derive macro should support an enum-level opt-in such as:

```rust
#[slot(enum_encoding = "external")]
```

Status: resolved by user direction and compatibility constraints.

### Q2: Should external encoding support value, record, and unit payloads?

Suggested answer: yes.

Context: The motivating shader-source shape needs scalar/newtype payloads (`file = "compute.glsl"` and `inline = "..."`). The user also explicitly called out structured payloads (`a = { x = 10, y = 10 }`). Unit payloads are a natural Serde-compatible edge case and keep the model complete.

Status: resolved.

### Q3: Should this plan include field-presence / `#[slot(key)]` support?

Suggested answer: no.

Context: Field-presence discrimination requires extra shape metadata per variant, derive validation that each variant has one unique key field, and reader ambiguity handling. It is worth designing, but external tagging gives immediate value and cleaner implementation boundaries.

Status: resolved for this plan: document as future work only.

### Q4: Should JSON dynamic slot codec support external enum encoding at the same time?

Suggested answer: yes if the change is shared at the dynamic reader/writer level; otherwise keep JSON unchanged only if TOML-specific support can be clearly isolated.

Context: `TomlSyntaxSource` and `JsonSyntaxSource` both feed the same dynamic slot reader. External enum read support can naturally be syntax-source-agnostic. JSON writer support lives in the dynamic writer next to TOML writer support and should be kept consistent unless there is a concrete reason not to.

Status: suggested yes.

### Q5: How should variant names map to authored property names?

Suggested answer: support both existing per-variant `#[slot(name = "...")]` overrides and an enum-level `#[slot(rename_all = "snake_case")]` policy.

Context: The user specifically noted multi-word names (`OptionA -> option_a`). Existing `#[derive(Slotted)]` accepts `#[slot(name = "...")]` on variants today, but external tags are an authored API surface and should have Serde-like naming ergonomics.

Status: suggested yes.
