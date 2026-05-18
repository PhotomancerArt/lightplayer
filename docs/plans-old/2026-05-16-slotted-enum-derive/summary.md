# Summary

## What Was Built

- Added `#[derive(Slotted)]` support for structured enums.
- Supported unit variants, one-field tuple wrapper variants, and named-field record variants.
- Added Rust-style `#[default]` handling for enum defaults and `#[slot(name = "...")]` as a variant-name escape hatch.
- Split the slotted derive macro implementation into record, wrapper, and enum files.
- Converted mockup `MappingConfig` and `PathSpec` to derived enum slot machinery.
- Converted real `NodeDef`, `MappingConfig`, and `PathSpec` to derived enum slot machinery.
- Added `MappingConfig::Unset` as the explicit neutral/default mapping state.
- Updated slot authored fixture strings to use Rust variant discriminators such as `Project`, `Output`, `PathPoints`, and `RingArray`.
- Updated slot design docs to describe derived enum behavior and `EnumSlot<T>` revision ownership.

## Decisions For Future Reference

#### Rust Variant Discriminators

- **Decision:** Derived slot enum discriminators default to the Rust variant name.
- **Why:** This keeps authored data searchable against code and avoids hidden snake_case policy.
- **Rejected alternatives:** `rename_all = "snake_case"` mirroring; external discriminator tables.

#### Enum Defaults

- **Decision:** Slotted enums use Rust-style `#[default]`; multiple-variant enums must choose one.
- **Why:** Generic default-and-mutate loading needs every model object to have a default state.
- **Rejected alternatives:** `#[slot(default = "...")]`; separate portable default blobs.

#### Revision Ownership

- **Decision:** Raw slotted enums expose shape and data, while `EnumSlot<T>` owns the active-variant revision.
- **Why:** Plain Rust enums cannot carry slot revision state by themselves.
- **Rejected alternatives:** Treating raw enums as full slot roots; hiding enum revision in payload fields.

#### Unset Mapping

- **Decision:** `MappingConfig` now has `Unset` as its neutral default.
- **Why:** Empty `PathPoints` is not an honest authored mapping; `Unset` makes the sentinel explicit.
- **Rejected alternatives:** Defaulting to `PathPoints`; keeping a manual `default_variant`.
