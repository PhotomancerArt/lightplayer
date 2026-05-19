# Phase 1: Remove Mockup Codec Experiments

## Scope Of Phase

Delete mockup tests that exercise old hand-written or generated codec systems.
Preserve only useful primitive coverage in `lpc-model` tests if needed.

Out of scope: deleting production code.

## Code Organization Reminders

- Keep mockup tests focused on generic registry paths.
- Do not add mockup-specific codec helpers.
- Put any moved low-level tests in `lpc-model/src/slot_codec/mod.rs`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report.

## Implementation Details

Delete or unhook:

- `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/native_stream.rs`

Update:

- `lp-core/lpc-slot-mockup/src/tests/mod.rs`

If `native_stream` has unique low-level coverage not already present in
`lpc-model::slot_codec` tests, add a compact equivalent there.

## Validate

```bash
cargo test -p lpc-slot-mockup dynamic_slot_codec
cargo test -p lpc-model slot_codec
```
