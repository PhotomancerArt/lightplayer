# M1.1 Summary

## Outcome

Slot record derives now infer fields by default. A named field is included unless it has `#[slot(skip)]`, and the field type must implement `FieldSlot`.

Root records can use `#[slot(root)]`; their shape id is inferred from `module_path!()` plus the struct name. `#[slot(shape_id = "...")]` still works as an explicit compatibility override.

Semantic field slots are real newtypes under `lpc-model/src/slot/slots/`, with terse filenames such as `ratio.rs`, `dim2u.rs`, and `resource_ref.rs`. `ValueSlot<T>` remains the generic versioned value slot.

## Notable Changes

- Added `FieldSlot` as the field-level inference trait.
- Added inferred `FieldSlot` support for `ValueSlot<T>`, `MapSlot<K,V>`, `OptionSlot<T>`, derived records, and semantic slot newtypes.
- Kept `SlotData` dynamic containers separate from typed slot wrappers.
- Updated `lpc-slot-mockup` derives to remove noisy `#[slot(value = ...)]`, `#[slot(leaf = ...)]`, `#[slot(record)]`, and `#[slot(map(...))]` annotations.
- Implemented manual `FieldSlot` for the mockup fixture mapping enum.

## Validation

```bash
cargo fmt --package lpc-model --package lpc-slot-macros --package lpc-slot-mockup
cargo test -p lpc-model --lib --tests
cargo test -p lpc-model --features derive --test slot_record_derive
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --features schema-gen
cargo check -p lpc-engine -p lpa-client -p lpa-server -p lp-cli
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
cargo clippy -p lpc-slot-macros --all-targets -- -D warnings
```
