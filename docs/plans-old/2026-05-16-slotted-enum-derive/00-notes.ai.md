# Slotted Enum Derive Notes

## Scope

Add first-class `#[derive(Slotted)]` support for Rust enums so structured enum
slot machinery is generated from the slot model instead of hand-written beside
it.

The first complete target set is:

- unit variants
- one-field tuple variants, treated as transparent wrapper payloads
- named-field variants, treated as record payloads

Multiple-field tuple variants are explicitly out of scope. A one-field tuple
variant is the useful wrapper form we need for `NodeDef`; if a future enum needs
multiple tuple fields, it should probably become a named-field variant so the
slot fields have names.

## Current State

`#[derive(Slotted)]` currently supports:

- named-field structs as slot records
- one-field tuple structs as slot wrappers

The derive implementation lives mostly in:

- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`

The file name `record.rs` is now too narrow because it handles both records and
tuple wrappers, and the next step adds enums.

`NodeDef` currently proves the desired runtime shape but does so manually:

- `NodeArtifact(pub EnumSlot<NodeDef>)` derives `Slotted` as a wrapper.
- `NodeDef` manually implements `Default`, `SlotEnumShape`, `SlottedEnum`, and
  `SlottedEnumMut`.
- `NodeDef::from_toml_str_with_registry` now loads through
  `registry.read_slot_toml(NodeArtifact::SHAPE_ID, ...)`.

`MappingConfig` and `PathSpec` already use `EnumSlot<T>` in real model code, but
they are also manually implemented:

- `impl SlotEnumShape`
- `impl SlottedEnum`
- `impl SlottedEnumMut`
- `impl SlotRecordAccess`
- `impl SlotRecordMutAccess`
- manual shape builder functions
- manual `default_variant`

The mockup has parallel manual enum implementations in:

- `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-mockup/src/source/node_def.rs`

## User Guidance

- We need full support now, not just a proof.
- The three enum variant forms we need are unit, single tuple, and named-field.
- Multiple tuple fields are not needed.
- Single tuple is effectively a wrapper.
- The motivation is to remove hand-coded slot machinery, not add more parallel
  boilerplate.

## Suggested Answers To Design Questions

### Should `NodeDef` derive `Slotted`?

Yes. `NodeDef` is a slotted enum payload. `EnumSlot<NodeDef>` owns the active
variant revision, and `NodeArtifact` owns the artifact/root boundary.

### Should raw slotted enums implement `SlotAccess`?

No for now. A raw Rust enum does not own a revision boundary. It should
implement `SlotEnumShape`, `SlottedEnum`, and `SlottedEnumMut`. `EnumSlot<T>`
and wrappers such as `NodeArtifact` provide runtime slot object boundaries.

### How should variant names be chosen?

Default to the Rust variant name exactly. This means authored discriminators should become `PathPoints`, `RingArray`, `Project`, `Texture`, etc. Allow `#[slot(name = "...")]` only as an escape hatch for a real compatibility or compactness need, not as the normal style.

### How should the default variant be chosen?

Use Rust-style `#[default]` on the enum variant. `Slotted` should declare `default` as a helper attribute and generate the `Default` impl itself. Do not use a separate `#[slot(default = "...")]` attribute unless we later discover a real need.

If the enum has exactly one variant, defaulting to that variant is acceptable.
If it has multiple variants and no explicit default, emit a compile error.

For domain enums whose "real" variants require meaningful data, prefer an
explicit neutral unit variant such as `Unset` / `None` / `Disabled` and mark it
`#[default]`. In particular, `MappingConfig` should grow an `Unset`-style nop
variant instead of making `PathPoints` the default empty state.

### How should tuple variants work?

Only a single unnamed field is supported. The variant shape/data delegates to
that field's `FieldSlot` / `FieldSlotMut`, exactly like tuple struct wrappers.

### How should named variants work?

Generate record-shaped payloads:

- fields use the same public-field, `#[slot(name = "...")]`, and shape override
  rules as record structs
- `SlottedEnum::data` returns `SlotDataAccess::Record(self)`
- `SlottedEnumMut::data_mut` returns `SlotDataMutAccess::Record(self)`
- generated `SlotRecordAccess` and `SlotRecordMutAccess` dispatch only over the
  active named-field variant

### How should unit variants work?

Generate unit payload shape and data:

- shape is `SlotShape::Unit`
- immutable data is `SlotDataAccess::Unit(Revision::default())`
- mutable data is not directly owned by the raw enum, so `SlottedEnumMut` should
  use `SlotDataMutAccess::Unit` only when a mutable revision is available. Since
  raw enums do not carry revisions, unit variant data should be exposed through
  `EnumSlot<T>`'s existing unit-special-case revision behavior.

Practical implementation note: for raw enum unit variants,
`SlottedEnum::data` can return `SlotDataAccess::Unit(Revision::default())`.
`EnumSlot<T>` already replaces unit immutable data with the enum slot's variant
revision. For mutable access, `EnumSlot<T>` already detects unit immutable data
and returns `SlotDataMutAccess::Unit(self.inner.changed_at_mut())` before
calling `T::data_mut()`, so `SlottedEnumMut::data_mut` for unit variants can
return a placeholder unit revision or be unreachable for those active variants.

### Should enum derive generate `Default`?

Yes, if doing so does not conflict with an existing user impl. Procedural macros
cannot reliably detect external impl conflicts, so the practical rule should be:
`Slotted` enum derive generates `Default`. Authors should not also derive or
implement `Default` on slotted enums. This matches our "model layer has
defaults everywhere" direction.

If this turns out too restrictive, we can add `#[slot(no_default)]` later.

## Risks

- Generated enum code can get verbose. Keep the derive implementation helperized
  in the proc macro crate, but generated runtime code should mostly be simple
  matches and calls to existing traits.
- Named-field enum variants need generated record access. This is the most
  important correctness point because `MappingConfig` and `PathSpec` depend on
  fields being addressable through slot paths.
- Existing serde annotations remain only where the surrounding code still needs
  them temporarily. The slot derive should not parse or mirror serde rename
  policy. Slot discriminator names come from Rust variant identifiers by
  default.
- Unit variant mutable data is subtle because raw enums do not own revision.
  Keep revision ownership in `EnumSlot<T>`.
