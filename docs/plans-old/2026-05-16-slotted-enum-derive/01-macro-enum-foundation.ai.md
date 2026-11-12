# Phase 1: Macro Enum Foundation

## Scope Of Phase

Implement `#[derive(Slotted)]` enum support in `lpc-slot-macros` and validate it
with focused derive tests in `lpc-model`.

In scope:

- refactor `lpc-slot-macros/src/record.rs` into clearer slotted modules
- parse enum container and variant attributes
- support unit variants
- support one-field tuple variants
- support named-field variants
- generate the core slot enum traits
- add focused tests proving generated behavior

Out of scope:

- converting real model enums
- converting mockup enums
- supporting multiple-field tuple variants
- changing serializer/discriminator behavior

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep the derive dispatcher small.
- Put parsing helpers near the code that consumes them.
- Keep generated runtime code simple match expressions; complexity belongs in
  the macro implementation.

Suggested files:

- `lp-core/lpc-slot-macros/src/slotted.rs`
- `lp-core/lpc-slot-macros/src/slotted_record.rs`
- `lp-core/lpc-slot-macros/src/slotted_wrapper.rs`
- `lp-core/lpc-slot-macros/src/slotted_enum.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-model/tests/slotted_enum_derive.rs`

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Refactor the current derive implementation:

- Move named-field struct logic out of `record.rs` into a record-focused module.
- Move one-field tuple struct logic into a wrapper-focused module.
- Add an enum-focused module.
- Keep `#[proc_macro_derive(Slotted, attributes(slot, default))]` as the public
  entry so enum variants can use Rust-style `#[default]`.

Extend attribute parsing:

- Add `VariantAttrs { name: Option<LitStr>, is_default: bool }`.
- Add `parse_variant`.
- Keep field attribute behavior shared between record structs and named enum
  variants.

Enum validation:

- unit variants are allowed.
- named-field variants are allowed; all fields must be public.
- unnamed variants must have exactly one field.
- multiple-field tuple variants emit a clear compile error.
- multiple variants require one `#[default]` variant.
- one variant can default implicitly.
- `#[default]` is read from variant attributes, not from a `#[slot(...)]`
  container attribute.

Generated impls:

- `Default`
- `SlotEnumShape`
- `SlottedEnum`
- `SlottedEnumMut`
- `SlotRecordAccess`
- `SlotRecordMutAccess`

Test cases in `slotted_enum_derive.rs`:

- unit-only enum exposes unit shape/data and can switch defaults
- single tuple variant delegates shape/data to wrapped payload
- named variant exposes fields by index and mutable field access works
- variant discriminator defaults to the Rust variant name; `#[slot(name = "...")]` is only an escape hatch
- `#[default]` controls default variant
- unknown variant mutation error lists expected slot names

## Validate

```bash
cargo fmt -p lpc-slot-macros -p lpc-model
cargo test -p lpc-model --features derive --test slotted_enum_derive
cargo check -p lpc-model
```
