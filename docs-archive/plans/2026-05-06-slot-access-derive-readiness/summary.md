# Slot Access Derive Readiness Summary

## Completed

- Promoted reusable slot shape builder helpers into `lpc-model` under `slot::shape`.
- Added `SlotRecordShape` and `SlotEnumShape` as the static shape traits used by Rust-authored slot records and enums.
- Added the `lpc-slot-macros` proc-macro crate and exposed `#[derive(SlotRecord)]` through the `lpc-model` `derive` feature.
- Converted static source/runtime records in `lpc-slot-mockup` to use the derive macro.
- Kept dynamic shader params and the fixture mapping enum manual so they continue to pressure the non-derived access model.
- Added derive integration coverage in `lpc-model`.

## Validation

- `cargo fmt -p lpc-model -p lpc-slot-macros -p lpc-slot-mockup`
- `cargo test -p lpc-model`
- `cargo test -p lpc-model --features derive`
- `cargo check -p lpc-model --no-default-features`
- `cargo check -p lpc-model --features schema-gen,derive`
- `cargo test -p lpc-slot-mockup`
- `cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1`
- `cargo check -p lpc-view`
- `cargo check -p lpc-wire --features schema-gen`
- `git diff --check`

## Notes

- The macro is intentionally scoped to record-shaped Rust structs. Enums, dynamic shader param records, and richer generated mutation helpers remain future work.
- The mockup is now closer to a realistic proving ground for converting real LightPlayer source and engine records.
